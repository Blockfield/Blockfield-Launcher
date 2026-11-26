//! Skin and cape upload — thin Tauri wrappers; the engine lives in launcher-net.
use crate::commands::LauncherAppState;
use tauri::State;

pub use launcher_net::skins::{SkinPreview, SkinUploadResult};

async fn skins_base(state: &LauncherAppState) -> Option<String> {
    state
        .pack
        .read()
        .await
        .as_ref()
        .and_then(|p| p.info.skins.clone())
}

/// Uploads the chosen PNGs; `None` leaves that texture untouched on the server.
///
/// `player_uuid` is the account the UI showed when the user clicked upload: the upload is
/// bound to it and aborts when another account is active now, so one account's skin can
/// never land on another account's session (an account switch between click and run).
#[tauri::command]
pub async fn upload_skin(
    state: State<'_, LauncherAppState>,
    skin_path: Option<String>,
    cape_path: Option<String>,
    slim: bool,
    player_uuid: Option<String>,
) -> Result<SkinUploadResult, String> {
    let config = state.config.read().await.clone();
    if config.api_token.is_empty() || config.player_uuid.is_empty() {
        return Err(format!(
            "{}: log in again",
            launcher_net::account::SESSION_EXPIRED
        ));
    }
    launcher_net::skins::check_upload_account(player_uuid.as_deref(), &config.player_uuid)?;
    let base = skins_base(&state).await;
    launcher_net::skins::upload_skin(&config, base, skin_path, cape_path, slim).await
}

/// Resolve the textures the game server will show for this name: the launcher upload on Drasl
/// first, then the Mojang skin of the same name (mirrors SkinRestorer's fallback order).
/// Local PNG paths override the resolved textures so the preview reflects a pending upload.
#[tauri::command]
pub async fn preview_skin(
    state: State<'_, LauncherAppState>,
    username: String,
    skin_path: Option<String>,
    cape_path: Option<String>,
    slim: bool,
) -> Result<SkinPreview, String> {
    let base = skins_base(&state).await;
    launcher_net::skins::preview_skin(base, username, skin_path, cape_path, slim).await
}
