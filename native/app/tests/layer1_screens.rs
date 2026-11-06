//! Layer 1: in-process functional e2e for the Slint app.
//!
//! Decision (brief step A, Slint 1.18.1): the real Slint UI tree is driven
//! headless with the Slint testing backend (`i-slint-backend-testing`:
//! find by accessible label, invoke default action, mock drag) instead of
//! AccessKit. The testing API is pure Rust, works identically on
//! Windows/macOS/Linux with mock time, and needs no display. It requires
//! debug info in the generated code, so the suite only builds meaningfully
//! with `SLINT_EMIT_DEBUG_INFO=1` (set job-wide in CI; see tests/README.md).
//! State assertions go through the same generated property/callback API the
//! binary uses, so navigation/tab/slim/deploy coverage exercises the
//! production wiring from `wire_app`, not a stub.
//!
//! `init_no_event_loop` may run once per process, so this binary has one
//! `#[test]` entry that runs every scenario in sequence. Scenarios are plain
//! `scenario_*` fns written as Given/When/Then.

use std::rc::Rc;

use blockfield_native::{AppWindow, PreviewHost, fixtures, wire_app};
use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementQuery};
use slint::{ComponentHandle, Model};

fn init_once() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        i_slint_backend_testing::init_no_event_loop();
    });
}

/// Given a fresh, fully wired app window for `state`/`scenario`.
fn given_app(state: &str, scenario: &str) -> (AppWindow, Rc<PreviewHost>) {
    init_once();
    let app = AppWindow::new().expect("test window");
    let (host, _activity) =
        wire_app(&app, blockfield_native::backend::Backend::Fixtures).expect("wire test app");
    if state == "settings-skin" {
        fixtures::apply_settings(&app).expect("settings fixtures");
    } else if state == "login" {
        fixtures::apply_login(&app, scenario).expect("login fixtures");
    } else if state == "firstrun" {
        fixtures::apply_firstrun(&app).expect("firstrun fixtures");
    } else if state == "update" {
        fixtures::apply_update(&app, scenario).expect("update fixtures");
    } else {
        fixtures::apply_main(&app, state, scenario).expect("main fixtures");
    }
    // Guard: without SLINT_EMIT_DEBUG_INFO=1 the ElementHandle tree is empty.
    let anchor = match (state, scenario) {
        ("login", "register") => "ЗАРЕГИСТРИРОВАТЬСЯ",
        ("login", "password") => "СОХРАНИТЬ ПАРОЛЬ",
        ("login", _) => "ВОЙТИ",
        ("firstrun", _) => "ПРОДОЛЖИТЬ",
        _ => "ИГРАТЬ",
    };
    assert!(
        ElementHandle::find_by_accessible_label(&app, anchor)
            .next()
            .is_some(),
        "testing tree has no accessible labels: rebuild with SLINT_EMIT_DEBUG_INFO=1"
    );
    (app, host)
}

/// When helper: click the first element carrying `label` via its accessible
/// default action (the real UI-tree input path, not a direct callback).
/// Returns the clicked handle.
fn when_click_label(app: &AppWindow, label: &str) -> ElementHandle {
    // A label matches both the control (Button/Tab/Switch role, with a
    // default action) and the Text that renders it (role Text, no action).
    // The control is what the operator activates.
    let el = ElementHandle::find_by_accessible_label(app, label)
        .find(|el| el.accessible_role() != Some(AccessibleRole::Text))
        .unwrap_or_else(|| panic!("element with label {label:?} exists"));
    assert!(el.is_valid(), "element {label:?} is valid");
    el.invoke_accessible_default_action();
    el
}

/// All *controls* carrying `label` (Text renderings excluded, see above).
fn all_labels(app: &AppWindow, label: &str) -> Vec<ElementHandle> {
    ElementHandle::find_by_accessible_label(app, label)
        .filter(|el| el.accessible_role() != Some(AccessibleRole::Text))
        .collect()
}

#[test]
fn layer1_screen_walk() {
    scenario_nav_main_to_settings_and_back();
    scenario_settings_each_tab();
    scenario_skin_tab_controls();
    scenario_main_deploy_states();
    scenario_inflight_file_leads_launch_status();
    scenario_stale_pack_flags_update_prompt();
    scenario_login_validation_and_submit();
    scenario_login_modes();
    scenario_firstrun_ram_and_continue();
    scenario_update_nav_and_retry();
    scenario_update_deploy();
    scenario_launcher_update_panel();
    scenario_minimize_probe();
    println!("PASS layer1 screen walk (13 scenarios)");
}

/// Navigate main → settings → each tab label → back, through the UI tree.
fn scenario_nav_main_to_settings_and_back() {
    // Given the default online main screen.
    let (app, _host) = given_app("main", "default");
    assert_eq!(app.get_active(), 0, "starts on main");

    // When the operator opens settings via the nav tree…
    when_click_label(&app, "НАСТРОЙКИ");
    // …then the settings screen mounts…
    assert_eq!(app.get_active(), 3, "settings mounts");
    // …with every tab label (the binary loads the tab model with the
    // settings fixtures, same as `--state settings-skin`).
    fixtures::apply_settings(&app).expect("settings fixtures");
    for (index, tab) in ["ОБЩИЕ", "JAVA И ПАМЯТЬ", "КОМАНДЫ", "СКИН", "ЛАУНЧЕР"]
        .iter()
        .enumerate()
    {
        when_click_label(&app, tab);
        assert_eq!(app.get_settings_tab(), index as i32, "tab {tab} selects");
    }

    // When the operator returns to the skin tab and then home…
    when_click_label(&app, "СКИН");
    assert_eq!(app.get_settings_tab(), 3, "back on skin tab");
    // "ИГРАТЬ" matches both the nav item and the deploy button; the nav item
    // is the slim header row, the deploy button is 230x64.
    let nav = ElementHandle::find_by_accessible_label(&app, "ИГРАТЬ")
        .find(|el| el.size().height < 50.0)
        .expect("nav item ИГРАТЬ exists");
    nav.invoke_accessible_default_action();
    // …then main is back.
    assert_eq!(app.get_active(), 0, "back on main");
}

/// Every settings tab label selects its page and the skin tab keeps content.
fn scenario_settings_each_tab() {
    // Given the settings screen on the skin tab.
    let (app, _host) = given_app("settings-skin", "default");
    assert_eq!((app.get_active(), app.get_settings_tab()), (3, 3));

    // When each tab is activated through the tree, its page selects…
    for (index, tab) in ["ОБЩИЕ", "JAVA И ПАМЯТЬ", "КОМАНДЫ", "СКИН", "ЛАУНЧЕР"]
        .iter()
        .enumerate()
    {
        when_click_label(&app, tab);
        assert_eq!(app.get_settings_tab(), index as i32, "tab {tab} selects");
    }
    // …and the skin tab still shows the preview with its caption.
    when_click_label(&app, "СКИН");
    let preview = ElementHandle::find_by_accessible_label(
        &app,
        "Анимированный скин Minecraft; вращайте перетаскиванием",
    )
    .next();
    assert!(preview.is_some(), "skin preview exposes its label");
}

/// Skin tab: slim toggle, browse buttons, drag, reset/save states.
fn scenario_skin_tab_controls() {
    // Given the skin tab with the classic (non-slim) model.
    let (app, host) = given_app("settings-skin", "default");
    assert!(!app.get_slim(), "classic model by default");

    // When the slim switch is toggled through the tree, the model flips…
    let slim = when_click_label(&app, "ТОНКИЕ РУКИ");
    assert!(app.get_slim(), "slim toggles on");
    assert_eq!(
        slim.accessible_checked(),
        Some(true),
        "switch reports checked"
    );
    when_click_label(&app, "ТОНКИЕ РУКИ");
    assert!(!app.get_slim(), "slim toggles off");

    // …and both file rows expose a browse button (skin row first, cape second).
    let browse = all_labels(&app, "ОБЗОР");
    assert_eq!(browse.len(), 2, "skin + cape browse buttons");
    browse[0].invoke_accessible_default_action(); // stub backend: must not crash
    browse[1].invoke_accessible_default_action();

    // When the preview is dragged, rotation reaches the preview host…
    let before = host.drag_moves();
    app.invoke_skin_drag_by(45.0, 20.0);
    assert_eq!(host.drag_moves(), before + 1, "drag-by reaches the host");
    // …also through a real pointer drag over the preview element.
    if let Some(preview) = ElementHandle::find_by_accessible_label(
        &app,
        "Анимированный скин Minecraft; вращайте перетаскиванием",
    )
    .next()
    {
        // mock_drag drags from the element center to a window-absolute target.
        let center = preview_center(&preview);
        let target = slint::LogicalPosition::new(center.x + 45.0, center.y + 20.0);
        preview.mock_drag(target, slint::platform::PointerEventButton::Left);
        assert!(
            host.drag_moves() > before + 1,
            "pointer drag over the preview rotates it"
        );
    }

    // Then reset is available and save stays disabled until dirty.
    let reset = when_click_label(&app, "СБРОС");
    assert_eq!(reset.accessible_enabled(), Some(true), "reset enabled");
    let save = all_labels(&app, "СОХРАНИТЬ");
    assert_eq!(save.len(), 1, "one save button");
    assert_eq!(
        save[0].accessible_enabled(),
        Some(false),
        "save disabled when clean"
    );
}

fn preview_center(el: &ElementHandle) -> slint::LogicalPosition {
    let pos = el.absolute_position();
    let size = el.size();
    slint::LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0)
}

/// Main screen: profile select plus deploy-button state per fixture.
fn scenario_main_deploy_states() {
    // Given the idle online build…
    let (app, _host) = given_app("main", "default");
    // …when the deploy button (230x64, not the nav item) is read…
    let deploy = ElementHandle::find_by_accessible_label(&app, "ИГРАТЬ")
        .find(|el| el.size().height > 50.0)
        .expect("deploy button exists");
    assert_eq!(
        deploy.accessible_enabled(),
        Some(true),
        "idle build deploys"
    );
    // …then deploying flips the fixture into launching.
    deploy.invoke_accessible_default_action();
    assert!(app.get_main_data().launching, "deploy starts the launch");
    assert_eq!(app.get_main_data().phase.to_string(), "launching");

    // Given a build that still needs setup, deploy explains instead of launching.
    let (app, _host) = given_app("main", "setup");
    assert!(app.get_main_data().needs_setup);
    app.invoke_deploy();
    assert!(!app.get_main_data().launching, "setup does not launch");
    assert!(
        !app.get_room_message().is_empty(),
        "setup explains the next step"
    );

    // Then busy/error/blocked fixtures keep deploy disabled.
    for (state, scenario) in [
        ("main-launching", "default"),
        ("error", "default"),
        ("main", "profile-blocked"),
        ("offline", "empty"),
    ] {
        let (app, _host) = given_app(state, scenario);
        // Offline/empty has no rooms and no deploy target at all.
        if state == "offline" {
            assert!(app.get_rooms().row_count() == 0, "offline lists no rooms");
            continue;
        }
        // The launch label varies by fixture, so find the deploy control by
        // its size (230x64), not by text.
        let deploy_enabled = ElementQuery::from_root(&app)
            .match_accessible_role(AccessibleRole::Button)
            .find_all()
            .into_iter()
            .find(|el| el.size().height > 50.0)
            .map(|el| el.accessible_enabled());
        assert_eq!(
            deploy_enabled,
            Some(Some(false)),
            "deploy disabled for {state}/{scenario}"
        );
        assert!(!app.get_main_data().launching || state == "main-launching");
    }

    // Given a builder profile, the profile switch offers the workshop…
    let (app, _host) = given_app("main", "workshop");
    let select = all_labels(&app, "Профиль")
        .into_iter()
        .next()
        .expect("profile select exists");
    assert_eq!(
        select.accessible_value().as_deref(),
        Some("Мастерская"),
        "workshop profile selected"
    );
}

/// Inflight installer file (PR #64, core ops::install): the main screen shows
/// the live file, bytes and source in the launch status while launching.
fn scenario_inflight_file_leads_launch_status() {
    let (app, _host) = given_app("main", "inflight");
    let data = app.get_main_data();
    assert!(data.launching, "inflight fixture is launching");
    assert!(
        data.launch_status
            .to_string()
            .starts_with("12.3/40.1 MB - "),
        "live file bytes lead the launch status"
    );
    assert!(
        data.launch_status.to_string().ends_with("Gcore CDN"),
        "live file source stays visible"
    );
    assert_eq!(data.progress, 8, "file count drives the progress");
}

/// PACK_STALE fixture (core ops::launch refusal): the main screen carries the
/// raw refusal plus the stale flag, so the friendly update prompt renders
/// instead of the generic launch failure.
fn scenario_stale_pack_flags_update_prompt() {
    let (app, _host) = given_app("main", "stale");
    let data = app.get_main_data();
    assert!(
        data.launch_error.to_string().contains("PACK_STALE"),
        "stale fixture keeps the raw refusal for details"
    );
    assert!(
        data.launch_stale,
        "stale fixture arms the friendly update prompt"
    );
}

/// Minimize must stop animation timers (idle self-check logic, headless form).
/// Login: fixture entry, validation, submit transition to FirstRun.
fn scenario_login_validation_and_submit() {
    // Given the empty login form…
    let (app, _host) = given_app("login", "default");
    assert!(app.get_show_login(), "login mounts");
    assert!(!app.get_show_main(), "shell stays hidden on login");
    // …both fields exist in the testing tree as text inputs…
    let name = ElementHandle::find_by_accessible_label(&app, "НИКНЕЙМ")
        .find(|el| el.accessible_role() == Some(AccessibleRole::TextInput))
        .expect("username input exists");
    assert!(name.is_valid(), "username input valid");
    let password = ElementHandle::find_by_accessible_label(&app, "ПАРОЛЬ")
        .find(|el| el.accessible_role() == Some(AccessibleRole::TextInput))
        .expect("password input exists");
    // …then submitting is blocked until both fields are valid…
    assert!(!app.get_login_can_submit(), "empty form cannot submit");
    name.set_accessible_value("ab");
    app.invoke_login_username_edited("ab".into());
    assert!(!app.get_login_name_valid(), "two-letter name rejected");
    assert!(!app.get_login_can_submit(), "invalid name cannot submit");
    name.set_accessible_value("Player_1");
    app.invoke_login_username_edited("Player_1".into());
    assert_eq!(
        app.get_login_username().to_string(),
        "Player_1",
        "typing reaches the form state"
    );
    password.set_accessible_value("secret123");
    app.invoke_login_password_edited("secret123".into());
    assert!(app.get_login_name_valid(), "valid name accepted");
    assert!(app.get_login_can_submit(), "complete form can submit");
    // …when valid credentials submit, the fixture signs in and moves on…
    when_click_label(&app, "ВОЙТИ");
    assert!(!app.get_show_login(), "login unmounts after submit");
    assert!(app.get_show_main(), "signed-in user reaches main");
}

/// Login: register toggle and password-migration variants.
fn scenario_login_modes() {
    // Given the login form, toggling switches to registration…
    let (app, _host) = given_app("login", "default");
    when_click_label(&app, "Нет аккаунта? Зарегистрироваться");
    assert_eq!(
        app.get_login_mode().to_string(),
        "register",
        "toggle switches to register"
    );
    assert_eq!(
        all_labels(&app, "ЗАРЕГИСТРИРОВАТЬСЯ").len(),
        1,
        "register submit exists"
    );
    // …and back.
    when_click_label(&app, "Уже есть аккаунт? Войти");
    assert_eq!(
        app.get_login_mode().to_string(),
        "login",
        "toggle switches back"
    );
    // Given password migration, the name field hides and logout shows…
    let (app, _host) = given_app("login", "password");
    assert!(app.get_login_set_password(), "migration sets password mode");
    assert!(
        all_labels(&app, "НИКНЕЙМ").is_empty(),
        "migration hides the name field"
    );
    assert_eq!(
        all_labels(&app, "СОХРАНИТЬ ПАРОЛЬ").len(),
        1,
        "migration submit exists"
    );
    // …when the operator logs out, the plain login form returns.
    when_click_label(&app, "Выйти");
    assert!(
        !app.get_login_set_password(),
        "logout leaves migration mode"
    );
    assert!(app.get_show_login(), "logout stays on login");
    let (app, _host) = given_app("login", "password");
    app.set_login_password("fresh-secret-1".into());
    app.invoke_login_password_edited("fresh-secret-1".into());
    app.invoke_login_submit();
    assert!(app.get_show_firstrun(), "migration continues to firstrun");
}

/// FirstRun: RAM stepper, presets, number clamp and continue.
fn scenario_firstrun_ram_and_continue() {
    // Given the first-run form…
    let (app, _host) = given_app("firstrun", "default");
    assert!(app.get_show_firstrun(), "firstrun mounts");
    assert!(!app.get_show_main(), "shell stays hidden on firstrun");
    assert_eq!(app.get_firstrun_ram(), 4, "fixture RAM");
    // …stepper, presets and the typed value target the same property…
    when_click_label(&app, "Увеличить память");
    assert_eq!(app.get_firstrun_ram(), 5, "plus steps up");
    when_click_label(&app, "Уменьшить память");
    assert_eq!(app.get_firstrun_ram(), 4, "minus steps down");
    when_click_label(&app, "8G");
    assert_eq!(app.get_firstrun_ram(), 8, "preset applies");
    app.invoke_firstrun_ram_text_edited("20".into());
    assert_eq!(app.get_firstrun_ram(), 16, "number clamps to max");
    app.invoke_firstrun_ram_text_edited("abc".into());
    assert_eq!(app.get_firstrun_ram(), 16, "garbage keeps RAM");
    // …the slider is exposed with its value, and both browse stubs hold…
    let slider = ElementHandle::find_by_accessible_label(&app, "Выделенная память, ГБ")
        .find(|el| el.accessible_role() == Some(AccessibleRole::Slider))
        .expect("ram slider exists");
    assert!(slider.is_valid(), "slider valid");
    assert_eq!(
        slider.accessible_value().as_deref(),
        Some("16"),
        "slider exposes clamped RAM"
    );
    assert_eq!(all_labels(&app, "ОБЗОР").len(), 2, "dir + java browse");
    // …when the operator continues, the fixture saves and shows main.
    when_click_label(&app, "ПРОДОЛЖИТЬ");
    assert!(!app.get_show_firstrun(), "firstrun unmounts on continue");
    assert!(app.get_show_main(), "continue shows main");
}

fn scenario_minimize_probe() {
    // Given the main screen…
    let (app, _host) = given_app("main", "default");
    // …when the window minimizes, no animations may stay active…
    app.window().set_minimized(true);
    if !app.window().is_minimized() {
        println!("SKIP minimize probe: testing backend refuses minimization here");
        return;
    }
    assert!(
        !app.window().has_active_animations(),
        "minimized window runs no animations"
    );
    // …and restore brings the window back.
    app.window().set_minimized(false);
    assert!(!app.window().is_minimized(), "window restores");
}

/// Update screen: nav mounts it, cancel/retry drive the fixture flow.
fn scenario_update_nav_and_retry() {
    // Given the default online main screen…
    let (app, _host) = given_app("main", "default");
    // …when the operator opens updates via the nav tree, the screen mounts…
    when_click_label(&app, "ОБНОВЛЕНИЯ");
    assert_eq!(app.get_active(), 1, "update mounts");
    // …and a cancelable download (update fixtures) cancels and retries.
    let (app, _host) = given_app("update", "default");
    assert_eq!(app.get_update_data().phase, "downloading");
    // …when the operator cancels, the flow errors…
    when_click_label(&app, "ОТМЕНА");
    assert_eq!(app.get_update_data().phase, "error");
    // …and retry resets it to checking with a fresh first step.
    when_click_label(&app, "ПОВТОРИТЬ");
    let data = app.get_update_data();
    assert_eq!(data.phase, "checking");
    assert_eq!((data.step0, data.step1), (1, 0));
}

/// Update screen: the finished pack offers play; a running game locks it.
fn scenario_update_deploy() {
    // Given a finished pack, the 64px play button (not the nav item) launches…
    let (app, _host) = given_app("update", "complete");
    let play = ElementHandle::find_by_accessible_label(&app, "ИГРАТЬ")
        .find(|el| el.size().height > 50.0)
        .expect("update play button exists");
    play.invoke_accessible_default_action();
    assert!(app.get_update_data().launching, "play starts the launch");
    assert_eq!(app.get_update_data().phase, "launching");
    // …while a running game replaces actions with its own state.
    let (app, _host) = given_app("update", "running");
    assert_eq!(app.get_update_data().game_phase, "running");
}

/// Launcher tab: the self-update panel checks, installs and restarts.
fn scenario_launcher_update_panel() {
    // Given the settings screen with a staged launcher update…
    let (app, _host) = given_app("settings-skin", "default");
    fixtures::apply_launcher_update(&app, "idle").expect("launcher fixtures");
    when_click_label(&app, "ЛАУНЧЕР");
    assert_eq!(app.get_settings_tab(), 4, "launcher tab selects");
    // …when the operator checks, the staged update is offered…
    when_click_label(&app, "Проверить обновления");
    assert_eq!(app.get_launcher_update_data().phase, "available");
    // …and installing finishes it, offering a restart.
    when_click_label(&app, "Установить обновление");
    let panel = app.get_launcher_update_data();
    assert!(panel.installed, "install finishes");
    assert_eq!(panel.phase, "installed");
}
