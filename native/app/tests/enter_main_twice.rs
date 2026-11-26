//! enter_main runs on every main entry (login, re-login): the one-shot pack
//! check re-runs each time, but the never-ending server poll and presence
//! loops must start exactly once per process (Tauri starts both once at
//! setup). Re-entering main must stay healthy and leak no extra loops.
#![cfg(unix)]
use std::{path::PathBuf, time::Duration};

use blockfield_native::{AppWindow, backend, runtime};
use slint::ComponentHandle;

#[test]
fn enter_main_twice_keeps_single_background_set() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache")
        .join(format!("native-enter-twice-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    unsafe {
        for key in [
            "HOME",
            "USERPROFILE",
            "APPDATA",
            "XDG_DATA_HOME",
            "XDG_CONFIG_HOME",
            "XDG_CACHE_HOME",
            "XDG_RUNTIME_DIR",
            "TMPDIR",
        ] {
            std::env::set_var(key, &root);
        }
        std::env::remove_var("VITE_BLOCKFIELD_PACK_URL");
    }
    let app_data_dir = runtime::app_data_dir().unwrap();
    assert!(
        app_data_dir.starts_with(&root),
        "{}",
        app_data_dir.display()
    );
    let config = launcher_core::config::LauncherConfig {
        discord_presence: false,
        ..launcher_core::config::LauncherConfig::default()
    };
    std::fs::create_dir_all(&app_data_dir).unwrap();
    let config_path = launcher_core::config::config_path(&app_data_dir);
    std::fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    let runtime = runtime::new().unwrap();
    let state = runtime
        .block_on(
            runtime.spawn_blocking(|| runtime::build_state(std::sync::Arc::new(|_| {})).unwrap()),
        )
        .unwrap();
    i_slint_backend_testing::init_integration_test_with_system_time();
    let app = AppWindow::new().unwrap();
    backend::enter_main(&app, state.clone(), runtime.handle().clone());
    let weak = app.as_weak();
    let entered = std::rc::Rc::new(std::cell::Cell::new(false));
    let poll = slint::Timer::default();
    poll.start(slint::TimerMode::Repeated, Duration::from_millis(10), {
        let entered = entered.clone();
        let state = state.clone();
        let handle = runtime.handle().clone();
        move || {
            let app = weak.upgrade().unwrap();
            if app.get_main_data().checking {
                return;
            }
            if !entered.get() {
                // First entry settled (offline pack check reports the
                // missing URL); re-enter like a logout-login cycle.
                assert!(app.get_show_main());
                assert!(!app.get_main_data().check_error.is_empty());
                entered.set(true);
                backend::enter_main(&app, state.clone(), handle.clone());
                return;
            }
            // Second entry settles the same way: healthy, no hang.
            assert!(app.get_show_main());
            assert!(!app.get_main_data().checking);
            slint::quit_event_loop().unwrap();
        }
    });
    slint::Timer::single_shot(Duration::from_secs(10), || {
        panic!("enter_main twice timed out")
    });
    app.run().unwrap();
    runtime.shutdown_timeout(Duration::from_secs(2));
    std::fs::remove_dir_all(root).unwrap();
}
