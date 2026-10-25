//! Room deep-link bridge, moved unchanged from `src-tauri/src/rooms.rs`
//! (phase 2, no behaviour change). Pure bodies over [`LauncherAppState`] and
//! [`PendingRoom`]; the Tauri adapter keeps the `#[tauri::command]` wrappers,
//! the deep-link entry points and the window showing. Storing a target emits
//! [`LauncherEvent::RoomPending`], which each shell maps back to today's exact
//! `room://pending` event (the Tauri shell also restores its window there).

use crate::{LauncherAppState, LauncherEvent};
use launcher_core::config::ClientProfile;
use launcher_core::game::GamePhase;
pub use launcher_core::rooms::{parse_link, parse_secret, valid_target};
use std::sync::Mutex;

#[derive(Default)]
pub struct PendingRoom(pub Mutex<Option<String>>);

/// Parses a deep link with the Discord application id; on success stores the
/// target and emits exactly one [`LauncherEvent::RoomPending`].
pub fn receive(
    state: &LauncherAppState,
    pending: &PendingRoom,
    link: &str,
    application_id: &str,
) -> Option<String> {
    let target = parse_link(link, application_id)?;
    receive_target(state, pending, &target);
    Some(target)
}

pub fn receive_target(state: &LauncherAppState, pending: &PendingRoom, target: &str) {
    if !valid_target(target) {
        return;
    }
    *pending.0.lock().unwrap() = Some(target.to_owned());
    (state.events)(LauncherEvent::RoomPending);
}

pub fn pending_room(pending: &PendingRoom) -> Option<String> {
    pending.0.lock().unwrap().clone()
}

pub fn select_room(
    state: &LauncherAppState,
    pending: &PendingRoom,
    id: Option<String>,
) -> Result<(), String> {
    if id.is_some() && state.config.blocking_read().active_profile != ClientProfile::Game {
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

pub fn join_running_room(state: &LauncherAppState, id: String) -> Result<(), String> {
    if state.config.blocking_read().active_profile != ClientProfile::Game {
        return Err("Приглашения в матчи доступны в профиле «Игра».".into());
    }
    if state.game.snapshot().phase != GamePhase::Running {
        return Err("Дождитесь запуска игры".into());
    }
    write_request(state, &id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_state;
    use crate::EventSink;
    use std::sync::{Arc, Mutex as StdMutex};

    fn recording_state() -> (LauncherAppState, Arc<StdMutex<Vec<LauncherEvent>>>) {
        let received = Arc::new(StdMutex::new(Vec::new()));
        let sink: EventSink = Arc::new({
            let received = received.clone();
            move |event| received.lock().unwrap().push(event)
        });
        let mut state = test_state(
            std::env::temp_dir().join("blockfield-rooms-test"),
            launcher_core::config::LauncherConfig::default(),
        );
        state.events = sink;
        (state, received)
    }

    #[test]
    fn receive_target_emits_exactly_one_room_pending_and_stores_the_target() {
        let (state, received) = recording_state();
        let pending = PendingRoom::default();
        receive_target(&state, &pending, "lobby");
        assert_eq!(pending.0.lock().unwrap().as_deref(), Some("lobby"));
        {
            let received = received.lock().unwrap();
            assert_eq!(received.len(), 1);
            assert!(matches!(received[0], LauncherEvent::RoomPending));
        }
        // An invalid target is ignored: nothing stored, nothing emitted.
        receive_target(&state, &pending, "../../evil");
        assert_eq!(pending.0.lock().unwrap().as_deref(), Some("lobby"));
        assert_eq!(received.lock().unwrap().len(), 1);
    }

    #[test]
    fn receive_parses_only_known_links_and_keeps_invalid_targets_untouched() {
        let (state, received) = recording_state();
        let pending = PendingRoom::default();
        let app_id = "1548300877125779456";
        assert_eq!(
            receive(&state, &pending, "blockfield://join/lobby", app_id),
            Some("lobby".to_owned())
        );
        assert_eq!(received.lock().unwrap().len(), 1);
        assert_eq!(
            receive(&state, &pending, "blockfield://join/../../evil", app_id),
            None
        );
        assert_eq!(pending.0.lock().unwrap().as_deref(), Some("lobby"));
        assert_eq!(received.lock().unwrap().len(), 1);
    }
}
