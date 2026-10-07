mod account;
mod commands;
mod config;
mod dev;
mod discord_ipc;
mod download;
mod game;
mod host_env;
mod installed_files;
mod launch_hooks;
mod minecraft;
#[cfg(target_os = "linux")]
mod native_update;
mod pack;
mod presence;
mod rooms;
mod skins;
mod status;
use tauri_plugin_deep_link::DeepLinkExt;
mod updater;

use commands::LauncherAppState;
use std::sync::Arc;
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(windows)]
    if std::env::args().nth(1).as_deref() == Some("--uninstall-cleanup") {
        std::process::exit(if installed_files::uninstall().is_ok() {
            0
        } else {
            1
        });
    }
    // Pre-load .env so env vars are available for plugin configuration below.
    // (setup() will load again and log the result; the second load is a no-op
    // because dotenvy preserves existing env vars.)
    let env_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join(".env");
    let _ = dotenvy::from_path(&env_path);

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            game::show_launcher(app);
        }))
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window
                    .try_state::<LauncherAppState>()
                    .is_some_and(|state| state.game.is_busy())
                {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .setup(move |app| {
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;
            app.manage(updater::LauncherUpdater::default());
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
                operation: tokio::sync::Mutex::new(()),
                game: Arc::new(game::GameState::new({
                    let app = app.handle().clone();
                    move |status| {
                        let _ = app.emit("game://status", status);
                        if status.phase == game::GamePhase::Idle {
                            game::restore_hidden_launcher(&app);
                        }
                    }
                })),
                pack: tokio::sync::RwLock::new(None),
                config: tokio::sync::RwLock::new(loaded_config),
                app_data_dir,
            });

            app.manage(rooms::PendingRoom::default());
            #[cfg(target_os = "linux")]
            if !cfg!(debug_assertions) {
                if let Err(error) = app.deep_link().register_all() {
                    log::warn!("Cannot register room links: {error}");
                }
            }
            if let Ok(Some(urls)) = app.deep_link().get_current() {
                for url in urls {
                    rooms::receive(app.handle(), url.as_str());
                }
            }
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    rooms::receive(&handle, url.as_str());
                }
            });
            presence::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            updater::check_launcher_update,
            updater::install_launcher_update,
            account::account_status,
            account::login,
            account::register,
            account::logout,
            account::change_password,
            commands::load_settings,
            commands::save_settings,
            commands::select_profile,
            commands::check_modpack_version,
            commands::download_modpack,
            commands::verify_files,
            commands::cancel_download,
            commands::launch_game,
            commands::game_status,
            commands::server_status,
            commands::stats_leaderboard,
            commands::stats_seasons,
            commands::stats_player,
            commands::open_url,
            commands::dev_status,
            skins::upload_skin,
            skins::preview_skin,
            rooms::pending_room,
            rooms::select_room,
            rooms::join_running_room,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
