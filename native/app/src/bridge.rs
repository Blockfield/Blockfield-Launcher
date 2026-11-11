//! The sole core-event path into Slint's UI thread.
use std::sync::Arc;

use launcher_app::{EventSink, LauncherEvent};
use launcher_core::game::GamePhase;
use launcher_net::download::ProgressUnit;
use slint::ComponentHandle;

use crate::AppWindow;

pub fn sink(app: &AppWindow) -> EventSink {
    let weak = app.as_weak();
    Arc::new(move |event| {
        if let Err(error) = weak.upgrade_in_event_loop(move |app| apply(&app, event)) {
            eprintln!("Cannot deliver launcher event: {error}");
        }
    })
}

pub fn apply(app: &AppWindow, event: LauncherEvent) {
    let mut data = app.get_main_data();
    match event {
        LauncherEvent::Status(status) => data.launch_status = status.message.into(),
        LauncherEvent::GameStatus(status) => {
            data.phase = match status.phase {
                GamePhase::Idle => "idle",
                GamePhase::Launching => "launching",
                GamePhase::Running => "running",
                GamePhase::Finishing => "finishing",
            }
            .into();
            data.launching = status.phase == GamePhase::Launching;
            if status.phase == GamePhase::Idle {
                data.launch_status = "".into();
                data.progress = -1;
            }
        }
        LauncherEvent::DownloadProgress(progress) => {
            let (done, total) = match progress.unit {
                ProgressUnit::Bytes => (progress.total_bytes_downloaded, progress.total_bytes_all),
                ProgressUnit::Files => (progress.file_index as u64, progress.file_count as u64),
            };
            data.progress = percent(done, total);
            if !progress.file_path.is_empty() {
                data.launch_status = progress.file_path.into();
            }
        }
        LauncherEvent::RoomPending => {
            data.pending_room = true;
            // Tauri-style pending-invite auto-join (MainScreen.tsx effect):
            // join now when the main screen is in a state where a manual
            // join would proceed; otherwise leave the flag for the banner.
            if let Some(id) = auto_join_target(app) {
                app.invoke_join(id);
            }
        }
        LauncherEvent::AccountExpired => {
            // Tauri answers `account://expired` with the login screen; the
            // stored username stays so the user can sign straight back in.
            crate::auth::show_login_expired(app);
        }
        event => {
            eprintln!("Launcher event has no native UI target yet: {event:?}");
            return;
        }
    }
    app.set_main_data(data);
}

/// Tauri `MainScreen.tsx` auto-join guards: game profile, no maintenance
/// block (the lobby bypasses maintenance), and either a running game or a
/// completed pack check with nothing already launching.
fn auto_join_target(app: &AppWindow) -> Option<slint::SharedString> {
    let id = crate::main_ops::shared_pending()
        .0
        .lock()
        .unwrap()
        .clone()?;
    if app.get_workshop() {
        return None;
    }
    let data = app.get_main_data();
    if id != "lobby" && !data.maintenance.is_empty() {
        return None;
    }
    if data.launching {
        return None;
    }
    match data.phase.as_str() {
        "running" => Some(id.into()),
        "idle" if data.checked && !data.checking => Some(id.into()),
        _ => None,
    }
}

fn percent(done: u64, total: u64) -> i32 {
    if total == 0 {
        -1
    } else {
        ((done.min(total) as u128 * 100) / total as u128) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    #[test]
    fn pending_invite_auto_joins_when_main_is_ready() {
        i_slint_backend_testing::init_no_event_loop();
        let app = crate::AppWindow::new().unwrap();
        let joined = Rc::new(RefCell::new(Vec::<String>::new()));
        let recorded = joined.clone();
        app.on_join(move |id| recorded.borrow_mut().push(id.to_string()));
        let set_pending = |id: Option<&str>| {
            *crate::main_ops::shared_pending().0.lock().unwrap() = id.map(str::to_owned);
        };
        let mut data = app.get_main_data();
        data.phase = "idle".into();
        data.checked = true;
        app.set_main_data(data);
        // Ready idle main: the invite joins at once, flag stays for the banner.
        set_pending(Some("lobby"));
        apply(&app, LauncherEvent::RoomPending);
        assert!(app.get_main_data().pending_room);
        assert_eq!(*joined.borrow(), ["lobby"]);
        // Workshop profile waits: the invite stays pending until the user
        // switches back to the game profile.
        app.set_workshop(true);
        set_pending(Some("room-1"));
        apply(&app, LauncherEvent::RoomPending);
        assert_eq!(joined.borrow().len(), 1);
        app.set_workshop(false);
        // Maintenance blocks room invites but not the lobby.
        let mut data = app.get_main_data();
        data.maintenance = "Техработы".into();
        app.set_main_data(data);
        apply(&app, LauncherEvent::RoomPending);
        assert_eq!(joined.borrow().len(), 1);
        set_pending(Some("lobby"));
        apply(&app, LauncherEvent::RoomPending);
        assert_eq!(joined.borrow().len(), 2);
        // Launching, an unfinished check and a busy phase wait as well.
        let mut data = app.get_main_data();
        data.maintenance = "".into();
        data.launching = true;
        app.set_main_data(data);
        set_pending(Some("room-1"));
        apply(&app, LauncherEvent::RoomPending);
        assert_eq!(joined.borrow().len(), 2);
        let mut data = app.get_main_data();
        data.launching = false;
        data.checking = true;
        app.set_main_data(data);
        apply(&app, LauncherEvent::RoomPending);
        assert_eq!(joined.borrow().len(), 2);
        let mut data = app.get_main_data();
        data.checking = false;
        data.phase = "finishing".into();
        app.set_main_data(data);
        apply(&app, LauncherEvent::RoomPending);
        assert_eq!(joined.borrow().len(), 2);
        // A running game takes the join-running path with the same callback.
        let mut data = app.get_main_data();
        data.phase = "running".into();
        app.set_main_data(data);
        apply(&app, LauncherEvent::RoomPending);
        assert_eq!(*joined.borrow(), ["lobby", "lobby", "room-1"]);
        // No stored target: only the flag, no join.
        set_pending(None);
        apply(&app, LauncherEvent::RoomPending);
        assert_eq!(joined.borrow().len(), 3);
        set_pending(None);
    }
    #[test]
    fn progress_handles_unknown_totals_overflow_and_completion() {
        assert_eq!(super::percent(0, 0), -1);
        assert_eq!(super::percent(1, 3), 33);
        assert_eq!(super::percent(4, 3), 100);
        assert_eq!(super::percent(u64::MAX, u64::MAX), 100);
    }
}
