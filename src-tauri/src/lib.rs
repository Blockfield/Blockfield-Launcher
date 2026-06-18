mod commands;
mod config;
mod download;
mod manifest;

use commands::LauncherAppState;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // Resolve app data directory for config persistence
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to resolve app data directory");

            let loaded_config = config::load_config(&app_data_dir);
            log::info!(
                "Loaded config: game_dir={}, ram={}MB, lang={}",
                loaded_config.game_dir,
                loaded_config.ram_mb,
                loaded_config.lang
            );

            // Bootstrap managed state
            let app_handle = app.handle().clone();
            let downloader = download::Downloader::new(app_handle);

            app.manage(LauncherAppState {
                downloader: Arc::new(tokio::sync::Mutex::new(downloader)),
                manifest: tokio::sync::RwLock::new(None),
                config: tokio::sync::RwLock::new(loaded_config),
                app_data_dir,
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::load_settings,
            commands::save_settings,
            commands::get_default_game_dir,
            commands::check_modpack_version,
            commands::download_modpack,
            commands::verify_files,
            commands::cancel_download,
            commands::launch_game,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
