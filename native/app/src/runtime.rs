//! Runtime ownership and the state construction shared with the Tauri adapter.
use std::{io, path::PathBuf, sync::Arc};

use launcher_app::{EventSink, LauncherAppState, LauncherEvent};
use launcher_core::game::GameState;
use launcher_net::{download::Downloader, installed_files};
use tokio::{
    runtime::Runtime,
    sync::{Mutex, RwLock},
};

pub fn new() -> io::Result<Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .thread_name("launcher-worker")
        .enable_all()
        .build()
}

pub fn identifier(dev: bool) -> &'static str {
    if dev {
        "com.blockfield.launcher.dev"
    } else {
        "com.blockfield.launcher"
    }
}

pub fn app_data_dir() -> io::Result<PathBuf> {
    // Tauri PathResolver::app_data_dir uses dirs::data_dir + bundle identifier.
    dirs::data_dir()
        .map(|base| base.join(identifier(launcher_core::dev::is_dev_build())))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Failed to resolve app data directory",
            )
        })
}

/// Called on a blocking worker: loading config and building HTTP clients may do IO.
pub fn build_state(events: EventSink) -> io::Result<Arc<LauncherAppState>> {
    let app_data_dir = app_data_dir()?;
    eprintln!("APP_DATA_DIR {}", app_data_dir.display());
    let config = launcher_core::config::load_config(&app_data_dir);
    let progress = events.clone();
    let retry = events.clone();
    let downloader = Arc::new(Downloader::new(
        Arc::new(move |payload| progress(LauncherEvent::DownloadProgress(payload))),
        Arc::new(move |file, attempt| {
            retry(LauncherAppState::status_event(
                "retry",
                format!("Retrying {file} ({attempt}/3)"),
                true,
            ))
        }),
        Arc::new(|path| {
            let guard = installed_files::Write::new(path);
            Box::new(move || guard.finish())
        }),
    ));
    let game_events = events.clone();
    Ok(Arc::new(LauncherAppState {
        cancel_flag: downloader.cancel_flag.clone(),
        downloader,
        pack: RwLock::new(None),
        config: RwLock::new(config),
        app_data_dir,
        game: Arc::new(GameState::new(move |status| {
            game_events(LauncherEvent::GameStatus(status))
        })),
        operation: Mutex::new(()),
        events,
    }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn identifiers_match_tauri_build_flavours() {
        for (dev, config) in [
            (false, include_str!("../../../src-tauri/tauri.conf.json")),
            (true, include_str!("../../../src-tauri/tauri.dev.conf.json")),
        ] {
            let config: serde_json::Value = serde_json::from_str(config).unwrap();
            assert_eq!(config["identifier"], super::identifier(dev));
            assert!(config["app"]["appDirectoriesOverride"].is_null());
        }
    }
}
