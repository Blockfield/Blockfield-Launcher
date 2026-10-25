//! Exercise the thread hop with the real core game lifecycle and both progress units.
use std::time::Duration;

use blockfield_native::{AppWindow, bridge, runtime};
use launcher_app::{LauncherAppState, LauncherEvent};
use launcher_core::game::GameState;
use launcher_net::download::DownloadProgressPayload;
use slint::ComponentHandle;

#[test]
fn worker_events_reach_only_the_ui_thread() {
    i_slint_backend_testing::init_integration_test_with_system_time();
    let app = AppWindow::new().unwrap();
    let events = bridge::sink(&app);
    let weak = app.as_weak();
    let ui_thread = std::thread::current().id();
    let runtime = runtime::new().unwrap();
    let worker = runtime.spawn(async move {
        assert_ne!(std::thread::current().id(), ui_thread);
        let game_events = events.clone();
        let game = std::sync::Arc::new(GameState::new(move |status| {
            game_events(LauncherEvent::GameStatus(status));
        }));
        let session = game.begin().unwrap();
        session.started();
        events(LauncherAppState::status_event(
            "launch",
            "Starting game",
            false,
        ));
        events(LauncherEvent::DownloadProgress(
            DownloadProgressPayload::files("a".into(), 1, 4),
        ));
        weak.upgrade_in_event_loop(move |app| {
            assert_eq!(std::thread::current().id(), ui_thread);
            let data = app.get_main_data();
            assert_eq!(data.phase, "running");
            assert!(!data.launching);
            assert_eq!(data.launch_status, "Starting game");
            assert_eq!(data.progress, 25);
            let mut bytes = DownloadProgressPayload::files("a".into(), 0, 0);
            bytes.unit = launcher_net::download::ProgressUnit::Bytes;
            bytes.total_bytes_downloaded = 50;
            bytes.total_bytes_all = 100;
            bridge::apply(&app, LauncherEvent::DownloadProgress(bytes));
            assert_eq!(app.get_main_data().progress, 50);
            bridge::apply(&app, LauncherEvent::RoomPending);
            assert!(app.get_main_data().pending_room);
            bridge::apply(&app, LauncherEvent::AccountExpired);
            assert_eq!(app.get_main_data().progress, 50);
            drop(session);
            let weak = app.as_weak();
            weak.upgrade_in_event_loop(|app| {
                assert_eq!(app.get_main_data().phase, "idle");
                assert_eq!(app.get_main_data().progress, -1);
                slint::quit_event_loop().unwrap();
            })
            .unwrap();
        })
        .unwrap();
    });
    slint::Timer::single_shot(Duration::from_secs(10), || panic!("event bridge timed out"));
    app.run().unwrap();
    runtime.block_on(worker).unwrap();
}
