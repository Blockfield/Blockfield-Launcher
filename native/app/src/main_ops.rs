//! Main-screen operations run on the core runtime; only property updates return to Slint.
use std::sync::Arc;

use launcher_app::{
    LauncherAppState, VersionCheckResult,
    ops::{install, launch, pack_check},
    rooms::{PendingRoom, pending_room},
};
use launcher_net::minecraft;
use slint::ComponentHandle;
use tokio::runtime::Handle;

use crate::{AppWindow, MainData};

/// One pending-invite store per process: the Discord presence loop, the
/// RoomPending auto-join and the manual join flow must see the same target
/// (Tauri keeps a single `PendingRoom` on its app handle).
pub fn shared_pending() -> Arc<PendingRoom> {
    static PENDING: std::sync::OnceLock<Arc<PendingRoom>> = std::sync::OnceLock::new();
    PENDING
        .get_or_init(|| Arc::new(PendingRoom::default()))
        .clone()
}

/// Serializes the tests that share the process-wide pending-invite store (and
/// the bridge auto-join one-shot): Rust runs tests on threads, so without
/// this two tests reset and read the store under each other.
#[cfg(test)]
pub(crate) fn test_serial() -> std::sync::MutexGuard<'static, ()> {
    static SERIAL: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    SERIAL
        .get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn launch_sub(status: slint::SharedString, progress: i32) -> slint::SharedString {
    let status = if status.is_empty() {
        "ЗАПУСК"
    } else {
        &status
    };
    let text = if progress < 0 {
        status.to_owned()
    } else {
        format!("{status} · {progress}%")
    };
    // React slices UTF-16 code units, including the percentage suffix.
    String::from_utf16_lossy(&text.encode_utf16().take(64).collect::<Vec<_>>()).into()
}

#[derive(Clone, Copy)]
enum Action {
    Deploy,
    Verify,
    Download,
    Cancel,
}

pub fn attach_fixtures(app: &AppWindow) {
    let weak = app.as_weak();
    app.on_download_modpack(move || {
        if let Some(app) = weak.upgrade() {
            let mut data = app.get_main_data();
            data.launching = true;
            data.launch_status = "Downloading fixture pack".into();
            data.progress = 42;
            app.set_main_data(data);
        }
    });
    let weak = app.as_weak();
    app.on_cancel_download(move || {
        if let Some(app) = weak.upgrade() {
            let mut data = app.get_main_data();
            data.launching = false;
            data.progress = -1;
            data.launch_status = "".into();
            app.set_main_data(data);
        }
    });
}

pub fn attach(app: &AppWindow, state: Arc<LauncherAppState>, handle: Handle) {
    let pending = shared_pending();
    for action in [
        Action::Deploy,
        Action::Verify,
        Action::Download,
        Action::Cancel,
    ] {
        let weak = app.as_weak();
        let state = state.clone();
        let pending = pending.clone();
        let handle = handle.clone();
        let callback = move || {
            let Some(app) = weak.upgrade() else { return };
            let mut data = app.get_main_data();
            if !matches!(action, Action::Cancel) {
                if data.checking
                    || data.launching
                    || data.phase != "idle"
                    || (!matches!(action, Action::Verify)
                        && (!data.checked || data.profile_changing || data.profile_blocked))
                {
                    return;
                }
                data.check_error = "".into();
                data.launch_error = "".into();
                if matches!(action, Action::Verify) {
                    data.checking = true;
                } else {
                    data.launching = true;
                    data.launch_status = "ЗАПУСК".into();
                    data.progress = -1;
                }
                app.set_main_data(data.clone());
            }
            let weak = weak.clone();
            let state = state.clone();
            let pending = pending.clone();
            handle.spawn(async move {
                if matches!(action, Action::Cancel) {
                    if let Err(error) = install::cancel_download(&state).await {
                        eprintln!("cancel_download: {error}");
                    }
                    return;
                }
                let result = run(action, &state, &pending, &weak, data.needs_setup).await;
                let _ = weak.upgrade_in_event_loop(move |app| {
                    let mut data = app.get_main_data();
                    data.checking = false;
                    data.launching = false;
                    match result {
                        Ok(Some(version)) => apply_version(&mut data, version),
                        Ok(None) => {}
                        Err(error) if matches!(action, Action::Verify) => {
                            data.check_error = error.into()
                        }
                        Err(error) => data.launch_error = error.into(),
                    }
                    app.set_main_data(data);
                    // Pack check done / launch idle: a still-waiting invite
                    // retries now (Tauri MainScreen.tsx pending-room effect).
                    crate::bridge::retry_pending_join(&app);
                });
            });
        };
        match action {
            Action::Deploy => app.on_deploy(callback),
            Action::Verify => app.on_verify(callback),
            Action::Download => app.on_download_modpack(callback),
            Action::Cancel => app.on_cancel_download(callback),
        }
    }
    let weak = app.as_weak();
    app.on_retry_join(move || {
        // main.slint `changed workshop`: the profile flipped back to Game.
        if let Some(app) = weak.upgrade() {
            crate::bridge::retry_pending_join(&app);
        }
    });
    let weak = app.as_weak();
    app.on_join(move |id| {
        // A join was attempted: consume the auto-join one-shot so later
        // state changes do not rejoin the same invite (Tauri `handledRoom`).
        crate::bridge::note_join(&id);
        let state = state.clone();
        let pending = pending.clone();
        let weak = weak.clone();
        handle.spawn_blocking(move || {
            let running = state.game.snapshot().phase == launcher_core::game::GamePhase::Running;
            let result = if running {
                launcher_app::rooms::join_running_room(&state, id.to_string())
                    .and_then(|()| launcher_app::rooms::select_room(&state, &pending, None))
            } else {
                launcher_app::rooms::select_room(&state, &pending, Some(id.to_string()))
            };
            let _ = weak.upgrade_in_event_loop(move |app| {
                app.set_room_message(match result {
                    Ok(()) if running => {
                        "Запрос передан игре. Результат входа появится в игровом меню.".into()
                    }
                    Ok(()) => {
                        app.invoke_deploy();
                        if id == "lobby" {
                            "После подготовки игра подключится к лобби."
                        } else {
                            "После подготовки игра подключится к выбранному матчу."
                        }
                        .into()
                    }
                    Err(error) => format!("Не удалось принять приглашение. {error}").into(),
                });
            });
        });
    });
}

async fn run(
    action: Action,
    state: &LauncherAppState,
    pending: &PendingRoom,
    weak: &slint::Weak<AppWindow>,
    needs_setup: bool,
) -> Result<Option<VersionCheckResult>, String> {
    if matches!(action, Action::Verify) {
        return check(state, true).await.map(Some);
    }
    let needs_setup = if matches!(action, Action::Deploy) && state.config.read().await.auto_update {
        let version = check(state, true).await?;
        version.needs_update || !version.java_ok || !version.loader_ok
    } else {
        needs_setup
    };
    if matches!(action, Action::Download) || needs_setup {
        install::download_modpack(state).await?;
        let version = check(state, false).await?;
        if !join_continues(action, pending) {
            return Ok(Some(version));
        }
        // Invite auto-join, Tauri auto-launch-after-sync
        // (src/lib/auto-launch.ts + UpdateScreen effect, ported as
        // ops::launch::should_auto_launch_after_sync): a join-triggered
        // deploy continues into launch with the saved room instead of
        // stopping on the done version.
        launch(state, pending, weak).await?;
        return Ok(Some(version));
    }
    launch(state, pending, weak).await?;
    Ok(None)
}

/// Invite auto-join continuation: only a deploy with a saved invite proceeds
/// into launch after the pack sync; a manual download stays on the done
/// version, like Tauri's Play-triggered-only auto-launch.
fn join_continues(action: Action, pending: &PendingRoom) -> bool {
    matches!(action, Action::Deploy) && pending_room(pending).is_some()
}

async fn launch(
    state: &LauncherAppState,
    pending: &PendingRoom,
    weak: &slint::Weak<AppWindow>,
) -> Result<(), String> {
    let hide = weak.clone();
    let restore = weak.clone();
    launch::launch_game(
        state,
        pending,
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
    .await
}

pub async fn check(state: &LauncherAppState, verify: bool) -> Result<VersionCheckResult, String> {
    let profile = state.config.read().await.active_profile;
    let version = pack_check::check_modpack_version(state, |dir, meta| {
        let loader = meta
            .loader
            .as_ref()
            .map(|loader| minecraft::loader_version_id(&meta.minecraft, loader));
        loader
            .as_ref()
            .is_none_or(|id| minecraft::loader_profile_exists(dir, id))
            && minecraft::has_launch_dependencies(dir, &meta.minecraft, loader.as_deref())
    })
    .await?;
    if verify && !version.needs_update && version.java_ok && version.loader_ok {
        let failed = install::verify_files(state, Some(profile)).await?;
        if !failed.is_empty() {
            return Err(format!("Не удалось проверить файлы: {}", failed.join(", ")));
        }
    }
    Ok(version)
}

fn apply_version(data: &mut MainData, version: VersionCheckResult) {
    data.checked = true;
    data.needs_update = version.needs_update;
    data.needs_setup = version.needs_update || !version.java_ok || !version.loader_ok;
    data.installed = version.installed_version.into();
    data.latest = version.remote_version.into();
    data.size = if version.total_size == 0 {
        "—".into()
    } else {
        format!("{:.1} GB", version.total_size as f64 / 1e9).into()
    };
}

fn launcher_version() -> &'static str {
    // Own crate version on the native release line; keep in sync with
    // `update::launcher_version` (same value, same source).
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    #[test]
    fn join_continues_into_launch_only_for_pending_deploys() {
        // After a join-triggered deploy finishes the pack sync, the saved
        // room continues into launch (Tauri auto-launch-after-sync,
        // src/lib/auto-launch.ts + UpdateScreen effect, ported as
        // ops::launch::should_auto_launch_after_sync); a manual download or
        // a deploy with no invite stays on the done version.
        let _serial = super::test_serial();
        let pending = super::shared_pending();
        *pending.0.lock().unwrap() = None;
        assert!(!super::join_continues(super::Action::Deploy, &pending));
        *pending.0.lock().unwrap() = Some("room-1".to_owned());
        assert!(super::join_continues(super::Action::Deploy, &pending));
        assert!(!super::join_continues(super::Action::Download, &pending));
        assert!(!super::join_continues(super::Action::Verify, &pending));
        assert!(!super::join_continues(super::Action::Cancel, &pending));
        *pending.0.lock().unwrap() = None;
    }

    #[test]
    fn launch_sub_matches_react_slice_and_suffix() {
        assert_eq!(super::launch_sub("".into(), -1), "ЗАПУСК");
        assert_eq!(super::launch_sub("Файлы".into(), 42), "Файлы · 42%");
        let status = "Я".repeat(62);
        assert_eq!(
            super::launch_sub(status.into(), 42),
            format!("{} ·", "Я".repeat(62))
        );
        assert_eq!(
            super::launch_sub("😀".repeat(33).into(), -1),
            "😀".repeat(32)
        );
        assert_eq!(super::launcher_version(), "1.0.20");
    }
}
