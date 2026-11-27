//! Parity tests for F12 (Login friendly errors), F14 (voluntary logout),
//! F31 (settings lang normalisation), and visual/layout items 5, 6, 13, 14, 16.
use blockfield_native::{
    AppWindow, SettingsData, SettingsForm, auth, backend::Backend, bridge, fixtures, runtime,
    settings_fixtures, settings_tabs, wire_app,
};
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use launcher_app::LauncherEvent;
use launcher_core::config::LauncherConfig;
use slint::{ComponentHandle, platform::Key};

fn test_state() -> (
    tokio::runtime::Runtime,
    std::sync::Arc<launcher_app::LauncherAppState>,
) {
    let rt = runtime::new().unwrap();
    let state = std::sync::Arc::new(launcher_app::LauncherAppState {
        cancel_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        downloader: std::sync::Arc::new(launcher_net::download::Downloader::new(
            std::sync::Arc::new(|_| {}),
            std::sync::Arc::new(|_, _| {}),
            launcher_net::download::noop_guard_sink(),
        )),
        config: tokio::sync::RwLock::new(LauncherConfig::default()),
        app_data_dir: std::env::temp_dir().join(format!("bf-auth-unit-{}", std::process::id())),
        pack: tokio::sync::RwLock::new(None),
        game: std::sync::Arc::new(launcher_core::game::GameState::new(|_| {})),
        operation: tokio::sync::Mutex::new(()),
        events: std::sync::Arc::new(|_| {}),
    });
    (rt, state)
}

fn press_key(app: &AppWindow, key: Key) {
    let text: slint::SharedString = key.into();
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.clone() });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased { text });
}

fn type_char(app: &AppWindow, ch: &str) {
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: ch.into() });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased { text: ch.into() });
}

#[test]
fn f12_login_auth_error_is_friendly_russian() {
    crate::init_slint();
    let app = AppWindow::new().unwrap();
    let rt = runtime::new().unwrap();
    let state = std::sync::Arc::new(launcher_app::LauncherAppState {
        cancel_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        downloader: std::sync::Arc::new(launcher_net::download::Downloader::new(
            std::sync::Arc::new(|_| {}),
            std::sync::Arc::new(|_, _| {}),
            launcher_net::download::noop_guard_sink(),
        )),
        config: tokio::sync::RwLock::new(LauncherConfig::default()),
        app_data_dir: std::env::temp_dir().join(format!("bf-f12-{}", std::process::id())),
        pack: tokio::sync::RwLock::new(None),
        game: std::sync::Arc::new(launcher_core::game::GameState::new(|_| {})),
        operation: tokio::sync::Mutex::new(()),
        events: std::sync::Arc::new(|_| {}),
    });
    auth::on_auth_done(
        &app,
        &state,
        rt.handle(),
        Err("Drasl: Invalid credentials".into()),
    );
    assert_eq!(
        app.get_login_error().as_str(),
        "Неверный никнейм или пароль."
    );
    assert_eq!(
        app.get_login_raw_error().as_str(),
        "Drasl: Invalid credentials"
    );
    // Startup account_status error also uses friendly_error + raw spoiler and records first-run eligibility.
    auth::show_login_error(&app, "", "Drasl request failed: dns error");
    assert!(app.get_firstrun_pending());
    assert!(app.get_login_error().contains("skins.blockfield.pro"));
    assert_eq!(
        app.get_login_raw_error().as_str(),
        "Drasl request failed: dns error"
    );
    app.show().unwrap();
    assert!(
        ElementHandle::find_by_accessible_label(&app, "Подробности")
            .any(|e| e.accessible_role() == Some(AccessibleRole::Button)),
        "F12: LoginScreen FormError must expose collapsible Подробности button"
    );
}

#[test]
fn f24_firstrun_low_ram_is_not_clamped_upward_and_disables_continue() {
    crate::init_slint();
    let (dir, java, ram, max, presets) =
        auth::firstrun_defaults_from_response(&serde_json::json!({
            "gameDir": "/tmp/game",
            "javaPath": "/tmp/game/java/bin/java",
            "ramMb": 4096,
            "maxRamMb": 1024
        }));
    assert_eq!(dir, "/tmp/game");
    assert_eq!(java, "/tmp/game/java/bin/java");
    assert_eq!(ram, 1, "F24: sub-2GB max keeps Tauri min(max, ...) order");
    assert_eq!(max, 1, "F24: maxram < 2 must not be clamped upward to 2");
    assert!(presets.is_empty());

    let app = AppWindow::new().unwrap();
    let (_host, _activity) = wire_app(&app, Backend::Fixtures).unwrap();
    fixtures::apply_firstrun(&app).unwrap();
    app.set_firstrun_maxram(max);
    app.show().unwrap();
    let cont = ElementHandle::find_by_accessible_label(&app, "ПРОДОЛЖИТЬ")
        .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
        .expect("continue button");
    assert_eq!(
        cont.accessible_enabled(),
        Some(false),
        "F24: Continue must be disabled when maxram < 2"
    );
}

#[test]
fn f14_voluntary_logout_preserves_username_without_expired_error() {
    crate::init_slint();
    let app = AppWindow::new().unwrap();
    let (_host, _activity) = wire_app(&app, Backend::Fixtures).unwrap();
    app.set_username("NetherG".into());
    app.set_show_main(true);
    app.set_show_login(false);
    // A previous owner's session: role badge + Workshop must not leak.
    app.set_role("ВЛАДЕЛЕЦ".into());
    let mut data = app.get_main_data();
    data.can_workshop = true;
    data.profile_blocked = true;
    data.profile_error = "stale".into();
    app.set_main_data(data);

    app.global::<SettingsForm>().invoke_logout();

    assert!(app.get_show_login(), "logout opens LoginScreen");
    assert!(!app.get_show_main(), "logout hides MainScreen");
    assert_eq!(
        app.get_login_username().as_str(),
        "NetherG",
        "voluntary logout keeps stored username prefilled"
    );
    assert!(
        app.get_login_error().is_empty(),
        "voluntary logout must not show session-expired error, got: {}",
        app.get_login_error()
    );
    assert_eq!(app.get_role().as_str(), "ИГРОК", "logout resets role badge");
    assert!(
        !app.get_main_data().can_workshop,
        "logout revokes Workshop access"
    );
    assert!(
        !app.get_main_data().profile_blocked,
        "logout clears profile block"
    );
    assert!(
        app.get_main_data().profile_error.is_empty(),
        "logout clears profile error"
    );

    // Genuine session expiry still shows the expired message and preserves username.
    app.set_show_login(false);
    app.set_show_main(true);
    app.set_username("NetherG".into());
    app.set_login_username("".into());
    app.set_role("ВЛАДЕЛЕЦ".into());
    bridge::apply(&app, LauncherEvent::AccountExpired);
    assert!(app.get_show_login());
    assert_eq!(app.get_login_username().as_str(), "NetherG");
    assert_eq!(app.get_role().as_str(), "ИГРОК");
    assert_eq!(
        app.get_login_error().as_str(),
        "Сессия истекла — войдите снова."
    );
}

#[test]
fn f31_settings_normalize_corrupt_and_foreign_lang_to_ru() {
    crate::init_slint();
    let app = AppWindow::new().unwrap();
    settings_tabs::apply_response(
        &app,
        &serde_json::json!({
            "activeProfile": "game",
            "gameDir": "/tmp/game",
            "javaPath": "",
            "ramMb": 4096,
            "maxRamMb": 8192,
            "lang": "capture-motion:{\"drawCalls\":0,\"visibility\":\"visible\"}"
        }),
    );
    assert_eq!(
        app.global::<SettingsForm>().get_data().lang.as_str(),
        "ru",
        "apply_response must normalize corrupt lang to ru"
    );

    for raw_lang in [
        "capture-motion:{\"drawCalls\":0,\"visibility\":\"visible\"}",
        "en",
        "uk",
        "fr",
        "",
    ] {
        let data = SettingsData {
            game_dir: "/tmp/game".into(),
            ram: 4,
            max_ram: 8,
            lang: raw_lang.into(),
            ..Default::default()
        };
        let cfg = settings_tabs::config_from_form(&data).unwrap();
        assert_eq!(
            cfg.lang, "ru",
            "config_from_form must normalize lang {raw_lang:?} to ru"
        );
    }
}

#[test]
fn visual_password_alignment_and_login_firstrun_geometry_and_focus() {
    crate::init_slint();

    // Item 6: SettingsGeneral password input aligns with game-dir input at x≈326
    // and ends cleanly at Save without overlapping it.
    {
        let app = AppWindow::new().unwrap();
        let (_host, _activity) = wire_app(&app, Backend::Fixtures).unwrap();
        settings_fixtures::apply(&app, "settings-general", "default").unwrap();
        app.global::<SettingsForm>().set_password_open(true);
        app.show().unwrap();
        slint::platform::update_timers_and_animations();

        let dir_input = ElementHandle::find_by_accessible_label(&app, "ПАПКА ИГРЫ")
            .find(|e| e.accessible_role() == Some(AccessibleRole::TextInput))
            .expect("game dir input");
        let pass_input = ElementHandle::find_by_accessible_label(&app, "Новый пароль")
            .find(|e| e.accessible_role() == Some(AccessibleRole::TextInput))
            .expect("new password input");
        let save_btn = ElementHandle::find_by_accessible_label(&app, "СОХРАНИТЬ")
            .find(|e| {
                e.accessible_role() == Some(AccessibleRole::Button)
                    && e.absolute_position().y < 400.0
            })
            .expect("inline password save button");

        assert!(
            (pass_input.absolute_position().x - dir_input.absolute_position().x).abs() <= 1.0,
            "item 6: password input x ({}) must align with game dir input x ({})",
            pass_input.absolute_position().x,
            dir_input.absolute_position().x
        );
        let pass_right = pass_input.absolute_position().x + pass_input.size().width;
        let save_left = save_btn.absolute_position().x;
        assert!(
            pass_right <= save_left + 1.5,
            "item 6: password input right ({pass_right}) must not overlap Save left ({save_left})"
        );
    }

    // Items 13 & 14: Login fields start around x=625 (with icon column) and receive
    // initial focus: nickname when empty, password when nickname is saved.
    {
        let app = AppWindow::new().unwrap();
        let (_host, _activity) = wire_app(&app, Backend::Fixtures).unwrap();
        fixtures::apply_login(&app, "default").unwrap();
        app.show().unwrap();
        slint::platform::update_timers_and_animations();

        let name_input = ElementHandle::find_by_accessible_label(&app, "НИКНЕЙМ")
            .find(|e| e.accessible_role() == Some(AccessibleRole::TextInput))
            .expect("login username input");
        // TextInput is inset 12px inside the 625px field border => x ≈ 637.
        assert!(
            (name_input.absolute_position().x - 637.0).abs() <= 2.5,
            "item 13: login username TextInput x ({}) must sit inside x=625 field",
            name_input.absolute_position().x
        );
        type_char(&app, "a");
        assert_eq!(
            app.get_login_username().as_str(),
            "a",
            "item 14: empty login must focus username initially"
        );

        // Item 15: switch-mode link is vertically centered with the submit button.
        let link =
            ElementHandle::find_by_accessible_label(&app, "Нет аккаунта? Зарегистрироваться")
                .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
                .expect("switch-mode link");
        let submit = ElementHandle::find_by_accessible_label(&app, "ВОЙТИ")
            .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
            .expect("login submit button");
        let link_mid = link.absolute_position().y + link.size().height / 2.0;
        let submit_mid = submit.absolute_position().y + submit.size().height / 2.0;
        assert!(
            (link_mid - submit_mid).abs() <= 3.0,
            "item 15: switch link center ({link_mid}) must align with submit button center ({submit_mid})"
        );
        assert!(
            submit.size().width >= 115.0,
            "item 15: login button width ({}) must include leading arrow + padding",
            submit.size().width
        );
    }
    {
        let app = AppWindow::new().unwrap();
        let (_host, _activity) = wire_app(&app, Backend::Fixtures).unwrap();
        app.set_username("NetherG".into());
        app.set_show_main(true);
        app.global::<SettingsForm>().invoke_logout();
        app.show().unwrap();
        slint::platform::update_timers_and_animations();

        type_char(&app, "p");
        assert_eq!(
            app.get_login_password().as_str(),
            "p",
            "item 14: login with saved username must focus password initially"
        );
        assert_eq!(app.get_login_username().as_str(), "NetherG");
    }

    // Items 5 & 16: FirstRun hints wrap (>20px tall) and path inputs are editable at x≈625.
    {
        let app = AppWindow::new().unwrap();
        let (_host, _activity) = wire_app(&app, Backend::Fixtures).unwrap();
        fixtures::apply_firstrun(&app).unwrap();
        app.show().unwrap();
        slint::platform::update_timers_and_animations();

        let dir_hint = ElementHandle::find_by_accessible_label(
            &app,
            "Рекомендуемое значение по умолчанию — можно изменить.",
        )
        .next()
        .expect("firstrun game dir hint");
        assert!(
            dir_hint.size().height > 20.0,
            "item 5: firstrun game dir hint must wrap onto 2 lines, got height {}",
            dir_hint.size().height
        );

        let dir_input = ElementHandle::find_by_accessible_label(&app, "Папка игры")
            .find(|e| e.accessible_role() == Some(AccessibleRole::TextInput))
            .expect("item 16: firstrun game dir must be an editable TextInput");
        assert!(
            (dir_input.absolute_position().x - 637.0).abs() <= 2.5,
            "item 16: firstrun game dir TextInput x ({}) must sit inside x=625 field",
            dir_input.absolute_position().x
        );
    }
}

#[test]
fn auth_transitions_clear_init_error_flag() {
    crate::init_slint();
    let app = AppWindow::new().unwrap();
    // A retried startup that lands back on auth must not keep init chrome.
    app.set_show_init_error(true);
    auth::show_login_error(&app, "NetherG", "dns error");
    assert!(
        !app.get_show_init_error(),
        "startup auth error must clear the init-error screen"
    );
    app.set_show_init_error(true);
    auth::show_login_logout(&app, "NetherG");
    assert!(
        !app.get_show_init_error(),
        "voluntary logout must clear the init-error screen"
    );
}

#[test]
fn auth_busy_stays_set_until_transition_completes() {
    crate::init_slint();
    let app = AppWindow::new().unwrap();
    let (rt, state) = test_state();
    // Successful first login: busy stays set while enter_firstrun loads.
    let status = launcher_net::account::AccountStatus::from(&*state.config.blocking_read());
    auth::on_auth_done(&app, &state, rt.handle(), Ok(status));
    assert!(
        app.get_login_busy(),
        "login_busy must stay set until FirstRunScreen is up"
    );
    // Failed login releases busy with a friendly error.
    auth::on_auth_done(
        &app,
        &state,
        rt.handle(),
        Err("Drasl: Invalid credentials".into()),
    );
    assert!(!app.get_login_busy());
    assert!(!app.get_login_error().is_empty());
}

#[test]
fn logout_clears_stale_login_errors() {
    crate::init_slint();
    let app = AppWindow::new().unwrap();
    let (rt, state) = test_state();
    std::fs::create_dir_all(&state.app_data_dir).unwrap();
    auth::attach(&app, state.clone(), rt.handle().clone());
    app.set_login_error("Неверный пароль.".into());
    app.set_login_raw_error("Drasl: Invalid password".into());
    app.invoke_login_logout();
    assert!(
        app.get_login_error().is_empty(),
        "logout must clear the stale password error while running"
    );
    assert!(
        app.get_login_raw_error().is_empty(),
        "logout must clear the stale raw error while running"
    );
    rt.shutdown_timeout(std::time::Duration::from_secs(2));
    let _ = std::fs::remove_dir_all(&state.app_data_dir);
}

#[test]
fn settings_tabs_mouse_click_and_keyboard_navigation() {
    crate::init_slint();
    let app = AppWindow::new().unwrap();
    let (_host, _activity) = wire_app(&app, Backend::Fixtures).unwrap();
    settings_fixtures::apply(&app, "settings-general", "default").unwrap();
    app.show().unwrap();
    slint::platform::update_timers_and_animations();
    // Mouse path: clicking a tab switches it (keyboard focus outline stays off).
    ElementHandle::find_by_accessible_label(&app, "СКИН")
        .find(|e| e.accessible_role() == Some(AccessibleRole::Tab))
        .expect("skin tab")
        .invoke_accessible_default_action();
    assert_eq!(app.get_settings_tab(), 3, "mouse click switches tabs");
    // Keyboard path: the click moved focus into the tab bar, arrows move on.
    press_key(&app, Key::RightArrow);
    assert_eq!(app.get_settings_tab(), 4, "RightArrow advances tabs");
    press_key(&app, Key::LeftArrow);
    assert_eq!(app.get_settings_tab(), 3, "LeftArrow goes back");
    press_key(&app, Key::Home);
    assert_eq!(app.get_settings_tab(), 0, "Home jumps to first tab");
    press_key(&app, Key::End);
    assert_eq!(app.get_settings_tab(), 4, "End jumps to last tab");
    // Full-box focus outline is pixel chrome, not test-tree state; paired
    // screenshots cover items 22-24, geometry is asserted above.
}
