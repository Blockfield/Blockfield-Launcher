//! Init failure must leave a visible screen: startup hides every show-*
//! flag until auth resolves, so the error branch has to restore the main
//! Shell or the check error sits in an empty window nobody can see.
use blockfield_native::{AppWindow, backend};

#[test]
fn init_failure_shows_main_with_error() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    assert!(!app.get_show_main());
    backend::apply_init_error(&app, "Cannot initialize launcher: boom".into());
    assert!(app.get_show_main(), "init failure must show main");
    assert!(
        !app.get_main_data().check_error.is_empty(),
        "check error must be visible"
    );
}
