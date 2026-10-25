use std::cell::Cell;

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
}

impl Activity {
    pub fn event(&self, event: &WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => self.redraws.set(self.redraws.get() + 1),
            WindowEvent::Occluded(value) => self.occluded.set(*value),
            WindowEvent::Resized(size) => self.zero_size.set(size.width == 0 || size.height == 0),
            _ => {}
        }
    }

    pub fn live(&self, app: &AppWindow) -> bool {
        // Re-poll on events and preview requests: X11 may not emit Occluded on minimize.
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
