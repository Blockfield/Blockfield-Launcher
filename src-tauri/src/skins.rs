//! Skin and cape upload to the Drasl skin server. The launcher owns one Drasl account per
//! offline username (password kept in the launcher config); SkinRestorer on the game server
//! pulls the textures back by player name, falling back to the Mojang skin of that name.
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
    let username = config.username.trim().to_string();
    if !valid_username(&username) {
        return Err("Set a Minecraft username in Settings before uploading a skin".to_string());
    }
    let skin = skin_path.as_deref().map(read_texture).transpose()?;
    let cape = cape_path.as_deref().map(read_texture).transpose()?;
    if skin.is_none() && cape.is_none() {
        return Err("Choose a skin or cape PNG to upload".to_string());
    }

    let base = state
        .pack
        .read()
        .await
        .as_ref()
        .and_then(|p| p.info.skins.clone())
        .unwrap_or_else(|| DEFAULT_SERVER.to_string());
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    // Persist before registration so a failed response cannot strand an account with a lost password.
    let password = {
        let mut current = state.config.write().await;
        if current.skin_password.is_empty() {
            let mut updated = current.clone();
            updated.skin_password = random_password();
            crate::config::save_config(&state.app_data_dir, &updated)?;
            *current = updated;
        }
        current.skin_password.clone()
    };
    let session = match login(&client, &base, &username, &password).await {
        Ok(session) => session,
        // No account for this name yet (first upload, or the username changed since).
        Err(_) => register(&client, &base, &username, &password).await?,
    };

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
            session.player_uuid
        ))
        .bearer_auth(&session.token)
        .json(&body))
    .await?;
    Ok(SkinUploadResult {
        skin_url: player["skinUrl"].as_str().map(String::from),
        cape_url: player["capeUrl"].as_str().map(String::from),
    })
}

struct Session {
    token: String,
    player_uuid: String,
}

async fn login(
    client: &reqwest::Client,
    base: &str,
    username: &str,
    password: &str,
) -> Result<Session, String> {
    let response = api(client
        .post(format!("{base}/drasl/api/v3/login"))
        .json(&json!({ "username": username, "password": password })))
    .await?;
    session_from(response)
}

async fn register(
    client: &reqwest::Client,
    base: &str,
    username: &str,
    password: &str,
) -> Result<Session, String> {
    let response = api(client
        .post(format!("{base}/drasl/api/v3/users"))
        .json(&json!({
            "username": username,
            "password": password,
            "playerName": username,
            "requestApiToken": true,
        })))
    .await
    .map_err(|e| format!("Cannot register {username} on the skin server: {e}"))?;
    session_from(response)
}

fn session_from(response: Value) -> Result<Session, String> {
    let token = response["apiToken"].as_str();
    let player_uuid = response["user"]["players"][0]["uuid"].as_str();
    match (token, player_uuid) {
        (Some(token), Some(uuid)) => Ok(Session {
            token: token.to_string(),
            player_uuid: uuid.to_string(),
        }),
        _ => Err("Skin server returned no session".to_string()),
    }
}

async fn api(request: reqwest::RequestBuilder) -> Result<Value, String> {
    let response = request
        .send()
        .await
        .map_err(|e| format!("Skin server request failed: {e}"))?;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        let message = body["message"].as_str().unwrap_or(status.as_str());
        return Err(format!("Skin server: {message}"));
    }
    Ok(body)
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

fn random_password() -> String {
    rand::random::<[u8; 24]>()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
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

        assert!(
            session_from(json!({"apiToken": "t", "user": {"players": [{"uuid": "u"}]}}),).is_ok()
        );
        assert!(session_from(json!({"user": {}})).is_err());
        assert_eq!(random_password().len(), 48);
    }
}
