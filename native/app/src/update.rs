//! Update-screen operations: pack sync (nav screen) + launcher self-update
//! (settings launcher tab). Fixtures drive a deterministic local state
//! machine; the Core backend runs the real check → download → verify sequence
//! (same install/launch ops as the main screen) and reports back on the UI
//! thread. The launcher panel runs the real signed self-update backend
//! (`self_update` below, same release channel and pinned key as the Tauri
//! updater); fixtures only stand in for Layer-1 goldens.

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
/// real installer call. The launcher panel runs the real signed self-update
/// backend from the same task: check → download → verify → install with
/// progress, then restart (Tauri `launcher-update.ts` flow, same phases).
pub fn attach_core(app: &AppWindow, state: Arc<LauncherAppState>, handle: Handle) {
    // Mirrors the Tauri `runningRef`: one pack-sync or launch run at a time.
    let busy = Arc::new(AtomicBool::new(false));
    // Mirrors the Tauri `cancelledRef`: labels a failed download as a cancel.
    let cancel_requested = Arc::new(AtomicBool::new(false));
    // Launcher self-update staging (check result + in-flight guard), shared by
    // its three panel callbacks like the TS `pending` guard. Cloned up front:
    // the pack-sync launch callback below moves `handle` into its closure.
    let launcher = LauncherPending::default();
    let launcher_handle = handle.clone();
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
    {
        let weak = app.as_weak();
        let state = state.clone();
        let handle = launcher_handle.clone();
        let pending = launcher.clone();
        app.on_launcher_check(move || {
            on_launcher_check(&weak, &state, &handle, &pending);
        });
    }
    {
        let weak = app.as_weak();
        let state = state.clone();
        let handle = launcher_handle.clone();
        let pending = launcher.clone();
        app.on_launcher_install(move || {
            on_launcher_install(&weak, &state, &handle, &pending);
        });
    }
    {
        let weak = app.as_weak();
        let state = state.clone();
        let pending = launcher.clone();
        app.on_launcher_restart(move || {
            on_launcher_restart(&weak, &state, &pending);
        });
    }
}

/// One in-flight launcher self-update at a time (mirrors the TS `pending`
/// guard shared by check/install/restart) plus the staged release the check
/// found, cleared once it is installed or superseded.
#[derive(Clone, Default)]
struct LauncherPending {
    busy: Arc<AtomicBool>,
    release: Arc<std::sync::Mutex<Option<self_update::Release>>>,
    /// `current_exe()` captured before the atomic replace: after Linux swaps
    /// the binary, `current_exe()` resolves to the deleted inode, so restart
    /// must use this path.
    installed_exe: Arc<std::sync::Mutex<Option<std::path::PathBuf>>>,
}

/// Tauri `checkLauncherUpdate`: checking → available/current/error. DEV builds
/// report the current version with no check; debug builds refuse like the
/// Tauri `check_launcher_update` command.
fn on_launcher_check(
    weak: &slint::Weak<AppWindow>,
    state: &Arc<LauncherAppState>,
    handle: &Handle,
    pending: &LauncherPending,
) {
    let _ = state;
    let Some(app) = weak.upgrade() else { return };
    if launcher_core::dev::is_dev_build() {
        let mut data = app.get_launcher_update_data();
        data.phase = "current".into();
        data.current_version = launcher_version().into();
        app.set_launcher_update_data(data);
        return;
    }
    if cfg!(debug_assertions) {
        let mut data = app.get_launcher_update_data();
        data.phase = "error".into();
        data.error = "Обновления доступны в установленной release-версии лаунчера".into();
        app.set_launcher_update_data(data);
        return;
    }
    if pending.busy.swap(true, Ordering::SeqCst) {
        return;
    }
    let mut data = app.get_launcher_update_data();
    data.phase = "checking".into();
    data.error = "".into();
    data.current_version = launcher_version().into();
    app.set_launcher_update_data(data);
    let weak = weak.clone();
    let pending = pending.clone();
    handle.spawn(async move {
        let outcome = self_update::check_for_update().await;
        let _ = weak.upgrade_in_event_loop(move |app| {
            pending.busy.store(false, Ordering::SeqCst);
            let mut data = app.get_launcher_update_data();
            match outcome {
                Ok(Some(release)) => {
                    data.phase = "available".into();
                    data.update_version = release.version.clone().into();
                    data.update_body = release.body.clone().into();
                    data.checked_at = stamp().into();
                    *pending.release.lock().unwrap() = Some(release);
                }
                Ok(None) => {
                    data.phase = "current".into();
                    data.update_version = "".into();
                    data.update_body = "".into();
                    data.checked_at = stamp().into();
                    *pending.release.lock().unwrap() = None;
                }
                Err(error) => {
                    data.phase = "error".into();
                    data.error = error.into();
                }
            }
            app.set_launcher_update_data(data);
        });
    });
}

/// Tauri `installLauncherUpdate`: the staged release downloads with progress,
/// verifies its signature, installs, then the panel offers a restart. Needs a
/// staged check result and an idle game, like the Tauri command.
fn on_launcher_install(
    weak: &slint::Weak<AppWindow>,
    state: &Arc<LauncherAppState>,
    handle: &Handle,
    pending: &LauncherPending,
) {
    let Some(app) = weak.upgrade() else { return };
    if launcher_core::dev::is_dev_build() {
        let mut data = app.get_launcher_update_data();
        data.phase = "error".into();
        data.error = "Автоматические обновления недоступны в DEV-версии лаунчера".into();
        app.set_launcher_update_data(data);
        return;
    }
    let release = pending.release.lock().unwrap().clone();
    let Some(release) = release else { return };
    {
        let data = app.get_launcher_update_data();
        if data.installed || data.phase == "restarting" {
            return;
        }
    }
    if state.game.is_busy() {
        let mut data = app.get_launcher_update_data();
        data.phase = "error".into();
        data.error = "Закройте игру перед обновлением лаунчера".into();
        app.set_launcher_update_data(data);
        return;
    }
    if pending.busy.swap(true, Ordering::SeqCst) {
        return;
    }
    let mut data = app.get_launcher_update_data();
    data.phase = "downloading".into();
    data.error = "".into();
    data.downloaded = "".into();
    data.total = "".into();
    data.has_total = false;
    data.percent = 0;
    app.set_launcher_update_data(data);
    let weak = weak.clone();
    let pending = pending.clone();
    // Capture before the atomic replace: after Linux swaps the binary,
    // `current_exe()` points at the deleted inode.
    let exe_before = std::env::current_exe().ok();
    handle.spawn(async move {
        let progress = weak.clone();
        let stage_weak = weak.clone();
        let mut last_emit = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(1))
            .unwrap_or_else(std::time::Instant::now);
        let outcome = self_update::install_release(
            &release,
            move |downloaded: u64, total: Option<u64>| {
                let now = std::time::Instant::now();
                let finished = total.is_some_and(|total| total != 0 && downloaded >= total);
                if !finished
                    && now.duration_since(last_emit) < std::time::Duration::from_millis(100)
                {
                    return;
                }
                last_emit = now;
                let _ = progress.upgrade_in_event_loop(move |app| {
                    let mut data = app.get_launcher_update_data();
                    data.phase = "downloading".into();
                    data.downloaded = fmt_launcher_bytes(downloaded).into();
                    match total {
                        Some(total) => {
                            data.total = fmt_launcher_bytes(total).into();
                            data.has_total = true;
                            data.percent = downloaded
                                .min(total)
                                .saturating_mul(100)
                                .checked_div(total)
                                .unwrap_or(0) as i32;
                        }
                        None => {
                            data.total = "".into();
                            data.has_total = false;
                            data.percent = 0;
                        }
                    }
                    app.set_launcher_update_data(data);
                });
            },
            move |stage: self_update::InstallStage| {
                let phase = match stage {
                    self_update::InstallStage::Verifying => "verifying",
                    self_update::InstallStage::Installing => "installing",
                };
                let _ = stage_weak.upgrade_in_event_loop(move |app| {
                    let mut data = app.get_launcher_update_data();
                    set_panel_phase(&mut data, phase);
                    app.set_launcher_update_data(data);
                });
            },
        )
        .await;
        let _ = weak.upgrade_in_event_loop(move |app| {
            pending.busy.store(false, Ordering::SeqCst);
            let mut data = app.get_launcher_update_data();
            match outcome {
                Ok(()) => {
                    set_panel_phase(&mut data, "installed");
                    data.installed = true;
                    *pending.release.lock().unwrap() = None;
                    *pending.installed_exe.lock().unwrap() = exe_before;
                }
                Err(error) => {
                    data.phase = "error".into();
                    data.error = error.into();
                }
            }
            app.set_launcher_update_data(data);
        });
    });
}

/// Tauri `restartLauncher`: an installed update restarts the process once the
/// game is idle; anything else is a no-op.
fn on_launcher_restart(
    weak: &slint::Weak<AppWindow>,
    state: &Arc<LauncherAppState>,
    pending: &LauncherPending,
) {
    let Some(app) = weak.upgrade() else { return };
    {
        let data = app.get_launcher_update_data();
        if !data.installed || data.phase == "restarting" {
            return;
        }
    }
    if state.game.is_busy() {
        let mut data = app.get_launcher_update_data();
        data.phase = "error".into();
        data.error = "Закройте игру перед перезапуском лаунчера".into();
        app.set_launcher_update_data(data);
        return;
    }
    let mut data = app.get_launcher_update_data();
    data.phase = "restarting".into();
    data.error = "".into();
    app.set_launcher_update_data(data);
    // Restart the pre-replace path captured at install time: after the
    // atomic replace `current_exe()` is the deleted inode.
    let restart = match pending.installed_exe.lock().unwrap().clone() {
        Some(executable) => self_update::restart_launcher_with(&executable),
        None => self_update::restart_launcher(),
    };
    match restart {
        Ok(()) => std::process::exit(0),
        Err(error) => {
            let mut data = app.get_launcher_update_data();
            data.phase = "error".into();
            data.error = error.into();
            app.set_launcher_update_data(data);
        }
    }
}

/// Progress amounts in the panel's Russian units (`12 МБ`, `21.4 МБ`).
fn fmt_launcher_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} МБ", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.0} КБ", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} Б")
    }
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

/// Signed self-update for the native launcher: same release channel, pinned
/// key and verification rules as the Tauri updater
/// (`src-tauri/src/updater.rs` + `src-tauri/src/native_update.rs`).
/// Keep the three in sync on rotation.
///
/// Check fetches latest.json and stages the platform release. Install streams
/// the artifact with progress, verifies the minisign signature (tamper
/// reject, including the signed-version pin when the signature carries one),
/// then installs. Linux replaces the binary atomically with a `.previous`
/// backup (`rollback` restores it). Windows hands the verified bytes to the
/// OS installer, which owns rollback there.
pub mod self_update {
    use std::collections::HashMap;
    use std::path::Path;

    /// Pinned minisign public key. Source of truth:
    /// `src-tauri/tauri.conf.json` `plugins.updater.pubkey`.
    pub const UPDATE_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDk0RTBFNTI0NUE1MjBGQTYKUldTbUQxSmFKT1hnbEY3SllsMkF0dm1BNTdBK1Bnc29KWVdVK2x6SG93RmJhWUUyaGYzaUpISmMK";
    /// Same manifest endpoint the Tauri updater polls.
    pub const UPDATE_ENDPOINT: &str =
        "https://github.com/Blockfield/Blockfield-Launcher/releases/latest/download/latest.json";

    const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
    const MAX_PACKAGE_BYTES: u64 = 256 * 1024 * 1024;
    #[cfg(target_os = "linux")]
    const MAX_BINARY_BYTES: u64 = 128 * 1024 * 1024;
    #[cfg(target_os = "linux")]
    const MAX_UNPACKED_BYTES: u64 = 512 * 1024 * 1024;
    #[cfg(target_os = "linux")]
    const MARKER: &str = ".blockfield-native-install";

    /// Install stage reported back to the panel between download and done.
    pub enum InstallStage {
        Verifying,
        Installing,
    }

    /// A staged platform release: version the manifest announced, its notes,
    /// and where to fetch the signed bytes.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Release {
        pub version: String,
        pub body: String,
        pub url: String,
        pub signature: String,
    }

    #[derive(serde::Deserialize)]
    struct Manifest {
        version: String,
        #[serde(default)]
        notes: Option<String>,
        #[serde(default)]
        body: Option<String>,
        #[serde(default)]
        platforms: HashMap<String, ManifestPlatform>,
    }

    #[derive(serde::Deserialize)]
    struct ManifestPlatform {
        url: String,
        signature: String,
    }

    fn http_client() -> Result<reqwest::Client, String> {
        // Same 30 s ceiling the Tauri updater configures for check+download.
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| e.to_string())
    }

    /// Manifest platform keys to try in order. Linux installs from the
    /// `linux-{arch}-deb` artifact exactly like the Tauri updater target, and
    /// only from a marker install; Windows tries the msi entry first.
    pub fn platform_targets(executable: &Path) -> Result<Vec<String>, String> {
        #[cfg(target_os = "linux")]
        {
            let marker = executable
                .parent()
                .is_some_and(|dir| dir.join(MARKER).is_file());
            if !marker {
                return Err("Эта установка не поддерживает встроенное обновление".into());
            }
            Ok(vec![format!("linux-{}-deb", std::env::consts::ARCH)])
        }
        #[cfg(target_os = "windows")]
        {
            let _ = executable;
            return Ok(vec![
                format!("windows-{}", std::env::consts::ARCH),
                format!("windows-{}-nsis", std::env::consts::ARCH),
            ]);
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let _ = executable;
            return Err("Self-update is supported on Linux and Windows only".into());
        }
    }

    /// Parse the manifest, pick the first platform entry we ship, and report
    /// the release only when it is newer than the running binary (Tauri
    /// `release.version > current_version`, no downgrades).
    pub fn check_manifest(
        text: &str,
        targets: &[String],
        current: &str,
    ) -> Result<Option<Release>, String> {
        let manifest: Manifest =
            serde_json::from_str(text).map_err(|e| format!("Invalid update manifest: {e}"))?;
        let current = semver::Version::parse(current)
            .map_err(|e| format!("Invalid current version {current:?}: {e}"))?;
        let announced = semver::Version::parse(manifest.version.trim())
            .map_err(|e| format!("Invalid announced version {:?}: {e}", manifest.version))?;
        if announced <= current {
            return Ok(None);
        }
        let platform = targets
            .iter()
            .find_map(|target| manifest.platforms.get(target))
            .ok_or_else(|| format!("No update for this platform (tried {})", targets.join(", ")))?;
        if platform.url.trim().is_empty() || platform.signature.trim().is_empty() {
            return Err("Update entry is missing its URL or signature".into());
        }
        Ok(Some(Release {
            version: announced.to_string(),
            body: manifest
                .notes
                .or(manifest.body)
                .map(|notes| notes.trim().to_string())
                .unwrap_or_default(),
            url: platform.url.trim().to_string(),
            signature: platform.signature.trim().to_string(),
        }))
    }

    /// Full check pipeline: current binary → targets → manifest → staged
    /// release or `None` when up to date.
    pub async fn check_for_update() -> Result<Option<Release>, String> {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let targets = platform_targets(&executable)?;
        let client = http_client()?;
        let response = client
            .get(UPDATE_ENDPOINT)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        let text = response.text().await.map_err(|e| e.to_string())?;
        if text.len() as u64 > MAX_MANIFEST_BYTES {
            return Err("Update manifest is too large".into());
        }
        check_manifest(&text, &targets, super::launcher_version())
    }

    /// Stream the artifact with `(downloaded, total)` progress, capped like
    /// the Tauri deb path; the signature is verified by the caller before any
    /// byte is trusted.
    async fn download_bytes(
        client: &reqwest::Client,
        url: &str,
        mut on_progress: impl FnMut(u64, Option<u64>),
    ) -> Result<Vec<u8>, String> {
        let mut response = client
            .get(url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        let total = response.content_length().filter(|total| *total > 0);
        if total.is_some_and(|total| total > MAX_PACKAGE_BYTES) {
            return Err("Update package is too large".into());
        }
        on_progress(0, total);
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
            bytes.extend_from_slice(&chunk);
            if bytes.len() as u64 > MAX_PACKAGE_BYTES {
                return Err("Update package is too large".into());
            }
            on_progress(bytes.len() as u64, total);
        }
        on_progress(bytes.len() as u64, total.or(Some(bytes.len() as u64)));
        Ok(bytes)
    }

    /// Minisign verification exactly like the Tauri updater plugin: the
    /// manifest signature is base64 wrapping the minisign text, the key id
    /// must match the pinned key, and when the signature carries a `version:`
    /// trusted-comment field it must equal the announced version — otherwise a
    /// tampered manifest could pair a new version number with an older signed
    /// release.
    pub fn verify_signature(
        data: &[u8],
        signature_base64: &str,
        announced_version: &str,
    ) -> Result<(), String> {
        verify_signature_with_key(data, signature_base64, announced_version, UPDATE_PUBKEY)
    }

    fn verify_signature_with_key(
        data: &[u8],
        signature_base64: &str,
        announced_version: &str,
        pubkey_base64: &str,
    ) -> Result<(), String> {
        use base64::Engine as _;
        let text = base64::engine::general_purpose::STANDARD
            .decode(signature_base64.trim())
            .map_err(|e| format!("Invalid update signature: {e}"))?;
        let text =
            std::str::from_utf8(&text).map_err(|_| "Invalid update signature".to_string())?;
        let signature = minisign_verify::Signature::decode(text)
            .map_err(|e| format!("Invalid update signature: {e}"))?;
        // UPDATE_PUBKEY (and tauri.conf.json `plugins.updater.pubkey`) is
        // outer-base64(minisign public-key text): decode it first, then parse
        // the text form.
        let key_text = base64::engine::general_purpose::STANDARD
            .decode(pubkey_base64.trim())
            .map_err(|e| format!("Invalid pinned update key: {e}"))?;
        let key_text =
            std::str::from_utf8(&key_text).map_err(|_| "Invalid pinned update key".to_string())?;
        let public_key = minisign_verify::PublicKey::decode(key_text)
            .map_err(|e| format!("Invalid pinned update key: {e}"))?;
        public_key
            .verify(data, &signature, true)
            .map_err(|e| format!("Update signature check failed: {e}"))?;
        if let Some(signed) = signature
            .trusted_comment()
            .split('\t')
            .find_map(|field| field.strip_prefix("version:"))
            && signed != announced_version
        {
            return Err("Update was signed for a different version".into());
        }
        Ok(())
    }

    /// Download → verify → install one staged release, reporting the
    /// verify/install transitions so the panel can show them.
    pub async fn install_release(
        release: &Release,
        on_progress: impl FnMut(u64, Option<u64>),
        mut on_stage: impl FnMut(InstallStage),
    ) -> Result<(), String> {
        let client = http_client()?;
        let bytes = download_bytes(&client, &release.url, on_progress).await?;
        on_stage(InstallStage::Verifying);
        verify_signature(&bytes, &release.signature, &release.version)?;
        on_stage(InstallStage::Installing);
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        tokio::task::spawn_blocking(move || install_package(&bytes, &executable))
            .await
            .map_err(|e| e.to_string())?
    }

    /// Restart into the freshly installed binary (Tauri `relaunch`).
    /// `captured` is `current_exe()` taken before the atomic replace;
    /// `current` is `current_exe()` now. Prefers the capture; strips the
    /// Linux ` (deleted)` suffix when falling back.
    pub fn resolve_restart_path(captured: Option<&Path>, current: &Path) -> std::path::PathBuf {
        if let Some(captured) = captured {
            return captured.to_path_buf();
        }
        let current = current.to_string_lossy();
        if let Some(stripped) = current.strip_suffix(" (deleted)") {
            return std::path::PathBuf::from(stripped);
        }
        std::path::PathBuf::from(current.as_ref())
    }

    /// Restart the given binary (pre-replace path after an atomic install).
    pub fn restart_launcher_with(executable: &Path) -> Result<(), String> {
        launcher_core::host_env::command(executable)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("Не удалось перезапустить лаунчер: {e}"))?;
        Ok(())
    }

    /// Restart into the freshly installed binary (Tauri `relaunch`).
    pub fn restart_launcher() -> Result<(), String> {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        restart_launcher_with(&resolve_restart_path(None, &executable))
    }

    /// Install verified bytes for this OS. Linux only: see below.
    #[cfg(target_os = "linux")]
    pub fn install_package(package: &[u8], executable: &Path) -> Result<(), String> {
        let binary = extract_binary(package)?;
        replace_binary(&binary, executable)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Install verified bytes for this OS. Windows: stage the MSI/setup next
    /// to no user data and hand it to the OS installer, which owns the
    /// replace and its own rollback (same division as the Tauri updater).
    #[cfg(target_os = "windows")]
    pub fn install_package(package: &[u8], _executable: &Path) -> Result<(), String> {
        let msi = is_windows_msi(package);
        let staged = std::env::temp_dir().join(windows_staged_filename(msi));
        std::fs::write(&staged, package).map_err(|e| e.to_string())?;
        // Tauri updater behaviour: the OS installer owns completion. Launch
        // it and exit this (still-old) process instead of reporting
        // "installed" before the installer result is known.
        windows_installer_command(&staged, msi)
            .spawn()
            .map_err(|e| e.to_string())?;
        std::process::exit(0);
    }

    /// OLE compound magic = MSI; anything else we ship for Windows is the
    /// NSIS setup exe.
    #[cfg(any(target_os = "windows", test))]
    fn is_windows_msi(package: &[u8]) -> bool {
        package.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1])
    }

    #[cfg(any(target_os = "windows", test))]
    fn windows_staged_filename(msi: bool) -> &'static str {
        if msi {
            "blockfield-launcher-update.msi"
        } else {
            "blockfield-launcher-update.exe"
        }
    }

    #[cfg(any(target_os = "windows", test))]
    fn windows_installer_command(staged: &Path, msi: bool) -> std::process::Command {
        if msi {
            let mut command = std::process::Command::new("msiexec");
            command.arg("/i").arg(staged).arg("/passive");
            command
        } else {
            std::process::Command::new(staged)
        }
    }

    /// Restore the `.previous` backup over the executable (Linux; the backup
    /// is written by every install before the running binary is replaced).
    /// Re-validates the backup format first so rollback never installs a
    /// corrupt binary, but never touches the backup itself. No `ldd` gate
    /// here: the recovery path stays usable when the toolchain moved on —
    /// the backup ran as this binary before.
    #[cfg(target_os = "linux")]
    pub fn rollback(executable: &Path) -> Result<(), String> {
        use std::io::Write as _;
        use std::os::unix::fs::PermissionsExt as _;
        let directory = executable
            .parent()
            .ok_or_else(|| "No install directory".to_string())?;
        let backup = directory.join("blockfield-launcher.previous");
        let previous =
            std::fs::read(&backup).map_err(|e| format!("No rollback backup available: {e}"))?;
        validate_binary(&previous).map_err(|e| format!("Rollback backup is invalid: {e}"))?;
        let mut staged = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
        staged.write_all(&previous).map_err(|e| e.to_string())?;
        staged
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        staged.as_file().sync_all().map_err(|e| e.to_string())?;
        staged.persist(executable).map_err(|e| e.to_string())?;
        std::fs::File::open(directory)
            .and_then(|dir| dir.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// No local backup on this OS: the MSI/setup owns replace and rollback.
    #[cfg(not(target_os = "linux"))]
    pub fn rollback(_executable: &Path) -> Result<(), String> {
        Err("Rollback is handled by the OS installer on this platform".into())
    }

    /// Same deb layout the Tauri `native_update` installs: `data.tar.gz` must
    /// carry exactly one regular `usr/bin/blockfield-launcher` file holding a
    /// native executable for this CPU (AppImages rejected).
    #[cfg(target_os = "linux")]
    fn extract_binary(package: &[u8]) -> Result<Vec<u8>, String> {
        use std::io::{Cursor, Read as _};
        if package.len() as u64 > MAX_PACKAGE_BYTES || !package.starts_with(b"!<arch>\n") {
            return Err("Invalid or oversized Debian update package".into());
        }
        let mut archive = ar::Archive::new(Cursor::new(package));
        let mut binary = None;
        while let Some(entry) = archive.next_entry() {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.header().identifier() != b"data.tar.gz" {
                continue;
            }
            if binary.is_some() {
                return Err("Duplicate update payload".into());
            }
            let decoder = flate2::read::GzDecoder::new(entry).take(MAX_UNPACKED_BYTES);
            let mut data = tar::Archive::new(decoder);
            for entry in data.entries().map_err(|e| e.to_string())? {
                let mut entry = entry.map_err(|e| e.to_string())?;
                let path = entry.path().map_err(|e| e.to_string())?;
                let path = path.strip_prefix(".").unwrap_or(&path);
                if path != Path::new("usr/bin/blockfield-launcher") {
                    continue;
                }
                if binary.is_some()
                    || !entry.header().entry_type().is_file()
                    || entry.size() > MAX_BINARY_BYTES
                {
                    return Err("Invalid launcher executable in update".into());
                }
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
                validate_binary(&bytes)?;
                binary = Some(bytes);
            }
        }
        binary.ok_or_else(|| {
            "Update does not contain usr/bin/blockfield-launcher in data.tar.gz".into()
        })
    }

    /// Same CPU/AppImage gate the Tauri `native_update` enforces.
    #[cfg(target_os = "linux")]
    fn validate_binary(bytes: &[u8]) -> Result<(), String> {
        let machine = match std::env::consts::ARCH {
            "x86_64" => 62u16,
            "aarch64" => 183u16,
            _ => return Err("Native updates are not supported on this CPU architecture".into()),
        };
        if bytes.len() < 64
            || &bytes[..4] != b"\x7fELF"
            || bytes[4] != 2
            || bytes[5] != 1
            || u16::from_le_bytes([bytes[18], bytes[19]]) != machine
            || &bytes[8..10] == b"AI"
        {
            return Err(
                "Update must contain a native ELF executable for this CPU, not an AppImage".into(),
            );
        }
        Ok(())
    }

    /// Linked C libraries the new binary needs must already resolve (same
    /// `ldd` gate as the Tauri side: a reboot into a broken launcher is worse
    /// than refusing the update).
    #[cfg(target_os = "linux")]
    fn ldd_ok(path: &Path) -> Result<(), String> {
        let check = launcher_core::host_env::command("ldd")
            .arg(path)
            .env("LC_ALL", "C")
            .output()
            .map_err(|e| e.to_string())?;
        let diagnostics = format!(
            "{}{}",
            String::from_utf8_lossy(&check.stdout),
            String::from_utf8_lossy(&check.stderr)
        );
        if !check.status.success() || diagnostics.contains("not found") {
            return Err(format!(
                "Update requires unavailable system libraries: {diagnostics}"
            ));
        }
        Ok(())
    }

    /// Atomic replace with a `.previous` backup, same fsync discipline as the
    /// Tauri side: a crash before the final rename leaves the running binary
    /// (and the backup) intact.
    #[cfg(target_os = "linux")]
    fn replace_binary(binary: &[u8], executable: &Path) -> std::io::Result<std::path::PathBuf> {
        use std::io::Write as _;
        use std::os::unix::fs::PermissionsExt as _;
        validate_binary(binary).map_err(std::io::Error::other)?;
        let directory = executable
            .parent()
            .ok_or_else(|| std::io::Error::other("No install directory"))?;
        let mut staged = tempfile::NamedTempFile::new_in(directory)?;
        staged.write_all(binary)?;
        staged
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o755))?;
        staged.as_file().sync_all()?;
        ldd_ok(staged.path()).map_err(std::io::Error::other)?;
        let backup = tempfile::NamedTempFile::new_in(directory)?;
        std::fs::copy(executable, backup.path())?;
        backup.as_file().sync_all()?;
        let previous = directory.join("blockfield-launcher.previous");
        backup.persist(&previous)?;
        staged.persist(executable)?;
        std::fs::File::open(directory)?.sync_all()?;
        Ok(previous)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use base64::Engine as _;

        // Throwaway test key (NOT the pinned release key) in the exact
        // production encoding — outer-base64(minisign text), like
        // tauri.conf.json `plugins.updater.pubkey`. The inner second line is
        // b64("Ed" || 0123456789abcdef || ed25519 pubkey), minted for these
        // tests with openssl; the signatures below are its genuine legacy
        // ("Ed") signatures over TEST_DATA.
        const TEST_PUBKEY_PRODUCTION: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDAxMjM0NTY3ODlBQkNERUYKUldRQkkwVm5pYXZON3pPZFJ5VGN0RDE1ejRobTNDMmpyQXlkcXJNcTcwOWdadkhnR2JlQWxTMDEK";
        const TEST_DATA: &[u8] = b"native self-update test payload v1";
        // outer-base64(minisign text), trusted comment carries version:9.9.9.
        const TEST_SIG_VERSIONED: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHRlc3Qga2V5LCBkbyBub3QgdHJ1c3QKUldRQkkwVm5pYXZONzdkK0VrYTA1R1R4RFY1dXJIWjZQbEc4M3lEYU1ITzRRWGhVVHB1eERzbkhUMDBubmtqanlmdUk0NUtOTm5tK0RBSlgrVHloTGVrYlJBWE4wa0ZaMkFzPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkwMDAwMDAwCWZpbGU6dGVzdC1wYXlsb2FkCXZlcnNpb246OS45LjkKWnpHTm5ZVUplL1N6V1poV0NTVE9xdE1YRUNnZzNjV3JSQmZaWTkyTkJlcTJJNTk2RkVXZTJnL0VQSms5RS83SzlLUThwbituSVNoUC9zRGJjTm9TRFE9PQ==";
        // Same key and data, trusted comment without a version field.
        const TEST_SIG_UNVERSIONED: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHRlc3Qga2V5LCBkbyBub3QgdHJ1c3QKUldRQkkwVm5pYXZONzdkK0VrYTA1R1R4RFY1dXJIWjZQbEc4M3lEYU1ITzRRWGhVVHB1eERzbkhUMDBubmtqanlmdUk0NUtOTm5tK0RBSlgrVHloTGVrYlJBWE4wa0ZaMkFzPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkwMDAwMDAwCWZpbGU6dGVzdC1wYXlsb2FkClJaMTdaMDE2eDhKZm8rKzhQVlJycmgwZ1hmM25DdzlJWVBiVHI4Z3F2RHZnTTc2c2JVOVlqSVoreWZWVTlTSUZVekZRU2JXRnVWWGZMWGNsY2ljK0J3PT0=";

        fn check(sig: &str, announced: &str) -> Result<(), String> {
            verify_signature_with_key(TEST_DATA, sig, announced, TEST_PUBKEY_PRODUCTION)
        }

        #[test]
        fn tampered_package_is_rejected() {
            // The genuine signature verifies: the reject cases below fail on
            // the cryptography, not on always-reject.
            assert!(check(TEST_SIG_VERSIONED, "9.9.9").is_ok());
            // One flipped payload byte breaks the signature.
            let mut tampered = TEST_DATA.to_vec();
            tampered[0] ^= 1;
            assert!(
                verify_signature_with_key(
                    &tampered,
                    TEST_SIG_VERSIONED,
                    "9.9.9",
                    TEST_PUBKEY_PRODUCTION
                )
                .is_err()
            );
            // Not base64 at all, and base64 that is not a minisign text.
            assert!(check("!!!", "9.9.9").is_err());
            assert!(
                check(
                    &base64::engine::general_purpose::STANDARD.encode(b"nope"),
                    "9.9.9"
                )
                .is_err()
            );
            // A different key id never verifies our bytes.
            assert!(
                verify_signature_with_key(
                    TEST_DATA,
                    TEST_SIG_VERSIONED,
                    "9.9.9",
                    "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IE9USEVSS0VZClJXUWY2TFJDR0E5aTUzbWxZZWNPNEl6VDUxVEdQcHZXdWNOU0NoMUNCTTBRVGFMbjczWTdHRk8zCg==",
                )
                .is_err()
            );
        }

        #[test]
        fn signed_version_pin_rejects_a_repaired_manifest() {
            // The versioned signature pins 9.9.9: the announced version must match.
            assert!(check(TEST_SIG_VERSIONED, "9.9.9").is_ok());
            assert!(check(TEST_SIG_VERSIONED, "9.9.8").is_err());
            // No version field: nothing to pin (Tauri default behaviour).
            assert!(check(TEST_SIG_UNVERSIONED, "9.9.8").is_ok());
        }

        #[test]
        fn production_pubkey_format_verifies_a_genuine_signature() {
            // The pinned production key is outer-base64(minisign text), not a
            // raw binary-key encoding: a genuine signature made by the test
            // key must verify through that exact encoding.
            assert!(
                verify_signature_with_key(
                    TEST_DATA,
                    TEST_SIG_VERSIONED,
                    "9.9.9",
                    TEST_PUBKEY_PRODUCTION
                )
                .is_ok()
            );
            // The shipped production constant parses the same way (different
            // key, so the test payload itself must not verify).
            assert!(verify_signature(TEST_DATA, TEST_SIG_VERSIONED, "9.9.9").is_err());
        }

        #[test]
        fn restart_uses_the_pre_replace_executable_path() {
            // After the atomic replace, `current_exe()` is the deleted inode:
            // restart must use the path captured before replacing.
            let captured = Path::new("/opt/blockfield/blockfield-launcher");
            let deleted = Path::new("/opt/blockfield/blockfield-launcher (deleted)");
            assert_eq!(
                resolve_restart_path(Some(captured), deleted),
                captured.to_path_buf()
            );
            // Without a capture, fall back to the live path (stripping the
            // Linux deleted suffix).
            assert_eq!(
                resolve_restart_path(None, deleted),
                Path::new("/opt/blockfield/blockfield-launcher").to_path_buf()
            );
            assert_eq!(resolve_restart_path(None, captured), captured.to_path_buf());
        }

        #[test]
        fn windows_installer_hands_off_to_the_os_installer() {
            // The launcher never reports "installed" itself on Windows: the
            // OS installer owns completion (Tauri updater behaviour), so the
            // staged MSI goes through msiexec and anything else runs directly.
            assert!(is_windows_msi(&[
                0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0x00
            ]));
            assert!(!is_windows_msi(b"MZ fake setup exe"));
            assert_eq!(
                windows_staged_filename(true),
                "blockfield-launcher-update.msi"
            );
            assert_eq!(
                windows_staged_filename(false),
                "blockfield-launcher-update.exe"
            );
            let staged = Path::new("/tmp/blockfield-launcher-update.msi");
            let command = windows_installer_command(staged, true);
            assert_eq!(command.get_program(), "msiexec");
            let args: Vec<_> = command.get_args().collect();
            assert_eq!(args.len(), 3);
            let setup = Path::new("/tmp/blockfield-launcher-update.exe");
            let command = windows_installer_command(setup, false);
            assert_eq!(command.get_program(), setup.as_os_str());
            // A launch failure surfaces as an error, never as "installed".
            assert!(
                std::process::Command::new("/nonexistent/blockfield-installer")
                    .spawn()
                    .map_err(|e| e.to_string())
                    .is_err()
            );
        }

        #[test]
        fn manifest_picks_the_platform_entry_and_compares_versions() {
            let manifest = serde_json::json!({
                "version": "1.0.20",
                "notes": "Fixes",
                "platforms": {
                    "windows-x86_64": {"url": "https://example.invalid/w.msi", "signature": "eA=="},
                    "linux-x86_64-deb": {"url": "https://example.invalid/l.deb", "signature": "c2ln"},
                }
            })
            .to_string();
            let targets = vec!["linux-x86_64-deb".to_string(), "windows-x86_64".to_string()];
            let release = check_manifest(&manifest, &targets, "1.0.19")
                .unwrap()
                .unwrap();
            assert_eq!(release.version, "1.0.20");
            assert_eq!(release.body, "Fixes");
            assert_eq!(release.url, "https://example.invalid/l.deb");
            assert_eq!(release.signature, "c2ln");
            // Up to date and older announced versions stage nothing.
            assert!(
                check_manifest(&manifest, &targets, "1.0.20")
                    .unwrap()
                    .is_none()
            );
            let older = manifest.replace("1.0.20", "1.0.18");
            assert!(
                check_manifest(&older, &targets, "1.0.19")
                    .unwrap()
                    .is_none()
            );
            // First listed target wins.
            let release = check_manifest(
                &manifest,
                &["windows-x86_64".to_string(), "linux-x86_64-deb".to_string()],
                "1.0.19",
            )
            .unwrap()
            .unwrap();
            assert_eq!(release.url, "https://example.invalid/w.msi");
            // Unknown platform, broken JSON and broken versions are errors, not silence.
            assert!(check_manifest(&manifest, &["plan9-ppc".to_string()], "1.0.19").is_err());
            assert!(check_manifest("not json", &targets, "1.0.19").is_err());
            assert!(check_manifest(&manifest, &targets, "bogus").is_err());
            let broken = manifest.replace("1.0.20", "bogus");
            assert!(check_manifest(&broken, &targets, "1.0.19").is_err());
        }

        #[test]
        fn progress_bytes_use_russian_units() {
            assert_eq!(super::super::fmt_launcher_bytes(512), "512 Б");
            assert_eq!(super::super::fmt_launcher_bytes(2048), "2 КБ");
            assert_eq!(
                super::super::fmt_launcher_bytes(12 * 1024 * 1024),
                "12.0 МБ"
            );
        }

        #[cfg(target_os = "linux")]
        mod linux {
            use super::super::*;
            use std::io::Cursor;

            /// Minimal ar+tar.gz deb carrying the given entries, like the
            /// Tauri `native_update` tests.
            fn package(entries: &[(&str, tar::EntryType, &[u8])]) -> Vec<u8> {
                let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
                    Vec::new(),
                    flate2::Compression::default(),
                ));
                for (path, kind, bytes) in entries {
                    let mut header = tar::Header::new_gnu();
                    header.set_size(bytes.len() as u64);
                    header.set_mode(0o755);
                    header.set_entry_type(*kind);
                    header.set_cksum();
                    tar.append_data(&mut header, path, *bytes).unwrap();
                }
                let data = tar.into_inner().unwrap().finish().unwrap();
                let mut ar = ar::Builder::new(Vec::new());
                ar.append(
                    &ar::Header::new(b"data.tar.gz".to_vec(), data.len() as u64),
                    Cursor::new(data),
                )
                .unwrap();
                ar.into_inner().unwrap()
            }

            /// 64-byte ELF header for `machine`, otherwise zeroed.
            fn elf(machine: u16) -> Vec<u8> {
                let mut header = vec![0u8; 64];
                header[..4].copy_from_slice(b"\x7fELF");
                header[4] = 2;
                header[5] = 1;
                header[18..20].copy_from_slice(&machine.to_le_bytes());
                header
            }

            const HOST_MACHINE: u16 = if cfg!(target_arch = "aarch64") {
                183
            } else {
                62
            };
            const FOREIGN_MACHINE: u16 = if cfg!(target_arch = "aarch64") {
                62
            } else {
                183
            };

            #[test]
            fn foreign_arch_payload_is_rejected() {
                // Host CPU passes; anything else is refused before install.
                assert!(validate_binary(&elf(HOST_MACHINE)).is_ok());
                assert!(validate_binary(&elf(FOREIGN_MACHINE)).is_err());
                // AppImage masquerade, truncation and bad magic are refused.
                let mut appimage = elf(HOST_MACHINE);
                appimage[8..10].copy_from_slice(b"AI");
                assert!(validate_binary(&appimage).is_err());
                assert!(validate_binary(&elf(HOST_MACHINE)[..63]).is_err());
                assert!(validate_binary(b"not an elf at all, padding........").is_err());
            }

            #[test]
            fn rollback_restores_the_previous_binary() {
                let directory = tempfile::tempdir().unwrap();
                let executable = directory.path().join("blockfield-launcher");
                let old = std::fs::read("/bin/true").unwrap();
                let new = std::fs::read("/bin/false").unwrap();
                std::fs::write(&executable, &old).unwrap();
                // No backup yet: rollback refuses instead of writing garbage.
                assert!(rollback(&executable).is_err());
                std::fs::write(directory.path().join(MARKER), b"1\n").unwrap();
                // Install stages the new binary and backs up the old one.
                replace_binary(&new, &executable).unwrap();
                assert_eq!(std::fs::read(&executable).unwrap(), new);
                assert_eq!(
                    std::fs::read(directory.path().join("blockfield-launcher.previous")).unwrap(),
                    old
                );
                // A failed update leaves garbage behind; rollback revives the backup.
                std::fs::write(&executable, b"broken").unwrap();
                rollback(&executable).unwrap();
                assert_eq!(std::fs::read(&executable).unwrap(), old);
                // A corrupt backup is never installed.
                std::fs::write(
                    directory.path().join("blockfield-launcher.previous"),
                    b"broken",
                )
                .unwrap();
                assert!(rollback(&executable).is_err());
                assert_eq!(std::fs::read(&executable).unwrap(), old);
            }

            #[test]
            fn deb_extract_keeps_only_the_native_executable() {
                let binary = std::fs::read("/bin/true").unwrap();
                let deb = package(&[
                    ("usr/share/ignored", tar::EntryType::Regular, b"ignore"),
                    (
                        "usr/bin/blockfield-launcher",
                        tar::EntryType::Regular,
                        &binary,
                    ),
                ]);
                assert_eq!(extract_binary(&deb).unwrap(), binary);
                assert!(extract_binary(b"not a deb").is_err());
                let deb = package(&[("usr/bin/blockfield-launcher", tar::EntryType::Symlink, b"")]);
                assert!(extract_binary(&deb).is_err());
                let mut appimage = binary.clone();
                appimage[8..11].copy_from_slice(b"AI\x02");
                assert!(validate_binary(&appimage).is_err());
            }
        }
    }
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
