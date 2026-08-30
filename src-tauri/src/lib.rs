mod commands;
mod config;
mod download;
mod minecraft;
mod pack;
mod status;

use commands::LauncherAppState;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Pre-load .env so env vars are available for plugin configuration below.
    // (setup() will load again and log the result; the second load is a no-op
    // because dotenvy preserves existing env vars.)
    let env_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join(".env");
    let _ = dotenvy::from_path(&env_path);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(move |app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // Log whether .env was loaded successfully (or already loaded above).
            match dotenvy::from_path(&env_path) {
                Ok(()) => log::info!(".env loaded: {}", env_path.display()),
                Err(e) => log::warn!(".env not loaded: {e} (tried {})", env_path.display()),
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
            // Share the atomic cancel flag so cancel_download can set it
            // without locking the downloader.
            let cancel_flag = downloader.cancel_flag.clone();
            let downloader = Arc::new(downloader);

            app.manage(LauncherAppState {
                downloader,
                cancel_flag,
                pack: tokio::sync::RwLock::new(None),
                config: tokio::sync::RwLock::new(loaded_config),
                app_data_dir,
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::load_settings,
            commands::save_settings,
            commands::check_modpack_version,
            commands::download_modpack,
            commands::verify_files,
            commands::cancel_download,
            commands::launch_game,
            commands::server_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
