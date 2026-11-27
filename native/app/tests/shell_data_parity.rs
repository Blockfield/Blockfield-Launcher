//! Parity integration tests for D1 (shell + main screen data).
//! Exercises real production wiring (`wire_app`, `Backend::Core`, `settings_tabs`)
//! without fixtures so regressions to fixture leaks or stale state fail loudly.
#![cfg(unix)]

use std::{
    cell::RefCell,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use blockfield_native::{
    AppWindow, SettingsForm,
    backend::{self, Backend},
    runtime, wire_app,
};
use i_slint_backend_testing::ElementHandle;
use launcher_core::game::GameSession;
use slint::{ComponentHandle, Model, winit_030::winit::event::WindowEvent as WinitEvent};

fn isolated_env(tag: &str, pack_url: Option<&str>) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache")
        .join(format!("native-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    // Single-threaded test binary before runtime spawn.
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
        match pack_url {
            Some(url) => std::env::set_var("VITE_BLOCKFIELD_PACK_URL", url),
            None => std::env::remove_var("VITE_BLOCKFIELD_PACK_URL"),
        }
    }
    root
}

fn spawn_content_server(body: String) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let launcher_hits = Arc::new(AtomicUsize::new(0));
    let hits = launcher_hits.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().take(32).flatten() {
            let mut stream = stream;
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut buf = [0u8; 2048];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let (status, payload) = if req.starts_with("GET /content.json ") {
                ("200 OK", body.as_str())
            } else if req.starts_with("GET /launcher.json ") {
                hits.fetch_add(1, Ordering::SeqCst);
                ("404 Not Found", "{}")
            } else {
                ("404 Not Found", "{}")
            };
            let resp = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                payload.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    (format!("http://{addr}"), launcher_hits)
}

#[test]
fn core_boot_has_real_version_cms_no_fixture_leaks_and_resyncs_profile() {
    let cms_json = serde_json::json!({
        "brand": "CMS_BRAND",
        "brandSubtitle": "CMS SUBTITLE",
        "chromeTitle": "CMS CHROME",
        "operationName": "CMS_OP",
        "season": "CMS 1.21.1",
        "serverName": "BLOCKFIELD",
        "serverIp": "play.blockfield.pro:25565",
        "description": "CMS description text.",
        "supportLabel": "CMS SUPPORT",
        "updateDescription": "Лаунчер проверяет файлы сборки и скачивает необходимые обновления.",
        "settingsPreferences": "/ CMS PREFS",
        "features": [
            { "icon": "flag", "title": "ИГРОВОЕ МЕНЮ", "desc": "Выбирайте комнату, класс и точку появления через меню в игре." }
        ]
    })
    .to_string();
    let (pack_url, launcher_hits) = spawn_content_server(cms_json);
    let root = isolated_env("d1-parity", Some(&pack_url));
    let app_data_dir = runtime::app_data_dir().unwrap();
    std::fs::create_dir_all(&app_data_dir).unwrap();
    let config = launcher_core::config::LauncherConfig {
        discord_presence: false,
        workshop_server: "raknet;workshop.blockfield.pro:25567".into(),
        ..launcher_core::config::LauncherConfig::default()
    };
    std::fs::write(
        launcher_core::config::config_path(&app_data_dir),
        serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();

    i_slint_backend_testing::init_integration_test_with_system_time();
    let app = AppWindow::new().unwrap();
    let rt = runtime::new().unwrap();
    let (_host, activity) = wire_app(&app, Backend::Core(rt.handle().clone())).unwrap();

    // F01: footer version must reflect CARGO_PKG_VERSION, never fixture v1.0.19.
    assert_eq!(
        app.get_launcher_label(),
        format!("ЛАУНЧЕР v{}", env!("CARGO_PKG_VERSION"))
    );
    // F10: initial region before first poll must be "—", never blank.
    assert_eq!(app.get_main_data().region, "—");
    // No fixture dev data or capture username may leak into a real run.
    assert_ne!(app.get_dev_data().server, "127.0.0.1:25565");
    assert_ne!(app.get_username(), "CaptureOperator");

    let state = rt
        .block_on(rt.spawn_blocking(|| runtime::build_state(Arc::new(|_| {})).unwrap()))
        .unwrap();
    backend::enter_main(&app, state.clone(), rt.handle().clone());
    app.show().unwrap();

    // F05: before online status arrives, MainScreen must never render hardcoded "/ 100".
    assert!(
        ElementHandle::find_by_accessible_label(&app, "/ 100")
            .next()
            .is_none(),
        "MainScreen must not hardcode '/ 100' when players_max is absent"
    );

    let weak = app.as_weak();
    let stage = Rc::new(std::cell::Cell::new(0u8));
    let hits_before_focus = Rc::new(std::cell::Cell::new(0usize));
    let session_holder: Rc<RefCell<Option<GameSession>>> = Rc::new(RefCell::new(None));
    let poll = slint::Timer::default();
    poll.start(slint::TimerMode::Repeated, Duration::from_millis(20), {
        let stage = stage.clone();
        let state = state.clone();
        let session_holder = session_holder.clone();
        let activity = activity.clone();
        let launcher_hits = launcher_hits.clone();
        let hits_before_focus = hits_before_focus.clone();
        move || {
            let app = weak.upgrade().unwrap();
            match stage.get() {
                0 => {
                    // Wait until settings load and CMS content.json finish.
                    if app.global::<SettingsForm>().get_loading() || app.get_brand() != "CMS_BRAND"
                    {
                        return;
                    }
                    // F06: CMS content.json fields applied to chrome, shell, main, update, settings.
                    assert_eq!(app.get_chrome_title(), "CMS CHROME");
                    assert_eq!(app.get_subtitle(), "CMS SUBTITLE");
                    assert_eq!(app.get_support_label(), "CMS SUPPORT");
                    assert_eq!(app.get_settings_subtitle(), "/ CMS PREFS");
                    assert_eq!(
                        app.get_update_data().description,
                        "Лаунчер проверяет файлы сборки и скачивает необходимые обновления."
                    );
                    assert_eq!(app.get_server_label(), "IP · play.blockfield.pro:25565");
                    assert_eq!(app.get_main_data().server_name, "BLOCKFIELD");
                    assert_eq!(app.get_main_features().row_count(), 1);
                    let feat = app.get_main_features().row_data(0).unwrap();
                    assert_eq!(feat.icon, "flag");
                    assert_eq!(
                        feat.description,
                        "Выбирайте комнату, класс и точку появления через меню в игре."
                    );

                    // F18: window focus triggers background modpack re-check when idle & checked.
                    let mut data = app.get_main_data();
                    data.checked = true;
                    data.checking = false;
                    data.can_workshop = true;
                    app.set_main_data(data);
                    hits_before_focus.set(launcher_hits.load(Ordering::SeqCst));
                    activity.event(&WinitEvent::Focused(true));
                    stage.set(1);
                }
                1 => {
                    if launcher_hits.load(Ordering::SeqCst) <= hits_before_focus.get() {
                        return;
                    }
                    // F16: a failed profile switch (game running) must revert `workshop` and
                    // surface the error on `main_data.profile_error` (not only hidden SettingsForm).
                    let session = state.game.begin().unwrap();
                    session.started();
                    *session_holder.borrow_mut() = Some(session);
                    app.set_workshop(true);
                    stage.set(2);
                }
                2 => {
                    let form = app.global::<SettingsForm>();
                    if form.get_blocked() || app.get_main_data().profile_changing {
                        return;
                    }
                    if form.get_error().is_empty() && app.get_main_data().profile_error.is_empty() {
                        return;
                    }
                    assert!(
                        !app.get_workshop(),
                        "failed profile switch must revert app.workshop back to game"
                    );
                    assert!(
                        !app.get_main_data().profile_error.is_empty(),
                        "failed profile switch must surface main_data.profile_error"
                    );
                    assert!(
                        app.get_main_data().profile_blocked,
                        "failed profile switch keeps profile_blocked until resolved"
                    );
                    slint::quit_event_loop().unwrap();
                }
                _ => {}
            }
        }
    });
    slint::Timer::single_shot(Duration::from_secs(10), || {
        panic!("d1 parity integration test timed out")
    });
    app.run().unwrap();
    rt.shutdown_timeout(Duration::from_secs(2));
    let _ = std::fs::remove_dir_all(root);
}
