//! Consolidated Layer-1/unit integration suite: one test binary instead of
//! twelve, so the huge Slint link happens once per OS (was ~15 links).
//!
//! Everything here shares the `init_no_event_loop` testing backend (the
//! platform is thread-local and libtest runs each `#[test]` on its own
//! thread, so every test installs its own backend) or uses no Slint backend
//! at all. Kept as separate binaries: the `HOME`-mutating tests
//! (process-global env races) and the `init_integration_test_with_system_time`
//! event-loop tests (one event-loop proxy per process).

mod avatars;
mod chrome;
mod init_failure;
mod layer1_screens;
mod main_controls_probe;
mod main_gaps;
mod self_update_e2e;
mod settings_backend;
mod settings_tabs;
mod stats_cache;
mod stats_render;
mod uninstall_cleanup;
