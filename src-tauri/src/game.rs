// `GameState` moved to launcher-core (phase 2, no behaviour change); the window
// helpers stay in the Tauri adapter. Re-export keeps every `crate::game::…`
// call site unchanged.
pub use launcher_core::game::*;

use tauri::{AppHandle, Manager};

pub fn show_launcher(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn restore_hidden_launcher(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if handle
            .get_webview_window("main")
            .is_some_and(|window| !window.is_visible().unwrap_or(true))
        {
            show_launcher(&handle);
        }
    });
}
