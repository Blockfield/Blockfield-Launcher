use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use launcher_app::LauncherAppState;
use skin3d::slint::{ClockMode, FrameSink, HostOptions, PreviewHost, SkinSource, default_source};
use slint::{
    ComponentHandle, Timer,
    platform::WindowEvent,
    winit_030::{EventResult, WinitWindowAccessor, winit::event::WindowEvent as WinitEvent},
};
use tokio::runtime::Handle;

use crate::{AppWindow, window_activity::Activity};

const MAX_TEXTURE_BYTES: u64 = 1024 * 1024;
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
const LOCAL_CAPTION: &str = "ПРЕДПРОСМОТР ФАЙЛА";
const DEFAULT_CAPTION: &str = "СТАНДАРТНЫЙ СКИН";
const MOJANG_CAPTION: &str = "ЛИЦЕНЗИОННЫЙ СКИН";
const CUSTOM_CAPTION: &str = "ВАШ СКИН";
const FALLBACK_CAPTION: &str = "СКИН НЕ НАЙДЕН";
const DATA_URL_PREFIX: &str = "data:image/png;base64,";

/// Derive the preview caption from the resolved `preview_skin` result,
/// mirroring `SettingsScreen.tsx` + `SkinPreview.tsx`.
pub fn caption_for_preview(
    skin_path: &str,
    preview: &launcher_net::skins::SkinPreview,
) -> &'static str {
    if preview.skin.is_none() {
        return FALLBACK_CAPTION;
    }
    if !skin_path.trim().is_empty() {
        return LOCAL_CAPTION;
    }
    match preview.source {
        "none" => DEFAULT_CAPTION,
        "mojang" => MOJANG_CAPTION,
        _ => CUSTOM_CAPTION,
    }
}

fn decode_data_url(url: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    let encoded = url.strip_prefix(DATA_URL_PREFIX)?;
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()
}

#[derive(Clone, PartialEq, Eq)]
struct LiveKey {
    username: String,
    skin_path: String,
    cape_path: String,
    slim: bool,
    upload_rev: u64,
}

struct LiveBackend {
    state: Arc<LauncherAppState>,
    handle: Handle,
    generation: Arc<AtomicU64>,
    upload_rev: Cell<u64>,
    last_key: RefCell<Option<LiveKey>>,
    staged: Cell<bool>,
}

enum PendingPreview {
    AttachLive(Arc<LauncherAppState>, Handle),
    Invalidate,
    FetchNow,
    Apply {
        host_key: (String, String, bool),
        skin: Option<Vec<u8>>,
        cape: Option<Vec<u8>>,
        slim: bool,
        caption: &'static str,
    },
}

thread_local! {
    static PENDING: RefCell<Option<PendingPreview>> = const { RefCell::new(None) };
}

fn dispatch_pending(app: &AppWindow, pending: PendingPreview) {
    PENDING.with(|slot| *slot.borrow_mut() = Some(pending));
    app.invoke_skin_preview_sync();
    // Drop if the window has no `skin::attach` handler (e.g. isolated upload tests).
    PENDING.with(|slot| *slot.borrow_mut() = None);
}

/// Attach live off-UI-thread `preview_skin` fetching to this window's preview host.
pub fn attach_live(app: &AppWindow, state: Arc<LauncherAppState>, handle: Handle) {
    dispatch_pending(app, PendingPreview::AttachLive(state, handle));
}

/// Force a fresh Drasl/Mojang skin preview fetch after an upload succeeds.
pub fn invalidate_preview(app: &AppWindow) {
    dispatch_pending(app, PendingPreview::Invalidate);
}

struct SkinController {
    host: Rc<PreviewHost>,
    live: RefCell<Option<LiveBackend>>,
    debounce: Timer,
}

impl SkinController {
    fn handle_sync(&self, app: &AppWindow) {
        let pending = PENDING.with(|slot| slot.borrow_mut().take());
        match pending {
            Some(PendingPreview::AttachLive(state, handle)) => {
                self.live.replace(Some(LiveBackend {
                    state,
                    handle,
                    generation: Arc::new(AtomicU64::new(0)),
                    upload_rev: Cell::new(0),
                    last_key: RefCell::new(None),
                    staged: Cell::new(false),
                }));
            }
            Some(PendingPreview::Invalidate) => {
                if let Some(live) = self.live.borrow().as_ref() {
                    live.upload_rev.set(live.upload_rev.get().wrapping_add(1));
                    live.last_key.replace(None);
                }
            }
            Some(PendingPreview::FetchNow) => {
                self.spawn_fetch(app);
                return;
            }
            Some(PendingPreview::Apply {
                host_key,
                skin,
                cape,
                slim,
                caption,
            }) => {
                app.set_skin_caption(caption.into());
                match skin {
                    Some(bytes) => {
                        self.host.set_source(
                            host_key,
                            SkinSource {
                                skin: bytes,
                                cape,
                                slim,
                            },
                        );
                        if let Some(live) = self.live.borrow().as_ref() {
                            live.staged.set(true);
                        }
                        self.host.set_tab_visible(skin_visible(app));
                        self.host.refresh();
                    }
                    None => {
                        self.host.set_tab_visible(false);
                        self.host.refresh();
                    }
                }
                return;
            }
            None => {}
        }

        let live_borrow = self.live.borrow();
        let Some(live) = live_borrow.as_ref() else {
            sync_source(app, &self.host);
            self.host.set_tab_visible(skin_visible(app));
            self.host.refresh();
            return;
        };

        if !skin_visible(app) {
            self.host.set_tab_visible(false);
            self.host.refresh();
            return;
        }

        let key = LiveKey {
            username: app.get_username().to_string(),
            skin_path: app.get_skin_path().to_string(),
            cape_path: app.get_cape_path().to_string(),
            slim: app.get_slim(),
            upload_rev: live.upload_rev.get(),
        };
        if live.last_key.borrow().as_ref() == Some(&key) {
            if live.staged.get() && !self.host.failed() {
                self.host.set_tab_visible(true);
                self.host.refresh();
            }
            return;
        }
        if !live.staged.get() {
            // Avoid initializing wgpu with the bundled fixture skin before real bytes arrive.
            // Tauri shows its "not found" fallback until then (SkinPreview.tsx).
            self.host.set_tab_visible(false);
            app.set_skin_caption(FALLBACK_CAPTION.into());
        }
        // 300 ms single-shot debounce like Tauri SettingsScreen: every
        // keystroke restarts the timer, the fetch spawns only on fire.
        // `last_key` updates at fire time, so leaving the tab mid-debounce
        // simply drops the fetch and re-arms on return.
        let weak = app.as_weak();
        self.debounce.start(
            slint::TimerMode::SingleShot,
            Duration::from_millis(300),
            move || {
                let Some(app) = weak.upgrade() else { return };
                dispatch_pending(&app, PendingPreview::FetchNow);
            },
        );
    }

    /// Spawn the off-UI-thread `preview_skin` fetch for the current inputs.
    fn spawn_fetch(&self, app: &AppWindow) {
        let live_borrow = self.live.borrow();
        let Some(live) = live_borrow.as_ref() else {
            return;
        };
        if !skin_visible(app) {
            return;
        }
        let key = LiveKey {
            username: app.get_username().to_string(),
            skin_path: app.get_skin_path().to_string(),
            cape_path: app.get_cape_path().to_string(),
            slim: app.get_slim(),
            upload_rev: live.upload_rev.get(),
        };
        live.last_key.replace(Some(key.clone()));
        let generation = live.generation.fetch_add(1, Ordering::SeqCst) + 1;

        let state = live.state.clone();
        let gen_counter = live.generation.clone();
        let weak = app.as_weak();
        let username = key.username;
        let skin_path = key.skin_path;
        let cape_path = key.cape_path;
        let slim = key.slim;
        live.handle.spawn(async move {
            let skins_base = state
                .pack
                .read()
                .await
                .as_ref()
                .and_then(|pack| pack.info.skins.clone());
            let skin_arg = (!skin_path.trim().is_empty()).then(|| skin_path.clone());
            let cape_arg = (!cape_path.trim().is_empty()).then(|| cape_path.clone());
            let result =
                launcher_net::skins::preview_skin(skins_base, username, skin_arg, cape_arg, slim)
                    .await;
            let _ = weak.upgrade_in_event_loop(move |app| {
                if gen_counter.load(Ordering::SeqCst) != generation {
                    return;
                }
                match result {
                    Ok(preview) => {
                        let caption = caption_for_preview(&skin_path, &preview);
                        let skin = preview.skin.as_deref().and_then(decode_data_url);
                        let cape = preview.cape.as_deref().and_then(decode_data_url);
                        dispatch_pending(
                            &app,
                            PendingPreview::Apply {
                                host_key: (skin_path, cape_path, preview.slim),
                                skin,
                                cape,
                                slim: preview.slim,
                                caption,
                            },
                        );
                    }
                    Err(error) => {
                        eprintln!("preview_skin: {error}");
                    }
                }
            });
        });
    }
}

/// The preview is live only on the Skin tab of the settings screen.
pub fn skin_visible(app: &AppWindow) -> bool {
    app.get_active() == 3 && app.get_settings_tab() == 3
}

fn read_bounded(source: impl std::io::Read) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    source.take(MAX_TEXTURE_BYTES + 1).read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn texture_bytes(path: &str) -> Result<Option<Vec<u8>>, String> {
    if path.is_empty() {
        return Ok(None);
    }
    // Reject by size before reading: the preview syncs on the event-loop
    // thread, so a huge file must never be fully loaded to be refused.
    // Check the opened handle, not the path, so the file cannot be swapped for a
    // FIFO between check and open. Linux opens non-blocking (O_NONBLOCK), so even a
    // FIFO with no writer returns at once; regular files ignore the flag.
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0o4000);
    }
    let file = options
        .open(path)
        .map_err(|error| format!("Не удалось прочитать файл: {error}"))?;
    let meta = file
        .metadata()
        .map_err(|error| format!("Не удалось прочитать файл: {error}"))?;
    if !meta.is_file() || meta.len() > MAX_TEXTURE_BYTES {
        return Err("Файл должен быть PNG не больше 1 МБ".into());
    }
    // take() bounds the read itself, so a file that grows past the
    // metadata check still cannot exhaust memory on this thread.
    let bytes =
        read_bounded(file).map_err(|error| format!("Не удалось прочитать файл: {error}"))?;
    if bytes.len() as u64 > MAX_TEXTURE_BYTES || !bytes.starts_with(PNG_MAGIC) {
        return Err("Файл должен быть PNG не больше 1 МБ".into());
    }
    Ok(Some(bytes))
}

/// Mirror the Tauri preview: local PNGs override the bundled fallback and the
/// slim toggle selects the arm model. Visibility transitions only; the key
/// check keeps steady state free of file IO.
pub fn sync_source(app: &AppWindow, host: &Rc<PreviewHost>) {
    let key = (
        app.get_skin_path().to_string(),
        app.get_cape_path().to_string(),
        app.get_slim(),
    );
    if host.source_matches(&key) && !host.failed() {
        // Validation failures never stage, so a stale error outlives the bad
        // path: clear it once the operator reverts to the staged source.
        if !app.get_skin_error().is_empty() {
            app.set_skin_error("".into());
            app.set_skin_caption(if key.0.is_empty() && key.1.is_empty() {
                "ВАШ СКИН".into()
            } else {
                LOCAL_CAPTION.into()
            });
        }
        return;
    }
    let staged = match (texture_bytes(&key.0), texture_bytes(&key.1)) {
        (Ok(skin), Ok(cape)) => {
            let mut source = default_source(key.2);
            if let Some(bytes) = skin {
                source.skin = bytes;
            }
            if let Some(bytes) = cape {
                source.cape = Some(bytes);
            }
            source
        }
        (Err(error), _) | (_, Err(error)) => {
            app.set_skin_error(error.into());
            return;
        }
    };
    host.set_source(key, staged);
    app.set_skin_error("".into());
    if !app.get_skin_path().is_empty() || !app.get_cape_path().is_empty() {
        app.set_skin_caption(LOCAL_CAPTION.into());
    } else {
        app.set_skin_caption("ВАШ СКИН".into());
    }
}

pub fn sync_visibility(app: &AppWindow, _host: &Rc<PreviewHost>) {
    app.invoke_skin_preview_sync();
}

pub fn attach(app: &AppWindow, activity: &Rc<Activity>) -> std::io::Result<Rc<PreviewHost>> {
    let weak = app.as_weak();
    let notify = Arc::new(move || {
        if let Some(app) = weak.upgrade() {
            app.invoke_skin_frame_ready();
        }
    });
    let weak = app.as_weak();
    let poll_activity = activity.clone();
    let host = PreviewHost::spawn(HostOptions {
        // Match the existing still-capture flag without changing live preview pacing.
        clock: if std::env::args().any(|arg| arg == "--freeze-motion") {
            ClockMode::Fixed(0.0)
        } else {
            ClockMode::Live(0.0)
        },
        frame_hook: None,
        sink: FrameSink {
            set_frame: Box::new({
                let weak = app.as_weak();
                move |frame| {
                    if let Some(app) = weak.upgrade() {
                        app.set_skin_frame(frame);
                    }
                }
            }),
            set_error: Box::new({
                let weak = app.as_weak();
                move |error: slint::SharedString| {
                    if let Some(app) = weak.upgrade() {
                        app.set_skin_error(error);
                    }
                }
            }),
            notify_frame: notify,
            poll_window: Box::new(move || {
                let Some(app) = weak.upgrade() else {
                    return (false, 1.0);
                };
                let live = poll_activity.live(&app) && skin_visible(&app);
                (live, app.window().scale_factor())
            }),
        },
    })?;
    let controller = Rc::new(SkinController {
        host: host.clone(),
        live: RefCell::new(None),
        debounce: Timer::default(),
    });
    let sync_weak = app.as_weak();
    app.on_skin_preview_sync(move || {
        if let Some(app) = sync_weak.upgrade() {
            controller.handle_sync(&app);
        }
    });
    let this = host.clone();
    let activity = activity.clone();
    app.window().on_winit_window_event(move |_, event| {
        activity.event(event);
        if matches!(event, WinitEvent::Focused(false)) {
            this.on_drag_state(false);
        }
        this.refresh();
        EventResult::Propagate
    });
    Ok(host)
}

pub fn arm_self_check(app: &AppWindow, host: &Rc<PreviewHost>) {
    let weak = app.as_weak();
    let check = host.clone();
    host.set_on_ready(move || {
        let weak = weak.clone();
        let check = check.clone();
        Timer::single_shot(Duration::from_millis(300), move || {
            let Some(app) = weak.upgrade() else { return };
            // Synthetic drag over the preview center (220x300 at panel right).
            for event in [
                WindowEvent::PointerPressed {
                    position: slint::LogicalPosition::new(1121., 366.),
                    button: slint::platform::PointerEventButton::Left,
                },
                WindowEvent::PointerMoved {
                    position: slint::LogicalPosition::new(1166., 391.),
                },
                WindowEvent::PointerReleased {
                    position: slint::LogicalPosition::new(1166., 391.),
                    button: slint::platform::PointerEventButton::Left,
                },
            ] {
                app.window().dispatch_event(event);
            }
            assert!(
                check.drag_moves() > 0,
                "synthetic drag reaches the skin preview"
            );
            println!("SELF_CHECK drag dispatched");
            // Leaving the Skin tab (and the settings screen) must freeze pacing.
            app.set_active(0);
            sync_visibility(&app, &check);
            let count = check.delivered();
            let time = check.active_time();
            let frozen = check.clone();
            let frozen_app = app.as_weak();
            Timer::single_shot(Duration::from_millis(600), move || {
                assert_eq!(frozen.delivered(), count, "no hidden frame delivery");
                assert_eq!(frozen.active_time(), time, "hidden active time must freeze");
                assert!(!frozen.timer_running(), "hidden frame timer stopped");
                let app = frozen_app.upgrade().unwrap();
                app.set_active(3);
                app.set_settings_tab(3);
                sync_visibility(&app, &frozen);
                let done = frozen.clone();
                Timer::single_shot(Duration::from_millis(300), move || {
                    assert!(done.delivered() > count, "visible animation resumes");
                    // Slint marks expired one-shots stopped before dispatching their callback batch.
                    assert!(done.active_time() > time, "visible animation clock resumes");
                    println!("SELF_CHECK passed: drag, hidden timer/frame/clock, resume");
                });
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_activity::Activity;

    fn init_testing_backend() {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            i_slint_backend_testing::init_no_event_loop();
        });
    }

    fn temp_file(name: &str, bytes: &[u8]) -> String {
        let dir = std::env::temp_dir().join(format!("skin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join(format!("{name}.bin"));
        std::fs::write(&path, bytes).expect("temp file");
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn stale_validation_error_clears_on_revert() {
        init_testing_backend();
        let app = AppWindow::new().expect("test window");
        let activity = Rc::new(Activity::default());
        let host = attach(&app, &activity).expect("skin host");
        app.set_skin_path("".into());
        app.set_cape_path("".into());
        app.set_slim(false);
        app.set_skin_error("".into());
        app.set_skin_caption("ВАШ СКИН".into());
        sync_source(&app, &host);
        assert!(app.get_skin_error().is_empty(), "clean stage has no error");

        let bad = temp_file("stale", b"not a png");
        app.set_skin_path(bad.clone().into());
        sync_source(&app, &host);
        assert!(
            !app.get_skin_error().is_empty(),
            "invalid PNG sets an error"
        );

        app.set_skin_path("".into());
        sync_source(&app, &host);
        assert!(
            app.get_skin_error().is_empty(),
            "reverting to the staged source clears the stale error"
        );
        assert_eq!(
            app.get_skin_caption(),
            slint::SharedString::from("ВАШ СКИН")
        );
        let _ = std::fs::remove_file(&bad);
        host.shutdown();
    }

    #[test]
    fn bounded_read_stops_after_limit_plus_one() {
        struct Endless;
        impl std::io::Read for Endless {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                buf.fill(0xAB);
                Ok(buf.len())
            }
        }
        // An unbounded read_to_end on this stream never terminates; the
        // bounded helper must stop at the cap instead of loading it all.
        let bytes = read_bounded(Endless).expect("bounded read terminates");
        assert_eq!(bytes.len() as u64, MAX_TEXTURE_BYTES + 1);
    }

    #[cfg(unix)]
    #[test]
    fn fifo_rejected_without_opening() {
        let dir = std::env::temp_dir().join(format!("bf-skin-fifo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fifo = dir.join("skin.png");
        let _ = std::fs::remove_file(&fifo);
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap();
        assert!(status.success());
        // Returns instead of blocking: opened non-blocking, then refused as not a regular file.
        assert!(texture_bytes(fifo.to_str().unwrap()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn oversize_file_rejected() {
        let mut big = PNG_MAGIC.to_vec();
        big.resize(MAX_TEXTURE_BYTES as usize + 16, 0);
        let path = temp_file("oversize", &big);
        let error = texture_bytes(&path).expect_err("over-limit file is rejected");
        assert!(error.contains("1 МБ"), "size message, got: {error}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn small_png_and_empty_accepted() {
        let mut small = PNG_MAGIC.to_vec();
        small.extend_from_slice(&[0u8; 64]);
        let path = temp_file("small", &small);
        assert!(texture_bytes(&path).expect("small PNG loads").is_some());
        assert!(texture_bytes("").expect("empty path").is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn caption_matches_tauri_source_rules() {
        let mk = |skin: Option<&str>, source: &'static str| launcher_net::skins::SkinPreview {
            skin: skin.map(str::to_owned),
            cape: None,
            slim: false,
            source,
        };
        assert_eq!(caption_for_preview("", &mk(None, "none")), "СКИН НЕ НАЙДЕН");
        assert_eq!(
            caption_for_preview(
                "/tmp/s.png",
                &mk(Some("data:image/png;base64,AA=="), "none")
            ),
            "ПРЕДПРОСМОТР ФАЙЛА"
        );
        assert_eq!(
            caption_for_preview("", &mk(Some("data:image/png;base64,AA=="), "none")),
            "СТАНДАРТНЫЙ СКИН"
        );
        assert_eq!(
            caption_for_preview("", &mk(Some("data:image/png;base64,AA=="), "mojang")),
            "ЛИЦЕНЗИОННЫЙ СКИН"
        );
        assert_eq!(
            caption_for_preview("", &mk(Some("data:image/png;base64,AA=="), "custom")),
            "ВАШ СКИН"
        );
    }
}
