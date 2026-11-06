// auth gate: fresh first-run read per submit, secret cleared on success.
// Drives startup routing plus a second sign-in in the same install, and
// asserts the password never survives a successful auth.
#![cfg(unix)]
use std::{path::PathBuf, time::Duration};

use blockfield_native::{AppWindow, auth, runtime};
use slint::ComponentHandle;

#[test]
fn auth_gate_clears_password_and_reads_fresh_first_run() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache")
        .join(format!("native-firstrun-{}", std::process::id()));
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
    // Fake signed-in session with no stored username: logged in, first run.
    // Kept away from Discord IPC like the bootstrap test.
    let config = launcher_core::config::LauncherConfig {
        access_token: "x".into(),
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
    // Offline-safe: no migration, no skin copy without tokens.
    let status = runtime
        .block_on(launcher_net::account::account_status(
            &state.config,
            &state.app_data_dir,
            None,
        ))
        .expect("offline account status");
    assert!(auth::status_logged_in(&status));
    assert!(auth::status_username(&status).is_empty());
    runtime.block_on(async {
        state.config.write().await.username = "Operator".into();
    });
    let signed_in = runtime
        .block_on(launcher_net::account::account_status(
            &state.config,
            &state.app_data_dir,
            None,
        ))
        .expect("offline account status");
    assert!(auth::status_logged_in(&signed_in));
    assert_eq!(auth::status_username(&signed_in), "Operator");
    i_slint_backend_testing::init_integration_test_with_system_time();
    let app = AppWindow::new().unwrap();
    // Called from this bare test thread: no Tokio runtime context here,
    // exactly like the Slint UI thread that owns the auth gate.
    auth::enter_startup(&app, state.clone(), runtime.handle().clone(), status);
    let again = runtime
        .block_on(launcher_net::account::account_status(
            &state.config,
            &state.app_data_dir,
            None,
        ))
        .expect("offline account status");
    let signed_in = std::rc::Rc::new(std::cell::RefCell::new(Some(signed_in)));
    let again = std::rc::Rc::new(std::cell::RefCell::new(Some(again)));
    let weak = app.as_weak();
    let entered = std::rc::Rc::new(std::cell::Cell::new(false));
    let poll = slint::Timer::default();
    poll.start(slint::TimerMode::Repeated, Duration::from_millis(10), {
        let entered = entered.clone();
        let state = state.clone();
        let handle = runtime.handle().clone();

        move || {
            let app = weak.upgrade().unwrap();
            if !entered.get() {
                if !app.get_show_firstrun() {
                    return;
                }
                assert!(!app.get_show_main());
                let max = app.get_firstrun_maxram();
                assert!(max >= 2);
                let ram = app.get_firstrun_ram();
                assert!((2..=max).contains(&ram));
                entered.set(true);
                let routed = signed_in.borrow_mut().take().expect("route once");
                auth::on_auth_done(&app, &state, &handle, Ok(routed));
                return;
            }
            if app.get_show_main() {
                app.set_login_password("s3cret".into());
                let routed = again.borrow_mut().take().expect("route once");
                auth::on_auth_done(&app, &state, &handle, Ok(routed));
                assert!(app.get_login_password().is_empty());
                slint::quit_event_loop().unwrap();
            }
        }
    });
    slint::Timer::single_shot(Duration::from_secs(10), || {
        panic!("firstrun route timed out")
    });
    app.run().unwrap();
    runtime.shutdown_timeout(Duration::from_secs(2));
    std::fs::remove_dir_all(root).unwrap();
}
