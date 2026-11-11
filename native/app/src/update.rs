//! Update-screen operations: pack sync (nav screen) + launcher self-update
//! (settings launcher tab). Fixtures drive a deterministic local state
//! machine; the Core backend runs the real check → download → verify sequence
//! (same install/launch ops as the main screen) and reports back on the UI
//! thread. The launcher panel stays fixture-driven: no native updater backend
//! exists (Tauri self-update lives in `src-tauri` behind
//! `tauri_plugin_updater`; the cross-engine update check is a coordinator
//! merge gate, not a unit).

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use launcher_app::{
    LauncherAppState, VersionCheckResult,
    ops::{install, launch, pack_check},
    rooms::PendingRoom,
};
use launcher_net::minecraft;
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

/// Core backend: pack sync runs the real check → download → verify sequence
/// (same install/launch ops as the main screen deploy); cancel reuses the
/// real installer call. The launcher panel has no native updater backend, so
/// its buttons stay no-ops here — Layer-1 fixtures drive that panel.
pub fn attach_core(app: &AppWindow, state: Arc<LauncherAppState>, handle: Handle) {
    // Mirrors the Tauri `runningRef`: one pack-sync or launch run at a time.
    let busy = Arc::new(AtomicBool::new(false));
    // Mirrors the Tauri `cancelledRef`: labels a failed download as a cancel.
    let cancel_requested = Arc::new(AtomicBool::new(false));
    {
        let weak = app.as_weak();
        let state = state.clone();
        let handle = handle.clone();
        let busy = busy.clone();
        let cancel_requested = cancel_requested.clone();
        app.on_update_retry(move || {
            let Some(app) = weak.upgrade() else { return };
            if state.game.is_busy() || busy.swap(true, Ordering::SeqCst) {
                return;
            }
            cancel_requested.store(false, Ordering::SeqCst);
            reset_for_run(&app);
            let weak = weak.clone();
            let state = state.clone();
            let busy = busy.clone();
            let cancel_requested = cancel_requested.clone();
            handle.spawn(async move {
                let _guard = ClearBusy(busy.clone());
                let started = weak.clone();
                let outcome = run_retry(&state, &cancel_requested, |version| {
                    let version = version.clone();
                    let _ = started.upgrade_in_event_loop(move |app| {
                        mark_download_started(&app, &version);
                    });
                })
                .await;
                let _ = weak.upgrade_in_event_loop(move |app| {
                    apply_retry_outcome(&app, outcome);
                });
            });
        });
    }
    {
        let weak = app.as_weak();
        let state = state.clone();
        let handle = handle.clone();
        let busy = busy.clone();
        app.on_update_cancel(move || {
            let Some(app) = weak.upgrade() else { return };
            if !busy.load(Ordering::SeqCst) {
                return;
            }
            let mut data = app.get_update_data();
            if !try_begin_cancel(&mut data) {
                return;
            }
            cancel_requested.store(true, Ordering::SeqCst);
            app.set_update_data(data);
            let state = state.clone();
            handle.spawn(async move {
                if let Err(error) = install::cancel_download(&state).await {
                    eprintln!("cancel_download: {error}");
                }
            });
        });
    }
    {
        let weak = app.as_weak();
        let state = state.clone();
        let busy = busy.clone();
        app.on_update_launch(move || {
            let Some(app) = weak.upgrade() else { return };
            {
                let data = app.get_update_data();
                if data.phase != "complete" && data.phase != "uptodate" {
                    return;
                }
            }
            if state.game.is_busy() || busy.swap(true, Ordering::SeqCst) {
                return;
            }
            let mut data = app.get_update_data();
            data.launching = true;
            data.phase = "launching".into();
            data.status_message = "ЗАПУСК".into();
            data.step3 = 1;
            app.set_update_data(data);
            let weak = weak.clone();
            let state = state.clone();
            let hide = weak.clone();
            let restore = weak.clone();
            let busy = busy.clone();
            handle.spawn(async move {
                let _guard = ClearBusy(busy.clone());
                let pending = PendingRoom::default();
                let result = launch::launch_game(
                    &state,
                    &pending,
                    launcher_version(),
                    launch::LaunchWindow {
                        hide: Box::new(move || {
                            let _ = hide.upgrade_in_event_loop(|app| {
                                let _ = app.hide();
                            });
                        }),
                        restore: Box::new(move || {
                            let _ = restore.upgrade_in_event_loop(|app| {
                                let _ = app.show();
                                app.window().set_minimized(false);
                            });
                        }),
                    },
                )
                .await;
                let _ = weak.upgrade_in_event_loop(move |app| {
                    let mut data = app.get_update_data();
                    data.launching = false;
                    match result {
                        Ok(()) => {
                            data.phase = "complete".into();
                            data.status_message = "".into();
                            data.step3 = 2;
                        }
                        Err(error) => {
                            data.phase = "error".into();
                            data.error = error.into();
                        }
                    }
                    app.set_update_data(data);
                });
            });
        });
    }
    app.on_launcher_check(|| {
        eprintln!("POC launcher-update: no native updater backend (see module docs)");
    });
    app.on_launcher_install(|| {
        eprintln!("POC launcher-update: no native updater backend (see module docs)");
    });
    app.on_launcher_restart(|| {
        eprintln!("POC launcher-update: no native updater backend (see module docs)");
    });
}

/// Tauri `handleRetry` reset: checking with a fresh first step.
fn reset_for_run(app: &AppWindow) {
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

/// One pack-sync run: check the manifest, download when outdated (Tauri
/// `UpdateScreen` run: setup → java/loader/prune → download → verify).
/// `on_download_start` arms the UI (downloading phase, cancel) the moment the
/// install starts, mirroring the Tauri screen — the check phase alone leaves
/// cancel unreachable.
async fn run_retry(
    state: &LauncherAppState,
    cancel_requested: &AtomicBool,
    on_download_start: impl FnOnce(&VersionCheckResult),
) -> RetryOutcome {
    let version = match pack_check::check_modpack_version(state, |dir, meta| {
        let loader = meta
            .loader
            .as_ref()
            .map(|loader| minecraft::loader_version_id(&meta.minecraft, loader));
        loader
            .as_ref()
            .is_none_or(|loader| minecraft::loader_profile_exists(dir, loader))
            && minecraft::has_launch_dependencies(dir, &meta.minecraft, loader.as_deref())
    })
    .await
    {
        Ok(version) => version,
        Err(error) => return RetryOutcome::CheckFailed(error),
    };
    if !version.needs_update && version.java_ok && version.loader_ok {
        return RetryOutcome::UpToDate(version);
    }
    on_download_start(&version);
    match install::download_modpack(state).await {
        Ok(()) => RetryOutcome::Complete(version),
        Err(error) => RetryOutcome::DownloadFailed(if cancel_requested.load(Ordering::SeqCst) {
            "Загрузка отменена".into()
        } else {
            error
        }),
    }
}

enum RetryOutcome {
    UpToDate(VersionCheckResult),
    Complete(VersionCheckResult),
    CheckFailed(String),
    DownloadFailed(String),
}

/// Tauri `UpdateScreen` run: once the check says an update is needed, the
/// screen enters `downloading` before the backend install tasks run — that
/// transition is what arms cancel.
fn mark_download_started(app: &AppWindow, version: &VersionCheckResult) {
    let mut data = app.get_update_data();
    data.phase = "downloading".into();
    data.can_cancel = true;
    data.mirror_name = host_of(&version.mirror).into();
    data.step0 = 2;
    data.step1 = 1;
    let ts = stamp();
    let started = format!(
        "Update needed: {} → {}",
        version.installed_version, version.remote_version
    );
    data.log = log(&[
        (ts.as_str(), started.as_str(), 1),
        (ts.as_str(), "Waiting for backend install tasks...", 1),
    ]);
    app.set_update_data(data);
}

/// Cancel reaches the installer only from a cancelable download; anything
/// else leaves `cancel_requested` clear so a later failure is not mislabeled
/// as a cancel.
fn try_begin_cancel(data: &mut crate::UpdateData) -> bool {
    if data.phase != "downloading" || !data.can_cancel {
        return false;
    }
    data.can_cancel = false;
    data.status_message = "Отмена загрузки...".into();
    true
}

/// Clears `busy` when a spawned run ends, even on panic or a dead window —
/// without it one wedged task blocks every later retry/launch/cancel.
struct ClearBusy(Arc<AtomicBool>);

impl Drop for ClearBusy {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Apply a finished pack-sync run on the UI thread (Tauri phase/step/log flow).
fn apply_retry_outcome(app: &AppWindow, outcome: RetryOutcome) {
    let mut data = app.get_update_data();
    data.can_cancel = false;
    let ts = stamp();
    match outcome {
        RetryOutcome::UpToDate(version) => {
            data.phase = "uptodate".into();
            data.progress = 100.0;
            data.mirror_name = host_of(&version.mirror).into();
            data.step0 = 2;
            data.step1 = 2;
            data.step2 = 2;
            data.log = log(&[(
                ts.as_str(),
                "Modpack is up to date — no download needed.",
                0,
            )]);
        }
        RetryOutcome::Complete(version) => {
            let done = format!("Modpack {} installed successfully.", version.remote_version);
            data.phase = "complete".into();
            data.progress = 100.0;
            data.mirror_name = host_of(&version.mirror).into();
            data.step0 = 2;
            data.step1 = 2;
            data.step2 = 2;
            data.log = log(&[
                (ts.as_str(), "Install tasks complete.", 0),
                (
                    ts.as_str(),
                    "File integrity checked by packwiz-installer during sync.",
                    0,
                ),
                (ts.as_str(), done.as_str(), 0),
                (ts.as_str(), "Ready for deployment.", 1),
            ]);
        }
        RetryOutcome::CheckFailed(error) => {
            let line = format!("ERROR: Failed to fetch manifest — {error}");
            data.phase = "error".into();
            data.error = error.into();
            data.step0 = 1;
            data.log = log(&[(ts.as_str(), line.as_str(), 3)]);
        }
        RetryOutcome::DownloadFailed(error) => {
            let line = format!("ERROR: Download failed — {error}");
            data.phase = "error".into();
            data.error = error.into();
            data.step0 = 2;
            data.step1 = 1;
            data.log = log(&[(ts.as_str(), line.as_str(), 3)]);
        }
    }
    app.set_update_data(data);
}

/// Tauri `formatMirror`: the update screen shows the mirror host, not the URL.
fn host_of(url: &str) -> &str {
    let url = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    url.split('/').next().unwrap_or(url)
}

fn stamp() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

fn launcher_version() -> &'static str {
    // Shell-crate version, like the Tauri shell's `env!("CARGO_PKG_VERSION")`
    // at its call site (a core crate would see 0.0.0).
    env!("CARGO_PKG_VERSION")
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
    fn core_retry_outcomes_follow_the_tauri_flow() {
        i_slint_backend_testing::init_no_event_loop();
        let app = crate::AppWindow::new().unwrap();
        crate::fixtures::apply(&app);
        let version = || VersionCheckResult {
            needs_update: false,
            remote_version: "2.75".into(),
            installed_version: "2.75".into(),
            mirror: "https://mirror.blockfield.pro/pack".into(),
            file_count: 0,
            total_size: 0,
            java: None,
            java_ok: true,
            loader_ok: true,
        };
        super::apply_retry_outcome(&app, super::RetryOutcome::UpToDate(version()));
        let data = app.get_update_data();
        assert_eq!(data.phase, "uptodate");
        assert_eq!((data.step0, data.step1, data.step2), (2, 2, 2));
        assert_eq!(data.progress, 100.0);
        assert_eq!(data.mirror_name, "mirror.blockfield.pro");
        super::apply_retry_outcome(&app, super::RetryOutcome::Complete(version()));
        let data = app.get_update_data();
        assert_eq!(data.phase, "complete");
        assert_eq!((data.step0, data.step1, data.step2), (2, 2, 2));
        super::apply_retry_outcome(
            &app,
            super::RetryOutcome::CheckFailed("manifest down".into()),
        );
        let data = app.get_update_data();
        assert_eq!(data.phase, "error");
        assert_eq!(data.error, "manifest down");
        assert_eq!(data.step0, 1);
        super::apply_retry_outcome(
            &app,
            super::RetryOutcome::DownloadFailed("Загрузка отменена".into()),
        );
        let data = app.get_update_data();
        assert_eq!(data.phase, "error");
        assert_eq!(data.error, "Загрузка отменена");
        assert_eq!((data.step0, data.step1), (2, 1));
        assert_eq!(
            super::host_of("https://mirror.blockfield.pro/pack"),
            "mirror.blockfield.pro"
        );
        assert_eq!(
            super::host_of("mirror.blockfield.pro"),
            "mirror.blockfield.pro"
        );
    }
    #[test]
    fn download_start_arms_cancel_mid_download() {
        i_slint_backend_testing::init_no_event_loop();
        let app = crate::AppWindow::new().unwrap();
        crate::fixtures::apply(&app);
        // A fresh retry sits in checking: cancel must not arm there.
        super::reset_for_run(&app);
        let mut data = app.get_update_data();
        assert_eq!(data.phase, "checking");
        assert!(!super::try_begin_cancel(&mut data));
        // The install starting moves to downloading: cancel becomes reachable.
        let version = VersionCheckResult {
            needs_update: true,
            remote_version: "2.75".into(),
            installed_version: "2.74".into(),
            mirror: "https://mirror.blockfield.pro/pack".into(),
            file_count: 0,
            total_size: 0,
            java: None,
            java_ok: true,
            loader_ok: true,
        };
        super::mark_download_started(&app, &version);
        let mut data = app.get_update_data();
        assert_eq!(data.phase, "downloading");
        assert!(data.can_cancel);
        assert_eq!(data.mirror_name, "mirror.blockfield.pro");
        assert!(super::try_begin_cancel(&mut data));
        assert!(!data.can_cancel);
        assert!(!data.status_message.is_empty());
        // Second cancel is a no-op once disarmed.
        assert!(!super::try_begin_cancel(&mut data));
    }
    #[test]
    fn launcher_version_is_compile_time() {
        assert!(!super::launcher_version().is_empty());
    }
    #[test]
    fn busy_guard_clears_on_drop() {
        let busy = Arc::new(AtomicBool::new(true));
        drop(super::ClearBusy(busy.clone()));
        assert!(!busy.load(Ordering::SeqCst));
    }
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
