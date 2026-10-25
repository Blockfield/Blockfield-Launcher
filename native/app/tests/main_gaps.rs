use std::{cell::RefCell, rc::Rc};

use blockfield_native::{AppWindow, MainMotion, backend::Backend, fixtures, wire_app};
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::{ComponentHandle, Model};

fn button(app: &AppWindow, label: &str) -> ElementHandle {
    ElementHandle::find_by_accessible_label(app, label)
        .find(|el| el.accessible_role() == Some(AccessibleRole::Button))
        .unwrap_or_else(|| panic!("missing button {label}"))
}

#[test]
fn main_pager_join_verify_and_status_contract() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    let (_host, _activity) = wire_app(&app, Backend::Fixtures).unwrap();
    app.global::<MainMotion>().set_enabled(false);
    fixtures::apply_main(&app, "main", "default").unwrap();
    app.show().unwrap();
    slint::platform::update_timers_and_animations();

    let previous = button(&app, "Предыдущий матч");
    let next = button(&app, "Следующий матч");
    assert_eq!(previous.accessible_enabled(), Some(false));
    previous.invoke_accessible_default_action();
    assert_eq!(app.get_room_index(), 0);
    next.invoke_accessible_default_action();
    assert_eq!(app.get_room_index(), 1);
    assert_eq!(next.accessible_enabled(), Some(false));
    next.invoke_accessible_default_action();
    assert_eq!(app.get_room_index(), 1);
    previous.invoke_accessible_default_action();
    assert_eq!(app.get_room_index(), 0);
    assert!(
        next.absolute_position().x < previous.absolute_position().x + 100.0,
        "pager uses intrinsic count width, without stretching the status box"
    );

    let calls = Rc::new(RefCell::new(Vec::new()));
    let joined = calls.clone();
    app.on_join(move |id| joined.borrow_mut().push(id.to_string()));
    for (scenario, state, match_enabled, lobby_enabled) in [
        ("default", "main", true, true),
        ("checking", "main", false, false),
        ("default", "error", false, false),
        ("busy", "main", false, false),
        ("default", "main-launching", false, false),
        ("running", "main", true, true),
        ("finishing", "main", false, false),
        ("closed", "main", false, true),
        // Tauri keeps room joining independent of profile-switch deploy guards.
        ("profile-changing", "main", true, true),
        ("profile-blocked", "main", true, true),
    ] {
        fixtures::apply_main(&app, state, scenario).unwrap();
        let room = app.get_rooms().row_data(0).unwrap();
        let join = button(&app, &format!("{}: {}", room.join, room.name));
        let lobby = button(&app, "Войти в лобби");
        assert_eq!(
            join.accessible_enabled(),
            Some(match_enabled),
            "{scenario}/{state}"
        );
        assert_eq!(
            lobby.accessible_enabled(),
            Some(lobby_enabled),
            "lobby {scenario}/{state}"
        );
        let before = calls.borrow().len();
        join.invoke_accessible_default_action();
        lobby.invoke_accessible_default_action();
        assert_eq!(
            calls.borrow().len() - before,
            usize::from(match_enabled) + usize::from(lobby_enabled)
        );
    }
    fixtures::apply_main(&app, "main", "default").unwrap();
    for phase in ["idle", "launching", "running", "finishing", "unknown"] {
        for checking in [false, true] {
            for checked in [false, true] {
                for launching in [false, true] {
                    for updating in [false, true] {
                        let mut data = app.get_main_data();
                        data.phase = phase.into();
                        data.checking = checking;
                        data.checked = checked;
                        data.launching = launching;
                        data.updating = updating;
                        app.set_main_data(data);
                        let enabled = !launching
                            && (phase == "running" || (!checking && checked && phase == "idle"));
                        assert_eq!(
                            button(&app, "Войти: ALPHA").accessible_enabled(),
                            Some(enabled && !updating),
                            "match {phase}/{checking}/{checked}/{launching}/{updating}"
                        );
                        assert_eq!(
                            button(&app, "Войти в лобби").accessible_enabled(),
                            Some(enabled),
                            "lobby {phase}/{checking}/{checked}/{launching}/{updating}"
                        );
                    }
                }
            }
        }
    }
    fixtures::apply_main(&app, "main", "default").unwrap();
    let mut data = app.get_main_data();
    data.updating = true;
    app.set_main_data(data);
    assert_eq!(
        button(&app, "Войти: ALPHA").accessible_enabled(),
        Some(false)
    );
    assert_eq!(
        button(&app, "Войти в лобби").accessible_enabled(),
        Some(true)
    );

    fixtures::apply_main(&app, "main", "default").unwrap();
    app.invoke_download_modpack();
    assert!(app.get_main_data().launching);
    assert_eq!(app.get_main_data().progress, 42);
    app.invoke_cancel_download();
    assert!(!app.get_main_data().launching);
    assert_eq!(app.get_main_data().progress, -1);
    button(&app, "ПРОВЕРИТЬ ФАЙЛЫ").invoke_accessible_default_action();
    assert!(app.get_main_data().checking);
    assert_eq!(
        button(&app, "ПРОВЕРКА...").accessible_enabled(),
        Some(false)
    );
    assert_eq!(
        button(&app, "Войти в лобби").accessible_enabled(),
        Some(false)
    );
    let mut data = app.get_main_data();
    data.launching = true;
    data.launch_status = "Я".repeat(70).into();
    data.progress = 42;
    app.set_main_data(data);
    assert!(
        ElementHandle::find_by_accessible_label(&app, &"Я".repeat(64))
            .next()
            .is_some(),
        "rendered status slices after appending progress"
    );
}
