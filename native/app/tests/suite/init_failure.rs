//! Init failure must show the dedicated centered error screen with retry
//! (Tauri `App.tsx` `configError` parity) rather than landing on MainScreen.
use blockfield_native::{AppWindow, backend};

#[test]
fn init_failure_shows_dedicated_error_screen() {
    crate::init_slint();
    let app = AppWindow::new().unwrap();
    assert!(!app.get_show_main());
    assert!(!app.get_show_init_error());
    backend::apply_init_error(&app, "Cannot initialize launcher: boom".into());
    assert!(!app.get_show_main(), "init failure must not show main");
    assert!(
        app.get_show_init_error(),
        "init failure must show dedicated error screen"
    );
    assert_eq!(
        app.get_init_error(),
        "Не удалось загрузить настройки. Проверьте доступ к папке настроек и повторите."
    );
    assert!(
        app.get_init_error_raw().contains("boom"),
        "raw error must be preserved for details toggle"
    );
}
