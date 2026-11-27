use std::cell::{Cell, RefCell};

use slint::{
    ComponentHandle,
    winit_030::{WinitWindowAccessor, winit::event::WindowEvent},
};

use crate::{AppWindow, WindowActivity};

#[derive(Default)]
pub struct Activity {
    occluded: Cell<bool>,
    zero_size: Cell<bool>,
    pub redraws: Cell<u64>,
    on_focus: RefCell<Option<Box<dyn Fn()>>>,
}

impl Activity {
    pub fn set_on_focus(&self, cb: impl Fn() + 'static) {
        *self.on_focus.borrow_mut() = Some(Box::new(cb));
    }

    pub fn event(&self, event: &WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => self.redraws.set(self.redraws.get() + 1),
            WindowEvent::Occluded(value) => self.occluded.set(*value),
            WindowEvent::Resized(size) => self.zero_size.set(size.width == 0 || size.height == 0),
            WindowEvent::Focused(true) => {
                if let Some(cb) = self.on_focus.borrow().as_ref() {
                    cb();
                }
            }
            _ => {}
        }
    }

    pub fn live(&self, app: &AppWindow) -> bool {
        // Re-poll on events and preview requests: X11 may not emit Occluded on minimize.
        // No focus gate: an unfocused window is still visible (Tauri keeps its
        // pulse too), and headless Xvfb harnesses never report focus, so a
        // focus gate would freeze Layer-2 captures and fake bench numbers.
        let minimized = app
            .window()
            .with_winit_window(|window| window.is_minimized())
            .flatten()
            .unwrap_or(false);
        let live = app.window().is_visible()
            && !minimized
            && !self.occluded.get()
            && !self.zero_size.get();
        app.global::<WindowActivity>().set_live(live);
        live
    }
}
