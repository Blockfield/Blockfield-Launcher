//! F03 fail-before test: opening the Skin tab in live mode must fetch the
//! player's real Drasl skin/cape/slim off the UI thread via
//! `launcher_net::skins::preview_skin` and derive `skin-caption` from
//! `preview.source` (never hardcoding Steve + "ВАШ СКИН").
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use base64::Engine;
use blockfield_native::{AppWindow, backend::Backend, runtime, skin_upload, wire_app};
use launcher_app::{LauncherAppState, PackState};
use launcher_core::{
    config::{ClientProfile, LauncherConfig},
    game::GameState,
};
use launcher_net::{
    download::{Downloader, noop_guard_sink},
    pack::{Artifact, LauncherInfo, PackMeta},
};
use slint::ComponentHandle;
use tokio::sync::{Mutex, RwLock};

fn spawn_mock_drasl(
    skin_png: Vec<u8>,
) -> (
    String,
    Arc<AtomicUsize>,
    Arc<AtomicUsize>,
    std::thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock drasl");
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");
    let hits = Arc::new(AtomicUsize::new(0));
    let profile_hits = Arc::new(AtomicUsize::new(0));
    let hits_clone = hits.clone();
    let all_clone = profile_hits.clone();
    let base_clone = base.clone();
    let handle = std::thread::spawn(move || {
        let _ = listener.set_nonblocking(false);
        // Handle up to 8 requests (profile, session, texture, plus potential refresh).
        for _ in 0..8 {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut buf = [0u8; 2048];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let path = req
                .lines()
                .next()
                .and_then(|l| l.split_whitespace().nth(1))
                .unwrap_or("");
            if path == "/__stop" {
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
                break;
            }
            if path.starts_with("/users/profiles/minecraft/") {
                all_clone.fetch_add(1, Ordering::SeqCst);
            }
            if path == "/users/profiles/minecraft/DraslHero" {
                hits_clone.fetch_add(1, Ordering::SeqCst);
                let body = r#"{"id":"0123456789abcdef0123456789abcdef","name":"DraslHero"}"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes());
            } else if path == "/session/minecraft/profile/0123456789abcdef0123456789abcdef" {
                let textures = serde_json::json!({
                    "textures": {
                        "SKIN": {
                            "url": format!("{base_clone}/textures/hero.png"),
                            "metadata": { "model": "slim" }
                        }
                    }
                });
                let encoded =
                    base64::engine::general_purpose::STANDARD.encode(textures.to_string());
                let body = serde_json::json!({
                    "id": "0123456789abcdef0123456789abcdef",
                    "name": "DraslHero",
                    "properties": [{ "name": "textures", "value": encoded }]
                })
                .to_string();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes());
            } else if path == "/textures/hero.png" {
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    skin_png.len()
                );
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(&skin_png);
            } else {
                let _ = stream.write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
        }
    });
    (base, hits, profile_hits, handle)
}

fn state_with_skins(skins_base: Option<String>, username: &str) -> LauncherAppState {
    let downloader = Arc::new(Downloader::new(
        Arc::new(|_| {}),
        Arc::new(|_, _| {}),
        noop_guard_sink(),
    ));
    let pack = PackState {
        profile: ClientProfile::Game,
        info: LauncherInfo {
            pack: "https://blockfield.pro/pack.toml".into(),
            server: "play.blockfield.pro:25565".into(),
            skins: skins_base,
            installer: Artifact {
                url: "https://blockfield.pro/installer.jar".into(),
                sha256: String::new(),
                source: None,
            },
            bootstrap: Artifact {
                url: "https://blockfield.pro/bootstrap.jar".into(),
                sha256: String::new(),
                source: None,
            },
            java: HashMap::new(),
            pack_mirrors: Vec::new(),
        },
        meta: PackMeta {
            version: "1.0.0".into(),
            index_hash: String::new(),
            minecraft: "1.21.1".into(),
            loader: None,
        },
    };
    LauncherAppState {
        cancel_flag: downloader.cancel_flag.clone(),
        downloader,
        config: RwLock::new(LauncherConfig {
            username: username.into(),
            ..LauncherConfig::default()
        }),
        app_data_dir: std::env::temp_dir(),
        pack: RwLock::new(Some(pack)),
        game: Arc::new(GameState::new(|_| {})),
        operation: Mutex::new(()),
        events: Arc::new(|_| {}),
    }
}

#[test]
fn f03_skin_tab_fetches_real_drasl_skin_and_sets_source_caption() {
    i_slint_backend_testing::init_integration_test_with_system_time();
    let app = AppWindow::new().unwrap();
    let runtime = runtime::new().unwrap();
    // Wire app with Fixtures so Backend::Core does not spawn real network startup,
    // then attach live skin upload/preview wiring directly like enter_main does.
    let (host, _activity) = wire_app(&app, Backend::Fixtures).unwrap();

    // Valid 64x64 PNG fixture bytes (bundled vanilla steve) for mock Drasl texture.
    let skin_png = include_bytes!("../../../assets/skins/steve.png").to_vec();
    let (base, hits, _profiles, server) = spawn_mock_drasl(skin_png);
    let skins_opt = cfg!(debug_assertions).then(|| base.clone());
    let state = Arc::new(state_with_skins(skins_opt, "DraslHero"));
    skin_upload::wire_upload(&app, state, runtime.handle().clone());

    // Step 1: invalid username ("—") must yield source == "none" -> "СТАНДАРТНЫЙ СКИН"
    // without hitting the network (works in both debug and release/ci-test builds).
    app.set_username("—".into());
    app.set_skin_path("".into());
    app.set_cape_path("".into());
    app.invoke_navigate(3);
    app.invoke_settings_tab_changed(3);

    let weak = app.as_weak();
    let hits_check = hits.clone();
    let phase = std::rc::Rc::new(std::cell::Cell::new(0u8));
    let phase_poll = phase.clone();
    let poll = slint::Timer::default();
    poll.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(20),
        move || {
            let app = weak.upgrade().unwrap();
            match phase_poll.get() {
                0 if app.get_skin_caption() == "СТАНДАРТНЫЙ СКИН" => {
                    if cfg!(debug_assertions) {
                        // Step 2 (debug builds where local HTTP Drasl is permitted):
                        // switching username to "DraslHero" must hit mock Drasl and
                        // update caption to "ВАШ СКИН".
                        phase_poll.set(1);
                        app.set_username("DraslHero".into());
                    } else {
                        slint::quit_event_loop().unwrap();
                    }
                }
                1 if hits_check.load(Ordering::SeqCst) >= 1
                    && app.get_skin_caption() == "ВАШ СКИН" =>
                {
                    slint::quit_event_loop().unwrap();
                }
                _ => {}
            }
        },
    );
    let weak_timeout = app.as_weak();
    let hits_timeout = hits.clone();
    let phase_timeout = phase.clone();
    slint::Timer::single_shot(Duration::from_secs(5), move || {
        let caption = weak_timeout
            .upgrade()
            .map(|a| a.get_skin_caption().to_string())
            .unwrap_or_default();
        panic!(
            "F03 skin preview timed out in phase {}: caption={caption:?}, drasl_hits={}",
            phase_timeout.get(),
            hits_timeout.load(Ordering::SeqCst)
        );
    });
    app.run().unwrap();

    // Stop mock server cleanly.
    if let Ok(mut s) = std::net::TcpStream::connect(base.trim_start_matches("http://")) {
        let _ = s.write_all(b"GET /__stop HTTP/1.1\r\nHost: localhost\r\n\r\n");
    }
    let _ = server.join();
    host.shutdown();
    runtime.shutdown_timeout(Duration::from_secs(2));
}

#[test]
fn f03_debounce_coalesces_rapid_username_changes_into_single_fetch() {
    // Fail-before for review item 4 (300 ms debounce like Tauri
    // SettingsScreen): five rapid username changes must spawn exactly one
    // profile lookup, not one Drasl chain per keystroke.
    i_slint_backend_testing::init_integration_test_with_system_time();
    let app = AppWindow::new().unwrap();
    let runtime = runtime::new().unwrap();
    let (host, _activity) = wire_app(&app, Backend::Fixtures).unwrap();

    let skin_png = include_bytes!("../../../assets/skins/steve.png").to_vec();
    let (base, _hits, profile_hits, server) = spawn_mock_drasl(skin_png);
    let skins_opt = if cfg!(debug_assertions) {
        Some(base.clone())
    } else {
        None
    };
    let state = Arc::new(state_with_skins(skins_opt, "DraslHero"));
    skin_upload::wire_upload(&app, state, runtime.handle().clone());

    app.set_username("Starter01".into());
    app.set_skin_path("".into());
    app.set_cape_path("".into());
    app.invoke_navigate(3);
    app.invoke_settings_tab_changed(3);
    // Five keystrokes within one event-loop turn: only the last may fetch.
    for name in ["Typist02", "Typist03", "Typist04", "Typist05", "DraslHero"] {
        app.set_username(name.into());
    }

    let weak = app.as_weak();
    let profiles = profile_hits.clone();
    let poll = slint::Timer::default();
    poll.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(20),
        move || {
            let app = weak.upgrade().unwrap();
            if app.get_skin_caption() == "ВАШ СКИН" {
                assert_eq!(
                    profiles.load(Ordering::SeqCst),
                    1,
                    "rapid username changes must coalesce into one profile lookup"
                );
                slint::quit_event_loop().unwrap();
            }
        },
    );
    let weak_timeout = app.as_weak();
    let profiles_timeout = profile_hits.clone();
    slint::Timer::single_shot(Duration::from_secs(5), move || {
        let caption = weak_timeout
            .upgrade()
            .map(|a| a.get_skin_caption().to_string())
            .unwrap_or_default();
        panic!(
            "debounce test timed out: caption={caption:?}, profile_hits={}",
            profiles_timeout.load(Ordering::SeqCst)
        );
    });
    app.run().unwrap();

    if let Ok(mut s) = std::net::TcpStream::connect(base.trim_start_matches("http://")) {
        let _ = s.write_all(b"GET /__stop HTTP/1.1\r\nHost: localhost\r\n\r\n");
    }
    let _ = server.join();
    host.shutdown();
    runtime.shutdown_timeout(Duration::from_secs(2));
}

#[test]
fn f03_preload_caption_is_not_found_until_first_bytes() {
    // Fail-before for review item 5: while the first fetch is in flight the
    // host stays hidden and the caption must be the not-found fallback
    // (Tauri SkinPreview shows `fallback` when `data?.skin` is null), never
    // the fixture "ВАШ СКИН".
    i_slint_backend_testing::init_integration_test_with_system_time();
    let app = AppWindow::new().unwrap();
    let runtime = runtime::new().unwrap();
    let (host, _activity) = wire_app(&app, Backend::Fixtures).unwrap();

    // Black-hole Drasl: accept the profile lookup and never reply, so the
    // fetch stays in flight past the assertion.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind black-hole drasl");
    let base = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for _ in 0..4 {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            std::thread::sleep(Duration::from_secs(10));
        }
    });
    let skins_opt = if cfg!(debug_assertions) {
        Some(base)
    } else {
        None
    };
    let state = Arc::new(state_with_skins(skins_opt, "DraslHero"));
    skin_upload::wire_upload(&app, state, runtime.handle().clone());

    app.set_username("DraslHero".into());
    app.set_skin_path("".into());
    app.set_cape_path("".into());
    app.invoke_navigate(3);
    app.invoke_settings_tab_changed(3);

    let weak = app.as_weak();
    slint::Timer::single_shot(Duration::from_millis(900), move || {
        let app = weak.upgrade().unwrap();
        assert_eq!(
            app.get_skin_caption().as_str(),
            "СКИН НЕ НАЙДЕН",
            "pre-load caption must be the not-found fallback, not the fixture caption"
        );
        slint::quit_event_loop().unwrap();
    });
    slint::Timer::single_shot(Duration::from_secs(5), || {
        panic!("pre-load caption test timed out");
    });
    app.run().unwrap();

    host.shutdown();
    runtime.shutdown_timeout(Duration::from_secs(2));
}
