use blockfield_native::{AppWindow, SettingsForm, backend::Backend, settings_fixtures, wire_app};
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::{ComponentHandle, platform::PointerEventButton};

fn click(app: &AppWindow, label: &str) -> ElementHandle {
    let element = ElementHandle::find_by_accessible_label(app, label)
        .find(|e| e.accessible_role() != Some(AccessibleRole::Text))
        .unwrap_or_else(|| panic!("control {label} exists"));
    element.invoke_accessible_default_action();
    element
}

fn press(app: &AppWindow, key: slint::platform::Key) {
    let text: slint::SharedString = char::from(key).into();
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.clone() });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased { text });
}

#[test]
fn settings_controls_commit_reset_validate_and_keep_edits_across_tabs() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    let (_host, _activity) = wire_app(&app, Backend::Fixtures).unwrap();
    settings_fixtures::apply(&app, "settings-general", "default").unwrap();
    app.show().unwrap();
    let form = app.global::<SettingsForm>();
    assert!(!form.get_dirty());
    click(&app, "Показывать активность в Discord");
    assert!(form.get_data().discord_presence);
    assert!(form.get_dirty());
    click(&app, "Скрывать лаунчер во время игры");
    assert!(form.get_data().hide_while_playing);
    click(&app, "СМЕНИТЬ ПАРОЛЬ");
    assert!(form.get_password_open());
    form.set_password("short".into());
    form.invoke_change_password();
    assert_eq!(
        form.get_account_error(),
        "Пароль слишком короткий — нужно не меньше 8 символов."
    );
    form.set_password("fixture-password".into());
    form.invoke_change_password();
    assert!(form.get_password_changed());
    assert!(form.get_password().is_empty());
    assert!(!form.get_password_open());
    click(&app, "JAVA И ПАМЯТЬ");
    assert!(
        form.get_data().discord_presence,
        "tab changes keep unsaved edits"
    );
    click(&app, "8G");
    assert_eq!(form.get_data().ram, 8);
    click(&app, "Уменьшить память");
    assert_eq!(form.get_data().ram, 7);
    click(&app, "Увеличить память");
    assert_eq!(form.get_data().ram, 8);
    let ram = ElementHandle::find_by_accessible_label(&app, "Память, ГБ")
        .next()
        .unwrap();
    ram.mock_single_click(PointerEventButton::Left);
    ram.set_accessible_value("99");
    assert_eq!(form.get_data().ram, 8, "typing does not commit");
    press(&app, slint::platform::Key::Return);
    assert_eq!(
        form.get_data().ram,
        12,
        "Enter blurs and clamps the number box"
    );
    form.invoke_commit_ram("".into());
    assert_eq!(form.get_data().ram, 12, "empty box preserves RAM");
    form.invoke_commit_ram("2.5".into());
    assert_eq!(form.get_data().ram, 3, "rounding matches JS Math.round");
    form.invoke_commit_ram("NaN".into());
    assert_eq!(form.get_data().ram, 2);
    click(&app, "КОМАНДЫ");
    let input = ElementHandle::find_by_accessible_label(&app, "ПЕРЕД ЗАПУСКОМ")
        .find(|e| e.accessible_role() != Some(AccessibleRole::Text))
        .unwrap();
    input.set_accessible_value("echo one\necho two");
    assert_eq!(form.get_data().pre_launch, "echo one\necho two");
    assert!(form.get_dirty());
    form.invoke_save();
    assert!(!form.get_dirty());
    assert!(form.get_saved());
    let mut data = form.get_data();
    data.java_path = "/missing/java".into();
    form.set_data(data);
    form.invoke_edited();
    form.invoke_save();
    assert!(form.get_dirty(), "failed save preserves edits");
    assert!(form.get_error().contains("Java не найден"));
    assert_eq!(
        form.get_raw_error(),
        "Selected Java executable does not exist"
    );
    form.invoke_reset();
    assert!(form.get_data().java_path.is_empty());
    assert_eq!(
        form.get_data().pre_launch,
        "echo one\necho two",
        "reset restores last saved commands"
    );
    assert!(!form.get_dirty());
    // Mouse focus enters the one tab stop; arrows wrap and Home/End select.
    let tab = ElementHandle::find_by_accessible_label(&app, "КОМАНДЫ")
        .find(|e| e.accessible_role() == Some(AccessibleRole::Tab))
        .unwrap();
    tab.mock_single_click(PointerEventButton::Left);
    for (key, expected) in [
        (slint::platform::Key::End, 4),
        (slint::platform::Key::RightArrow, 0),
        (slint::platform::Key::LeftArrow, 4),
        (slint::platform::Key::Home, 0),
    ] {
        press(&app, key);
        assert_eq!(app.get_settings_tab(), expected);
    }
    let mut data = form.get_data();
    data.max_ram = 1;
    form.set_data(data);
    form.invoke_edited();
    form.invoke_save();
    assert_eq!(
        form.get_error(),
        "Недостаточно памяти: для игры нужно выделить минимум 2 ГБ."
    );
    println!("PASS settings: keyboard, RAM, account, toggles, commands, save/reset/error");
}
