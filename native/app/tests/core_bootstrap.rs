//! A plain backend must boot offline without touching an existing install.
// Windows known folders ignore HOME; its fixture and bridge tests run separately.
#![cfg(unix)]
use std::{path::PathBuf, time::Duration};

use blockfield_native::{AppWindow, backend::Backend, runtime};
use slint::ComponentHandle;

#[test]
fn empty_home_boots_real_core_and_reports_offline() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache")
        .join(format!("native-bootstrap-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // This integration-test process has no other tests or threads yet.
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
    let expected = runtime::app_data_dir().unwrap();
    assert!(expected.starts_with(&root), "{}", expected.display());
    let state = {
        let runtime = runtime::new().unwrap();
        runtime
            .block_on(
                runtime
                    .spawn_blocking(|| runtime::build_state(std::sync::Arc::new(|_| {})).unwrap()),
            )
            .unwrap()
    };
    assert_eq!(state.app_data_dir, expected);
    assert!(state.pack.blocking_read().is_none());
    assert_eq!(
        state.game.snapshot().phase,
        launcher_core::game::GamePhase::Idle
    );
    // Keep this host test away from the owner's Discord IPC sockets.
    std::fs::create_dir_all(&expected).unwrap();
    let config_path = launcher_core::config::config_path(&expected);
    let config = launcher_core::config::LauncherConfig {
        discord_presence: false,
        ..launcher_core::config::LauncherConfig::default()
    };
    let config_bytes = serde_json::to_vec(&config).unwrap();
    std::fs::write(&config_path, &config_bytes).unwrap();
    i_slint_backend_testing::init_integration_test_with_system_time();
    let app = AppWindow::new().unwrap();
    let runtime = runtime::new().unwrap();
    Backend::Core(runtime.handle().clone()).start(&app);
    assert!(app.get_show_main());
    assert!(app.get_main_data().checking);
    let weak = app.as_weak();
    let poll = slint::Timer::default();
    poll.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(10),
        move || {
            let app = weak.upgrade().unwrap();
            let data = app.get_main_data();
            if !data.checking {
                assert!(!data.checked);
                assert_eq!(data.check_error, "VITE_BLOCKFIELD_PACK_URL is required");
                assert_eq!(data.phase, "idle");
                assert!(!data.online);
                slint::quit_event_loop().unwrap();
            }
        },
    );
    slint::Timer::single_shot(Duration::from_secs(10), || {
        panic!("core bootstrap timed out")
    });
    app.run().unwrap();
    runtime.shutdown_timeout(Duration::from_secs(2));
    assert_eq!(std::fs::read(config_path).unwrap(), config_bytes);
    std::fs::remove_dir_all(root).unwrap();
}
