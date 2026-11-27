//! Post-fix visual evidence: live Drasl skin fetch pixels, not fixtures.
//!
//! Real winit + skia-software window under Xvfb (never the testing backend,
//! which renders nothing). A mock Drasl serves a solid-red 64x64 skin, so
//! the preview is visibly non-default. Saves `settings-skin-live.ppm` to
//! `$BLOCKFIELD_LIVE_CAPTURE_OUT` (or the temp dir) once the live caption
//! (`ВАШ СКИН`, custom source) lands and frames are delivered.
//!
//! Run on the VM: `xvfb-run -a cargo test --test live_skin_capture`.

use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
    time::Duration,
};

use base64::Engine as _;
use blockfield_native::{AppWindow, backend::Backend, runtime, skin_upload, wire_app};
use image::{ImageFormat, Rgba, RgbaImage};
use slint::ComponentHandle;

fn red_skin_png() -> Vec<u8> {
    let img = RgbaImage::from_pixel(64, 64, Rgba([224, 32, 32, 255]));
    let mut buf = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut buf), ImageFormat::Png)
        .expect("encode red skin");
    buf
}

fn spawn_red_drasl(skin_png: Vec<u8>) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock drasl");
    let base = format!("http://{}", listener.local_addr().unwrap());
    let base_clone = base.clone();
    let handle = std::thread::spawn(move || {
        let _ = listener.set_nonblocking(false);
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
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
                break;
            }
            if path == "/users/profiles/minecraft/DraslHero" {
                let body = r#"{"id":"0123456789abcdef0123456789abcdef","name":"DraslHero"}"#;
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
            } else if path == "/session/minecraft/profile/0123456789abcdef0123456789abcdef" {
                let textures = serde_json::json!({
                    "textures": {
                        "SKIN": {
                            "url": format!("{base_clone}/textures/hero.png"),
                            "metadata": { "model": "slim" }
                        }
                    }
                });
                let body = serde_json::json!({
                    "id": "0123456789abcdef0123456789abcdef",
                    "name": "DraslHero",
                    "properties": [{ "name": "textures", "value": base64::engine::general_purpose::STANDARD.encode(textures.to_string()) }]
                })
                .to_string();
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
            } else if path == "/textures/hero.png" {
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    skin_png.len()
                );
                let _ = stream.write_all(&skin_png);
            } else {
                let _ = stream.write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
        }
    });
    (base, handle)
}

#[test]
fn live_skin_capture_red_drasl_preview() {
    // Real window: winit + Skia CPU raster, like Layer-2 e2e under Xvfb.
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("skia-software".into())
        .select()
        .expect("select winit software backend");
    let app = AppWindow::new().expect("capture window");
    let runtime = runtime::new().expect("tokio runtime");
    let (host, _activity) = wire_app(&app, Backend::Fixtures).expect("wire");

    let (base, server) = spawn_red_drasl(red_skin_png());
    let state = Arc::new(skin_preview_state(&base));
    skin_upload::wire_upload(&app, state, runtime.handle().clone());

    app.set_username("DraslHero".into());
    app.set_skin_path("".into());
    app.set_cape_path("".into());
    app.invoke_navigate(3);
    app.invoke_settings_tab_changed(3);

    let out_dir = std::env::var("BLOCKFIELD_LIVE_CAPTURE_OUT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    std::fs::create_dir_all(&out_dir).expect("capture out dir");
    let start_delivered = host.delivered();
    let poll_host = host.clone();
    let weak = app.as_weak();
    let poll = slint::Timer::default();
    poll.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(100),
        move || {
            let app = weak.upgrade().expect("capture window");
            // Live custom skin resolved and the 3D host is animating it.
            if app.get_skin_caption() == "ВАШ СКИН" && poll_host.delivered() >= start_delivered + 5
            {
                let pixels = app.window().take_snapshot().expect("renderer snapshot");
                let path = out_dir.join("settings-skin-live.ppm");
                let mut file = std::fs::File::create(&path).expect("create capture");
                use std::io::Write as _;
                write!(file, "P6\n{} {}\n255\n", pixels.width(), pixels.height())
                    .expect("capture header");
                let rgb: Vec<u8> = pixels
                    .as_slice()
                    .iter()
                    .flat_map(|p| [p.r, p.g, p.b])
                    .collect();
                file.write_all(&rgb).expect("capture pixels");
                println!("LIVE_CAPTURE saved {}", path.display());
                slint::quit_event_loop().expect("finish live capture");
            }
        },
    );
    let weak_timeout = app.as_weak();
    slint::Timer::single_shot(Duration::from_secs(20), move || {
        let caption = weak_timeout
            .upgrade()
            .map(|a| a.get_skin_caption().to_string())
            .unwrap_or_default();
        panic!("live skin capture timed out, caption={caption:?}");
    });
    app.run().expect("run capture loop");

    if let Ok(mut s) = std::net::TcpStream::connect(base.trim_start_matches("http://")) {
        let _ = s.write_all(b"GET /__stop HTTP/1.1\r\nHost: localhost\r\n\r\n");
    }
    let _ = server.join();
    host.shutdown();
    runtime.shutdown_timeout(Duration::from_secs(2));
}

fn skin_preview_state(skins_base: &str) -> launcher_app::LauncherAppState {
    use launcher_app::PackState;
    use launcher_core::{
        config::{ClientProfile, LauncherConfig},
        game::GameState,
    };
    use launcher_net::{
        download::{Downloader, noop_guard_sink},
        pack::{Artifact, LauncherInfo, PackMeta},
    };
    use std::collections::HashMap;
    use tokio::sync::{Mutex, RwLock};
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
            skins: Some(skins_base.to_string()),
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
    launcher_app::LauncherAppState {
        cancel_flag: downloader.cancel_flag.clone(),
        downloader,
        config: RwLock::new(LauncherConfig {
            username: "DraslHero".into(),
            ..LauncherConfig::default()
        }),
        app_data_dir: std::env::temp_dir(),
        pack: RwLock::new(Some(pack)),
        game: Arc::new(GameState::new(|_| {})),
        operation: Mutex::new(()),
        events: Arc::new(|_| {}),
    }
}
