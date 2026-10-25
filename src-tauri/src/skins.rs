//! Skin and cape upload — thin Tauri wrappers; the engine lives in launcher-net.
use crate::commands::LauncherAppState;
use tauri::State;

pub use launcher_net::skins::{SkinPreview, SkinUploadResult, DEFAULT_SERVER};

async fn skins_base(state: &LauncherAppState) -> Option<String> {
    state
        .pack
        .read()
        .await
        .as_ref()
        .and_then(|p| p.info.skins.clone())
}

/// Uploads the chosen PNGs; `None` leaves that texture untouched on the server.
#[tauri::command]
pub async fn upload_skin(
    state: State<'_, LauncherAppState>,
    skin_path: Option<String>,
    cape_path: Option<String>,
    slim: bool,
) -> Result<SkinUploadResult, String> {
    let config = state.config.read().await.clone();
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

pub async fn copy_mojang_skin(state: &LauncherAppState) {
    let config = state.config.read().await.clone();
    let base = skins_base(state).await;
    launcher_net::skins::copy_mojang_skin(&config, base).await;
}
