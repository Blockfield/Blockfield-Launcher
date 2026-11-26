//! Drasl account commands — thin Tauri wrappers; the engine lives in launcher-net.
use crate::commands::LauncherAppState;
use tauri::State;

pub use launcher_net::account::AccountStatus;

async fn skins_base(state: &LauncherAppState) -> Option<String> {
    state
        .pack
        .read()
        .await
        .as_ref()
        .and_then(|p| p.info.skins.clone())
}

#[tauri::command]
pub async fn account_status(state: State<'_, LauncherAppState>) -> Result<AccountStatus, String> {
    let base = skins_base(&state).await;
    launcher_net::account::account_status(&state.config, &state.app_data_dir, base).await
}

#[tauri::command]
pub async fn login(
    state: State<'_, LauncherAppState>,
    username: String,
    password: String,
) -> Result<AccountStatus, String> {
    let base = skins_base(&state).await;
    launcher_net::account::login(&state.config, &state.app_data_dir, base, username, password).await
}

#[tauri::command]
pub async fn register(
    state: State<'_, LauncherAppState>,
    username: String,
    password: String,
) -> Result<AccountStatus, String> {
    let base = skins_base(&state).await;
    launcher_net::account::register(&state.config, &state.app_data_dir, base, username, password)
        .await
}

#[tauri::command]
pub async fn logout(state: State<'_, LauncherAppState>) -> Result<AccountStatus, String> {
    let base = skins_base(&state).await;
    launcher_net::account::logout(&state.config, &state.app_data_dir, base).await
}

#[tauri::command]
pub async fn change_password(
    state: State<'_, LauncherAppState>,
    password: String,
) -> Result<AccountStatus, String> {
    let base = skins_base(&state).await;
    launcher_net::account::change_password(&state.config, &state.app_data_dir, base, password).await
}
