pub mod backend;
pub mod bridge;
pub mod capture;
pub mod e2e;
pub mod fixtures;
pub mod idle_self_check;
pub mod runtime;
pub mod skin;
pub mod window_activity;

pub use skin3d::slint::PreviewHost;

use std::rc::Rc;

use slint::ComponentHandle;

slint::include_modules!();

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
    app.on_request_close(|| {
        if let Err(error) = slint::quit_event_loop() {
            eprintln!("Cannot close launcher: {error}");
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
    app.on_support(|| eprintln!("POC support action: https://discord.gg/cAUcpeCVDM"));
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
    app.on_browse_skin(|| eprintln!("POC settings: skin browse is a stub (no backend)"));
    app.on_browse_cape(|| eprintln!("POC settings: cape browse is a stub (no backend)"));
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
