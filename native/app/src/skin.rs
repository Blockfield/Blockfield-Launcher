use std::{rc::Rc, sync::Arc, time::Duration};

use skin3d::slint::{ClockMode, FrameSink, HostOptions, PreviewHost, default_source};
use slint::{
    ComponentHandle, Timer,
    platform::WindowEvent,
    winit_030::{EventResult, WinitWindowAccessor, winit::event::WindowEvent as WinitEvent},
};

use crate::{AppWindow, window_activity::Activity};

const MAX_TEXTURE_BYTES: u64 = 1024 * 1024;
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
const LOCAL_CAPTION: &str = "ПРЕДПРОСМОТР ФАЙЛА";

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
    let too_big = std::fs::metadata(path)
        .map(|meta| meta.len() > MAX_TEXTURE_BYTES)
        .map_err(|error| format!("Не удалось прочитать файл: {error}"))?;
    if too_big {
        return Err("Файл должен быть PNG не больше 1 МБ".into());
    }
    let file =
        std::fs::File::open(path).map_err(|error| format!("Не удалось прочитать файл: {error}"))?;
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

pub fn sync_visibility(app: &AppWindow, host: &Rc<PreviewHost>) {
    sync_source(app, host);
    host.set_tab_visible(skin_visible(app));
    host.refresh();
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
}
