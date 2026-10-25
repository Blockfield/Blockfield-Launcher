//! Small read-only operations (phase 2): server status, stats, game status,
//! external URL opening and dev status. Bodies moved unchanged from
//! `src-tauri/src/commands.rs`; the Tauri adapter keeps thin wrappers with
//! identical signatures, error strings and serde shapes.

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
    let game_target = match state.pack.read().await.as_ref() {
        Some(pack) => pack.info.server.clone(),
        None => pack::fetch_launcher_info(pack_base).await?.server,
    };
    let target = config
        .target(&game_target)?
        .ok_or("Адрес игрового сервера не настроен.")?;
    let (host, port) = status::split_host_port(&target);
    let ping_host = host.clone();
    let (status, (region_code, location_name), rooms) = tokio::join!(
        tokio::task::spawn_blocking(move || status::ping(&ping_host, port)),
        status::locate(&host, port),
        async {
            if config.active_profile == ClientProfile::Game {
                status::fetch_rooms(pack_base).await
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

pub async fn stats_leaderboard(
    pack_base: &str,
    query: &StatsQuery,
) -> Result<Option<serde_json::Value>, String> {
    status::fetch_stats(pack_base, "leaderboard", query).await
}

pub async fn stats_seasons(pack_base: &str) -> Result<Option<serde_json::Value>, String> {
    status::fetch_stats(pack_base, "seasons", &StatsQuery::new()).await
}

pub async fn stats_player(
    pack_base: &str,
    uuid: &str,
    query: &StatsQuery,
) -> Result<Option<serde_json::Value>, String> {
    status::fetch_stats(pack_base, &format!("players/{uuid}"), query).await
}

pub fn game_status(state: &LauncherAppState) -> GameStatus {
    state.game.snapshot()
}

/// Pure allowlist check split out so it is testable without spawning a browser.
/// Only plain https links are accepted — never a local path, a `file:`/
/// `javascript:` URL or an option. The URL can come from remote launcher content.
pub fn validate_open_url(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") || url.contains(char::is_whitespace) {
        return Err(format!("Refusing to open {url}"));
    }
    Ok(())
}

pub fn open_url(url: &str) -> Result<(), String> {
    validate_open_url(url)?;
    #[cfg(windows)]
    let mut command = {
        // Empty title argument: `start` treats a lone quoted argument as the window title.
        let mut command = launcher_core::host_env::command("cmd.exe");
        command.args(["/D", "/S", "/C", "start", "", url]);
        command
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut command = launcher_core::host_env::command("xdg-open");
        command.arg(url);
        command
    };
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Cannot open link: {e}"))
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
    fn game_status_snapshot_matches_todays_shape() {
        let state = test_state();
        assert_eq!(
            serde_json::to_value(game_status(&state)).unwrap(),
            serde_json::json!({"phase": "idle", "revision": 0})
        );
    }

    #[tokio::test]
    async fn stats_reject_invalid_requests_without_network() {
        let base = "https://example.com/pack";
        // Bad player UUID segment.
        let err = stats_player(base, "not-a-uuid", &StatsQuery::new())
            .await
            .unwrap_err();
        assert_eq!(err, "Invalid stats request: players/not-a-uuid");
        // Unknown query key.
        let mut query = StatsQuery::new();
        query.insert("evil".to_string(), "1".to_string());
        let err = stats_leaderboard(base, &query).await.unwrap_err();
        assert_eq!(err, "Invalid stats request: leaderboard");
    }
}
