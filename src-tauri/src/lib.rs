mod account;
mod commands;
mod config;
mod dev;
mod download;
mod game;
mod host_env;
mod installed_files;
mod minecraft;
#[cfg(target_os = "linux")]
mod native_update;
mod presence;
mod rooms;
mod skins;
mod status;
use tauri_plugin_deep_link::DeepLinkExt;
mod updater;

use commands::LauncherAppState;
use launcher_app::LauncherEvent;
use std::sync::Arc;
use tauri::{Emitter, Manager};

/// Names of the commands served through Tauri `invoke`, in `generate_handler!`
/// order below. `generate_handler!` cannot be introspected, so this adjacent
/// fixed-size list is the testable copy: the size forces a compile error when
/// a command is added or removed, and `tests/adapter_contract.rs` fails CI if
/// the contents drift from either side (macro or frontend `invoke()` calls).
pub const REGISTERED_COMMANDS: [&str; 27] = [
    "check_launcher_update",
    "install_launcher_update",
    "account_status",
    "login",
    "register",
    "logout",
    "change_password",
    "load_settings",
    "save_settings",
    "select_profile",
    "check_modpack_version",
    "download_modpack",
    "verify_files",
    "cancel_download",
    "launch_game",
    "game_status",
    "server_status",
    "stats_leaderboard",
    "stats_seasons",
    "stats_player",
    "open_url",
    "dev_status",
    "upload_skin",
    "preview_skin",
    "pending_room",
    "select_room",
    "join_running_room",
];

/// Frontend event name for a [`LauncherEvent`], exactly as emitted today.
pub fn event_name(event: &LauncherEvent) -> &'static str {
    match event {
        LauncherEvent::Status(_) => "launcher://status",
        LauncherEvent::DownloadProgress(_) => "download://progress",
        LauncherEvent::DownloadMirror(_) => "download://mirror",
        LauncherEvent::GameStatus(_) => "game://status",
        LauncherEvent::RoomPending => "room://pending",
        LauncherEvent::AccountExpired => "account://expired",
    }
}

/// Frontend payload for a [`LauncherEvent`], exactly as emitted today
/// (`room://pending` and `account://expired` carry `null`, like `()` does).
/// Raw JSON keeps today's key order byte-for-byte: routing the structs through
/// `serde_json::Value` would re-sort keys and change the wire bytes.
pub fn event_payload(event: &LauncherEvent) -> Box<serde_json::value::RawValue> {
    let json = match event {
        LauncherEvent::Status(payload) => serde_json::to_string(payload),
        LauncherEvent::DownloadProgress(payload) => serde_json::to_string(payload),
        LauncherEvent::DownloadMirror(mirror) => serde_json::to_string(&serde_json::json!({
            "name": mirror.name,
            "speed": mirror.speed,
        })),
        LauncherEvent::GameStatus(status) => serde_json::to_string(status),
        LauncherEvent::RoomPending | LauncherEvent::AccountExpired => Ok("null".to_string()),
    }
    .expect("event payloads serialize to valid JSON");
    serde_json::value::RawValue::from_string(json).expect("event payloads serialize to valid JSON")
}

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

            // Phase 2 event seam: core state emits LauncherEvent; the adapter maps
            // each variant back to today's exact event name and payload.
            let sink_app = app.handle().clone();
            let events: launcher_app::EventSink = Arc::new(move |event| {
                if matches!(event, LauncherEvent::RoomPending) {
                    // The rooms bridge showed the window before emitting;
                    // every core RoomPending (deep link or Discord join)
                    // restores it here so the shell mapping stays exact.
                    crate::game::show_launcher(&sink_app);
                }
                let _ = sink_app.emit(event_name(&event), event_payload(&event));
            });
            let progress_app = app.handle().clone();
            let progress_sink: download::ProgressSink =
                std::sync::Arc::new(move |payload: download::DownloadProgressPayload| {
                    if let Err(e) = progress_app.emit("download://progress", payload) {
                        log::error!("Failed to emit progress event: {e}");
                    }
                });
            let retry_app = app.handle().clone();
            let retry_sink: download::RetrySink =
                std::sync::Arc::new(move |file_path: String, attempt: u64| {
                    let _ = retry_app.emit(
                        "launcher://status",
                        serde_json::json!({
                            "phase": "retry",
                            "message": format!("Retrying {file_path} ({attempt}/3)"),
                            "cancelable": true,
                        }),
                    );
                });
            let guard_sink: download::GuardSink = std::sync::Arc::new(|path: &std::path::Path| {
                let guard = crate::installed_files::Write::new(path);
                let finish: download::WriteFinish = Box::new(move || guard.finish());
                finish
            });
            let downloader = download::Downloader::new(progress_sink, retry_sink, guard_sink);
            // Share the atomic cancel flag so cancel_download can set it
            // without locking the downloader.
            let cancel_flag = downloader.cancel_flag.clone();
            let downloader = Arc::new(downloader);

            app.manage(LauncherAppState {
                downloader,
                cancel_flag,
                operation: tokio::sync::Mutex::new(()),
                events,
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
