use std::{cell::Cell, path::PathBuf, rc::Rc, sync::Arc, time::Duration};

use skin3d::slint::{ClockMode, FrameSink, HostOptions, PreviewHost};
use slint::winit_030::{EventResult, WinitWindowAccessor, winit::event::WindowEvent};
use slint::{ComponentHandle, SharedString, Timer};

slint::slint! { export { PreviewWindow } from "slint_preview/preview.slint"; }

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut capture_dir: Option<PathBuf> = None;
    let mut hidden = false;
    let mut self_check = false;
    let mut start_time = 0.0_f64;
    let mut idle_seconds = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--capture-dir" => {
                capture_dir = Some(PathBuf::from(args.next().ok_or("capture path required")?))
            }
            "--hidden" => hidden = true,
            "--self-check" => self_check = true,
            "--start-time" => start_time = args.next().ok_or("start time required")?.parse()?,
            "--idle-seconds" => {
                idle_seconds = Some(args.next().ok_or("duration required")?.parse::<u64>()?)
            }
            _ => return Err(format!("Unknown argument: {arg}").into()),
        }
    }
    if !start_time.is_finite() || start_time < 0.0 {
        return Err("start time must be finite and nonnegative".into());
    }
    if let Some(directory) = &capture_dir {
        std::fs::create_dir_all(directory)?;
    }
    if self_check && (capture_dir.is_some() || hidden) {
        return Err("self-check requires visible live animation".into());
    }
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .select()?;
    let window = PreviewWindow::new()?;
    window.set_preview_visible(!hidden);
    let occluded = Rc::new(Cell::new(false));
    let nonzero_size = Rc::new(Cell::new(true));
    let weak = window.as_weak();
    let notify = Arc::new(move || {
        if let Some(window) = weak.upgrade() {
            window.invoke_frame_ready();
        }
    });
    let weak = window.as_weak();
    let poll_occluded = occluded.clone();
    let poll_nonzero = nonzero_size.clone();
    let host = PreviewHost::spawn(HostOptions {
        clock: match &capture_dir {
            Some(_) => ClockMode::Indexed(start_time),
            None => ClockMode::Live(start_time),
        },
        frame_hook: capture_dir.clone().map(|directory| {
            let hook_window = window.as_weak();
            let hook: Box<dyn Fn(u64, f64)> = Box::new(move |index, time| {
                if index.is_multiple_of(60) {
                    let window = hook_window.upgrade().expect("capture window");
                    let snapshot = window.window().take_snapshot().expect("Slint snapshot");
                    image::save_buffer(
                        directory.join(format!("frame-{index:03}.png")),
                        snapshot.as_bytes(),
                        snapshot.width(),
                        snapshot.height(),
                        image::ColorType::Rgba8,
                    )
                    .expect("save snapshot");
                    println!("CAPTURE index={index} time={time:.6}");
                }
                if index == 180 {
                    slint::quit_event_loop().expect("finish capture");
                }
            });
            hook
        }),
        sink: FrameSink {
            set_frame: Box::new({
                let weak = window.as_weak();
                move |frame| {
                    if let Some(window) = weak.upgrade() {
                        window.set_frame(frame);
                    }
                }
            }),
            set_error: Box::new({
                let weak = window.as_weak();
                move |error: SharedString| {
                    if let Some(window) = weak.upgrade() {
                        window.set_error(error);
                    }
                }
            }),
            notify_frame: notify,
            poll_window: Box::new(move || {
                let Some(window) = weak.upgrade() else {
                    return (false, 1.0);
                };
                let minimized = window
                    .window()
                    .with_winit_window(|window| window.is_minimized())
                    .flatten()
                    .unwrap_or(false);
                let live = window.window().is_visible()
                    && !poll_occluded.get()
                    && poll_nonzero.get()
                    && !minimized;
                (live, window.window().scale_factor())
            }),
        },
    })?;
    if self_check {
        let weak = window.as_weak();
        let check = host.clone();
        host.set_on_ready(move || start_example_self_check(weak.clone(), check.clone()));
    }
    let this = host.clone();
    let weak = window.as_weak();
    window.on_visibility_changed(move || {
        if let Some(window) = weak.upgrade() {
            this.set_tab_visible(window.get_preview_visible());
        }
    });
    let this = host.clone();
    window.on_drag_state(move |active| this.on_drag_state(active));
    let this = host.clone();
    window.on_drag_by(move |dx, dy| this.on_drag_by(dx, dy));
    let this = host.clone();
    window.window().on_winit_window_event(move |_, event| {
        match event {
            WindowEvent::Occluded(occluded_event) => occluded.set(*occluded_event),
            WindowEvent::Resized(size) => nonzero_size.set(size.width > 0 && size.height > 0),
            WindowEvent::Focused(false) => this.on_drag_state(false),
            _ => {}
        }
        this.refresh();
        EventResult::Propagate
    });
    if let Some(seconds) = idle_seconds {
        Timer::single_shot(Duration::from_secs(seconds), || {
            slint::quit_event_loop().unwrap();
        });
    }
    window.show()?;
    host.set_tab_visible(!hidden);
    if hidden {
        println!("READY pid={} hidden=true", std::process::id());
    }
    let outcome = slint::run_event_loop();
    let failed = host.failed();
    host.shutdown();
    outcome?;
    if failed {
        return Err("skin rendering failed".into());
    }
    Ok(())
}

fn start_example_self_check(window: slint::Weak<PreviewWindow>, host: Rc<PreviewHost>) {
    Timer::single_shot(Duration::from_millis(300), move || {
        let shown = window.upgrade().unwrap();
        use slint::{
            LogicalPosition,
            platform::{PointerEventButton, WindowEvent},
        };
        for event in [
            WindowEvent::PointerPressed {
                position: LogicalPosition::new(160., 140.),
                button: PointerEventButton::Left,
            },
            WindowEvent::PointerMoved {
                position: LogicalPosition::new(205., 165.),
            },
            WindowEvent::PointerReleased {
                position: LogicalPosition::new(205., 165.),
                button: PointerEventButton::Left,
            },
        ] {
            shown.window().dispatch_event(event);
        }
        assert!(
            host.drag_moves() > 0,
            "Slint pointer drag reaches worker controls"
        );
        println!("SELF_CHECK drag dispatched");
        shown.set_preview_visible(false);
        // Slint defers changed-handlers past the current callback, so sync the
        // host explicitly: the assertions below need the stopped state now.
        host.set_tab_visible(false);
        let count = host.delivered();
        let time = host.active_time();
        let resume = window.clone();
        let resume_host = host.clone();
        Timer::single_shot(Duration::from_millis(600), move || {
            assert_eq!(resume_host.delivered(), count, "no hidden frame delivery");
            assert_eq!(
                resume_host.active_time(),
                time,
                "hidden active time must freeze"
            );
            assert!(!resume_host.timer_running(), "hidden frame timer stopped");
            resume.upgrade().unwrap().set_preview_visible(true);
            resume_host.set_tab_visible(true);
            let done = resume_host.clone();
            Timer::single_shot(Duration::from_millis(300), move || {
                assert!(done.delivered() > count, "visible animation resumes");
                println!("SELF_CHECK passed: drag, hidden timer/frame/clock, resume");
                slint::quit_event_loop().unwrap();
            });
        });
    });
}
