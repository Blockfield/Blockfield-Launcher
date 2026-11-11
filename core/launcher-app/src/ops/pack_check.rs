//! Pack-check orchestration, moved unchanged from `src-tauri/src/commands.rs`
//! (phase 2, no behaviour change).
//!
//! The loader check stays injectable: `has_launch_dependencies` still lives in
//! the Tauri adapter (`minecraft.rs`, owned by a later unit), so the wrapper
//! passes it as `loader_ok` and tests pass a stub.

use super::java::java_runtime_ready;
use crate::{LauncherAppState, PackState, VersionCheckResult};
use launcher_core::config::ClientProfile;
use launcher_net::pack::PackMeta;
use std::path::{Path, PathBuf};

pub const INSTALLED_VERSION_FILE: &str = ".blockfield-pack-version";

/// The marker holds the installed `[index] hash`, then the pack version for display.
/// Older markers hold only the version, so they mismatch once and trigger one update.
pub fn installed_pack(game_dir: &Path) -> (String, String) {
    let marker = std::fs::read_to_string(game_dir.join(INSTALLED_VERSION_FILE)).unwrap_or_default();
    let mut lines = marker.lines().map(str::trim).filter(|l| !l.is_empty());
    let hash = lines.next().unwrap_or("none").to_string();
    let version = lines.next().map_or_else(|| hash.clone(), String::from);
    (hash, version)
}

/// Base URL of the hosted packwiz pack (directory containing pack.toml and launcher.json).
pub fn pack_base_url() -> Result<String, String> {
    pack_base_url_for(ClientProfile::Game)
}

/// The Workshop profile installs a separate pack (second URL) into its own game
/// directory with the same marker + file hash check; the game profile keeps the
/// original URL and behavior unchanged.
pub fn pack_base_url_for(profile: ClientProfile) -> Result<String, String> {
    match profile {
        ClientProfile::Game => select_pack_base_url(
            "VITE_BLOCKFIELD_PACK_URL",
            std::env::var("VITE_BLOCKFIELD_PACK_URL").ok().as_deref(),
            option_env!("VITE_BLOCKFIELD_PACK_URL"),
        ),
        ClientProfile::Workshop => select_pack_base_url(
            "VITE_BLOCKFIELD_WORKSHOP_PACK_URL",
            std::env::var("VITE_BLOCKFIELD_WORKSHOP_PACK_URL")
                .ok()
                .as_deref(),
            option_env!("VITE_BLOCKFIELD_WORKSHOP_PACK_URL"),
        ),
    }
}

/// A pack checked for one profile must never install into the other profile's
/// game directory: refuse the cached state after a profile switch.
pub fn require_pack_profile(pack: &PackState, profile: ClientProfile) -> Result<(), String> {
    if pack.profile != profile {
        return Err("Профиль изменился. Повторите проверку.".to_string());
    }
    Ok(())
}

/// Fail closed when the profile switched while a check was in flight: the
/// captured profile's manifest must never bind the other profile's game dir.
fn ensure_profile_unchanged(captured: ClientProfile, current: ClientProfile) -> Result<(), String> {
    if captured != current {
        return Err("Профиль изменился. Повторите проверку.".into());
    }
    Ok(())
}

/// Pure URL selection + validation so unit tests pin both profiles without env mutation.
pub fn select_pack_base_url(
    var: &str,
    runtime: Option<&str>,
    compiled: Option<&str>,
) -> Result<String, String> {
    let url = runtime
        .filter(|v| !v.trim().is_empty())
        .or(compiled)
        .ok_or_else(|| format!("{var} is required"))?;
    let url = url.trim().trim_end_matches('/').to_string();
    if !url.starts_with("https://") && !(cfg!(debug_assertions) && url.starts_with("http://")) {
        return Err(format!("{var} must use https"));
    }
    Ok(url)
}

pub async fn check_modpack_version(
    state: &LauncherAppState,
    loader_ok: impl FnOnce(&Path, &PackMeta) -> bool,
) -> Result<VersionCheckResult, String> {
    let profile = state.config.read().await.active_profile;
    let base = pack_base_url_for(profile)?;
    let info = launcher_net::pack::fetch_launcher_info(&base).await?;
    let meta = launcher_net::pack::fetch_pack_meta(&info.pack).await?;

    // No operation lock across the network fetches above: holding it there
    // starves select_profile/save_settings (and the UI retry that waits on a
    // busy->idle game transition) whenever a probe is in flight. The lock only
    // guards the pack-state write below.
    let _operation = state
        .operation
        .try_lock()
        .map_err(|_| "Дождитесь завершения текущей операции.")?;
    let config = state.config.read().await.active();
    ensure_profile_unchanged(profile, config.active_profile)?;
    let game_dir = PathBuf::from(&config.game_dir);
    let (installed_hash, installed) = installed_pack(&game_dir);
    // A frozen dev pack must not turn "Играть" into "Обновить": the developer manages these files.
    let needs_update = installed_hash != meta.index_hash && !launcher_core::dev::freeze_pack();
    log::info!(
        "[check_modpack_version] pack={} remote={} installed={} needs_update={needs_update}",
        info.pack,
        meta.version,
        installed
    );

    let java = info.java_for_this_platform();
    let java_ok = java
        .as_ref()
        .is_none_or(|java| java_runtime_ready(&game_dir, &java.version));

    let loader_ok = loader_ok(&game_dir, &meta);

    let result = VersionCheckResult {
        needs_update,
        remote_version: meta.version.clone(),
        // Same files as published = same client version. The marker keeps the version seen at the last
        // install, and a pack reinstalled while a server-only release was live carried that release's number.
        installed_version: if installed_hash == meta.index_hash {
            meta.version.clone()
        } else {
            installed
        },
        mirror: base,
        file_count: 0,
        total_size: 0,
        java,
        java_ok,
        loader_ok,
    };
    *state.pack.write().await = Some(PackState {
        profile,
        info,
        meta,
    });
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::super::test_state;
    use super::*;
    use launcher_core::config::LauncherConfig;
    use std::sync::{Mutex, OnceLock};

    // pack_base_url and check_modpack_version read the process-wide env, so
    // tests that mutate it serialize on this lock and restore the old value.
    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn installed_pack_parses_current_and_legacy_markers() {
        let dir = std::env::temp_dir().join(format!(
            "blockfield-marker-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            installed_pack(&dir),
            ("none".to_string(), "none".to_string())
        );
        std::fs::write(dir.join(INSTALLED_VERSION_FILE), "hash123\n2.0.0\n").unwrap();
        assert_eq!(
            installed_pack(&dir),
            ("hash123".to_string(), "2.0.0".to_string())
        );
        std::fs::write(dir.join(INSTALLED_VERSION_FILE), "1.9.0\n").unwrap();
        assert_eq!(
            installed_pack(&dir),
            ("1.9.0".to_string(), "1.9.0".to_string())
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pack_base_url_trims_slash_and_rejects_non_https() {
        let _guard = env_lock().lock().unwrap();
        let old = std::env::var("VITE_BLOCKFIELD_PACK_URL").ok();
        unsafe { std::env::remove_var("VITE_BLOCKFIELD_PACK_URL") };
        // option_env! is baked at compile time; without it an unset runtime
        // var must fail, otherwise this environment cannot test the miss path.
        if option_env!("VITE_BLOCKFIELD_PACK_URL").is_none() {
            assert_eq!(
                pack_base_url().unwrap_err(),
                "VITE_BLOCKFIELD_PACK_URL is required"
            );
        }
        unsafe {
            std::env::set_var(
                "VITE_BLOCKFIELD_PACK_URL",
                "https://packs.example.com/blockfield/",
            )
        };
        assert_eq!(
            pack_base_url().unwrap(),
            "https://packs.example.com/blockfield"
        );
        unsafe { std::env::set_var("VITE_BLOCKFIELD_PACK_URL", "gopher://x") };
        assert_eq!(
            pack_base_url().unwrap_err(),
            "VITE_BLOCKFIELD_PACK_URL must use https"
        );
        unsafe {
            match old {
                Some(v) => std::env::set_var("VITE_BLOCKFIELD_PACK_URL", v),
                None => std::env::remove_var("VITE_BLOCKFIELD_PACK_URL"),
            }
        };
    }

    #[test]
    fn check_modpack_version_fails_fast_without_pack_url() {
        // No network: without a pack URL the command must fail before fetching.
        // Sync test + block_on so the env lock is never held across `.await`.
        let _guard = env_lock().lock().unwrap();
        if option_env!("VITE_BLOCKFIELD_PACK_URL").is_some()
            || std::env::var("VITE_BLOCKFIELD_PACK_URL").is_ok()
        {
            eprintln!("SKIP: VITE_BLOCKFIELD_PACK_URL is set, live fetch out of scope");
            return;
        }
        let dir = std::env::temp_dir().join(format!(
            "blockfield-check-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let state = test_state(dir.clone(), LauncherConfig::default());
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(check_modpack_version(&state, |_, _| {
                panic!("loader check must not run before the pack URL resolves")
            }));
        let Err(err) = result else {
            panic!("expected the missing pack URL error")
        };
        assert_eq!(err, "VITE_BLOCKFIELD_PACK_URL is required");
        assert!(state.pack.blocking_read().is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn workshop_profile_installs_from_second_pack_url() {
        let game = "https://blockfield.pro/pack/";
        let workshop = "https://blockfield.pro/workshop-pack";
        assert_eq!(
            select_pack_base_url("VITE_BLOCKFIELD_PACK_URL", Some(game), Some(workshop)).unwrap(),
            "https://blockfield.pro/pack",
        );
        assert_eq!(
            select_pack_base_url(
                "VITE_BLOCKFIELD_WORKSHOP_PACK_URL",
                Some(workshop),
                Some(game),
            )
            .unwrap(),
            "https://blockfield.pro/workshop-pack",
        );
        // Empty runtime falls back to the compiled value, as before.
        assert_eq!(
            select_pack_base_url(
                "VITE_BLOCKFIELD_WORKSHOP_PACK_URL",
                Some("  "),
                Some(workshop),
            )
            .unwrap(),
            "https://blockfield.pro/workshop-pack",
        );
    }

    #[test]
    fn pack_url_names_the_missing_profile_var() {
        assert_eq!(
            select_pack_base_url("VITE_BLOCKFIELD_WORKSHOP_PACK_URL", None, None).unwrap_err(),
            "VITE_BLOCKFIELD_WORKSHOP_PACK_URL is required",
        );
        assert_eq!(
            select_pack_base_url("VITE_BLOCKFIELD_PACK_URL", None, None).unwrap_err(),
            "VITE_BLOCKFIELD_PACK_URL is required",
        );
    }

    #[test]
    fn profile_switch_mid_check_fails_closed() {
        ensure_profile_unchanged(ClientProfile::Game, ClientProfile::Game).unwrap();
        ensure_profile_unchanged(ClientProfile::Workshop, ClientProfile::Workshop).unwrap();
        assert_eq!(
            ensure_profile_unchanged(ClientProfile::Game, ClientProfile::Workshop).unwrap_err(),
            "Профиль изменился. Повторите проверку.",
        );
        assert_eq!(
            ensure_profile_unchanged(ClientProfile::Workshop, ClientProfile::Game).unwrap_err(),
            "Профиль изменился. Повторите проверку.",
        );
    }

    #[test]
    fn cached_pack_refuses_the_other_profile() {
        use launcher_net::pack::{LauncherInfo, PackMeta};
        fn pack_for(profile: ClientProfile) -> PackState {
            PackState {
                profile,
                info: LauncherInfo {
                    pack: String::new(),
                    server: String::new(),
                    installer: launcher_net::pack::Artifact {
                        url: String::new(),
                        sha256: String::new(),
                    },
                    bootstrap: launcher_net::pack::Artifact {
                        url: String::new(),
                        sha256: String::new(),
                    },
                    java: Default::default(),
                    skins: None,
                },
                meta: PackMeta {
                    version: "1.0".into(),
                    index_hash: "hash".into(),
                    minecraft: "1.21.1".into(),
                    loader: None,
                },
            }
        }
        require_pack_profile(&pack_for(ClientProfile::Game), ClientProfile::Game).unwrap();
        require_pack_profile(&pack_for(ClientProfile::Workshop), ClientProfile::Workshop).unwrap();
        assert_eq!(
            require_pack_profile(&pack_for(ClientProfile::Game), ClientProfile::Workshop)
                .unwrap_err(),
            "Профиль изменился. Повторите проверку.",
        );
        assert_eq!(
            require_pack_profile(&pack_for(ClientProfile::Workshop), ClientProfile::Game)
                .unwrap_err(),
            "Профиль изменился. Повторите проверку.",
        );
    }
}
