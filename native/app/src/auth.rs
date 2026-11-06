//! Login + FirstRun screens: thin Slint adapters over the shared core.
//! Auth itself stays in `launcher-net::account`, settings persistence in
//! `launcher-app::ops::settings`. Mirrors the Tauri gating in App.tsx:
//! logged-out (or password migration) shows LoginScreen; logged-in but no
//! stored username shows FirstRunScreen; otherwise the main Shell.
use std::sync::Arc;

use launcher_app::{LauncherAppState, ops::settings as settings_ops};
use launcher_core::config::{max_ram_mb, valid_username};
use launcher_net::account;
use slint::{ComponentHandle, ModelRc, VecModel};
use tokio::runtime::Handle;

use crate::AppWindow;

const RAM_MIN_GB: i32 = 2;
const RAM_PRESETS_GB: [i32; 6] = [4, 6, 8, 12, 16, 24];

/// Mirrors `ramLimitGb` in src/lib/ram.ts.
fn ram_limit_gb(limit_mb: u32) -> i32 {
    (limit_mb / 1024).min(32) as i32
}

/// Mirrors `clampRamGb` in src/lib/ram.ts.
fn clamp_ram_gb(value: i32, max: i32) -> i32 {
    value.clamp(RAM_MIN_GB, max.max(RAM_MIN_GB))
}

async fn skins_base(state: &LauncherAppState) -> Option<String> {
    state
        .pack
        .read()
        .await
        .as_ref()
        .and_then(|pack| pack.info.skins.clone())
}

fn status_field(status: &account::AccountStatus, field: &str) -> String {
    // AccountStatus keeps its fields private; read the redacted op response
    // the same way backend.rs reads SettingsResponse.
    serde_json::to_value(status)
        .ok()
        .and_then(|v| v.get(field).cloned())
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

pub fn status_logged_in(status: &account::AccountStatus) -> bool {
    status_flag(status, "loggedIn")
}

pub fn status_needs_password(status: &account::AccountStatus) -> bool {
    status_flag(status, "needsPassword")
}

pub fn status_username(status: &account::AccountStatus) -> String {
    status_field(status, "username")
}

fn status_flag(status: &account::AccountStatus, field: &str) -> bool {
    serde_json::to_value(status)
        .ok()
        .and_then(|v| v.get(field).cloned())
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Recompute the name check and submit availability after any field edit.
fn refresh_login(app: &AppWindow) {
    let set_password = app.get_login_set_password();
    let name = app.get_login_username();
    let name_valid = set_password || valid_username(name.trim());
    app.set_login_name_valid(name_valid);
    app.set_login_can_submit(
        name_valid && !app.get_login_password().is_empty() && !app.get_login_busy(),
    );
}

fn show_login(app: &AppWindow, mode: &str, set_password: bool, username: &str) {
    app.set_show_main(false);
    app.set_show_firstrun(false);
    app.set_login_mode(mode.into());
    app.set_login_set_password(set_password);
    app.set_login_username(username.into());
    app.set_login_password("".into());
    app.set_login_busy(false);
    app.set_login_error("".into());
    app.set_show_login(true);
    refresh_login(app);
}

fn apply_status(app: &AppWindow, status: &account::AccountStatus) {
    app.set_username(status_field(status, "username").into());
}

/// Wire the auth screens to the core runtime (same code path as production).
pub fn attach(app: &AppWindow, state: Arc<LauncherAppState>, handle: Handle, first_run: bool) {
    let weak = app.as_weak();
    app.on_login_username_edited(move |_| {
        if let Some(app) = weak.upgrade() {
            refresh_login(&app);
        }
    });
    let weak = app.as_weak();
    app.on_login_password_edited(move |_| {
        if let Some(app) = weak.upgrade() {
            refresh_login(&app);
        }
    });
    let weak = app.as_weak();
    app.on_login_accept(move || {
        if let Some(app) = weak.upgrade()
            && app.get_login_can_submit()
        {
            app.invoke_login_submit();
        }
    });
    let weak = app.as_weak();
    app.on_login_toggle_mode(move || {
        if let Some(app) = weak.upgrade() {
            app.set_login_mode(
                if app.get_login_mode() == "register" {
                    "login"
                } else {
                    "register"
                }
                .into(),
            );
            app.set_login_error("".into());
        }
    });
    let weak = app.as_weak();
    let submit_state = state.clone();
    let submit_handle = handle.clone();
    let submit_first_run = first_run;
    app.on_login_submit(move || {
        let Some(app) = weak.upgrade() else { return };
        refresh_login(&app);
        if !app.get_login_can_submit() {
            return;
        }
        app.set_login_busy(true);
        app.set_login_error("".into());
        refresh_login(&app);
        let weak = weak.clone();
        let state = submit_state.clone();
        let handle = submit_handle.clone();
        let spawn = handle.clone();
        let set_password = app.get_login_set_password();
        let register = app.get_login_mode() == "register";
        let username = app.get_login_username().to_string();
        let password = app.get_login_password().to_string();
        spawn.spawn(async move {
            let skins = skins_base(&state).await;
            let result = if set_password {
                account::change_password(&state.config, &state.app_data_dir, skins, password).await
            } else if register {
                account::register(
                    &state.config,
                    &state.app_data_dir,
                    skins,
                    username,
                    password,
                )
                .await
            } else {
                account::login(
                    &state.config,
                    &state.app_data_dir,
                    skins,
                    username,
                    password,
                )
                .await
            };
            let _ = weak.upgrade_in_event_loop(move |app| {
                on_auth_done(&app, &state, &handle, submit_first_run, result);
            });
        });
    });
    let weak = app.as_weak();
    let logout_state = state.clone();
    let logout_handle = handle.clone();
    app.on_login_logout(move || {
        let Some(app) = weak.upgrade() else { return };
        app.set_login_busy(true);
        let weak = weak.clone();
        let state = logout_state.clone();
        let handle = logout_handle.clone();
        let spawn = handle.clone();
        spawn.spawn(async move {
            let skins = skins_base(&state).await;
            let result = account::logout(&state.config, &state.app_data_dir, skins).await;
            let _ = weak.upgrade_in_event_loop(move |app| {
                app.set_login_busy(false);
                match result {
                    Ok(status) => {
                        apply_status(&app, &status);
                        show_login(&app, "login", false, &status_username(&status));
                    }
                    Err(error) => app.set_login_error(error.into()),
                }
            });
        });
    });
    let weak = app.as_weak();
    app.on_firstrun_ram_text_edited(move |text| {
        if let Some(app) = weak.upgrade() {
            let max = app.get_firstrun_maxram();
            if let Ok(value) = text.trim().parse::<i32>() {
                app.set_firstrun_ram(clamp_ram_gb(value, max));
            }
        }
    });
    let weak = app.as_weak();
    app.on_firstrun_browse_dir(move || {
        if weak.upgrade().is_some() {
            // Pickers are U11; settings skin browse is a stub the same way.
            eprintln!("POC firstrun: game-dir browse is a stub (no backend)");
        }
    });
    let weak = app.as_weak();
    app.on_firstrun_browse_java(move || {
        if weak.upgrade().is_some() {
            eprintln!("POC firstrun: java browse is a stub (no backend)");
        }
    });
    let weak = app.as_weak();
    let continue_state = state.clone();
    let continue_handle = handle.clone();
    app.on_firstrun_continue(move || {
        let Some(app) = weak.upgrade() else { return };
        if app.get_firstrun_saving() || app.get_firstrun_maxram() < RAM_MIN_GB {
            return;
        }
        app.set_firstrun_saving(true);
        app.set_firstrun_error("".into());
        let weak = weak.clone();
        let state = continue_state.clone();
        let handle = continue_handle.clone();
        let spawn = handle.clone();
        let dir = app.get_firstrun_game_dir().trim().to_string();
        let java = app.get_firstrun_java_path().trim().to_string();
        let ram_mb =
            (clamp_ram_gb(app.get_firstrun_ram(), app.get_firstrun_maxram()) as u32) * 1024;
        spawn.spawn(async move {
            let blocking = state.clone();
            let save = tokio::task::spawn_blocking(move || {
                let mut current = blocking.config.blocking_read().clone();
                current.game_dir = dir;
                current.java_path = java;
                current.ram_mb = ram_mb;
                let dir = blocking.app_data_dir.clone();
                settings_ops::save_settings(&blocking, &dir, current)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);
            let _ = weak.upgrade_in_event_loop(move |app| {
                app.set_firstrun_saving(false);
                match save {
                    Ok(()) => crate::backend::enter_main(&app, state, handle),
                    Err(error) => app.set_firstrun_error(error.into()),
                }
            });
        });
    });
}

fn on_auth_done(
    app: &AppWindow,
    state: &Arc<LauncherAppState>,
    handle: &Handle,
    first_run: bool,
    result: Result<account::AccountStatus, String>,
) {
    app.set_login_busy(false);
    match result {
        Err(error) => {
            app.set_login_error(error.into());
            refresh_login(app);
        }
        Ok(status) => {
            apply_status(app, &status);
            if status_needs_password(&status) {
                show_login(app, "login", true, &status_username(&status));
            } else if first_run || status_username(&status).is_empty() {
                enter_firstrun(app, state.clone(), handle.clone());
            } else {
                crate::backend::enter_main(app, state.clone(), handle.clone());
            }
        }
    }
}

/// Fill the FirstRun form from the stored (or platform-default) settings and
/// show it. `load_settings` already returns sane defaults when nothing was
/// saved yet, so this mirrors the Tauri `!cfg.username` gate exactly.
pub fn enter_firstrun(app: &AppWindow, state: Arc<LauncherAppState>, handle: Handle) {
    let weak = app.as_weak();
    // handle.spawn, not tokio::spawn: callers run on the Slint UI thread
    // (upgrade_in_event_loop), where no Tokio runtime context exists.
    handle.spawn(async move {
        let active = state.config.read().await.active();
        let max = ram_limit_gb(max_ram_mb()).max(RAM_MIN_GB);
        let ram = clamp_ram_gb((active.ram_mb / 1024) as i32, max);
        let presets: Vec<i32> = RAM_PRESETS_GB.into_iter().filter(|p| *p <= max).collect();
        let dir = active.game_dir.clone();
        let java = active.java_path.clone();
        let _ = weak.upgrade_in_event_loop(move |app| {
            app.set_firstrun_game_dir(dir.into());
            app.set_firstrun_java_path(java.into());
            app.set_firstrun_ram(ram);
            app.set_firstrun_maxram(max);
            app.set_firstrun_presets(ModelRc::new(VecModel::from(presets)));
            app.set_firstrun_saving(false);
            app.set_firstrun_error("".into());
            app.set_show_login(false);
            app.set_show_main(false);
            app.set_show_firstrun(true);
        });
    });
}

/// Answer an expired session the way Tauri does: back to login with the
/// stored username kept and the cause shown.
pub fn show_login_expired(app: &AppWindow) {
    let username = app.get_login_username().to_string();
    let kept = if username.is_empty() {
        app.get_username().to_string()
    } else {
        username
    };
    show_login(app, "login", false, &kept);
    app.set_login_error("Сессия истекла. Войдите снова.".into());
}

/// Decide the entry screen after startup, mirroring App.tsx: logged-out (or
/// password migration) goes to LoginScreen, logged-in without a stored
/// username goes to FirstRunScreen, otherwise the main Shell.
pub fn enter_startup(
    app: &AppWindow,
    state: Arc<LauncherAppState>,
    handle: Handle,
    status: account::AccountStatus,
    first_run: bool,
) {
    apply_status(app, &status);
    if !status_logged_in(&status) || status_needs_password(&status) {
        show_login(
            app,
            "login",
            status_flag(&status, "needsPassword"),
            &status_field(&status, "username"),
        );
    } else if first_run || status_field(&status, "username").is_empty() {
        enter_firstrun(app, state, handle);
    } else {
        crate::backend::enter_main(app, state, handle);
    }
}

/// Fixture transitions for Layer-1/Layer-2: synchronous, no network.
pub fn attach_fixtures(app: &AppWindow) {
    let weak = app.as_weak();
    app.on_login_username_edited(move |_| {
        if let Some(app) = weak.upgrade() {
            refresh_login(&app);
        }
    });
    let weak = app.as_weak();
    app.on_login_password_edited(move |_| {
        if let Some(app) = weak.upgrade() {
            refresh_login(&app);
        }
    });
    let weak = app.as_weak();
    app.on_login_accept(move || {
        if let Some(app) = weak.upgrade()
            && app.get_login_can_submit()
        {
            app.invoke_login_submit();
        }
    });
    let weak = app.as_weak();
    app.on_login_toggle_mode(move || {
        if let Some(app) = weak.upgrade() {
            app.set_login_mode(
                if app.get_login_mode() == "register" {
                    "login"
                } else {
                    "register"
                }
                .into(),
            );
            app.set_login_error("".into());
        }
    });
    let weak = app.as_weak();
    app.on_login_logout(move || {
        if let Some(app) = weak.upgrade() {
            show_login(&app, "login", false, "");
        }
    });
    let weak = app.as_weak();
    app.on_login_submit(move || {
        let Some(app) = weak.upgrade() else { return };
        refresh_login(&app);
        if !app.get_login_can_submit() {
            return;
        }
        let name = app.get_login_username().to_string();
        app.set_show_login(false);
        if app.get_login_set_password() || name.trim().is_empty() {
            crate::fixtures::apply_firstrun(&app).expect("firstrun fixtures");
        } else {
            app.set_username(name.into());
            app.set_show_main(true);
            crate::fixtures::apply_main(&app, "main", "default").expect("main fixtures");
        }
    });
    let weak = app.as_weak();
    app.on_firstrun_ram_text_edited(move |text| {
        if let Some(app) = weak.upgrade()
            && let Ok(value) = text.trim().parse::<i32>()
        {
            app.set_firstrun_ram(clamp_ram_gb(value, app.get_firstrun_maxram()));
        }
    });
    let weak = app.as_weak();
    app.on_firstrun_browse_dir(move || {
        let _ = weak.upgrade();
    });
    let weak = app.as_weak();
    app.on_firstrun_browse_java(move || {
        let _ = weak.upgrade();
    });
    let weak = app.as_weak();
    app.on_firstrun_continue(move || {
        let Some(app) = weak.upgrade() else { return };
        if app.get_firstrun_saving() || app.get_firstrun_maxram() < RAM_MIN_GB {
            return;
        }
        app.set_show_firstrun(false);
        app.set_show_main(true);
        crate::fixtures::apply_main(&app, "main", "default").expect("main fixtures");
    });
}

#[cfg(test)]
mod tests {
    use super::{clamp_ram_gb, ram_limit_gb};

    #[test]
    fn ram_helpers_match_the_tauri_form() {
        // ramLimitGb floors to whole GB and caps at 32.
        assert_eq!(ram_limit_gb(0), 0);
        assert_eq!(ram_limit_gb(16 * 1024), 16);
        assert_eq!(ram_limit_gb(64 * 1024), 32);
        // clampRamGb keeps the form inside [2, max].
        assert_eq!(clamp_ram_gb(4, 16), 4);
        assert_eq!(clamp_ram_gb(1, 16), 2);
        assert_eq!(clamp_ram_gb(99, 16), 16);
        assert_eq!(clamp_ram_gb(4, 0), 2);
    }
}
