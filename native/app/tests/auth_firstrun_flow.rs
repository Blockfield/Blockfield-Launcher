//! End-to-end event-loop parity test for F09 (FirstRun on real first login),
//! F12 (FirstRun friendly save error), F14 (core async voluntary logout), and
//! F24 (FirstRun bundled Java auto-detection).
#![cfg(unix)]
use std::{path::PathBuf, time::Duration};

use blockfield_native::{AppWindow, SettingsForm, auth, runtime, settings_tabs};
use launcher_app::ops::java::java_exe_name;
use launcher_core::config::LauncherConfig;
use slint::ComponentHandle;

#[test]
fn first_login_runs_firstrun_with_bundled_java_friendly_errors_and_clean_logout() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache")
        .join(format!("native-auth-firstrun-flow-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
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
    assert!(app_data_dir.starts_with(&root));

    let game_dir = root.join("game");
    let bundled_java = game_dir.join("java").join("bin").join(java_exe_name());
    std::fs::create_dir_all(bundled_java.parent().unwrap()).unwrap();
    std::fs::write(&bundled_java, b"#!/bin/sh\n").unwrap();

    // Fresh install: no username, no access_token, empty java_path (bundled exists).
    let config = LauncherConfig {
        game_dir: game_dir.to_string_lossy().into(),
        java_path: String::new(),
        username: String::new(),
        access_token: String::new(),
        discord_presence: false,
        ..LauncherConfig::default()
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

    let logged_out = runtime
        .block_on(launcher_net::account::account_status(
            &state.config,
            &state.app_data_dir,
            None,
        ))
        .expect("offline logged-out status");
    assert!(!auth::status_logged_in(&logged_out));

    i_slint_backend_testing::init_integration_test_with_system_time();
    let app = AppWindow::new().unwrap();
    auth::attach(&app, state.clone(), runtime.handle().clone());
    auth::enter_startup(&app, state.clone(), runtime.handle().clone(), logged_out);
    assert!(app.get_show_login(), "fresh install starts on LoginScreen");

    // Simulate real Drasl login persisting username + token BEFORE on_auth_done.
    let signed_in = runtime.block_on(async {
        let mut cfg = state.config.write().await;
        cfg.username = "NewPlayer".into();
        cfg.access_token = "tok".into();
        launcher_core::config::save_config(&state.app_data_dir, &cfg).unwrap();
        launcher_net::account::AccountStatus::from(&*cfg)
    });
    auth::on_auth_done(&app, &state, runtime.handle(), Ok(signed_in));

    let phase = std::rc::Rc::new(std::cell::Cell::new(0u8));
    let poll = slint::Timer::default();
    poll.start(slint::TimerMode::Repeated, Duration::from_millis(10), {
        let weak = app.as_weak();
        let phase = phase.clone();
        let state = state.clone();
        let handle = runtime.handle().clone();
        let bundled_java_str = bundled_java.to_string_lossy().into_owned();
        move || {
            let app = weak.upgrade().unwrap();
            match phase.get() {
                // Phase 0 (F09 + F24): FirstRunScreen MUST open (not MainScreen),
                // and bundled Java MUST be auto-detected into firstrun_java_path.
                0 => {
                    assert!(
                        !app.get_show_main(),
                        "F09: real first login must enter FirstRunScreen, not skip straight to MainScreen"
                    );
                    if !app.get_show_firstrun() {
                        return;
                    }
                    assert_eq!(
                        app.get_firstrun_java_path().as_str(),
                        bundled_java_str,
                        "F24: FirstRunScreen must auto-detect bundled Java via load_settings"
                    );
                    // Trigger F12: submit with a nonexistent Java path.
                    app.set_firstrun_java_path("/nonexistent/java-bin".into());
                    app.set_firstrun_ram(2);
                    app.invoke_firstrun_continue();
                    phase.set(1);
                }
                // Phase 1 (F12): FirstRun save error must be friendly Russian.
                1 => {
                    if app.get_firstrun_saving() || app.get_firstrun_error().is_empty() {
                        return;
                    }
                    assert_eq!(
                        app.get_firstrun_error().as_str(),
                        "Выбранный файл Java не найден. Очистите поле для автоматического выбора или укажите существующий файл.",
                        "F12: FirstRun save error must pass through friendly_error"
                    );
                    // Now save valid settings to finish first run.
                    app.set_firstrun_java_path(bundled_java_str.as_str().into());
                    app.invoke_firstrun_continue();
                    phase.set(2);
                }
                // Phase 2: MainScreen opens after FirstRun completes. Now verify
                // voluntary logout through real core settings_tabs::wire.
                2 => {
                    if !app.get_show_main() {
                        return;
                    }
                    // Re-login after completed FirstRun skips FirstRunScreen.
                    let re_signed = {
                        let cfg = state.config.blocking_read();
                        launcher_net::account::AccountStatus::from(&*cfg)
                    };
                    auth::on_auth_done(&app, &state, &handle, Ok(re_signed));
                    assert!(
                        app.get_show_main() && !app.get_show_firstrun(),
                        "re-login after FirstRun completes goes straight to MainScreen"
                    );

                    // Clear access_token so account::logout skips network /auth/invalidate
                    // while still running the full core logout + config save path.
                    state.config.blocking_write().access_token.clear();
                    settings_tabs::wire(&app, Some((state.clone(), handle.clone())));
                    app.global::<SettingsForm>().invoke_logout();
                    phase.set(3);
                }
                // Phase 3 (F14): Core voluntary logout lands on LoginScreen with
                // preserved username and NO session-expired error.
                _ => {
                    if !app.get_show_login() {
                        return;
                    }
                    assert_eq!(
                        app.get_login_username().as_str(),
                        "NewPlayer",
                        "F14: core voluntary logout must keep stored username"
                    );
                    assert!(
                        app.get_login_error().is_empty(),
                        "F14: core voluntary logout must not show session-expired error, got: {}",
                        app.get_login_error()
                    );
                    slint::quit_event_loop().unwrap();
                }
            }
        }
    });
    let timeout_phase = phase.clone();
    slint::Timer::single_shot(Duration::from_secs(15), move || {
        panic!(
            "auth_firstrun_flow timed out in phase {}",
            timeout_phase.get()
        );
    });
    app.run().unwrap();
    runtime.shutdown_timeout(Duration::from_secs(2));
    let _ = std::fs::remove_dir_all(root);
}
