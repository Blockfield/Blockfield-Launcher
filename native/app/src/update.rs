//! Update-screen operations: pack sync (nav screen) + launcher self-update
//! (settings launcher tab). Fixtures drive a deterministic local state
//! machine (Layer-1/Layer-2 run on it); the Core backend keeps POC stubs
//! until the U4/U5/U6 ops land, except cancel which reuses the real call.

use std::sync::Arc;

use launcher_app::{LauncherAppState, ops::install};
use slint::{ComponentHandle, ModelRc, VecModel};
use tokio::runtime::Handle;

use crate::{AppWindow, UpdateLogLine};

fn log_line(ts: &str, msg: &str, tone: i32) -> UpdateLogLine {
    UpdateLogLine {
        ts: ts.into(),
        msg: msg.into(),
        tone,
    }
}

fn log(lines: &[(&str, &str, i32)]) -> ModelRc<UpdateLogLine> {
    ModelRc::new(VecModel::from(
        lines
            .iter()
            .map(|(ts, msg, tone)| log_line(ts, msg, *tone))
            .collect::<Vec<_>>(),
    ))
}

/// Fixture state machine: each callback moves to the next deterministic
/// state, mirroring the Tauri UpdateScreen/LauncherUpdatePanel transitions.
pub fn attach_fixtures(app: &AppWindow) {
    let weak = app.as_weak();
    app.on_update_retry(move || {
        if let Some(app) = weak.upgrade() {
            // Tauri handleRetry: reset to checking with fresh steps.
            let mut data = app.get_update_data();
            data.phase = "checking".into();
            data.status_message = "".into();
            data.can_cancel = false;
            data.progress = 0.0;
            data.error = "".into();
            data.launching = false;
            data.step0 = 1;
            data.step1 = 0;
            data.step2 = 0;
            data.step3 = 0;
            data.log = log(&[(
                "12:00:01",
                "Establishing connection to deployment server...",
                1,
            )]);
            app.set_update_data(data);
        }
    });
    let weak = app.as_weak();
    app.on_update_cancel(move || {
        if let Some(app) = weak.upgrade() {
            let mut data = app.get_update_data();
            if data.phase != "downloading" || !data.can_cancel {
                return;
            }
            data.phase = "error".into();
            data.can_cancel = false;
            data.error = "Загрузка отменена".into();
            data.log = log(&[("12:00:02", "Download cancelled by operator.", 2)]);
            app.set_update_data(data);
        }
    });
    let weak = app.as_weak();
    app.on_update_launch(move || {
        if let Some(app) = weak.upgrade() {
            let mut data = app.get_update_data();
            if data.phase != "complete" && data.phase != "uptodate" {
                return;
            }
            data.launching = true;
            data.phase = "launching".into();
            data.status_message = "ЗАПУСК".into();
            data.step3 = 1;
            app.set_update_data(data);
        }
    });
    let weak = app.as_weak();
    app.on_launcher_check(move || {
        if let Some(app) = weak.upgrade() {
            let mut data = app.get_launcher_update_data();
            if data.phase == "checking"
                || data.phase == "downloading"
                || data.phase == "verifying"
                || data.phase == "installing"
                || data.phase == "restarting"
            {
                return;
            }
            if data.update_version.is_empty() {
                set_panel_phase(&mut data, "current");
            } else {
                set_panel_phase(&mut data, "available");
            }
            app.set_launcher_update_data(data);
        }
    });
    let weak = app.as_weak();
    app.on_launcher_install(move || {
        if let Some(app) = weak.upgrade() {
            let mut data = app.get_launcher_update_data();
            if data.update_version.is_empty() || data.installed {
                return;
            }
            set_panel_phase(&mut data, "installed");
            data.installed = true;
            app.set_launcher_update_data(data);
        }
    });
    let weak = app.as_weak();
    app.on_launcher_restart(move || {
        if let Some(app) = weak.upgrade() {
            let mut data = app.get_launcher_update_data();
            if !data.installed {
                return;
            }
            // Fixture backend cannot relaunch the process; record the intent.
            set_panel_phase(&mut data, "restarting");
            app.set_launcher_update_data(data);
        }
    });
}

/// Core backend: POC stubs (real ops arrive with the U4/U5/U6 port units).
/// Cancel reuses the real installer call, like the main screen does.
pub fn attach_core(app: &AppWindow, state: Arc<LauncherAppState>, handle: Handle) {
    app.on_update_retry(|| eprintln!("POC update: retry needs the U4/U5 install ops"));
    app.on_update_cancel(move || {
        let state = state.clone();
        handle.spawn(async move {
            if let Err(error) = install::cancel_download(&state).await {
                eprintln!("cancel_download: {error}");
            }
        });
    });
    app.on_update_launch(|| eprintln!("POC update: launch needs the U4/U5 install ops"));
    app.on_launcher_check(|| eprintln!("POC launcher-update: check needs the U14 updater"));
    app.on_launcher_install(|| eprintln!("POC launcher-update: install needs the U14 updater"));
    app.on_launcher_restart(|| eprintln!("POC launcher-update: restart needs the U14 updater"));
}

pub fn checking_log() -> ModelRc<UpdateLogLine> {
    log(&[(
        "12:00:01",
        "Establishing connection to deployment server...",
        1,
    )])
}

pub fn downloading_log() -> ModelRc<UpdateLogLine> {
    log(&[
        (
            "12:00:01",
            "Establishing connection to deployment server...",
            1,
        ),
        (
            "12:00:02",
            "Manifest fetched: remote=2.75, installed=2.74",
            0,
        ),
        ("12:00:03", "Update needed: 2.74 → 2.75", 1),
        ("12:00:04", "Waiting for backend install tasks...", 1),
    ])
}

pub fn default_data() -> crate::UpdateData {
    crate::UpdateData {
        phase: "checking".into(),
        operation: "BLOCKFIELD".into(),
        patch: "/ ПАТЧ …".into(),
        description: "Синхронизация модпака, проверка файлов и запуск игры.".into(),
        mirror_name: "—".into(),
        step0: 1,
        log: checking_log(),
        game_phase: "idle".into(),
        ..Default::default()
    }
}

pub fn default_launcher_data() -> crate::LauncherUpdateData {
    crate::LauncherUpdateData {
        phase: "idle".into(),
        current_version: "—".into(),
        ..Default::default()
    }
}

/// One place for panel phase changes: any non-error phase drops a stale error.
fn set_panel_phase(data: &mut crate::LauncherUpdateData, phase: &str) {
    data.phase = phase.into();
    if phase != "error" {
        data.error = "".into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_transitions_follow_the_tauri_flow() {
        i_slint_backend_testing::init_no_event_loop();
        let app = crate::AppWindow::new().unwrap();
        crate::fixtures::apply(&app);
        crate::fixtures::apply_main(&app, "main", "default").unwrap();
        crate::fixtures::apply_update(&app, "error").unwrap();
        attach_fixtures(&app);
        // Retry resets an error to checking with a fresh first step.
        app.invoke_update_retry();
        let data = app.get_update_data();
        assert_eq!(data.phase, "checking");
        assert_eq!(
            (data.step0, data.step1, data.step2, data.step3),
            (1, 0, 0, 0)
        );
        // Cancel is a no-op outside a cancelable download.
        app.invoke_update_cancel();
        assert_eq!(app.get_update_data().phase, "checking");
        // Cancel stays callable: invoking twice settles on error, never moves.
        crate::fixtures::apply_update(&app, "default").unwrap();
        app.invoke_update_cancel();
        assert_eq!(app.get_update_data().phase, "error");
        app.invoke_update_cancel();
        assert_eq!(app.get_update_data().phase, "error");
        // Launch is a no-op until the pack is complete.
        app.invoke_update_launch();
        assert!(!app.get_update_data().launching);
        crate::fixtures::apply_update(&app, "complete").unwrap();
        app.invoke_update_launch();
        assert!(app.get_update_data().launching);
        assert_eq!(app.get_update_data().phase, "launching");
        // Launcher panel: check exposes the staged update, install finishes it.
        crate::fixtures::apply_launcher_update(&app, "idle").unwrap();
        app.invoke_launcher_check();
        assert_eq!(app.get_launcher_update_data().phase, "available");
        app.invoke_launcher_install();
        let panel = app.get_launcher_update_data();
        assert!(panel.installed);
        assert_eq!(panel.phase, "installed");
        app.invoke_launcher_restart();
        assert_eq!(app.get_launcher_update_data().phase, "restarting");
        // Fresh defaults carry no stale phase: a new panel starts idle.
        let fresh = crate::update::default_launcher_data();
        assert_eq!(fresh.phase, "idle");
        // A prior panel error clears on the next successful transition.
        crate::fixtures::apply_launcher_update(&app, "error").unwrap();
        assert_ne!(app.get_launcher_update_data().error, "");
        app.invoke_launcher_check();
        let panel = app.get_launcher_update_data();
        assert_eq!(panel.phase, "available");
        assert_eq!(panel.error, "");
        crate::fixtures::apply_launcher_update(&app, "error").unwrap();
        app.invoke_launcher_install();
        let panel = app.get_launcher_update_data();
        assert_eq!(panel.phase, "installed");
        assert_eq!(panel.error, "");
    }
}
