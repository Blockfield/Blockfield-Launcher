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

use blockfield_native_poc::{AppWindow, PreviewHost, fixtures, wire_app};
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
    let (host, _activity) = wire_app(&app).expect("wire test app");
    if state == "settings-skin" {
        fixtures::apply_settings(&app).expect("settings fixtures");
    } else {
        fixtures::apply_main(&app, state, scenario).expect("main fixtures");
    }
    // Guard: without SLINT_EMIT_DEBUG_INFO=1 the ElementHandle tree is empty.
    assert!(
        ElementHandle::find_by_accessible_label(&app, "ИГРАТЬ")
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
    scenario_minimize_probe();
    println!("PASS layer1 screen walk (5 scenarios)");
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

/// Minimize must stop animation timers (idle self-check logic, headless form).
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
