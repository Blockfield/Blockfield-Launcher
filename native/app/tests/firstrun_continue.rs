// G9: FirstRun continue click-through vs Tauri FirstRunScreen.
// Drives the production continue path (auth::attach + on_firstrun_continue)
// with real state: the form saves game dir / java / RAM through
// settings_ops::save_settings, then the shell opens (Tauri onComplete).
// Failure to save keeps the form up with the backend error, like Tauri's
// friendlyError(e, 'settings-save') branch.
#![cfg(unix)]
use std::{path::PathBuf, time::Duration};

use blockfield_native::{AppWindow, auth, runtime};
use slint::ComponentHandle;

#[test]
fn firstrun_continue_saves_settings_and_enters_main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache")
        .join(format!("native-firstrun-continue-{}", std::process::id()));
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
    let status = runtime
        .block_on(launcher_net::account::account_status(
            &state.config,
            &state.app_data_dir,
            None,
        ))
        .expect("offline account status");
    assert!(auth::status_logged_in(&status));
    assert!(auth::status_username(&status).is_empty());
    i_slint_backend_testing::init_integration_test_with_system_time();
    let app = AppWindow::new().unwrap();
    auth::attach(&app, state.clone(), runtime.handle().clone());
    // Called from this bare test thread: no Tokio runtime context here,
    // exactly like the Slint UI thread that owns the auth gate.
    auth::enter_startup(&app, state.clone(), runtime.handle().clone(), status);
    let game_dir = root.join("game");
    let phase = std::rc::Rc::new(std::cell::Cell::new(0u8));
    let poll = slint::Timer::default();
    poll.start(slint::TimerMode::Repeated, Duration::from_millis(10), {
        let weak = app.as_weak();
        let phase = phase.clone();
        let state = state.clone();
        let game_dir = game_dir.clone();
        move || {
            let app = weak.upgrade().unwrap();
            match phase.get() {
                // Wait for the async enter_firstrun fill, then fill the form
                // the way FirstRunScreen does and press continue.
                0 => {
                    if !app.get_show_firstrun() {
                        return;
                    }
                    let max = app.get_firstrun_maxram();
                    assert!(max >= 2, "test host needs at least 2 GB RAM");
                    app.set_firstrun_game_dir(game_dir.to_string_lossy().as_ref().into());
                    app.set_firstrun_java_path("".into());
                    app.set_firstrun_ram(2);
                    app.invoke_firstrun_continue();
                    phase.set(1);
                }
                // The save lands on disk and the shell opens; the bad-RAM
                // guard never fires because the form clamps into [2, max].
                _ => {
                    if app.get_show_main() {
                        let saved: serde_json::Value = serde_json::from_slice(
                            &std::fs::read(launcher_core::config::config_path(&state.app_data_dir))
                                .expect("saved config"),
                        )
                        .expect("saved config json");
                        assert_eq!(
                            saved["gameDir"].as_str().unwrap(),
                            game_dir.to_string_lossy()
                        );
                        assert_eq!(saved["javaPath"].as_str().unwrap(), "");
                        assert_eq!(saved["ramMb"].as_u64().unwrap(), 2048);
                        assert!(app.get_firstrun_error().is_empty());
                        slint::quit_event_loop().unwrap();
                    }
                }
            }
        }
    });
    slint::Timer::single_shot(Duration::from_secs(15), || {
        panic!("firstrun continue timed out")
    });
    app.run().unwrap();
    runtime.shutdown_timeout(Duration::from_secs(2));
    std::fs::remove_dir_all(root).unwrap();
}
