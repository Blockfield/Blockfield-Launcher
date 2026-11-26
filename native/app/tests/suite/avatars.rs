//! PlayerHead avatar pipeline: stats rows, self card and shell paint faces
//! (Tauri `PlayerHead.tsx` parity), with the same fixture crop every run.
//!
//! One `#[test]` walk like the other Layer-1 suites: window creation races
//! when `#[test]` fns run in parallel in one binary.

use std::sync::Once;

use blockfield_native::{AppWindow, avatars, fixtures};
use slint::Model;

fn init_once() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        i_slint_backend_testing::init_no_event_loop();
    });
}

fn test_app() -> AppWindow {
    init_once();
    AppWindow::new().expect("test window")
}

fn face_size(image: &slint::Image) -> (u32, u32) {
    (image.size().width, image.size().height)
}

fn scenario_fixture_stats_paints_faces() {
    avatars::clear_cache();
    let app = test_app();
    fixtures::apply_stats(&app, "default").expect("stats fixtures");
    let rows = app.get_stats_rows();
    assert!(rows.row_count() > 0, "fixture board has rows");
    for row in rows.iter() {
        assert_eq!(
            face_size(&row.face),
            (8, 8),
            "row {} has a cropped face",
            row.name
        );
    }
    assert_eq!(
        face_size(&app.get_stats_self_face()),
        (8, 8),
        "self card has a cropped face"
    );
    assert_eq!(face_size(&app.get_shell_face()), (8, 8), "shell has a face");
}

fn scenario_pager_keeps_faces() {
    avatars::clear_cache();
    let app = test_app();
    fixtures::apply_stats(&app, "pager").expect("pager fixtures");
    assert!(app.get_stats_rows().row_count() > 0);
    fixtures::apply_stats_page(&app, 1);
    for row in app.get_stats_rows().iter() {
        assert_eq!(
            face_size(&row.face),
            (8, 8),
            "page 1 row {} keeps its face",
            row.name
        );
    }
    assert_eq!(
        face_size(&app.get_shell_face()),
        (8, 8),
        "shell keeps its face"
    );
}

fn scenario_shell_follows_fixture_operator() {
    avatars::clear_cache();
    let app = test_app();
    fixtures::apply_main(&app, "main", "default").expect("main fixtures");
    assert_eq!(
        face_size(&app.get_shell_face()),
        (8, 8),
        "main shell shows the face"
    );
    assert_eq!(app.get_username().to_string(), "CaptureOperator");
}

fn scenario_invalid_usernames_never_resolve() {
    assert!(!avatars::valid_username("—"), "placeholder never fetches");
    assert!(!avatars::valid_username("ab"), "too short never fetches");
}

#[test]
fn avatars_walk() {
    scenario_fixture_stats_paints_faces();
    scenario_pager_keeps_faces();
    scenario_shell_follows_fixture_operator();
    scenario_invalid_usernames_never_resolve();
    println!("PASS avatars walk (4 scenarios)");
}
