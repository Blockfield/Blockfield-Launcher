//! Small read-only operations (phase 2): server status, stats, game status,
//! external URL opening and dev status. Bodies moved unchanged from
//! `src-tauri/src/commands.rs`; the Tauri adapter keeps thin wrappers with
//! identical signatures, error strings and serde shapes.

use crate::ops::pack_check::pack_base_url_for;
use crate::LauncherAppState;
use launcher_core::config::ClientProfile;
use launcher_core::dev::{self, DevStatus};
use launcher_core::game::GameStatus;
use launcher_net::pack;
use launcher_net::status::{self, ServerStatus};
use std::collections::BTreeMap;

pub type StatsQuery = BTreeMap<String, String>;

pub async fn server_status(
    state: &LauncherAppState,
    pack_base: &str,
) -> Result<ServerStatus, String> {
    let config = state.config.read().await.clone();
    // Profile-aware pack base (coordination#40): the Workshop profile reads its
    // second pack URL; the passed base stays as fallback when it is unset.
    let base = pack_base_url_for(config.active_profile).unwrap_or_else(|_| pack_base.to_string());
    // The Workshop pack's own server is its host by design, so the
    // Game-host cross-check in target() would refuse the probe as soon as
    // the pack cache refreshes (card flips to unavailable, #155). Status
    // is a read-only probe of the configured address; the cross-check
    // stays a launch-time guard.
    let target = match config.active_profile {
        ClientProfile::Workshop => config.workshop_probe_target()?,
        ClientProfile::Game => {
            let game_target = match state.pack.read().await.as_ref() {
                Some(pack) => pack.info.server.clone(),
                None => pack::fetch_launcher_info(&base).await?.server,
            };
            config
                .target(&game_target)?
                .ok_or("Адрес игрового сервера не настроен.")?
        }
    };
    let (host, port) = status::split_host_port(&target);
    let ping_host = host.clone();
    let (status, (region_code, location_name), rooms) = tokio::join!(
        tokio::task::spawn_blocking(move || status::ping(&ping_host, port)),
        status::locate(&host, port),
        async {
            if config.active_profile == ClientProfile::Game
                && !pack::default_health().is_cooled_down(&base)
            {
                status::fetch_rooms(&base).await
            } else {
                None
            }
        },
    );
    let mut status = status.map_err(|e| format!("status task failed: {e}"))?;
    if let Some((available, rooms, maintenance)) = rooms {
        status.game_available = Some(available);
        status.rooms = Some(rooms);
        status.maintenance = maintenance;
    }
    status.region_code = region_code;
    status.location_name = location_name;
    Ok(status)
}

pub async fn stats_leaderboard(query: &StatsQuery) -> Result<Option<serde_json::Value>, String> {
    status::fetch_stats(status::STATS_API_URL, "leaderboard", query).await
}

pub async fn stats_seasons() -> Result<Option<serde_json::Value>, String> {
    status::fetch_stats(status::STATS_API_URL, "seasons", &StatsQuery::new()).await
}

pub async fn stats_player(
    uuid: &str,
    query: &StatsQuery,
) -> Result<Option<serde_json::Value>, String> {
    status::fetch_stats(status::STATS_API_URL, &format!("players/{uuid}"), query).await
}

pub fn game_status(state: &LauncherAppState) -> GameStatus {
    state.game.snapshot()
}

/// Pure allowlist check split out so it is testable without spawning a browser.
/// Only plain https links are accepted — never a local path, a `file:`/
/// `javascript:` URL or an option. The URL can come from remote launcher content.
pub fn validate_open_url(url: &str) -> Result<(), String> {
    // Case-sensitive: `HTTPS://` must stay refused and `Url` would lowercase it.
    if !url.starts_with("https://") || url.contains(char::is_whitespace) {
        return Err(format!("Refusing to open {url}"));
    }
    let parsed = url::Url::parse(url).map_err(|_| format!("Refusing to open {url}"))?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() {
        return Err(format!("Refusing to open {url}"));
    }
    Ok(())
}

/// Windows launcher argv without a shell: the URL is one item, never parsed by cmd.exe.
#[cfg(any(windows, test))]
fn windows_open_argv(url: &str) -> (&'static str, [&'static str; 1], &str) {
    ("rundll32", ["url.dll,FileProtocolHandler"], url)
}

#[cfg(windows)]
fn open_url_command(url: &str) -> std::process::Command {
    let (program, fixed, url) = windows_open_argv(url);
    let mut command = launcher_core::host_env::command(program);
    command.args(fixed);
    command.arg(url);
    command
}

#[cfg(not(windows))]
fn open_url_command(url: &str) -> std::process::Command {
    let mut command = launcher_core::host_env::command("xdg-open");
    command.arg(url);
    command
}

pub fn open_url(url: &str) -> Result<(), String> {
    validate_open_url(url)?;
    let mut command = open_url_command(url);
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Cannot open link: {e}"))
}

/// Pure site player-profile URL builder: dashed lowercase UUID only (PR #177).
pub fn player_profile_url(uuid: &str) -> Result<String, String> {
    let uuid = uuid.to_lowercase();
    if !status::valid_player_uuid(&uuid) {
        return Err(format!("Refusing to open profile for {uuid}"));
    }
    Ok(format!("https://blockfield.pro/players/{uuid}"))
}

/// Leaderboard rows and self card open exactly the site player page — no other URL shape passes.
pub fn open_player_profile(uuid: &str) -> Result<(), String> {
    let url = player_profile_url(uuid)?;
    open_url(&url)
}

/// Dev-build overrides in effect, so the developer can see what the launcher will actually do.
/// Always `None` in a production build.
pub fn dev_status(pack_base: &str) -> Option<DevStatus> {
    dev::status(pack_base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use launcher_net::download::{noop_guard_sink, Downloader};
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::sync::{Mutex, RwLock};

    fn test_state() -> LauncherAppState {
        let downloader = Arc::new(Downloader::new(
            Arc::new(|_| {}),
            Arc::new(|_, _| {}),
            noop_guard_sink(),
        ));
        LauncherAppState {
            cancel_flag: downloader.cancel_flag.clone(),
            downloader,
            pack: RwLock::new(None),
            config: RwLock::new(launcher_core::config::LauncherConfig::default()),
            app_data_dir: PathBuf::from("/tmp/blockfield-launcher-app-test"),
            game: Arc::new(launcher_core::game::GameState::new(|_| {})),
            operation: Mutex::new(()),
            events: Arc::new(|_| {}),
        }
    }

    #[test]
    fn open_url_allowlist_accepts_plain_https_links() {
        for url in [
            "https://blockfield.example/play",
            "https://example.com/?a=1&b=2",
        ] {
            assert!(validate_open_url(url).is_ok(), "{url}");
        }
    }

    #[test]
    fn open_url_allowlist_rejects_anything_else() {
        for url in [
            "http://example.com/",
            "HTTPS://example.com/",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "https://example.com/has space",
            "https://example.com/has\ttab",
            "--help",
            "",
        ] {
            let err = validate_open_url(url).unwrap_err();
            assert_eq!(err, format!("Refusing to open {url}"), "{url:?}");
        }
    }

    #[test]
    fn open_url_allowlist_rejects_hostless_https() {
        for url in ["https://", "https://?x=1", "https://#frag"] {
            assert!(validate_open_url(url).is_err(), "{url:?}");
        }
    }

    #[test]
    fn open_url_shell_metachars_never_reach_a_shell() {
        let evil = "https://example.com/redir?a=1&b=2\"|^evil";
        validate_open_url(evil).unwrap();
        // Windows argv is exact even on Linux: no cmd.exe, no /C, URL is one item.
        let (program, fixed, url) = windows_open_argv(evil);
        assert_eq!(program, "rundll32");
        assert_eq!(fixed, ["url.dll,FileProtocolHandler"]);
        assert_eq!(url, evil);
        // Native command argv is exact too: one program, URL as a single arg.
        let command = open_url_command(evil);
        let args: Vec<_> = command.get_args().collect();
        #[cfg(windows)]
        {
            assert_eq!(command.get_program(), std::ffi::OsStr::new("rundll32"));
            assert_eq!(args, ["url.dll,FileProtocolHandler", evil]);
        }
        #[cfg(not(windows))]
        {
            assert_eq!(command.get_program(), std::ffi::OsStr::new("xdg-open"));
            assert_eq!(args, [std::ffi::OsStr::new(evil)]);
        }
    }

    #[test]
    fn game_status_snapshot_matches_todays_shape() {
        let state = test_state();
        assert_eq!(
            serde_json::to_value(game_status(&state)).unwrap(),
            serde_json::json!({"phase": "idle", "revision": 0})
        );
    }

    #[test]
    fn player_profile_url_accepts_only_dashed_uuids_and_lowercases() {
        assert_eq!(
            player_profile_url("F3E1B9A6-2270-3DEE-8782-B7CDD8519EFE").unwrap(),
            "https://blockfield.pro/players/f3e1b9a6-2270-3dee-8782-b7cdd8519efe"
        );
        for bad in [
            "",
            "NetherG",
            "f3e1b9a6_2270_3dee_8782_b7cdd8519efe",
            "f3e1b9a622703dee8782b7cdd8519efe",
            "https://blockfield.pro/players/f3e1b9a6-2270-3dee-8782-b7cdd8519efe",
            "../admin",
        ] {
            assert!(player_profile_url(bad).is_err(), "{bad:?}");
        }
    }

    #[tokio::test]
    async fn stats_reject_invalid_requests_without_network() {
        // Bad player UUID segment.
        let err = stats_player("not-a-uuid", &StatsQuery::new())
            .await
            .unwrap_err();
        assert_eq!(err, "Invalid stats request: players/not-a-uuid");
        // Unknown query key.
        let mut query = StatsQuery::new();
        query.insert("evil".to_string(), "1".to_string());
        let err = stats_leaderboard(&query).await.unwrap_err();
        assert_eq!(err, "Invalid stats request: leaderboard");
    }
}
