// Room bridge bodies moved to launcher-app (phase 2, no behaviour change);
// this adapter keeps the `#[tauri::command]` wrappers, the deep-link entry
// points and their exact error strings. Storing a target emits
// `LauncherEvent::RoomPending` through the shared state, which the setup sink
// maps back to today's exact `room://pending` event (restoring the window).
pub use launcher_app::rooms::PendingRoom;

use crate::commands::LauncherAppState;
use tauri::{Manager, State};

pub fn receive(app: &tauri::AppHandle, link: &str) {
    launcher_app::rooms::receive(
        &app.state::<LauncherAppState>(),
        &app.state::<PendingRoom>(),
        link,
        &crate::presence::application_id(),
    );
}

#[tauri::command]
pub fn pending_room(pending: State<'_, PendingRoom>) -> Option<String> {
    launcher_app::rooms::pending_room(&pending)
}

#[tauri::command]
pub fn select_room(
    id: Option<String>,
    pending: State<'_, PendingRoom>,
    state: State<'_, LauncherAppState>,
) -> Result<(), String> {
    launcher_app::rooms::select_room(&state, &pending, id)
}

pub fn write_request(state: &LauncherAppState, id: &str) -> Result<(), String> {
    launcher_app::rooms::write_request(state, id)
}

#[tauri::command]
pub fn join_running_room(id: String, state: State<'_, LauncherAppState>) -> Result<(), String> {
    launcher_app::rooms::join_running_room(&state, id)
}
