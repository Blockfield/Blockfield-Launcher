use crate::commands::LauncherAppState;
use std::sync::Mutex;
use tauri::{Emitter, Manager, State};

#[derive(Default)]
pub struct PendingRoom(pub Mutex<Option<String>>);

pub fn valid_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit() && !b.is_ascii_uppercase()
            }
        })
}

pub fn valid_target(target: &str) -> bool {
    target == "lobby" || valid_id(target)
}

pub fn parse_secret(secret: &str) -> Option<String> {
    let target = secret.strip_prefix("bf1:")?;
    valid_target(target).then(|| target.to_owned())
}

pub fn parse_link(link: &str) -> Option<String> {
    if let Some(target) = link.strip_prefix("blockfield://join/") {
        return valid_target(target).then(|| target.to_owned());
    }
    let url = tauri::Url::parse(link).ok()?;
    if url.scheme() != format!("discord-{}", crate::presence::application_id())
        || url.host_str() != Some("join")
        || !matches!(url.path(), "" | "/")
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return None;
    }
    let params: Vec<_> = url.query_pairs().collect();
    if params.len() != 1 || params[0].0 != "secret" {
        return None;
    }
    parse_secret(&params[0].1)
}

pub fn receive(app: &tauri::AppHandle, link: &str) {
    if let Some(target) = parse_link(link) {
        receive_target(app, &target);
    }
}

pub fn receive_target(app: &tauri::AppHandle, target: &str) {
    if !valid_target(target) {
        return;
    }
    *app.state::<PendingRoom>().0.lock().unwrap() = Some(target.to_owned());
    crate::game::show_launcher(app);
    let _ = app.emit("room://pending", ());
}

#[tauri::command]
pub fn pending_room(pending: State<'_, PendingRoom>) -> Option<String> {
    pending.0.lock().unwrap().clone()
}

#[tauri::command]
pub fn select_room(
    id: Option<String>,
    pending: State<'_, PendingRoom>,
    state: State<'_, LauncherAppState>,
) -> Result<(), String> {
    if id.is_some()
        && state.config.blocking_read().active_profile != crate::config::ClientProfile::Game
    {
        return Err("Для приглашения выберите профиль «Игра».".into());
    }
    if id.as_ref().is_some_and(|id| !valid_target(id)) {
        return Err("Некорректное приглашение".into());
    }
    *pending.0.lock().unwrap() = id;
    Ok(())
}

pub fn write_request(state: &LauncherAppState, id: &str) -> Result<(), String> {
    if !valid_target(id) {
        return Err("Некорректное приглашение".into());
    }
    std::fs::create_dir_all(&state.app_data_dir).map_err(|e| e.to_string())?;
    let request = serde_json::json!({"room": id, "createdAt": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()});
    let path = state.app_data_dir.join("room-request.json");
    let temporary = state.app_data_dir.join(format!(
        "room-request-{}.tmp",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::write(&temporary, request.to_string()).map_err(|e| e.to_string())?;
    std::fs::rename(&temporary, path).map_err(|e| {
        let _ = std::fs::remove_file(&temporary);
        e.to_string()
    })
}

#[tauri::command]
pub fn join_running_room(id: String, state: State<'_, LauncherAppState>) -> Result<(), String> {
    if state.config.blocking_read().active_profile != crate::config::ClientProfile::Game {
        return Err("Приглашения в матчи доступны в профиле «Игра».".into());
    }
    if state.game.snapshot().phase != crate::game::GamePhase::Running {
        return Err("Дождитесь запуска игры".into());
    }
    write_request(&state, &id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discord_links_decode_only_the_invite_secret() {
        assert_eq!(parse_link("blockfield://join/lobby"), Some("lobby".into()));
        let prefix = format!("discord-{}://join?", crate::presence::application_id());
        assert_eq!(
            parse_link(&format!("{prefix}secret=bf1%3Alobby")),
            Some("lobby".into())
        );
        for query in [
            "secret=bf1:lobby&host=evil",
            "secret=bf1:lobby&secret=bf1:lobby",
            "secret=bf2:lobby",
            "secret=bf1:lobby#evil",
            "secret=bf1:../../file",
        ] {
            assert_eq!(parse_link(&format!("{prefix}{query}")), None);
        }
    }

    #[test]
    fn links_accept_only_room_ids() {
        let id = "12345678-1234-1234-1234-123456789abc";
        assert_eq!(
            parse_link(&format!("blockfield://join/{id}")),
            Some(id.into())
        );
        for suffix in ["?host=evil", "/", "#fragment", "%20", "\n"] {
            assert!(parse_link(&format!("blockfield://join/{id}{suffix}")).is_none());
        }
        for link in [
            "https://join/1",
            "blockfield://join/1",
            "blockfield://join/../../file",
        ] {
            assert!(parse_link(link).is_none());
        }
    }
}
