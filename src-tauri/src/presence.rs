use crate::{discord_ipc::Connection, game::GamePhase, rooms};
use serde_json::{json, Value};
use std::{
    io,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;
use tokio::time::Instant;

pub const APPLICATION_ID: &str = "1548300877125779456";

pub fn application_id() -> String {
    std::env::var("BLOCKFIELD_DISCORD_APPLICATION_ID")
        .ok()
        .filter(|s| valid_application_id(s))
        .or_else(|| {
            option_env!("BLOCKFIELD_DISCORD_APPLICATION_ID")
                .filter(|s| valid_application_id(s))
                .map(str::to_owned)
        })
        .unwrap_or_else(|| APPLICATION_ID.into())
}

fn valid_application_id(id: &str) -> bool {
    (17..=20).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit())
}

pub fn start(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            if app
                .state::<crate::commands::LauncherAppState>()
                .config
                .read()
                .await
                .discord_presence
            {
                if let Err(error) = session(&app).await {
                    log::debug!("Discord presence disconnected: {error}");
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

async fn session(app: &tauri::AppHandle) -> io::Result<()> {
    let mut connection = Connection::connect(&application_id()).await?;
    connection
        .command("SUBSCRIBE", json!({}), Some("ACTIVITY_JOIN"))
        .await?;
    let mut timer = tokio::time::interval(Duration::from_secs(1));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_activity = Value::Null;
    let mut last_sent = Instant::now() - Duration::from_secs(10);
    let mut last_reply = Instant::now();
    let mut started = 0;
    let mut native_invites = false;
    loop {
        tokio::select! {
            frame = connection.next() => {
                let (op, value) = frame?;
                last_reply = Instant::now();
                match op {
                    3 => connection.send(4, &value).await?,
                    2 => return Err(io::ErrorKind::ConnectionAborted.into()),
                    1 if value["cmd"] == "SUBSCRIBE" && value["evt"] == "ACTIVITY_JOIN" => {
                        native_invites = true;
                    }
                    1 if value["evt"] == "ERROR" && value["cmd"] == "SUBSCRIBE" => {
                        log::info!("Discord client does not support native invitations; using room links");
                    }
                    1 if value["evt"] == "ERROR" => {
                        log::warn!("Discord rejected {} (code {})", value["cmd"], value["data"]["code"]);
                        return Err(io::Error::other("Discord rejected RPC command"));
                    }
                    1 => {
                        if let Some(target) = join_target(&value) {
                            if app.state::<crate::commands::LauncherAppState>().config.read().await.discord_presence {
                                rooms::receive_target(app, &target);
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ = timer.tick() => {
                let state = app.state::<crate::commands::LauncherAppState>();
                if !state.config.read().await.discord_presence {
                    connection.command("SET_ACTIVITY", json!({"pid":std::process::id(),"activity":null}), None).await?;
                    return Ok(());
                }
                if last_reply.elapsed() > Duration::from_secs(30) {
                    return Err(io::ErrorKind::TimedOut.into());
                }
                let phase = state.game.snapshot().phase;
                if phase == GamePhase::Running && started == 0 { started = now(); }
                if phase == GamePhase::Idle { started = 0; }
                let data = if phase == GamePhase::Running {
                    read_presence(&state.app_data_dir.join("game-presence.json"))
                } else { None };
                let activity = activity(phase, data.as_ref(), started, native_invites);
                if (activity != last_activity && last_sent.elapsed() >= Duration::from_secs(2)) || last_sent.elapsed() >= Duration::from_secs(10) {
                    connection.command("SET_ACTIVITY", json!({"pid":std::process::id(),"activity":activity}), None).await?;
                    last_activity = activity;
                    last_sent = Instant::now();
                }
            }
        }
    }
}

fn read_presence(path: &std::path::Path) -> Option<Value> {
    if std::fs::metadata(path).ok()?.len() > 16_384 {
        return None;
    }
    let data: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let timestamp = data["updatedAt"].as_i64()?;
    ((now() * 1000).abs_diff(timestamp) < 15_000).then_some(data)
}

fn join_target(value: &Value) -> Option<String> {
    if value["cmd"] != "DISPATCH" || value["evt"] != "ACTIVITY_JOIN" {
        return None;
    }
    rooms::parse_secret(value["data"]["secret"].as_str()?)
}

fn activity(phase: GamePhase, data: Option<&Value>, started: i64, native_invites: bool) -> Value {
    let fallback = match phase {
        GamePhase::Running => "В игре",
        GamePhase::Launching => "Запускает игру",
        GamePhase::Finishing => "Завершает игру",
        GamePhase::Idle => "В лаунчере",
    };
    let empty = Value::Null;
    let data = data.unwrap_or(&empty);
    let details = data["state"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(fallback);
    let mut result = json!({"type":0,"details":short(details),"instance":true,"assets":{"large_image":"blockfield","large_text":"Blockfield"}});
    let name = data["name"].as_str().unwrap_or("");
    let map = data["map"].as_str().unwrap_or("");
    let state = [name, map]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    if !state.is_empty() {
        result["state"] = short(&state).into();
    }
    if started > 0 {
        result["timestamps"] = json!({"start":started});
    }
    let target = data["id"].as_str().filter(|id| rooms::valid_target(id));
    let joinable = native_invites && phase == GamePhase::Running && data["joinable"] == true;
    if let (true, Some(target), Some(players), Some(capacity)) = (
        joinable,
        target,
        data["players"].as_u64(),
        data["capacity"].as_u64(),
    ) {
        if players > 0 && capacity > players && capacity <= 100_000 {
            result["party"] =
                json!({"id":format!("blockfield:{target}"),"size":[players,capacity],"privacy":1});
            result["secrets"] = json!({"join":format!("bf1:{target}")});
        }
    }
    // Discord custom buttons replace the native Join action in some clients.
    if result.get("secrets").is_none() {
        let url = target
            .map(|target| format!("https://modpack.nether.pp.ua/?room={target}#rooms"))
            .unwrap_or_else(|| "https://modpack.nether.pp.ua/#rooms".into());
        let label = if target == Some("lobby") {
            "Войти в лобби"
        } else if target.is_some() {
            "Войти в комнату"
        } else {
            "Игровые комнаты"
        };
        result["buttons"] = json!([{"label":label,"url":url}]);
    }
    result
}

fn short(value: &str) -> String {
    let mut text = String::new();
    for c in value.chars().filter(|c| !c.is_control()) {
        if text.len() + c.len_utf8() > 120 {
            break;
        }
        text.push(c);
    }
    text
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lobby_and_rooms_have_native_invites_only_while_joinable() {
        for target in ["lobby", "12345678-1234-1234-1234-123456789abc"] {
            let mut data = json!({"id":target,"state":"В лобби","name":"Лобби","players":2,"capacity":20,"joinable":true});
            let value = activity(GamePhase::Running, Some(&data), 1, true);
            assert_eq!(value["secrets"]["join"], format!("bf1:{target}"));
            assert_eq!(value["party"]["size"], json!([2, 20]));
            assert!(value.get("buttons").is_none());
            data["capacity"] = 2.into();
            assert!(activity(GamePhase::Running, Some(&data), 1, true)
                .get("secrets")
                .is_none());
            data["capacity"] = 20.into();
            data["joinable"] = false.into();
            assert!(activity(GamePhase::Running, Some(&data), 1, true)
                .get("secrets")
                .is_none());
            assert!(activity(GamePhase::Idle, Some(&data), 0, true)
                .get("secrets")
                .is_none());
        }
        assert!(activity(GamePhase::Running, None, 1, true)
            .get("secrets")
            .is_none());
    }
    #[test]
    fn only_join_dispatches_with_supported_secrets_are_accepted() {
        let mut value =
            json!({"cmd":"DISPATCH","evt":"ACTIVITY_JOIN","data":{"secret":"bf1:lobby"}});
        assert_eq!(join_target(&value), Some("lobby".into()));
        value["cmd"] = "SUBSCRIBE".into();
        assert_eq!(join_target(&value), None);
        value["cmd"] = "DISPATCH".into();
        value["data"]["secret"] = "bf1:../../evil".into();
        assert_eq!(join_target(&value), None);
    }
}
