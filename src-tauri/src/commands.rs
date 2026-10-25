use crate::config::{ClientProfile, LauncherConfig};
use crate::status::ServerStatus;
// Moved to launcher-app (phase 2, no behaviour change); re-export keeps every
// `crate::commands::…` call site unchanged.
use launcher_app::ops::pack_check::pack_base_url;
pub use launcher_app::ops::settings::SettingsResponse;
pub use launcher_app::{LauncherAppState, VersionCheckResult};
use tauri::{AppHandle, Manager, State};

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

// ── Game launch command ────────────────────────────────────────

// Thin Tauri wrapper: the body moved unchanged to
// `launcher_app::ops::launch` (phase 2, no behaviour change). The window
// stays a shell seam (hide/restore closures) and the launcher version is
// passed in from here: a core crate would evaluate CARGO_PKG_VERSION to 0.0.0.
#[tauri::command]
pub async fn launch_game(
    app_handle: AppHandle,
    state: State<'_, LauncherAppState>,
) -> Result<(), String> {
    let hide_app = app_handle.clone();
    let restore_app = app_handle.clone();
    launcher_app::ops::launch::launch_game(
        &state,
        &app_handle.state::<crate::rooms::PendingRoom>(),
        env!("CARGO_PKG_VERSION"),
        launcher_app::ops::launch::LaunchWindow {
            hide: Box::new(move || {
                if let Some(window) = hide_app.get_webview_window("main") {
                    if let Err(error) = window.hide() {
                        log::warn!("Cannot hide launcher: {error}");
                    }
                }
            }),
            restore: Box::new(move || {
                crate::game::restore_hidden_launcher(&restore_app);
            }),
        },
    )
    .await
}

/// Dev-build overrides in effect, so the developer can see what the launcher will actually do.
/// Always `None` in a production build.
#[tauri::command]
pub fn dev_status() -> Option<crate::dev::DevStatus> {
    launcher_app::status_ops::dev_status(&pack_base_url().unwrap_or_default())
}
