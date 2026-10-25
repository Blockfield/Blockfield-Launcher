//! Pack-check orchestration, moved unchanged from `src-tauri/src/commands.rs`
//! (phase 2, no behaviour change).
//!
//! The loader check stays injectable: `has_launch_dependencies` still lives in
//! the Tauri adapter (`minecraft.rs`, owned by a later unit), so the wrapper
//! passes it as `loader_ok` and tests pass a stub.

use super::java::java_runtime_ready;
use crate::{LauncherAppState, PackState, VersionCheckResult};
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
    let url = std::env::var("VITE_BLOCKFIELD_PACK_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| option_env!("VITE_BLOCKFIELD_PACK_URL").map(String::from))
        .ok_or_else(|| "VITE_BLOCKFIELD_PACK_URL is required".to_string())?;
    let url = url.trim().trim_end_matches('/').to_string();
    if !url.starts_with("https://") && !(cfg!(debug_assertions) && url.starts_with("http://")) {
        return Err("VITE_BLOCKFIELD_PACK_URL must use https".to_string());
    }
    Ok(url)
}

pub async fn check_modpack_version(
    state: &LauncherAppState,
    loader_ok: impl FnOnce(&Path, &PackMeta) -> bool,
) -> Result<VersionCheckResult, String> {
    let base = pack_base_url()?;
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
    *state.pack.write().await = Some(PackState { info, meta });
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
}
