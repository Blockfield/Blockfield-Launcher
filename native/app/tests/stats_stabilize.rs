//! Stats stabilization: fail-before coverage for the four PR #80 [major] threads.
//!
//! Windows known folders ignore HOME, so the state-building coverage stays
//! unix-only (same reason as core_bootstrap.rs).
#![cfg(unix)]

use std::path::PathBuf;

use blockfield_native::{AppWindow, backend::Backend, runtime, stats};
use slint::Model;

fn isolated_home() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache")
        .join(format!("native-stats-stabilize-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    // Same isolation as core_bootstrap.rs: the real core boots offline here.
    unsafe {
        for key in [
            "HOME",
            "USERPROFILE",
            "APPDATA",
            "XDG_DATA_HOME",
            "XDG_CONFIG_HOME",
            "XDG_CACHE_HOME",
            "XDG_RUNTIME_DIR",
            "TMPDIR",
        ] {
            std::env::set_var(key, &root);
        }
        std::env::remove_var("VITE_BLOCKFIELD_PACK_URL");
    }
    root
}

fn build_state() -> std::sync::Arc<launcher_app::LauncherAppState> {
    let runtime = runtime::new().unwrap();
    runtime
        .block_on(
            runtime.spawn_blocking(|| runtime::build_state(std::sync::Arc::new(|_| {})).unwrap()),
        )
        .unwrap()
}

/// Threads 1-3 through the real wiring. Each assert fails on the pre-fix
/// code and passes after; thread 4 (stale generations) is a pure-logic unit
/// test next to `StatsRequests` in stats.rs.
#[test]
fn stats_review_threads_live_behavior() {
    let _root = isolated_home();
    i_slint_backend_testing::init_no_event_loop();

    // Thread 1a: the Fixtures backend must keep binding fixture callbacks.
    let app = AppWindow::new().unwrap();
    blockfield_native::wire_app(&app, Backend::Fixtures).unwrap();
    app.invoke_stats_set_mode("room".into());
    assert_eq!(
        app.get_stats_mode().to_string(),
        "room",
        "thread 1: Fixtures backend binds stats callbacks"
    );

    // Thread 1b: a live backend must NOT bind fixture callbacks, so an init
    // failure (or a not-yet-run wire_live) leaves ТОП inert. The runtime is
    // dropped before invoking: no async wire_live can ever run, so any mode
    // change proves synchronous fixture binding (the bug).
    let app = AppWindow::new().unwrap();
    let live = runtime::new().unwrap();
    blockfield_native::wire_app(&app, Backend::Core(live.handle().clone())).unwrap();
    drop(live);
    app.invoke_stats_set_mode("room".into());
    assert_eq!(
        app.get_stats_mode().to_string(),
        "",
        "thread 1: live backend must not bind fixture callbacks"
    );

    // Threads 2+3: live wiring initializes option models and default filters.
    let app = AppWindow::new().unwrap();
    let live = runtime::new().unwrap();
    stats::wire_live(&app, live.handle().clone(), build_state());
    assert!(
        app.get_stats_modes().row_count() > 0,
        "thread 2: live wires mode options"
    );
    assert!(
        app.get_stats_sorts().row_count() > 0,
        "thread 2: live wires sort options"
    );
    assert!(
        app.get_stats_periods().row_count() > 0,
        "thread 2: live wires period options"
    );
    assert_eq!(
        app.get_stats_period().to_string(),
        "all",
        "thread 3: live seeds period from StatsFilters::default"
    );
    assert_eq!(
        app.get_stats_sort().to_string(),
        "score",
        "thread 3: live seeds sort from StatsFilters::default"
    );
    assert_eq!(
        app.get_stats_page(),
        1,
        "thread 3: live seeds page from StatsFilters::default"
    );

    // Thread 1c: live init failure surfaces on ТОП instead of a spinner.
    let app = AppWindow::new().unwrap();
    blockfield_native::backend::apply_init_error(&app, "Cannot initialize launcher: boom".into());
    assert_eq!(
        app.get_stats_board_state(),
        1,
        "thread 1: init failure shows the board error"
    );
    assert!(
        app.get_stats_board_error().to_string().contains("boom"),
        "thread 1: board error carries the message"
    );
    assert_eq!(
        app.get_stats_self_state(),
        2,
        "thread 1: init failure shows the profile error"
    );
    assert!(
        app.get_stats_self_error().to_string().contains("boom"),
        "thread 1: profile error carries the message"
    );
}
