pub mod auth;
pub mod avatars;
pub mod backend;
pub mod bridge;
pub mod capture;
pub mod deep_link;
pub mod e2e;
pub mod fixtures;
pub mod idle_self_check;
mod main_e2e;
pub mod main_ops;
pub mod pickers;
pub mod runtime;
pub mod settings_e2e;
pub mod settings_fixtures;
pub mod settings_tabs;
pub mod single_instance;
pub mod skin;
pub mod skin_upload;
pub mod stats;
pub mod update;
pub mod window_activity;

pub use skin3d::slint::PreviewHost;

use std::rc::Rc;

use launcher_app::status_ops;
use slint::ComponentHandle;

slint::include_modules!();

/// Support link target, mirroring `DISCORD_URL` in src/app/constants.ts:
/// a runtime override wins, then the build-time Vite value, then the invite
/// fallback. Runtime first follows the `pack_base_url` precedent so tests
/// and dev shells can point elsewhere without rebuilding.
pub fn support_url() -> String {
    std::env::var("VITE_DISCORD_URL")
        .ok()
        .filter(|url| !url.is_empty())
        .or_else(|| option_env!("VITE_DISCORD_URL").map(str::to_string))
        .unwrap_or_else(|| "https://discord.gg/cAUcpeCVDM".to_owned())
}

/// Tauri `CloseRequested` parity (src-tauri/src/lib.rs): while the game is
/// busy the window hides instead of quitting; the launch exit thread always
/// runs `restore` (core ops::launch), so the window comes back on game exit.
/// `main-data.phase` mirrors `GameState` through the bridge; `launching`
/// covers the click-to-event gap before the first status arrives.
pub fn hide_instead_of_quit(app: &AppWindow) -> bool {
    let data = app.get_main_data();
    // The update screen launches on its own (update_data.launching) before any
    // main-screen status arrives, so both screens count as busy.
    if data.phase == "idle" && !data.launching && !app.get_update_data().launching {
        return false;
    }
    let _ = app.hide();
    true
}

/// Build the app window and wire every callback exactly as the shipped binary
/// does, so headless integration tests drive the real UI tree (same code path
/// as production; only the backend and the event loop differ).
pub fn wire_app(
    app: &AppWindow,
    backend: backend::Backend,
) -> std::io::Result<(Rc<PreviewHost>, Rc<window_activity::Activity>)> {
    fixtures::apply(app);
    let weak = app.as_weak();
    app.on_minimize(move || {
        if let Some(app) = weak.upgrade() {
            app.window().set_minimized(true);
        }
    });
    let weak = app.as_weak();
    app.on_maximize(move || {
        if let Some(app) = weak.upgrade() {
            app.window().set_maximized(!app.window().is_maximized());
        }
    });
    let weak = app.as_weak();
    app.on_request_close(move || {
        if let Some(app) = weak.upgrade()
            && hide_instead_of_quit(&app)
        {
            return;
        }
        if let Err(error) = slint::quit_event_loop() {
            eprintln!("Cannot close launcher: {error}");
        }
    });
    // OS-level close (Alt+F4 / window manager): same guard as the chrome
    // button, so an idle close still quits instead of lingering hidden.
    let weak = app.as_weak();
    app.window().on_close_requested(move || {
        if weak.upgrade().is_some_and(|app| hide_instead_of_quit(&app)) {
            slint::CloseRequestResponse::HideWindow
        } else {
            let _ = slint::quit_event_loop();
            slint::CloseRequestResponse::HideWindow
        }
    });
    let weak = app.as_weak();
    let activity = Rc::new(window_activity::Activity::default());
    let skin_host = skin::attach(app, &activity)?;
    let navigate_host = skin_host.clone();
    app.on_navigate(move |index| {
        if let Some(app) = weak.upgrade() {
            app.set_active(index);
            skin::sync_visibility(&app, &navigate_host);
            if index == 2 {
                app.invoke_stats_shown();
            }
        }
    });
    let weak = app.as_weak();
    let update_host = skin_host.clone();
    app.on_launcher_update(move || {
        if let Some(app) = weak.upgrade() {
            app.set_active(3);
            skin::sync_visibility(&app, &update_host);
        }
    });
    // Tauri Shell opens DISCORD_URL through the open_url command; the
    // native footer calls the same shared helper (stats open-site does too).
    app.on_support(|| {
        if let Err(error) = status_ops::open_url(&support_url()) {
            eprintln!("Cannot open support link: {error}");
        }
    });
    let weak = app.as_weak();
    let tab_host = skin_host.clone();
    app.on_settings_tab_changed(move |index| {
        if let Some(app) = weak.upgrade() {
            app.set_settings_tab(index);
            skin::sync_visibility(&app, &tab_host);
        }
    });
    let slim_host = skin_host.clone();
    let slim_weak = app.as_weak();
    app.on_toggle_slim(move |value| {
        if let Some(app) = slim_weak.upgrade() {
            app.set_slim(value);
            skin::sync_visibility(&app, &slim_host);
        }
    });
    match &backend {
        backend::Backend::Fixtures => {
            app.on_browse_skin(|| eprintln!("POC settings: skin browse is a stub (no backend)"));
            app.on_browse_cape(|| eprintln!("POC settings: cape browse is a stub (no backend)"));
        }
        backend::Backend::Core(_) => {
            // Same PNG filter as Tauri `handleBrowsePng`; the picked file
            // refreshes the 3D preview at once like the debounced Tauri effect.
            let skin_host = skin_host.clone();
            let cape_host = skin_host.clone();
            let weak = app.as_weak();
            app.on_browse_skin(move || {
                if let Some(app) = weak.upgrade()
                    && let Some(path) = crate::pickers::pick_png()
                {
                    app.set_skin_path(path.into());
                    app.set_skin_message("".into());
                    crate::skin::sync_visibility(&app, &skin_host);
                }
            });
            let weak = app.as_weak();
            app.on_browse_cape(move || {
                if let Some(app) = weak.upgrade()
                    && let Some(path) = crate::pickers::pick_png()
                {
                    app.set_cape_path(path.into());
                    app.set_skin_message("".into());
                    crate::skin::sync_visibility(&app, &cape_host);
                }
            });
        }
    }
    app.on_upload_skin(|| eprintln!("POC settings: skin upload is a stub (no backend)"));
    app.on_settings_reset(|| eprintln!("POC settings: reset is a stub (no backend)"));
    app.on_settings_save(|| eprintln!("POC settings: save is a stub (no backend)"));
    let frame_host = skin_host.clone();
    app.on_skin_frame_ready(move || frame_host.poll_frame());
    let drag_host = skin_host.clone();
    app.on_skin_drag_state(move |active| drag_host.on_drag_state(active));
    let drag_by_host = skin_host.clone();
    app.on_skin_drag_by(move |dx, dy| drag_by_host.on_drag_by(dx, dy));
    app.global::<SelectSearch>()
        .on_find(|options, search| fixtures::find_option(&options, &search));
    app.global::<MainText>().on_launch_sub(main_ops::launch_sub);
    let weak = app.as_weak();
    app.on_deploy(move || {
        if let Some(app) = weak.upgrade() {
            let mut data = app.get_main_data();
            if data.needs_setup {
                app.set_room_message(
                    "POC: подготовка сборки (UpdateScreen вне области этого экрана).".into(),
                );
            } else {
                data.launching = true;
                data.phase = "launching".into();
                app.set_main_data(data);
            }
        }
    });
    let weak = app.as_weak();
    app.on_verify(move || {
        if let Some(app) = weak.upgrade() {
            let mut data = app.get_main_data();
            data.checking = true;
            app.set_main_data(data);
            let weak = app.as_weak();
            slint::Timer::single_shot(std::time::Duration::from_secs(1), move || {
                if let Some(app) = weak.upgrade() {
                    let mut data = app.get_main_data();
                    data.checking = false;
                    app.set_main_data(data);
                }
            });
        }
    });
    let weak = app.as_weak();
    app.on_join(move |id| {
        if let Some(app) = weak.upgrade() {
            app.set_room_message(
                if app.get_main_data().phase == "running" {
                    "Запрос передан игре. Результат входа появится в игровом меню."
                } else if id == "lobby" {
                    "После подготовки игра подключится к лобби."
                } else {
                    "После подготовки игра подключится к выбранному матчу."
                }
                .into(),
            );
            if app.get_main_data().phase != "running" {
                app.invoke_deploy();
            }
        }
    });
    backend.start(app);
    Ok((skin_host, activity))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `std::env` is process-global while libtest runs threads in one
    /// process: serialize the override test so no other test can observe it.
    fn env_serial() -> std::sync::MutexGuard<'static, ()> {
        static SERIAL: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        SERIAL
            .get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn support_url_matches_the_tauri_discord_fallback() {
        let _serial = env_serial();
        unsafe { std::env::remove_var("VITE_DISCORD_URL") };
        // option_env! is baked at compile time; a local build without the
        // Vite var falls back to the same invite the Tauri shell links.
        if option_env!("VITE_DISCORD_URL").is_none() {
            assert_eq!(support_url(), "https://discord.gg/cAUcpeCVDM");
        }
        // The shared allowlist accepts whatever the URL resolves to, so the
        // footer action can never hand open_url a refused scheme.
        assert!(status_ops::validate_open_url(&support_url()).is_ok());
        unsafe {
            std::env::set_var("VITE_DISCORD_URL", "https://example.com/support");
        }
        assert_eq!(support_url(), "https://example.com/support");
        unsafe { std::env::remove_var("VITE_DISCORD_URL") };
    }

    #[test]
    fn busy_close_hides_instead_of_quitting() {
        i_slint_backend_testing::init_no_event_loop();
        let app = AppWindow::new().unwrap();
        app.show().unwrap();
        // Idle main: the close path quits, the window stays visible.
        let mut data = app.get_main_data();
        data.phase = "idle".into();
        data.launching = false;
        app.set_main_data(data);
        assert!(!hide_instead_of_quit(&app));
        assert!(app.window().is_visible());
        // Running game: the close is absorbed by hiding, like Tauri.
        let mut data = app.get_main_data();
        data.phase = "running".into();
        app.set_main_data(data);
        assert!(hide_instead_of_quit(&app));
        assert!(!app.window().is_visible());
        // Deploy clicked but no game status yet: still hides (Tauri already
        // began the session; only the event has not arrived).
        app.show().unwrap();
        let mut data = app.get_main_data();
        data.phase = "idle".into();
        data.launching = true;
        app.set_main_data(data);
        assert!(hide_instead_of_quit(&app));
        assert!(!app.window().is_visible());
    }
}
