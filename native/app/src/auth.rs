//! Login + FirstRun screens: thin Slint adapters over the shared core.
//! Auth itself stays in `launcher-net::account`, settings persistence in
//! `launcher-app::ops::settings`. Mirrors the Tauri gating in App.tsx:
//! logged-out (or password migration) shows LoginScreen; logged-in but no
//! stored username shows FirstRunScreen; otherwise the main Shell.
use std::sync::Arc;

use launcher_app::{LauncherAppState, ops::settings as settings_ops};
use launcher_core::{
    config::{max_ram_mb, valid_username},
    errors::{ErrorContext, friendly_error},
    validate::{clamp_ram_gb, ram_limit_gb},
};
use launcher_net::account;
use slint::{ComponentHandle, ModelRc, VecModel};
use tokio::runtime::Handle;

use crate::AppWindow;

const RAM_MIN_GB: i32 = 2;
const RAM_PRESETS_GB: [i32; 6] = [4, 6, 8, 12, 16, 24];

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

/// Whether this install has never signed in: the stored username is empty.
/// Read fresh at each auth exit, never cached across submits.
pub fn first_run_now(state: &LauncherAppState) -> bool {
    state.config.blocking_read().username.trim().is_empty()
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

/// Single busy guard for every auth operation that spawns work: refuses
/// while one is already running, so submit/logout/continue can never race
/// each other on the same config and tokens.
pub fn try_begin(app: &AppWindow) -> bool {
    !(app.get_login_busy() || app.get_firstrun_saving())
}

fn set_login_error(app: &AppWindow, raw: &str) {
    let friendly = friendly_error(raw, ErrorContext::Account);
    app.set_login_error(friendly.message.into());
    app.set_login_raw_error(friendly.raw.into());
}

fn set_firstrun_error(app: &AppWindow, raw: &str) {
    let friendly = friendly_error(raw, ErrorContext::SettingsSave);
    app.set_firstrun_error(friendly.message.into());
    app.set_firstrun_raw_error(friendly.raw.into());
}

fn show_login(app: &AppWindow, mode: &str, set_password: bool, username: &str) {
    app.set_show_main(false);
    app.set_show_firstrun(false);
    app.set_show_init_error(false);
    app.set_login_mode(mode.into());
    app.set_login_set_password(set_password);
    app.set_login_username(username.into());
    app.set_login_password("".into());
    app.set_login_busy(false);
    app.set_login_error("".into());
    app.set_login_raw_error("".into());
    if !set_password {
        // Tauri App.tsx clears staff on any !loggedIn: drop the role badge
        // and Workshop access so the next account starts clean (D1 fields).
        app.set_role("ИГРОК".into());
        let mut data = app.get_main_data();
        data.can_workshop = false;
        data.profile_blocked = false;
        data.profile_error = "".into();
        app.set_main_data(data);
    }
    app.set_show_login(true);
    refresh_login(app);
    app.set_login_focus_trigger(app.get_login_focus_trigger().wrapping_add(1));
}

/// Voluntary logout from Settings: return to LoginScreen with the stored
/// username prefilled and no session-expired error (Tauri App.tsx parity).
pub fn show_login_logout(app: &AppWindow, username: &str) {
    let kept = if username.is_empty() {
        app.get_username().to_string()
    } else {
        username.to_string()
    };
    app.set_username(kept.as_str().into());
    show_login(app, "login", false, &kept);
}

/// Startup account_status failure: record first-run eligibility, open login
/// and show the error via `friendly_error(..., Account)`.
pub fn show_login_error(app: &AppWindow, username: &str, error: &str) {
    app.set_firstrun_pending(username.trim().is_empty());
    show_login(app, "login", false, username);
    set_login_error(app, error);
}

fn apply_status(app: &AppWindow, status: &account::AccountStatus) {
    app.set_username(status_field(status, "username").into());
}

/// Wire the auth screens to the core runtime (same code path as production).
pub fn attach(app: &AppWindow, state: Arc<LauncherAppState>, handle: Handle) {
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
            app.set_login_raw_error("".into());
        }
    });
    let weak = app.as_weak();
    let submit_state = state.clone();
    let submit_handle = handle.clone();
    app.on_login_submit(move || {
        let Some(app) = weak.upgrade() else { return };
        refresh_login(&app);
        if !app.get_login_can_submit() || !try_begin(&app) {
            return;
        }
        app.set_login_busy(true);
        app.set_login_error("".into());
        app.set_login_raw_error("".into());
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
                on_auth_done(&app, &state, &handle, result);
            });
        });
    });
    let weak = app.as_weak();
    let logout_state = state.clone();
    let logout_handle = handle.clone();
    app.on_login_logout(move || {
        let Some(app) = weak.upgrade() else { return };
        if !try_begin(&app) {
            return;
        }
        app.set_login_busy(true);
        app.set_login_error("".into());
        app.set_login_raw_error("".into());
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
                    Err(error) => set_login_error(&app, &error),
                }
            });
        });
    });
    let weak = app.as_weak();
    app.on_firstrun_ram_text_edited(move |text| {
        if let Some(app) = weak.upgrade() {
            let max = app.get_firstrun_maxram();
            if let Ok(value) = text.trim().parse::<i32>() {
                app.set_firstrun_ram(clamp_ram_gb(value as f64, max as f64) as i32);
            }
        }
    });
    let weak = app.as_weak();
    app.on_firstrun_browse_dir(move || {
        if let Some(app) = weak.upgrade() {
            crate::pickers::apply_firstrun_browse_dir(&app, crate::pickers::pick_game_dir());
        }
    });
    let weak = app.as_weak();
    app.on_firstrun_browse_java(move || {
        if let Some(app) = weak.upgrade() {
            crate::pickers::apply_firstrun_browse_java(&app, crate::pickers::pick_java());
        }
    });
    let weak = app.as_weak();
    let continue_state = state.clone();
    let continue_handle = handle.clone();
    app.on_firstrun_continue(move || {
        let Some(app) = weak.upgrade() else { return };
        if !try_begin(&app) || app.get_firstrun_maxram() < RAM_MIN_GB {
            return;
        }
        app.set_firstrun_saving(true);
        app.set_firstrun_error("".into());
        app.set_firstrun_raw_error("".into());
        let weak = weak.clone();
        let state = continue_state.clone();
        let handle = continue_handle.clone();
        let spawn = handle.clone();
        let dir = app.get_firstrun_game_dir().trim().to_string();
        let java = app.get_firstrun_java_path().trim().to_string();
        let ram_mb = (clamp_ram_gb(
            app.get_firstrun_ram() as f64,
            app.get_firstrun_maxram() as f64,
        ) as u32)
            * 1024;
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
                    Ok(()) => {
                        app.set_firstrun_pending(false);
                        crate::backend::enter_main(&app, state, handle);
                    }
                    Err(error) => set_firstrun_error(&app, &error),
                }
            });
        });
    });
}

pub fn on_auth_done(
    app: &AppWindow,
    state: &Arc<LauncherAppState>,
    handle: &Handle,
    result: Result<account::AccountStatus, String>,
) {
    match result {
        Err(error) => {
            app.set_login_busy(false);
            set_login_error(app, &error);
            refresh_login(app);
        }
        Ok(status) => {
            apply_status(app, &status);
            app.set_login_password("".into());
            if status_needs_password(&status) {
                show_login(app, "login", true, &status_username(&status));
            } else if app.get_firstrun_pending()
                || first_run_now(state)
                || status_username(&status).is_empty()
            {
                // enter_firstrun clears busy when FirstRunScreen is up.
                enter_firstrun(app, state.clone(), handle.clone());
            } else {
                app.set_login_busy(false);
                crate::backend::enter_main(app, state.clone(), handle.clone());
            }
        }
    }
}

/// Extract FirstRun defaults from `settings_ops::load_settings` JSON.
/// `maxram` is never clamped upward, so `< 2 GB` warning/disabled Continue
/// stays reachable; both go through the shared `launcher_core` validators.
pub fn firstrun_defaults_from_response(
    value: &serde_json::Value,
) -> (String, String, i32, i32, Vec<i32>) {
    let dir = value["gameDir"].as_str().unwrap_or_default().to_string();
    let java = value["javaPath"].as_str().unwrap_or_default().to_string();
    let max = ram_limit_gb(value["maxRamMb"].as_f64().unwrap_or(0.0)) as i32;
    let ram = clamp_ram_gb(
        value["ramMb"].as_f64().unwrap_or(4096.0) / 1024.0,
        max as f64,
    ) as i32;
    let presets = RAM_PRESETS_GB.into_iter().filter(|p| *p <= max).collect();
    (dir, java, ram, max, presets)
}

/// Fill the FirstRun form from `settings_ops::load_settings` (which auto-detects
/// bundled Java and reports real `maxRamMb`) and show it.
pub fn enter_firstrun(app: &AppWindow, state: Arc<LauncherAppState>, handle: Handle) {
    let weak = app.as_weak();
    // handle.spawn, not tokio::spawn: callers run on the Slint UI thread
    // (upgrade_in_event_loop), where no Tokio runtime context exists.
    handle.spawn(async move {
        let loaded = tokio::task::spawn_blocking({
            let state = state.clone();
            move || {
                settings_ops::load_settings(&state)
                    .and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string()))
            }
        })
        .await
        .ok()
        .and_then(Result::ok);
        let (dir, java, ram, max, presets) = match loaded {
            Some(value) => firstrun_defaults_from_response(&value),
            None => {
                let active = state.config.read().await.active();
                let max = ram_limit_gb(max_ram_mb() as f64) as i32;
                let ram = clamp_ram_gb(active.ram_mb as f64 / 1024.0, max as f64) as i32;
                let presets = RAM_PRESETS_GB.into_iter().filter(|p| *p <= max).collect();
                (active.game_dir, active.java_path, ram, max, presets)
            }
        };
        let _ = weak.upgrade_in_event_loop(move |app| {
            app.set_login_busy(false);
            app.set_show_init_error(false);
            app.set_firstrun_game_dir(dir.into());
            app.set_firstrun_java_path(java.into());
            app.set_firstrun_ram(ram);
            app.set_firstrun_maxram(max);
            app.set_firstrun_presets(ModelRc::new(VecModel::from(presets)));
            app.set_firstrun_saving(false);
            app.set_firstrun_error("".into());
            app.set_firstrun_raw_error("".into());
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
    app.set_login_error("Сессия истекла — войдите снова.".into());
}

/// Decide the entry screen after startup, mirroring App.tsx: capture first-run
/// eligibility before login, logged-out (or password migration) goes to
/// LoginScreen, logged-in without a stored username goes to FirstRunScreen,
/// otherwise the main Shell.
pub fn enter_startup(
    app: &AppWindow,
    state: Arc<LauncherAppState>,
    handle: Handle,
    status: account::AccountStatus,
) {
    apply_status(app, &status);
    let needs_firstrun = first_run_now(&state);
    app.set_firstrun_pending(needs_firstrun);
    if !status_logged_in(&status) || status_needs_password(&status) {
        show_login(
            app,
            "login",
            status_flag(&status, "needsPassword"),
            &status_field(&status, "username"),
        );
    } else if needs_firstrun || status_field(&status, "username").is_empty() {
        enter_firstrun(app, state, handle);
    } else {
        app.set_firstrun_pending(false);
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
            app.set_login_raw_error("".into());
        }
    });
    let weak = app.as_weak();
    app.on_login_logout(move || {
        if let Some(app) = weak.upgrade() {
            if !try_begin(&app) {
                return;
            }
            show_login(&app, "login", false, "");
        }
    });
    let weak = app.as_weak();
    app.on_login_submit(move || {
        let Some(app) = weak.upgrade() else { return };
        refresh_login(&app);
        if !app.get_login_can_submit() || !try_begin(&app) {
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
            app.set_firstrun_ram(
                clamp_ram_gb(value as f64, app.get_firstrun_maxram() as f64) as i32
            );
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
        if !try_begin(&app) || app.get_firstrun_maxram() < RAM_MIN_GB {
            return;
        }
        app.set_show_firstrun(false);
        app.set_show_main(true);
        crate::fixtures::apply_main(&app, "main", "default").expect("main fixtures");
    });
}

#[cfg(test)]
mod tests {
    use super::firstrun_defaults_from_response;

    fn defaults(ram_mb: u64, max_ram_mb: u64) -> (i32, i32) {
        let (_, _, ram, max, _) = firstrun_defaults_from_response(&serde_json::json!({
            "gameDir": "/tmp/game",
            "javaPath": "/tmp/game/java/bin/java",
            "ramMb": ram_mb,
            "maxRamMb": max_ram_mb
        }));
        (ram, max)
    }

    #[test]
    fn firstrun_ram_rounds_and_matches_tauri_below_2gb() {
        // Fractional GiB rounds like Tauri Math.round, not integer division.
        assert_eq!(defaults(3584, 8192), (4, 8));
        // Below 2 GB the form keeps Tauri's min(max, ...) order: max wins.
        assert_eq!(defaults(4096, 1024), (1, 1));
        assert_eq!(defaults(4096, 0), (0, 0));
        assert_eq!(defaults(4096, 16384), (4, 16));
    }
}
