use crate::config::{ClientProfile, LauncherConfig};
use crate::pack;
use crate::status::ServerStatus;
// Moved to launcher-app (phase 2, no behaviour change); re-export keeps every
// `crate::commands::…` call site unchanged.
use launcher_app::ops::java::{resolved_java_path, system_java_available};
use launcher_app::ops::pack_check::pack_base_url;
pub use launcher_app::ops::settings::SettingsResponse;
use launcher_app::EventSink;
pub use launcher_app::{GameIdentity, LauncherAppState, VersionCheckResult};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, State};

fn emit_status(
    events: &EventSink,
    phase: &'static str,
    message: impl Into<String>,
    cancelable: bool,
) {
    events(LauncherAppState::status_event(phase, message, cancelable));
}

/// Status sink for the core minecraft runtime: produces exactly today's
/// `launcher://status` payload (`{"phase", "message", "cancelable"}`).
fn minecraft_status_sink(app_handle: &AppHandle) -> crate::minecraft::StatusSink {
    let app_handle = app_handle.clone();
    std::sync::Arc::new(move |phase: &str, message: &str, cancelable: bool| {
        let _ = app_handle.emit(
            "launcher://status",
            serde_json::json!({
                "phase": phase,
                "message": message,
                "cancelable": cancelable,
            }),
        );
    })
}

// ── Settings commands ──────────────────────────────────────────

#[tauri::command]
pub fn load_settings(state: State<'_, LauncherAppState>) -> Result<SettingsResponse, String> {
    launcher_app::ops::settings::load_settings(&state)
}

#[tauri::command]
pub async fn select_profile(
    profile: ClientProfile,
    state: State<'_, LauncherAppState>,
) -> Result<SettingsResponse, String> {
    launcher_app::ops::settings::select_profile(&state, profile).await
}

#[tauri::command]
pub fn save_settings(
    app_handle: AppHandle,
    state: State<'_, LauncherAppState>,
    config: LauncherConfig,
) -> Result<(), String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data dir: {e}"))?;
    launcher_app::ops::settings::save_settings(&state, &app_data_dir, config)
}

// ── Modpack commands ───────────────────────────────────────────

#[tauri::command]
pub async fn check_modpack_version(
    state: State<'_, LauncherAppState>,
) -> Result<VersionCheckResult, String> {
    launcher_app::ops::pack_check::check_modpack_version(&state, |game_dir, meta| {
        match &meta.loader {
            Some(loader) => {
                let id = crate::minecraft::loader_version_id(&meta.minecraft, loader);
                crate::minecraft::loader_profile_exists(game_dir, &id)
                    && crate::minecraft::has_launch_dependencies(
                        game_dir,
                        &meta.minecraft,
                        Some(&id),
                    )
            }
            None => crate::minecraft::has_launch_dependencies(game_dir, &meta.minecraft, None),
        }
    })
    .await
}

// Thin Tauri wrapper: the body moved unchanged to
// `launcher_app::ops::install` (phase 2, no behaviour change).
#[tauri::command]
pub async fn download_modpack(
    _app_handle: AppHandle,
    state: State<'_, LauncherAppState>,
) -> Result<(), String> {
    launcher_app::ops::install::download_modpack(&state).await
}

#[tauri::command]
pub async fn verify_files(
    _app_handle: AppHandle,
    state: State<'_, LauncherAppState>,
    profile: Option<ClientProfile>,
) -> Result<Vec<String>, String> {
    launcher_app::ops::install::verify_files(&state, profile).await
}

#[tauri::command]
pub async fn cancel_download(state: State<'_, LauncherAppState>) -> Result<(), String> {
    launcher_app::ops::install::cancel_download(&state).await
}

#[tauri::command]
pub async fn server_status(state: State<'_, LauncherAppState>) -> Result<ServerStatus, String> {
    launcher_app::status_ops::server_status(&state, &pack_base_url()?).await
}

type StatsQuery = std::collections::BTreeMap<String, String>;

#[tauri::command]
pub async fn stats_leaderboard(query: StatsQuery) -> Result<Option<serde_json::Value>, String> {
    launcher_app::status_ops::stats_leaderboard(&pack_base_url()?, &query).await
}

#[tauri::command]
pub async fn stats_seasons() -> Result<Option<serde_json::Value>, String> {
    launcher_app::status_ops::stats_seasons(&pack_base_url()?).await
}

#[tauri::command]
pub async fn stats_player(
    uuid: String,
    query: StatsQuery,
) -> Result<Option<serde_json::Value>, String> {
    launcher_app::status_ops::stats_player(&pack_base_url()?, &uuid, &query).await
}

#[tauri::command]
pub fn game_status(state: State<'_, LauncherAppState>) -> crate::game::GameStatus {
    launcher_app::status_ops::game_status(&state)
}

/// Opens a link in the user's browser. The URL can come from remote launcher content, so only
/// plain https links are accepted — never a local path, a `file:`/`javascript:` URL or an option.
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    launcher_app::status_ops::open_url(&url)
}

/// Server defaults for a first launch. Minecraft fills in every other option itself and rewrites
/// the whole file on exit, so a partial file is enough. An existing file is the player's own
/// settings and is never touched — and the modpack cannot carry these, the pack host treats
/// options.txt as a private file.
fn seed_game_options(game_dir: &Path) -> std::io::Result<()> {
    let path = game_dir.join("options.txt");
    if path.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(game_dir)?;
    std::fs::write(
        path,
        "fullscreen:true\nrenderDistance:20\nsimulationDistance:20\n",
    )
}

// ── Game launch command ────────────────────────────────────────

#[tauri::command]
pub async fn launch_game(
    app_handle: AppHandle,
    state: State<'_, LauncherAppState>,
) -> Result<(), String> {
    let _operation = state.begin_operation()?;
    let running = state.game.begin()?;
    let config = state.config.read().await.validated_active()?;
    let identity = crate::account::game_identity(&app_handle, &state).await?;
    let pack = state.pack.read().await.clone();

    let java = resolved_java_path(&config);
    if java == "java" && !system_java_available() {
        return Err(
            "Java runtime is not installed. Open Updates to install it or select a Java executable in Settings."
                .to_string(),
        );
    }

    let ram_mb = config.ram_mb;
    let game_dir = config.game_dir.clone();
    let game_dir_path = PathBuf::from(&game_dir);
    std::fs::create_dir_all(&game_dir_path).map_err(|e| e.to_string())?;
    let _tracking = crate::installed_files::begin(&state.app_data_dir, &game_dir_path)?;

    let loader_version_id = pack
        .as_ref()
        .and_then(|p| {
            p.meta
                .loader
                .as_deref()
                .map(|loader| crate::minecraft::loader_version_id(&p.meta.minecraft, loader))
        })
        .or_else(|| crate::minecraft::find_loader_version_id(&game_dir_path));
    let minecraft_version = pack
        .as_ref()
        .map(|p| p.meta.minecraft.clone())
        .or_else(|| {
            loader_version_id.as_ref().and_then(|id| {
                id.strip_prefix("fabric-loader-")?
                    .split_once('-')
                    .map(|(_, mc)| mc.to_string())
            })
        })
        .unwrap_or_else(|| {
            std::env::var("BLOCKFIELD_MINECRAFT_VERSION")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .or_else(|| option_env!("BLOCKFIELD_MINECRAFT_VERSION").map(String::from))
                .unwrap_or_else(|| "1.21.1".to_string())
        });
    let game_server = match &pack {
        Some(pack) => pack.info.server.clone(),
        None if config.active_profile == ClientProfile::Workshop => {
            pack::fetch_launcher_info(&pack_base_url()?).await?.server
        }
        None => String::new(),
    };
    let quick_play = config.target(&game_server)?;
    // Dev build: another server, or none at all so the game opens on the title screen.
    let quick_play = crate::dev::server_override().unwrap_or(quick_play);

    log::info!(
        "Launching game: java={java}, ram={ram_mb}MB, dir={game_dir}, user={}",
        identity.username
    );

    {
        state.downloader.reset_cancel();
        // Floor so the frontend always has a non-zero denominator for progress.
        state.downloader.set_grand_total(16 * 1024 * 1024);
        emit_status(&state.events, "launch", "Проверка компонентов игры", false);
        let launch_status = minecraft_status_sink(&app_handle);
        crate::minecraft::ensure_launch_dependencies(
            &state.downloader,
            &launch_status,
            &game_dir_path,
            &minecraft_version,
            loader_version_id.as_deref(),
        )
        .await?;
    }

    if let Err(error) = seed_game_options(&game_dir_path) {
        log::warn!("Cannot write default game options: {error}");
    }

    emit_status(&state.events, "launch", "Подготовка запуска", false);
    let args = crate::minecraft::build_launch_args(
        &game_dir_path,
        ram_mb,
        &minecraft_version,
        loader_version_id.as_deref(),
        &identity,
        quick_play.as_deref(),
        // Launcher crate version, not launcher-net's internal 0.0.0.
        env!("CARGO_PKG_VERSION"),
    )?;

    if !config.pre_launch_command.trim().is_empty() {
        emit_status(
            &state.events,
            "launch",
            "Выполнение команды перед запуском",
            false,
        );
        let hook_config = config.clone();
        let hook_java = java.clone();
        tokio::task::spawn_blocking(move || {
            crate::launch_hooks::run_hook(
                &hook_config.pre_launch_command,
                &hook_config,
                &hook_java,
                None,
            )
        })
        .await
        .map_err(|e| format!("Pre-launch task failed: {e}"))??;
    }
    emit_status(&state.events, "launch", "Запуск игры", false);
    let log_dir = game_dir_path.join("logs");
    std::fs::create_dir_all(&log_dir).map_err(|e| format!("Cannot create log directory: {e}"))?;
    let output = std::fs::File::create(log_dir.join("launcher-game.log"))
        .map_err(|e| format!("Cannot create game log: {e}"))?;
    let pending_room = app_handle
        .state::<crate::rooms::PendingRoom>()
        .0
        .lock()
        .unwrap()
        .clone();
    let pending_room = pending_room.filter(|_| config.active_profile == ClientProfile::Game);
    let bridge_dir = config.bridge_dir(&state.app_data_dir);
    std::fs::create_dir_all(&bridge_dir).map_err(|e| e.to_string())?;
    if let Some(id) = pending_room.as_ref() {
        crate::rooms::write_request(&state, id)?;
    } else {
        let _ = std::fs::remove_file(bridge_dir.join("room-request.json"));
    }
    let _ = std::fs::remove_file(bridge_dir.join("game-presence.json"));
    let mut child = crate::host_env::command(&java)
        .env("BLOCKFIELD_BRIDGE_DIR", &bridge_dir)
        .env("BLOCKFIELD_SERVER", quick_play.as_deref().unwrap_or(""))
        .args(&args)
        .current_dir(&game_dir)
        .stdin(std::process::Stdio::null())
        .stdout(output.try_clone().map_err(|e| e.to_string())?)
        .stderr(output)
        .spawn()
        .map_err(|e| format!("Failed to spawn game process: {e}"))?;
    running.started();
    {
        let pending = app_handle.state::<crate::rooms::PendingRoom>();
        let mut value = pending.0.lock().unwrap();
        if *value == pending_room {
            *value = None;
        }
    }
    if config.hide_while_playing {
        if let Some(window) = app_handle.get_webview_window("main") {
            if let Err(error) = window.hide() {
                log::warn!("Cannot hide launcher: {error}");
            }
        }
    }
    log::info!("Game process spawned (pid {})", child.id());
    emit_status(&state.events, "launch", "Игра запущена", false);
    let events = state.events.clone();
    std::thread::spawn(move || {
        let _running = running;
        let exit = child.wait();
        _running.finishing();
        crate::game::restore_hidden_launcher(&app_handle);
        match exit {
            Ok(exit) => {
                let result = crate::launch_hooks::run_hook(
                    &config.post_exit_command,
                    &config,
                    &java,
                    exit.code(),
                );
                let message = match result {
                    Ok(()) => format!("Game exited ({exit})"),
                    Err(error) => {
                        emit_status(&events, "hook-error", &error, false);
                        format!("Post-exit command failed: {error}")
                    }
                };
                emit_status(&events, "exited", message, false);
            }
            Err(error) => emit_status(
                &events,
                "exited",
                format!("Failed to wait for game: {error}"),
                false,
            ),
        }
    });
    Ok(())
}

/// Dev-build overrides in effect, so the developer can see what the launcher will actually do.
/// Always `None` in a production build.
#[tauri::command]
pub fn dev_status() -> Option<crate::dev::DevStatus> {
    launcher_app::status_ops::dev_status(&pack_base_url().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_options_are_seeded_once_and_never_overwrite_the_player() {
        let dir = std::env::temp_dir().join(format!("blockfield-options-{}", std::process::id()));
        let options = dir.join("options.txt");

        seed_game_options(&dir).unwrap();
        let seeded = std::fs::read_to_string(&options).unwrap();
        assert!(seeded.contains("fullscreen:true"));
        assert!(seeded.contains("renderDistance:20"));
        assert!(seeded.contains("simulationDistance:20"));

        std::fs::write(&options, "renderDistance:8\n").unwrap();
        seed_game_options(&dir).unwrap();
        assert_eq!(
            std::fs::read_to_string(&options).unwrap(),
            "renderDistance:8\n"
        );

        std::fs::remove_dir_all(dir).unwrap();
    }
}
