//! Phase-2 operation modules: Tauri-independent command bodies over
//! [`super::LauncherAppState`]. Each `src-tauri` wrapper keeps its exact
//! signature and error strings and forwards here.

pub mod java;
pub mod pack_check;
pub mod settings;

#[cfg(test)]
pub(crate) fn test_state(
    app_data_dir: std::path::PathBuf,
    config: launcher_core::config::LauncherConfig,
) -> crate::LauncherAppState {
    use launcher_core::game::GameState;
    use launcher_net::download::{noop_guard_sink, Downloader};
    use std::sync::Arc;
    use tokio::sync::{Mutex, RwLock};

    let downloader = Arc::new(Downloader::new(
        Arc::new(|_| {}),
        Arc::new(|_, _| {}),
        noop_guard_sink(),
    ));
    crate::LauncherAppState {
        cancel_flag: downloader.cancel_flag.clone(),
        downloader,
        pack: RwLock::new(None),
        config: RwLock::new(config),
        app_data_dir,
        game: Arc::new(GameState::new(|_| {})),
        operation: Mutex::new(()),
        events: Arc::new(|_| {}),
    }
}
