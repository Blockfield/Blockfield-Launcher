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
    // Tauri MainScreen.tsx pending-room effect re-runs on every readiness
    // dep change; arm the one-shot retry below for the events that change
    // readiness on this thread (the rest retry at their own set_main_data).
    let mut retry = false;
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
            retry = true;
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
            // A new invite re-arms the one-shot (Tauri `handledRoom.current =
            // null` on `room://pending` in src/app/components/MainScreen.tsx).
            *handled().lock().unwrap() = None;
            retry = true;
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
    if retry {
        retry_pending_join(app);
    }
}

/// Tauri `MainScreen.tsx` pending-room effect
/// (`src/app/components/MainScreen.tsx`): re-run the auto-join whenever
/// readiness may have changed (invite arrival, pack check done, launch idle,
/// profile back to Game, maintenance or game phase change). One-shot per
/// invite like Tauri's `handledRoom` ref: an invite that already joined never
/// refires until the next RoomPending event, so a failed launch does not
/// redeploy on every server poll.
pub fn retry_pending_join(app: &AppWindow) {
    let Some(id) = auto_join_target(app) else {
        return;
    };
    // Decide under the lock, then drop it before invoke_join: the production
    // on_join (main_ops::attach) synchronously calls note_join on the same
    // thread, which would deadlock on the reentrant lock.
    {
        let mut handled = handled().lock().unwrap();
        if handled.as_deref() == Some(id.as_str()) {
            return;
        }
        *handled = Some(id.to_string());
    }
    app.invoke_join(id);
}

/// A join was attempted (manual or auto): consume the one-shot so later state
/// changes do not rejoin the same invite; the next RoomPending event re-arms.
pub(crate) fn note_join(id: &str) {
    *handled().lock().unwrap() = Some(id.to_owned());
}

fn handled() -> &'static std::sync::Mutex<Option<String>> {
    static HANDLED: std::sync::OnceLock<std::sync::Mutex<Option<String>>> =
        std::sync::OnceLock::new();
    HANDLED.get_or_init(|| std::sync::Mutex::new(None))
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
    // A profile switch rejects the join-triggered deploy in main_ops::attach,
    // so wait for the flags to clear (settings profile completion retries)
    // instead of consuming the one-shot. SettingsForm.blocked is the flag the
    // async switch in settings_tabs::on_profile actually sets.
    if data.profile_changing
        || data.profile_blocked
        || app.global::<crate::SettingsForm>().get_blocked()
    {
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

    fn set_pending(id: Option<&str>) {
        *crate::main_ops::shared_pending().0.lock().unwrap() = id.map(str::to_owned);
    }

    fn reset_invite_state() {
        set_pending(None);
        *super::handled().lock().unwrap() = None;
    }

    fn ready_idle(app: &AppWindow) {
        let mut data = app.get_main_data();
        data.phase = "idle".into();
        data.checked = true;
        data.checking = false;
        data.launching = false;
        data.maintenance = "".into();
        app.set_main_data(data);
        app.set_workshop(false);
    }

    #[test]
    fn pending_invite_auto_joins_when_main_is_ready() {
        let _serial = crate::main_ops::test_serial();
        i_slint_backend_testing::init_no_event_loop();
        let app = crate::AppWindow::new().unwrap();
        let joined = Rc::new(RefCell::new(Vec::<String>::new()));
        let recorded = joined.clone();
        app.on_join(move |id| recorded.borrow_mut().push(id.to_string()));
        let set_pending = |id: Option<&str>| {
            *crate::main_ops::shared_pending().0.lock().unwrap() = id.map(str::to_owned);
        };
        reset_invite_state();
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
        reset_invite_state();
    }

    #[test]
    fn pending_invite_retries_when_blocking_state_clears() {
        // Tauri MainScreen.tsx pending-room effect: an invite that arrives
        // while the launcher is not ready joins once the blocking state
        // clears, without a second RoomPending event.
        let _serial = crate::main_ops::test_serial();
        i_slint_backend_testing::init_no_event_loop();
        let app = crate::AppWindow::new().unwrap();
        let joined = Rc::new(RefCell::new(Vec::<String>::new()));
        let recorded = joined.clone();
        app.on_join(move |id| recorded.borrow_mut().push(id.to_string()));
        reset_invite_state();
        ready_idle(&app);
        // Invite arrives mid-check: the banner flag sets, no join yet.
        let mut data = app.get_main_data();
        data.checking = true;
        data.checked = false;
        app.set_main_data(data);
        set_pending(Some("room-7"));
        apply(&app, LauncherEvent::RoomPending);
        assert!(app.get_main_data().pending_room);
        assert!(joined.borrow().is_empty());
        // Pack check completes: the same invite joins with no new event.
        let mut data = app.get_main_data();
        data.checking = false;
        data.checked = true;
        app.set_main_data(data);
        super::retry_pending_join(&app);
        assert_eq!(*joined.borrow(), ["room-7"]);
        // One-shot per invite (Tauri `handledRoom`): further state changes
        // do not rejoin the same invite.
        super::retry_pending_join(&app);
        assert_eq!(joined.borrow().len(), 1);
        // A launch-busy invite waits for idle.
        set_pending(Some("room-8"));
        let mut data = app.get_main_data();
        data.launching = true;
        app.set_main_data(data);
        super::retry_pending_join(&app);
        assert_eq!(joined.borrow().len(), 1);
        let mut data = app.get_main_data();
        data.launching = false;
        app.set_main_data(data);
        super::retry_pending_join(&app);
        assert_eq!(*joined.borrow(), ["room-7", "room-8"]);
        // Workshop profile waits until back on Game.
        set_pending(Some("room-9"));
        app.set_workshop(true);
        super::retry_pending_join(&app);
        assert_eq!(joined.borrow().len(), 2);
        app.set_workshop(false);
        super::retry_pending_join(&app);
        assert_eq!(*joined.borrow(), ["room-7", "room-8", "room-9"]);
        reset_invite_state();
    }

    #[test]
    fn game_status_idle_retries_a_waiting_invite() {
        // Readiness arriving via the game-status event (game exits to idle)
        // retries the same way.
        let _serial = crate::main_ops::test_serial();
        i_slint_backend_testing::init_no_event_loop();
        let app = crate::AppWindow::new().unwrap();
        let joined = Rc::new(RefCell::new(Vec::<String>::new()));
        let recorded = joined.clone();
        app.on_join(move |id| recorded.borrow_mut().push(id.to_string()));
        reset_invite_state();
        ready_idle(&app);
        let mut data = app.get_main_data();
        data.phase = "finishing".into();
        app.set_main_data(data);
        set_pending(Some("room-3"));
        apply(&app, LauncherEvent::RoomPending);
        assert!(joined.borrow().is_empty());
        apply(
            &app,
            LauncherEvent::GameStatus(launcher_core::game::GameStatus {
                phase: launcher_core::game::GamePhase::Idle,
                revision: 1,
            }),
        );
        assert_eq!(*joined.borrow(), ["room-3"]);
        reset_invite_state();
    }

    #[test]
    fn workshop_flip_wires_into_the_pending_retry() {
        // main.slint `changed workshop` fires the retry-join callback, so a
        // profile flip back to Game retries without any core event.
        let _serial = crate::main_ops::test_serial();
        i_slint_backend_testing::init_no_event_loop();
        let app = crate::AppWindow::new().unwrap();
        app.set_show_main(true);
        app.show().unwrap();
        slint::platform::update_timers_and_animations();
        let fired = Rc::new(RefCell::new(0));
        let counted = fired.clone();
        app.on_retry_join(move || *counted.borrow_mut() += 1);
        app.invoke_retry_join();
        assert_eq!(*fired.borrow(), 1);
        app.set_workshop(true);
        slint::platform::update_timers_and_animations();
        assert_eq!(*fired.borrow(), 2);
        app.set_workshop(true);
        slint::platform::update_timers_and_animations();
        assert_eq!(*fired.borrow(), 2);
        app.set_workshop(false);
        slint::platform::update_timers_and_animations();
        assert_eq!(*fired.borrow(), 3);
        reset_invite_state();
    }
    #[test]
    fn auto_join_releases_lock_before_invoke() {
        // Regression: retry_pending_join held `handled` across invoke_join while
        // the production on_join (main_ops::attach) synchronously calls note_join
        // on the same thread -> UI-thread deadlock. The helper thread below takes
        // the same blocking lock; on the old code it hangs until the timeout and
        // the test fails instead of hanging CI.
        let _serial = crate::main_ops::test_serial();
        i_slint_backend_testing::init_no_event_loop();
        let app = crate::AppWindow::new().unwrap();
        app.on_join(|id| {
            let (done, wait) = std::sync::mpsc::channel::<()>();
            std::thread::spawn(move || {
                let _guard = super::handled().lock().unwrap();
                let _ = done.send(());
            });
            wait.recv_timeout(std::time::Duration::from_secs(2))
                .expect("deadlock: handled lock held during invoke_join");
            super::note_join(&id);
        });
        reset_invite_state();
        ready_idle(&app);
        set_pending(Some("room-deadlock"));
        super::retry_pending_join(&app);
        reset_invite_state();
    }

    #[test]
    fn profile_change_blocks_auto_join_until_clear() {
        // Readiness must also require !profile_changing && !profile_blocked:
        // joining while either flag is set consumes the one-shot while the
        // deploy guard (main_ops::attach) rejects the deploy, losing the invite.
        let _serial = crate::main_ops::test_serial();
        i_slint_backend_testing::init_no_event_loop();
        let app = crate::AppWindow::new().unwrap();
        let joined = Rc::new(RefCell::new(Vec::<String>::new()));
        let recorded = joined.clone();
        app.on_join(move |id| recorded.borrow_mut().push(id.to_string()));
        reset_invite_state();
        ready_idle(&app);
        set_pending(Some("room-profile"));
        let mut data = app.get_main_data();
        data.profile_changing = true;
        app.set_main_data(data);
        super::retry_pending_join(&app);
        assert!(joined.borrow().is_empty());
        let mut data = app.get_main_data();
        data.profile_changing = false;
        data.profile_blocked = true;
        app.set_main_data(data);
        super::retry_pending_join(&app);
        assert!(joined.borrow().is_empty());
        // Flags cleared: the waiting invite joins without a new event.
        let mut data = app.get_main_data();
        data.profile_blocked = false;
        app.set_main_data(data);
        super::retry_pending_join(&app);
        assert_eq!(*joined.borrow(), ["room-profile"]);
        reset_invite_state();
    }

    #[test]
    fn progress_handles_unknown_totals_overflow_and_completion() {
        assert_eq!(super::percent(0, 0), -1);
        assert_eq!(super::percent(1, 3), 33);
        assert_eq!(super::percent(4, 3), 100);
        assert_eq!(super::percent(u64::MAX, u64::MAX), 100);
    }
}
