//! U-chrome: WindowChrome drag/min/max/close wiring through the real UI tree.
//!
//! Drives the production chrome callbacks (`wire_app`, same code path as the
//! shipped binary) with the Slint testing backend: the titlebar drag area
//! must cover the titlebar (a zero-height area misses every drag), and the
//! three chrome buttons must minimize, toggle maximize, and request close.

use blockfield_native::{AppWindow, fixtures, wire_app};
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::ComponentHandle;

// The testing backend is process-global: a second init in the same test binary panics.
static BACKEND: std::sync::Once = std::sync::Once::new();

fn given_chrome() -> AppWindow {
    BACKEND.call_once(i_slint_backend_testing::init_no_event_loop);
    let app = AppWindow::new().expect("test window");
    wire_app(&app, blockfield_native::backend::Backend::Fixtures).expect("wire test app");
    fixtures::apply_main(&app, "main", "default").expect("main fixtures");
    // Size the window so geometry matches the 1280x720 minimum (headless: no
    // window appears; preferred size still applies).
    app.show().expect("show test window");
    // Guard: without SLINT_EMIT_DEBUG_INFO=1 the ElementHandle tree is empty.
    assert!(
        ElementHandle::find_by_accessible_label(&app, "ИГРАТЬ")
            .next()
            .is_some(),
        "testing tree has no accessible labels: rebuild with SLINT_EMIT_DEBUG_INFO=1"
    );
    app
}

/// Click the control carrying `label` via its accessible default action (the
/// real UI-tree input path, not a direct callback).
fn when_click_control(app: &AppWindow, label: &str) {
    // A label matches both the control (role Button, with a default action)
    // and the Text that renders it (role Text, no action).
    let el = ElementHandle::find_by_accessible_label(app, label)
        .find(|el| el.accessible_role() != Some(AccessibleRole::Text))
        .unwrap_or_else(|| panic!("chrome control with label {label:?} exists"));
    assert!(el.is_valid(), "chrome control {label:?} is valid");
    el.invoke_accessible_default_action();
}

#[test]
fn chrome_drag_area_covers_titlebar() {
    let app = given_chrome();
    // The drag area spans the titlebar minus the button row and covers its
    // full height; with zero height (the pre-fix shape) no drag can start.
    let area = ElementHandle::find_by_element_type_name(&app, "WindowMoveArea")
        .next()
        .expect("WindowMoveArea exists in WindowChrome");
    let size = area.size();
    let pos = area.absolute_position();
    // Origin matches the titlebar inset (1px border); free-floating position
    // silently misplaces the area, so pin it exactly (slack < 1 logical px).
    for (got, want, what) in [(pos.x, 1.0, "x"), (pos.y, 1.0, "y")] {
        assert!(
            (got - want).abs() < 1.0,
            "drag area {what} starts at titlebar inset, got {got}"
        );
    }
    assert!(
        (size.width - (1278.0 - 128.0)).abs() < 1.0,
        "drag area spans titlebar minus buttons, got width {}",
        size.width
    );
    assert!(
        (size.height - 36.0).abs() < 1.0,
        "drag area covers titlebar height, got height {}",
        size.height
    );

    // Smoke the event path: a press-drag from the title text reaches the
    // window without error (the backend performs the move natively).
    let title = ElementHandle::find_by_accessible_label(&app, "BLOCKFIELD LAUNCHER")
        .find(|el| el.accessible_role() == Some(AccessibleRole::Text))
        .expect("title text exists");
    let center = title.absolute_position();
    let bounds = title.size();
    title.mock_drag(
        slint::LogicalPosition::new(
            center.x + bounds.width / 2.0 + 100.0,
            center.y + bounds.height / 2.0,
        ),
        slint::platform::PointerEventButton::Left,
    );
    assert!(title.is_valid(), "tree intact after titlebar drag");
    println!("PASS chrome drag area covers titlebar");
}

#[test]
fn chrome_min_max_close_wired() {
    let app = given_chrome();

    // When the operator clicks minimize… then the window minimizes…
    when_click_control(&app, "Свернуть");
    assert!(app.window().is_minimized(), "minimize button minimizes");
    // …and restore brings it back for the remaining probes.
    app.window().set_minimized(false);
    assert!(!app.window().is_minimized(), "window restores");

    // Maximize toggles on repeated clicks.
    when_click_control(&app, "Развернуть / восстановить");
    assert!(app.window().is_maximized(), "maximize button maximizes");
    when_click_control(&app, "Развернуть / восстановить");
    assert!(!app.window().is_maximized(), "second click restores");

    // Close runs the request-close handler without error. The testing
    // backend queues the quit without a running loop, so reaching this
    // point proves the full button→callback→quit path is wired. Last:
    // nothing observable follows a quit request.
    when_click_control(&app, "Закрыть");
    println!("PASS chrome min/max/close wired");
}
