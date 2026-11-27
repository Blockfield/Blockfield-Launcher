//! Drasl accounts. The game runs with a real Yggdrasil session (the client mod joins through
//! Drasl's session server); the API v3 token is used for skin uploads and password changes.
use crate::commands::{valid_username, GameIdentity, LauncherAppState};
use crate::config::LauncherConfig;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};

/// Prefix of errors the UI answers with the login screen.
pub const SESSION_EXPIRED: &str = "SESSION_EXPIRED";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    logged_in: bool,
    username: String,
    uuid: String,
    /// Signed in with the old launcher-generated password; the player must choose their own.
    needs_password: bool,
}

impl From<&LauncherConfig> for AccountStatus {
    fn from(config: &LauncherConfig) -> Self {
        let logged_in = !config.access_token.is_empty();
        Self {
            logged_in,
            username: config.username.clone(),
            uuid: config.player_uuid.clone(),
            needs_password: logged_in
                && !config.skin_password.is_empty()
                && legacy_username(config) == config.username,
        }
    }
}

#[derive(Debug)]
struct Account {
    username: String,
    uuid: String,
    access_token: String,
    client_token: String,
    api_token: String,
}

impl Account {
    fn apply(self, config: &mut LauncherConfig) {
        config.username = self.username;
        config.player_uuid = self.uuid;
        config.access_token = self.access_token;
        config.client_token = self.client_token;
        config.api_token = self.api_token;
    }
}

fn legacy_username(config: &LauncherConfig) -> &str {
    if config.skin_username.is_empty() {
        &config.username
    } else {
        &config.skin_username
    }
}

/// Signs in `account`; the legacy password is dropped only once its own account has signed in.
fn replace_account(config: &mut LauncherConfig, account: Account) {
    if legacy_username(config) == account.username {
        config.skin_password.clear();
        config.skin_username.clear();
    } else if !config.skin_password.is_empty() && config.skin_username.is_empty() {
        config.skin_username.clone_from(&config.username);
    }
    account.apply(config);
}

fn needs_migration(config: &LauncherConfig) -> bool {
    !config.skin_password.is_empty()
        && valid_username(legacy_username(config).trim())
        && config.access_token.is_empty()
}

/// Only an explicit Yggdrasil rejection ends the session; outages and rate limits keep the tokens.
fn session_rejected(status: u16) -> bool {
    matches!(status, 401 | 403)
}

#[tauri::command]
pub async fn account_status(state: State<'_, LauncherAppState>) -> Result<AccountStatus, String> {
    let config = state.config.read().await.clone();
    if needs_migration(&config) {
        let migrated = async {
            let base = drasl_base(&state).await?;
            sign_in(
                &base,
                legacy_username(&config).trim(),
                &config.skin_password,
            )
            .await
        };
        match migrated.await {
            Ok(account) => return signed_in(&state, |c| account.apply(c)).await,
            Err(error) => log::warn!("Account migration failed: {error}"),
        }
    }
    crate::skins::copy_mojang_skin(&state).await;
    Ok(AccountStatus::from(&config))
}

#[tauri::command]
pub async fn login(
    state: State<'_, LauncherAppState>,
    username: String,
    password: String,
) -> Result<AccountStatus, String> {
    let username = checked_username(&username)?;
    let account = sign_in(&drasl_base(&state).await?, username, &password).await?;
    signed_in(&state, |c| replace_account(c, account)).await
}

#[tauri::command]
pub async fn register(
    state: State<'_, LauncherAppState>,
    username: String,
    password: String,
) -> Result<AccountStatus, String> {
    let username = checked_username(&username)?;
    let base = drasl_base(&state).await?;
    api(client()
        .post(format!("{base}/drasl/api/v3/users"))
        .json(&json!({
            "username": username,
            "password": password,
            "playerName": username,
            "requestApiToken": true,
        })))
    .await?;
    let account = sign_in(&base, username, &password).await?;
    signed_in(&state, |c| replace_account(c, account)).await
}

#[tauri::command]
pub async fn logout(state: State<'_, LauncherAppState>) -> Result<AccountStatus, String> {
    let config = state.config.read().await.clone();
    if let (false, Ok(base)) = (config.access_token.is_empty(), drasl_base(&state).await) {
        let _ = client()
            .post(format!("{base}/auth/invalidate"))
            .json(&tokens(&config))
            .send()
            .await;
    }
    update(&state, |c| {
        c.player_uuid.clear();
        c.access_token.clear();
        c.client_token.clear();
        c.api_token.clear();
        c.skin_password.clear();
        c.skin_username.clear();
    })
    .await
}

#[tauri::command]
pub async fn change_password(
    state: State<'_, LauncherAppState>,
    password: String,
) -> Result<AccountStatus, String> {
    let config = state.config.read().await.clone();
    let base = drasl_base(&state).await?;
    api(client()
        .patch(format!("{base}/drasl/api/v3/user"))
        .bearer_auth(&config.api_token)
        .json(&json!({ "password": password })))
    .await?;
    // Drasl invalidates every Yggdrasil session of the user when the password changes.
    let account = sign_in(&base, &config.username, &password).await?;
    update(&state, |c| replace_account(c, account)).await
}

/// Validates the stored session, refreshing it when needed. A rejected session emits
/// `account://expired` and fails with [`SESSION_EXPIRED`]; network failures and server outages
/// stay plain errors.
pub async fn game_identity(
    app: &AppHandle,
    state: &LauncherAppState,
) -> Result<GameIdentity, String> {
    let config = state.config.read().await.clone();
    if !config.access_token.is_empty() {
        let base = drasl_base(state).await?;
        let identity = |access_token| GameIdentity {
            username: config.username.clone(),
            uuid: config.player_uuid.clone(),
            access_token,
        };
        let validate = send(
            client()
                .post(format!("{base}/auth/validate"))
                .json(&tokens(&config)),
        )
        .await?;
        if validate.status().is_success() {
            return Ok(identity(config.access_token.clone()));
        }
        let refresh = send(
            client()
                .post(format!("{base}/auth/refresh"))
                .json(&tokens(&config)),
        )
        .await?;
        let status = refresh.status();
        if !session_rejected(status.as_u16()) {
            let body: Value = refresh.json().await.unwrap_or(Value::Null);
            let Some(token) = body["accessToken"]
                .as_str()
                .filter(|_| status.is_success())
                .map(String::from)
            else {
                return Err(format!("Drasl account server unavailable (HTTP {status})"));
            };
            // A logout or another sign-in may have replaced the session while refreshing.
            let mut unchanged = false;
            update(state, |c| {
                unchanged =
                    c.client_token == config.client_token && c.player_uuid == config.player_uuid;
                if unchanged {
                    c.access_token.clone_from(&token);
                }
            })
            .await?;
            if !unchanged {
                return Err("Account changed while starting the game".to_string());
            }
            return Ok(identity(token));
        }
    }
    let _ = app.emit("account://expired", ());
    Err(format!("{SESSION_EXPIRED}: log in again"))
}

/// Passwords and tokens go only over https (plain http to localhost in debug builds).
pub async fn drasl_base(state: &LauncherAppState) -> Result<String, String> {
    let base = state
        .pack
        .read()
        .await
        .as_ref()
        .and_then(|p| p.info.skins.clone())
        .unwrap_or_else(|| crate::skins::DEFAULT_SERVER.to_string());
    let dev = ["http://127.0.0.1", "http://localhost"]
        .iter()
        .any(|prefix| base.starts_with(prefix));
    if !base.starts_with("https://") && !(cfg!(debug_assertions) && dev) {
        return Err("Drasl server URL must use https".to_string());
    }
    Ok(base)
}

pub async fn api(request: reqwest::RequestBuilder) -> Result<Value, String> {
    let response = send(request).await?;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        let message = body["message"]
            .as_str()
            .or(body["errorMessage"].as_str())
            .unwrap_or(status.as_str());
        return Err(format!("Drasl: {message}"));
    }
    Ok(body)
}

async fn send(request: reqwest::RequestBuilder) -> Result<reqwest::Response, String> {
    request
        .send()
        .await
        .map_err(|e| format!("Drasl request failed: {e}"))
}

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("valid HTTP client")
}

fn checked_username(username: &str) -> Result<&str, String> {
    let username = username.trim();
    if !valid_username(username) {
        return Err("Username must be 3-16 characters: letters, digits or _".to_string());
    }
    Ok(username)
}

fn tokens(config: &LauncherConfig) -> Value {
    json!({ "accessToken": config.access_token, "clientToken": config.client_token })
}

async fn sign_in(base: &str, username: &str, password: &str) -> Result<Account, String> {
    let session = api(client()
        .post(format!("{base}/auth/authenticate"))
        .json(&json!({
            "agent": { "name": "Minecraft", "version": 1 },
            "username": username,
            "password": password,
            "requestUser": false,
        })))
    .await?;
    let drasl = api(client()
        .post(format!("{base}/drasl/api/v3/login"))
        .json(&json!({ "username": username, "password": password })))
    .await?;
    account_from(&session, &drasl)
}

/// Joins the Yggdrasil session with the API v3 login; the player is the session's selected profile.
fn account_from(session: &Value, drasl: &Value) -> Result<Account, String> {
    let text = |value: &Value| value.as_str().filter(|s| !s.is_empty()).map(String::from);
    let profile_id = text(&session["selectedProfile"]["id"]).unwrap_or_default();
    let player = drasl["user"]["players"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| {
            !profile_id.is_empty()
                && p["uuid"]
                    .as_str()
                    .is_some_and(|uuid| uuid.replace('-', "") == profile_id)
        });
    match (
        player,
        text(&session["selectedProfile"]["name"]),
        text(&session["accessToken"]),
        text(&session["clientToken"]),
        text(&drasl["apiToken"]),
    ) {
        (Some(player), Some(username), Some(access_token), Some(client_token), Some(api_token)) => {
            Ok(Account {
                username,
                uuid: text(&player["uuid"]).unwrap_or_default(),
                access_token,
                client_token,
                api_token,
            })
        }
        _ => Err("Drasl returned no Minecraft player for this account".to_string()),
    }
}

async fn signed_in(
    state: &LauncherAppState,
    change: impl FnOnce(&mut LauncherConfig),
) -> Result<AccountStatus, String> {
    let status = update(state, change).await?;
    crate::skins::copy_mojang_skin(state).await;
    Ok(status)
}

async fn update(
    state: &LauncherAppState,
    change: impl FnOnce(&mut LauncherConfig),
) -> Result<AccountStatus, String> {
    let mut current = state.config.write().await;
    let mut updated = current.clone();
    change(&mut updated);
    crate::config::save_config(&state.app_data_dir, &updated)?;
    *current = updated;
    Ok(AccountStatus::from(&*current))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn http_decodes_gzip_streams_bytes_and_preserves_server_errors() {
        use flate2::{write::GzEncoder, Compression};
        use futures_util::StreamExt;
        use std::io::Write;
        use std::time::Duration;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
        gzip.write_all(br#"{"accessToken":"token"}"#).unwrap();
        let responses = [
            (
                "200 OK",
                "Content-Encoding: gzip\r\n",
                gzip.finish().unwrap(),
            ),
            (
                "401 Unauthorized",
                "",
                br#"{"errorMessage":"Invalid credentials"}"#.to_vec(),
            ),
            ("503 Service Unavailable", "", b"maintenance".to_vec()),
            ("200 OK", "", b"streamed binary\0\xff".to_vec()),
        ];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/test", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            tokio::time::timeout(Duration::from_secs(5), async {
                for (status, headers, body) in responses {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let mut request = Vec::new();
                    while !request.ends_with(b"\r\n\r\n") {
                        let mut chunk = [0; 4096];
                        let read = socket.read(&mut chunk).await.unwrap();
                        assert!(read > 0);
                        request.extend_from_slice(&chunk[..read]);
                    }
                    assert!(request.starts_with(b"GET /test?player=Alex+Smith HTTP/1.1\r\n"));
                    let head = format!(
                        "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    socket.write_all(head.as_bytes()).await.unwrap();
                    socket.write_all(&body).await.unwrap();
                }
            })
            .await
            .unwrap();
        });
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let request = || client.get(&url).query(&[("player", "Alex Smith")]);
        assert_eq!(api(request()).await.unwrap()["accessToken"], "token");
        assert_eq!(
            api(request()).await.unwrap_err(),
            "Drasl: Invalid credentials"
        );
        assert_eq!(api(request()).await.unwrap_err(), "Drasl: 503");
        let mut stream = send(request()).await.unwrap().bytes_stream();
        let mut downloaded = Vec::new();
        while let Some(chunk) = stream.next().await {
            downloaded.extend_from_slice(&chunk.unwrap());
        }
        assert_eq!(downloaded, b"streamed binary\0\xff");
        server.await.unwrap();
    }

    #[test]
    fn migrates_legacy_password_and_parses_drasl_session() {
        let mut config = LauncherConfig {
            username: "Steve".into(),
            skin_password: "legacy".into(),
            ..LauncherConfig::default()
        };
        assert!(needs_migration(&config));
        config.access_token = "token".into();
        assert!(!needs_migration(&config));
        assert!(AccountStatus::from(&config).needs_password);

        let session = json!({
            "accessToken": "access",
            "clientToken": "client",
            "selectedProfile": { "id": "5627dd98e6be3c21b8a8e92344183641", "name": "Steve" },
        });
        let drasl = json!({
            "apiToken": "api",
            "user": { "players": [
                { "uuid": "00000000-0000-0000-0000-000000000000", "name": "Other" },
                { "uuid": "5627dd98-e6be-3c21-b8a8-e92344183641", "name": "Steve" },
            ] },
        });
        let account = account_from(&session, &drasl).unwrap();
        assert_eq!(account.uuid, "5627dd98-e6be-3c21-b8a8-e92344183641");
        assert_eq!(
            (account.username.as_str(), account.api_token.as_str()),
            ("Steve", "api")
        );
        assert!(account_from(&json!({ "accessToken": "access" }), &drasl).is_err());
    }

    #[test]
    fn only_rejected_refresh_expires_and_legacy_password_follows_its_account() {
        assert!(session_rejected(401) && session_rejected(403));
        assert!(![429, 500, 502, 503, 522].into_iter().any(session_rejected));

        let account = |username: &str| Account {
            username: username.into(),
            uuid: "uuid".into(),
            access_token: "access".into(),
            client_token: "client".into(),
            api_token: "api".into(),
        };
        let mut config = LauncherConfig {
            username: "Steve".into(),
            skin_password: "legacy".into(),
            ..LauncherConfig::default()
        };
        replace_account(&mut config, account("Alex"));
        assert_eq!(config.skin_password, "legacy");
        assert!(!AccountStatus::from(&config).needs_password);
        replace_account(&mut config, account("steve"));
        assert_eq!(config.skin_password, "legacy");
        replace_account(&mut config, account("Steve"));
        assert!(config.skin_password.is_empty() && config.skin_username.is_empty());
    }
}
