//! Skin and cape upload to the Drasl account of the signed-in player; SkinRestorer on the game
//! server pulls the textures back by player name, falling back to the Mojang skin of that name.
use crate::account::{api, client, drasl_base, SESSION_EXPIRED};
use crate::commands::{valid_username, LauncherAppState};
use base64::Engine;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Mutex;
use tauri::State;

pub const DEFAULT_SERVER: &str = "https://skins.blockfield.pro";
const MAX_TEXTURE_BYTES: u64 = 1024 * 1024;
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
const MOJANG_PROFILES: &str = "https://api.mojang.com";
const MOJANG_SESSIONS: &str = "https://sessionserver.mojang.com";
static MOJANG_SKIN_CHECKED: Mutex<Vec<String>> = Mutex::new(Vec::new());

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
            (MOJANG_PROFILES, MOJANG_SESSIONS, "mojang"),
        ];
        for (profiles, sessions, source) in sources {
            if let Some(textures) = yggdrasil_textures(&client, profiles, sessions, &username).await
            {
                let data_url = |png: String| format!("data:image/png;base64,{png}");
                preview.skin = fetch_png(&client, textures["SKIN"]["url"].as_str())
                    .await
                    .map(data_url);
                preview.cape = fetch_png(&client, textures["CAPE"]["url"].as_str())
                    .await
                    .map(data_url);
                preview.slim = skin_model(&textures) == "slim";
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

/// Drasl's Mojang skin forwarding can't reach Mojang from the homeserver, so the launcher copies
/// the Mojang skin of the same name into a player without a skin, once per player per run.
pub async fn copy_mojang_skin(state: &LauncherAppState) {
    let config = state.config.read().await.clone();
    let Ok(base) = drasl_base(state).await else {
        return;
    };
    if config.api_token.is_empty() || config.player_uuid.is_empty() {
        return;
    }
    {
        let mut checked = MOJANG_SKIN_CHECKED
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if checked.contains(&config.player_uuid) {
            return;
        }
        checked.push(config.player_uuid.clone());
    }
    tauri::async_runtime::spawn(async move {
        if let Err(error) = copy_mojang_textures(&base, &config).await {
            log::warn!("Mojang skin copy for {} failed: {error}", config.username);
        }
    });
}

async fn copy_mojang_textures(
    base: &str,
    config: &crate::config::LauncherConfig,
) -> Result<(), String> {
    let client = client();
    let user = api(client
        .get(format!("{base}/drasl/api/v3/user"))
        .bearer_auth(&config.api_token))
    .await?;
    if !lacks_skin(&user, &config.player_uuid) {
        return Ok(());
    }
    let Some(textures) =
        yggdrasil_textures(&client, MOJANG_PROFILES, MOJANG_SESSIONS, &config.username).await
    else {
        return Ok(());
    };
    let Some(skin_url) = textures["SKIN"]["url"].as_str() else {
        return Ok(());
    };
    let skin = fetch_png(&client, Some(skin_url))
        .await
        .ok_or("Mojang skin download failed")?;
    let mut body = json!({ "skinBase64": skin, "skinModel": skin_model(&textures) });
    if let Some(cape) = fetch_png(&client, textures["CAPE"]["url"].as_str()).await {
        body["capeBase64"] = Value::String(cape);
    }
    api(client
        .patch(format!(
            "{base}/drasl/api/v3/players/{}",
            config.player_uuid
        ))
        .bearer_auth(&config.api_token)
        .json(&body))
    .await?;
    Ok(())
}

fn lacks_skin(user: &Value, uuid: &str) -> bool {
    user["players"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|p| p["uuid"] == uuid && p["skinUrl"].as_str().is_none_or(str::is_empty))
}

fn skin_model(textures: &Value) -> &'static str {
    if textures["SKIN"]["metadata"]["model"] == "slim" {
        "slim"
    } else {
        "classic"
    }
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

async fn fetch_png(client: &reqwest::Client, url: Option<&str>) -> Option<String> {
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
    Some(base64::engine::general_purpose::STANDARD.encode(&bytes))
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

    #[test]
    fn copies_mojang_skin_only_into_empty_player() {
        let uuid = "5627dd98-e6be-3c21-b8a8-e92344183641";
        let user = |skin: Value| json!({ "players": [{ "uuid": uuid, "skinUrl": skin }] });
        assert!(lacks_skin(&user(Value::Null), uuid));
        assert!(!lacks_skin(&user(json!("https://skins/a.png")), uuid));
        assert!(!lacks_skin(&user(Value::Null), "other"));
        let model = |m: Value| skin_model(&json!({ "SKIN": { "metadata": { "model": m } } }));
        assert_eq!(model(json!("slim")), "slim");
        assert_eq!(model(Value::Null), "classic");
    }
}
