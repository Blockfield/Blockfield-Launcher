//! Parity integration test for PR #197 review findings #1 and #2:
//! - Workshop startup must block profile until role lookup confirms access (and stay blocked on error).
//! - Unsaved Settings draft edits to `workshop_server` must never leak into the Shell footer IP.
#![cfg(unix)]

use std::{
    cell::Cell,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use blockfield_native::{
    AppWindow, SettingsForm,
    backend::{self, Backend},
    runtime,
    stats::StatsOutcome,
    wire_app,
};
use launcher_core::config::{ClientProfile, LauncherConfig};
use slint::ComponentHandle;

fn isolated_env(tag: &str, pack_url: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache")
        .join(format!("native-{tag}-{}", std::process::id()));
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
        std::env::set_var("VITE_BLOCKFIELD_PACK_URL", pack_url);
    }
    root
}

fn spawn_content_server(body: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming().take(16).flatten() {
            let mut stream = stream;
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut buf = [0u8; 2048];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let (status, payload) = if req.starts_with("GET /content.json ") {
                ("200 OK", body.as_str())
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
    format!("http://{addr}")
}

#[test]
fn workshop_startup_blocks_until_role_verified_and_footer_ignores_unsaved_draft() {
    let cms_json = serde_json::json!({
        "brand": "CMS_BRAND",
        "serverIp": "play.blockfield.pro:25565",
    })
    .to_string();
    let pack_url = spawn_content_server(cms_json);
    let root = isolated_env("d1-workshop-startup", &pack_url);
    let app_data_dir = runtime::app_data_dir().unwrap();
    std::fs::create_dir_all(&app_data_dir).unwrap();
    let config = LauncherConfig {
        active_profile: ClientProfile::Workshop,
        discord_presence: false,
        workshop_server: "raknet;workshop.blockfield.pro:25567".into(),
        ..LauncherConfig::default()
    };
    std::fs::write(
        launcher_core::config::config_path(&app_data_dir),
        serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();

    i_slint_backend_testing::init_integration_test_with_system_time();
    let app = AppWindow::new().unwrap();
    let rt = runtime::new().unwrap();
    let (_host, _activity) = wire_app(&app, Backend::Core(rt.handle().clone())).unwrap();
    let state = rt
        .block_on(rt.spawn_blocking(|| runtime::build_state(Arc::new(|_| {})).unwrap()))
        .unwrap();
    backend::enter_main(&app, state, rt.handle().clone());
    app.show().unwrap();

    let weak = app.as_weak();
    let stage = Rc::new(Cell::new(0u8));
    let poll = slint::Timer::default();
    poll.start(slint::TimerMode::Repeated, Duration::from_millis(20), {
        let stage = stage.clone();
        move || {
            let app = weak.upgrade().unwrap();
            let form = app.global::<SettingsForm>();
            match stage.get() {
                0 => {
                    if form.get_loading() || app.get_brand() != "CMS_BRAND" {
                        return;
                    }
                    // Finding #1: Workshop startup must immediately block profile and keep footer
                    // on Game IP until role lookup confirms Workshop access.
                    assert!(app.get_workshop());
                    assert!(!app.get_main_data().can_workshop);
                    assert!(
                        app.get_main_data().profile_blocked,
                        "workshop startup must block profile before role verification"
                    );
                    assert_eq!(
                        app.get_server_label(),
                        "IP · play.blockfield.pro:25565",
                        "blocked workshop startup must show Game server IP in footer"
                    );

                    // A failed role lookup must leave the profile blocked rather than unlocking it.
                    let failed_role = StatsOutcome {
                        data: None,
                        saved_at: None,
                        stale: false,
                        error: Some("network error".into()),
                    };
                    assert!(!backend::apply_role_outcome(&app, &failed_role));
                    assert!(
                        app.get_main_data().profile_blocked,
                        "failed role lookup on workshop startup must keep profile_blocked"
                    );

                    // Verified workshop role unblocks the profile and shows committed workshop IP.
                    let owner = StatsOutcome {
                        data: Some(
                            serde_json::json!({"profile": {"name": "Builder", "role": "owner"}}),
                        ),
                        saved_at: Some(1),
                        stale: false,
                        error: None,
                    };
                    assert!(!backend::apply_role_outcome(&app, &owner));
                    assert!(app.get_main_data().can_workshop);
                    assert!(!app.get_main_data().profile_blocked);
                    assert_eq!(
                        app.get_server_label(),
                        "IP · raknet;workshop.blockfield.pro:25567"
                    );

                    // Finding #2: typing an unsaved draft into Settings must never leak into the
                    // footer IP when a background role/content refresh calls sync_footer_server_label.
                    let mut draft = form.get_data();
                    draft.workshop_server = "unsaved.draft.invalid:9999".into();
                    form.set_data(draft);
                    form.invoke_edited();
                    assert!(!backend::apply_role_outcome(&app, &owner));
                    assert_eq!(
                        app.get_server_label(),
                        "IP · raknet;workshop.blockfield.pro:25567",
                        "unsaved workshop_server draft must not overwrite Shell footer IP"
                    );

                    // Saving Settings commits the new workshop server to the footer IP.
                    let mut saved = form.get_data();
                    saved.workshop_server = "raknet;saved.workshop.pro:25568".into();
                    form.set_data(saved);
                    form.invoke_edited();
                    form.invoke_save();
                    stage.set(1);
                }
                1 => {
                    if form.get_saving() || form.get_dirty() {
                        return;
                    }
                    assert_eq!(
                        form.get_error(),
                        "",
                        "settings save should succeed without error"
                    );
                    assert_eq!(
                        app.get_server_label(),
                        "IP · raknet;saved.workshop.pro:25568",
                        "saved workshop_server must update Shell footer IP"
                    );
                    slint::quit_event_loop().unwrap();
                }
                _ => {}
            }
        }
    });
    slint::Timer::single_shot(Duration::from_secs(10), || {
        panic!("workshop startup parity test timed out")
    });
    app.run().unwrap();
    rt.shutdown_timeout(Duration::from_secs(2));
    let _ = std::fs::remove_dir_all(root);
}
