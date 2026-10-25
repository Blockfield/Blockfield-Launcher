// Pure parse/validate fns moved to launcher-core (phase 2, no behaviour change);
// Tauri state, commands and bridge-file writes stay in the adapter. Re-export
// keeps every `crate::rooms::…` call site (presence) unchanged. `valid_id` now
// lives in launcher-net via launcher-core (used by `crate::status`).
pub use launcher_core::rooms::{parse_link, parse_secret, valid_target};

use crate::commands::LauncherAppState;
use std::sync::Mutex;
use tauri::{Emitter, Manager, State};

#[derive(Default)]
pub struct PendingRoom(pub Mutex<Option<String>>);

pub fn receive(app: &tauri::AppHandle, link: &str) {
    if let Some(target) = parse_link(link, &crate::presence::application_id()) {
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
