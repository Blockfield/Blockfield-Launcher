//! Every auth action shares one busy guard: submit/logout/continue refuse
//! while another auth operation is running, so concurrent ops can never race
//! on the same config and tokens.
#![cfg(unix)]
use std::{path::PathBuf, time::Duration};

use blockfield_native::{AppWindow, auth, runtime};
use slint::ComponentHandle;

#[test]
fn auth_actions_refuse_while_busy() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache")
        .join(format!("native-auth-busy-{}", std::process::id()));
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
    auth::attach(&app, state.clone(), runtime.handle().clone());
    assert!(auth::try_begin(&app));
    app.set_login_busy(true);
    assert!(!auth::try_begin(&app));
    app.set_login_busy(false);
    app.set_firstrun_saving(true);
    assert!(!auth::try_begin(&app));
    app.set_firstrun_saving(false);
    // A logout clicked while an auth op is in flight must not spawn work:
    // busy stays set and no failure surfaces, even after the loop pumps.
    app.set_login_busy(true);
    app.invoke_login_logout();
    let weak = app.as_weak();
    slint::Timer::single_shot(Duration::from_millis(3000), move || {
        let app = weak.upgrade().unwrap();
        assert!(app.get_login_busy());
        assert!(app.get_login_error().is_empty());
        slint::quit_event_loop().unwrap();
    });
    slint::Timer::single_shot(Duration::from_secs(10), || {
        panic!("auth busy probe timed out")
    });
    app.run().unwrap();
    runtime.shutdown_timeout(Duration::from_secs(2));
    std::fs::remove_dir_all(root).unwrap();
}
