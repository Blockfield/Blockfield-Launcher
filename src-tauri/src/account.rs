//! Drasl account commands — thin Tauri wrappers; the engine lives in launcher-net.
use crate::commands::{GameIdentity, LauncherAppState};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

pub use launcher_net::account::{api, client, AccountStatus, SESSION_EXPIRED};

async fn skins_base(state: &LauncherAppState) -> Option<String> {
    state
        .pack
        .read()
        .await
        .as_ref()
        .and_then(|p| p.info.skins.clone())
}

/// Shell-side `drasl_base`: same https rule, `State` kept out of the core engine.
pub async fn drasl_base(state: &LauncherAppState) -> Result<String, String> {
    launcher_net::account::drasl_base(skins_base(state).await.as_deref())
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

/// Validates the stored session, refreshing it when needed. The expiry emit keeps today's
/// exact `account://expired` name and `()` payload via the core callback.
pub async fn game_identity(
    app: &AppHandle,
    state: &LauncherAppState,
) -> Result<GameIdentity, String> {
    let base = skins_base(state).await;
    let app = app.clone();
    let on_expired: launcher_net::account::SessionExpiredSink =
        Arc::new(move || _ = app.emit("account://expired", ()));
    launcher_net::account::game_identity(&state.config, &state.app_data_dir, base, &on_expired)
        .await
        .map(|identity| GameIdentity {
            username: identity.username,
            uuid: identity.uuid,
            access_token: identity.access_token,
        })
}
