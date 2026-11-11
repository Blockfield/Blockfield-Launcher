//! G2 upload wiring: busy guard, error result and raw text through the real
//! Slint callback, offline-deterministic (anonymous config refuses before
//! any network, like Tauri's expired-session path).
use std::{path::PathBuf, sync::Arc, time::Duration};

use blockfield_native::{AppWindow, runtime, skin_upload};
use launcher_app::LauncherAppState;
use launcher_core::{config::LauncherConfig, game::GameState};
use launcher_net::download::{Downloader, noop_guard_sink};
use slint::ComponentHandle;
use tokio::sync::{Mutex, RwLock};

fn anonymous_state() -> LauncherAppState {
    let downloader = Arc::new(Downloader::new(
        Arc::new(|_| {}),
        Arc::new(|_, _| {}),
        noop_guard_sink(),
    ));
    LauncherAppState {
        cancel_flag: downloader.cancel_flag.clone(),
        downloader,
        config: RwLock::new(LauncherConfig::default()),
        app_data_dir: std::env::temp_dir(),
        pack: RwLock::new(None),
        game: Arc::new(GameState::new(|_| {})),
        operation: Mutex::new(()),
        events: Arc::new(|_| {}),
    }
}

#[test]
fn upload_reports_error_and_resets_busy() {
    i_slint_backend_testing::init_integration_test_with_system_time();
    let app = AppWindow::new().unwrap();
    let runtime = runtime::new().unwrap();
    let state = Arc::new(anonymous_state());
    skin_upload::wire_upload(&app, state, runtime.handle().clone());

    // Empty paths are a no-op: Tauri keeps the button disabled instead.
    app.invoke_upload_skin();
    assert!(!app.get_skin_busy());
    assert!(app.get_skin_message().is_empty());

    let png = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache")
        .join(format!("native-upload-{}.png", std::process::id()));
    std::fs::create_dir_all(png.parent().unwrap()).unwrap();
    std::fs::write(
        &png,
        [b"\x89PNG\r\n\x1a\n".as_slice(), b"data".as_slice()].concat(),
    )
    .unwrap();
    app.set_username("Operator".into());
    app.set_skin_path(png.to_string_lossy().as_ref().into());
    app.set_slim(true);
    app.invoke_upload_skin();
    assert!(app.get_skin_busy(), "upload marks busy synchronously");

    let weak = app.as_weak();
    let poll = slint::Timer::default();
    poll.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(10),
        move || {
            let app = weak.upgrade().unwrap();
            if app.get_skin_busy() {
                return;
            }
            assert!(
                app.get_skin_message().is_empty(),
                "error sets no success text"
            );
            assert_eq!(app.get_skin_error(), "Сессия истекла — войдите снова.");
            assert_eq!(app.get_skin_raw_error(), "SESSION_EXPIRED: log in again");
            slint::quit_event_loop().unwrap();
        },
    );
    slint::Timer::single_shot(Duration::from_secs(10), || {
        panic!("upload error probe timed out")
    });
    app.run().unwrap();
    runtime.shutdown_timeout(Duration::from_secs(2));
    std::fs::remove_file(png).unwrap();
}
