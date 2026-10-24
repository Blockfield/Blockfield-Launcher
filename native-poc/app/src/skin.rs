use std::{cell::Cell, rc::Rc, sync::Arc, time::Duration};

use skin3d::slint::{ClockMode, FrameSink, HostOptions, PreviewHost};
use slint::{
    ComponentHandle, Timer,
    platform::WindowEvent,
    winit_030::{EventResult, WinitWindowAccessor, winit::event::WindowEvent as WinitEvent},
};

use crate::AppWindow;

/// The preview is live only on the Skin tab of the settings screen.
pub fn skin_visible(app: &AppWindow) -> bool {
    app.get_active() == 3 && app.get_settings_tab() == 3
}

pub fn sync_visibility(app: &AppWindow, host: &Rc<PreviewHost>) {
    host.set_tab_visible(skin_visible(app));
    host.refresh();
}

pub fn attach(app: &AppWindow) -> std::io::Result<Rc<PreviewHost>> {
    let occluded = Rc::new(Cell::new(false));
    let nonzero = Rc::new(Cell::new(true));
    let weak = app.as_weak();
    let notify = Arc::new(move || {
        if let Some(app) = weak.upgrade() {
            app.invoke_skin_frame_ready();
        }
    });
    let weak = app.as_weak();
    let poll_occluded = occluded.clone();
    let poll_nonzero = nonzero.clone();
    let host = PreviewHost::spawn(HostOptions {
        clock: ClockMode::Live(0.0),
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
                let minimized = app
                    .window()
                    .with_winit_window(|window| window.is_minimized())
                    .flatten()
                    .unwrap_or(false);
                let live = app.window().is_visible()
                    && !minimized
                    && !poll_occluded.get()
                    && poll_nonzero.get()
                    && skin_visible(&app);
                (live, app.window().scale_factor())
            }),
        },
    })?;
    let this = host.clone();
    app.window().on_winit_window_event(move |_, event| {
        match event {
            WinitEvent::Occluded(occluded_event) => occluded.set(*occluded_event),
            WinitEvent::Resized(size) => nonzero.set(size.width > 0 && size.height > 0),
            WinitEvent::Focused(false) => this.on_drag_state(false),
            _ => {}
        }
        // Re-poll minimized state here: X11 does not always emit occlusion.
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
                    assert!(done.timer_running(), "visible frame timer runs");
                    println!("SELF_CHECK passed: drag, hidden timer/frame/clock, resume");
                });
            });
        });
    });
}
