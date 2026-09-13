//! Skin and cape upload to the Drasl account of the signed-in player; SkinRestorer on the game
//! server pulls the textures back by player name, falling back to the Mojang skin of that name.
use crate::account::{api, drasl_base, SESSION_EXPIRED};
use crate::commands::{valid_username, LauncherAppState};
use base64::Engine;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::Path;
use tauri::State;

pub const DEFAULT_SERVER: &str = "https://skins.nether.pp.ua";
const MAX_TEXTURE_BYTES: u64 = 1024 * 1024;
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinUploadResult {
    pub skin_url: Option<String>,
    pub cape_url: Option<String>,
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
    if config.api_token.is_empty() || config.player_uuid.is_empty() {
        return Err(format!("{SESSION_EXPIRED}: log in again"));
    }
    let skin = skin_path.as_deref().map(read_texture).transpose()?;
    let cape = cape_path.as_deref().map(read_texture).transpose()?;
    if skin.is_none() && cape.is_none() {
        return Err("Choose a skin or cape PNG to upload".to_string());
    }

    let base = drasl_base(&state).await?;
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    let mut body = json!({ "skinModel": if slim { "slim" } else { "classic" } });
    if let Some(skin) = skin {
        body["skinBase64"] = Value::String(skin);
    }
    if let Some(cape) = cape {
        body["capeBase64"] = Value::String(cape);
    }
    let player = api(client
        .patch(format!(
            "{base}/drasl/api/v3/players/{}",
            config.player_uuid
        ))
        .bearer_auth(&config.api_token)
        .json(&body))
    .await?;
    Ok(SkinUploadResult {
        skin_url: player["skinUrl"].as_str().map(String::from),
        cape_url: player["capeUrl"].as_str().map(String::from),
    })
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinPreview {
    /// PNG textures as `data:` URLs so the renderer needs no CSP entry per texture host.
    pub skin: Option<String>,
    pub cape: Option<String>,
    pub slim: bool,
    /// `custom` when the player has a launcher-uploaded skin, `mojang` for a licensed name, `none` otherwise.
    pub source: &'static str,
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
    let username = username.trim().to_string();
    let base = drasl_base(&state).await?;
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let mut preview = SkinPreview {
        skin: None,
        cape: None,
        slim,
        source: "none",
    };
    if valid_username(&username) {
        let sources = [
            (base.as_str(), base.as_str(), "custom"),
            (
                "https://api.mojang.com",
                "https://sessionserver.mojang.com",
                "mojang",
            ),
        ];
        for (profiles, sessions, source) in sources {
            if let Some(textures) = yggdrasil_textures(&client, profiles, sessions, &username).await
            {
                preview.skin = fetch_data_url(&client, textures["SKIN"]["url"].as_str()).await;
                preview.cape = fetch_data_url(&client, textures["CAPE"]["url"].as_str()).await;
                preview.slim = textures["SKIN"]["metadata"]["model"].as_str() == Some("slim");
                preview.source = source;
                break;
            }
        }
    }
    if let Some(path) = skin_path.as_deref().filter(|p| !p.is_empty()) {
        preview.skin = Some(format!("data:image/png;base64,{}", read_texture(path)?));
        preview.slim = slim;
    }
    if let Some(path) = cape_path.as_deref().filter(|p| !p.is_empty()) {
        preview.cape = Some(format!("data:image/png;base64,{}", read_texture(path)?));
    }
    Ok(preview)
}

/// `textures` object from a Yggdrasil-compatible session server (Drasl and Mojang share the shape).
async fn yggdrasil_textures(
    client: &reqwest::Client,
    profiles_base: &str,
    sessions_base: &str,
    username: &str,
) -> Option<Value> {
    let profile: Value = client
        .get(format!(
            "{profiles_base}/users/profiles/minecraft/{username}"
        ))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;
    let uuid = profile["id"].as_str()?;
    let session: Value = client
        .get(format!("{sessions_base}/session/minecraft/profile/{uuid}"))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;
    let encoded = session["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == "textures")?["value"]
        .as_str()?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let value: Value = serde_json::from_slice(&decoded).ok()?;
    value.get("textures").cloned()
}

async fn fetch_data_url(client: &reqwest::Client, url: Option<&str>) -> Option<String> {
    let bytes = client
        .get(url?)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .bytes()
        .await
        .ok()?;
    if bytes.len() as u64 > MAX_TEXTURE_BYTES || !bytes.starts_with(PNG_MAGIC) {
        return None;
    }
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    ))
}

fn read_texture(path: &str) -> Result<String, String> {
    let path = Path::new(path);
    let size = std::fs::metadata(path)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?
        .len();
    if size > MAX_TEXTURE_BYTES {
        return Err(format!("{} is larger than 1 MiB", path.display()));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if !bytes.starts_with(PNG_MAGIC) {
        return Err(format!("{} is not a PNG file", path.display()));
    }
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textures_must_be_small_pngs() {
        let dir = std::env::temp_dir().join(format!("blockfield-skins-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("skin.png");
        std::fs::write(&png, [PNG_MAGIC, b"data"].concat()).unwrap();
        assert!(read_texture(png.to_str().unwrap())
            .unwrap()
            .starts_with("iVBOR"));
        let txt = dir.join("skin.txt");
        std::fs::write(&txt, b"not a png").unwrap();
        assert!(read_texture(txt.to_str().unwrap()).is_err());
        assert!(read_texture(dir.join("missing.png").to_str().unwrap()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
