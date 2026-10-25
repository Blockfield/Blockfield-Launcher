//! Settings form adapter. All disk/network work stays on the backend runtime.
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

use launcher_app::{LauncherAppState, ops::settings};
use launcher_core::{
    config::{ClientProfile, LauncherConfig},
    errors::{ErrorContext, friendly_error},
    validate::{clamp_ram_gb, ram_limit_gb},
};
use slint::{ComponentHandle, ModelRc, VecModel};
use tokio::runtime::Handle;

use crate::{AppWindow, SettingsData, SettingsForm};

type Core = Option<(Arc<LauncherAppState>, Handle)>;

pub fn apply_response(app: &AppWindow, value: &serde_json::Value) {
    let max = ram_limit_gb(value["maxRamMb"].as_f64().unwrap_or(0.0));
    app.global::<SettingsForm>().set_data(SettingsData {
        game_dir: value["gameDir"].as_str().unwrap_or_default().into(),
        java_path: value["javaPath"].as_str().unwrap_or_default().into(),
        workshop_server: value["workshopServer"].as_str().unwrap_or_default().into(),
        workshop: value["activeProfile"] == "workshop",
        ram: clamp_ram_gb(value["ramMb"].as_f64().unwrap_or(4096.0) / 1024.0, max) as i32,
        max_ram: max as i32,
        auto_update: value["autoUpdate"].as_bool().unwrap_or(true),
        discord_presence: value["discordPresence"].as_bool().unwrap_or(true),
        hide_while_playing: value["hideWhilePlaying"].as_bool().unwrap_or(false),
        pre_launch: value["preLaunchCommand"]
            .as_str()
            .unwrap_or_default()
            .into(),
        post_exit: value["postExitCommand"].as_str().unwrap_or_default().into(),
        lang: value["lang"].as_str().unwrap_or("ru").into(),
    });
    let form = app.global::<SettingsForm>();
    form.set_presets(ModelRc::new(VecModel::from(
        [4, 6, 8, 12, 16, 24]
            .into_iter()
            .filter(|p| *p <= max as i32)
            .collect::<Vec<_>>(),
    )));
    form.set_loading(false);
    form.set_dirty(false);
    form.set_error("".into());
    form.set_raw_error("".into());
}

pub fn config_from_form(data: &SettingsData) -> Result<LauncherConfig, String> {
    if data.max_ram < 2 {
        return Err("Недостаточно памяти: для игры нужно выделить минимум 2 ГБ.".into());
    }
    Ok(LauncherConfig {
        active_profile: if data.workshop {
            ClientProfile::Workshop
        } else {
            ClientProfile::Game
        },
        workshop_server: data.workshop_server.to_string(),
        game_dir: data.game_dir.to_string(),
        java_path: data.java_path.to_string(),
        ram_mb: clamp_ram_gb(data.ram as f64, data.max_ram as f64) as u32 * 1024,
        auto_update: data.auto_update,
        discord_presence: data.discord_presence,
        hide_while_playing: data.hide_while_playing,
        pre_launch_command: data.pre_launch.to_string(),
        post_exit_command: data.post_exit.to_string(),
        lang: data.lang.to_string(),
        ..LauncherConfig::default()
    })
}

fn save_result(app: &AppWindow, result: Result<(), String>) {
    let form = app.global::<SettingsForm>();
    form.set_saving(false);
    match result {
        Ok(()) => {
            form.set_dirty(false);
            form.set_saved(true);
            let weak = app.as_weak();
            slint::Timer::single_shot(Duration::from_secs(3), move || {
                if let Some(app) = weak.upgrade() {
                    app.global::<SettingsForm>().set_saved(false);
                }
            });
        }
        Err(error) => {
            let friendly = friendly_error(&error, ErrorContext::SettingsSave);
            form.set_error(friendly.message.into());
            form.set_raw_error(friendly.raw.into());
        }
    }
}

pub fn wire(app: &AppWindow, core: Core) {
    app.set_settings_tabs(ModelRc::new(VecModel::from(
        ["ОБЩИЕ", "JAVA И ПАМЯТЬ", "КОМАНДЫ", "СКИН", "ЛАУНЧЕР"]
            .map(slint::SharedString::from)
            .to_vec(),
    )));
    app.set_settings_title("НАСТРОЙКИ".into());
    app.set_settings_subtitle("/ ПАРАМЕТРЫ ЛАУНЧЕРА".into());
    let saved = Rc::new(RefCell::new(app.global::<SettingsForm>().get_data()));
    let weak = app.as_weak();
    app.global::<SettingsForm>().on_edited(move || {
        if let Some(app) = weak.upgrade() {
            let form = app.global::<SettingsForm>();
            form.set_dirty(true);
            form.set_saved(false);
            form.set_error("".into());
            form.set_raw_error("".into());
        }
    });
    let weak = app.as_weak();
    app.global::<SettingsForm>().on_set_ram(move |value| {
        if let Some(app) = weak.upgrade() {
            let form = app.global::<SettingsForm>();
            let mut data = form.get_data();
            data.ram = clamp_ram_gb(value as f64, data.max_ram as f64) as i32;
            form.set_data(data);
            form.invoke_edited();
        }
    });
    let weak = app.as_weak();
    app.global::<SettingsForm>().on_commit_ram(move |text| {
        if let Some(app) = weak.upgrade()
            && !text.trim().is_empty()
        {
            let value = text.trim().parse::<f64>().unwrap_or(f64::NAN);
            let form = app.global::<SettingsForm>();
            let mut data = form.get_data();
            let next = clamp_ram_gb(value, data.max_ram as f64) as i32;
            if next != data.ram {
                data.ram = next;
                form.set_data(data);
                form.invoke_edited();
            }
        }
    });
    // No rfd dependency in this crate. u45 owns the platform dialog callback.
    app.global::<SettingsForm>()
        .on_browse(|field| eprintln!("HANDOFF u45: settings browse {field}"));
    let weak = app.as_weak();
    let profile_core = core.clone();
    let profile_snapshot = saved.clone();
    app.global::<SettingsForm>().on_profile(move |workshop| {
        let Some(app) = weak.upgrade() else { return };
        let form = app.global::<SettingsForm>();
        if form.get_data().workshop == workshop {
            return;
        }
        if let Some((state, handle)) = &profile_core {
            let state = state.clone();
            let weak = app.as_weak();
            form.set_blocked(true);
            handle.spawn(async move {
                let profile = if workshop {
                    ClientProfile::Workshop
                } else {
                    ClientProfile::Game
                };
                let result = settings::select_profile(&state, profile)
                    .await
                    .and_then(|value| serde_json::to_value(value).map_err(|e| e.to_string()));
                let _ = weak.upgrade_in_event_loop(move |app| match result {
                    Ok(value) => {
                        apply_response(&app, &value);
                        app.global::<SettingsForm>().set_blocked(false);
                    }
                    Err(error) => {
                        let friendly = friendly_error(&error, ErrorContext::SelectProfile);
                        app.global::<SettingsForm>()
                            .set_error(friendly.message.into());
                        app.global::<SettingsForm>()
                            .set_raw_error(friendly.raw.into());
                    }
                });
            });
        } else {
            let mut data = form.get_data();
            data.workshop = workshop;
            data.game_dir = if workshop {
                "/capture/workshop"
            } else {
                "/capture/game"
            }
            .into();
            form.set_data(data.clone());
            form.set_dirty(false);
            *profile_snapshot.borrow_mut() = data;
        }
    });
    let weak = app.as_weak();
    let save_core = core.clone();
    let save_snapshot = saved.clone();
    app.global::<SettingsForm>().on_save(move || {
        let Some(app) = weak.upgrade() else { return };
        let form = app.global::<SettingsForm>();
        if form.get_saving() || form.get_blocked() || !form.get_dirty() {
            return;
        }
        let data = form.get_data();
        let config = match config_from_form(&data) {
            Ok(config) => config,
            Err(error) => {
                form.set_error(error.into());
                return;
            }
        };
        form.set_saving(true);
        form.set_error("".into());
        form.set_raw_error("".into());
        if let Some((state, handle)) = &save_core {
            let state = state.clone();
            let weak = app.as_weak();
            handle.spawn_blocking(move || {
                let result = settings::save_settings(&state, &state.app_data_dir, config);
                let _ = weak.upgrade_in_event_loop(move |app| save_result(&app, result));
            });
        } else {
            let result = if data.java_path == "/missing/java" {
                Err("Selected Java executable does not exist".into())
            } else {
                Ok(())
            };
            if result.is_ok() {
                *save_snapshot.borrow_mut() = data;
            }
            save_result(&app, result);
        }
    });
    let weak = app.as_weak();
    let reset_core = core.clone();
    app.global::<SettingsForm>().on_reset(move || {
        let Some(app) = weak.upgrade() else { return };
        if app.global::<SettingsForm>().get_blocked() || app.global::<SettingsForm>().get_saving() {
            return;
        }
        if let Some((state, handle)) = &reset_core {
            let state = state.clone();
            let weak = app.as_weak();
            handle.spawn_blocking(move || {
                let result = settings::load_settings(&state)
                    .and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string()));
                let _ = weak.upgrade_in_event_loop(move |app| match result {
                    Ok(value) => apply_response(&app, &value),
                    Err(error) => save_result(&app, Err(error)),
                });
            });
        } else {
            let form = app.global::<SettingsForm>();
            form.set_data(saved.borrow().clone());
            form.set_dirty(false);
            form.set_saved(false);
            form.set_error("".into());
            form.set_raw_error("".into());
        }
    });
    for password_change in [false, true] {
        let weak = app.as_weak();
        let account_core = core.clone();
        let callback = move || {
            let Some(app) = weak.upgrade() else { return };
            let form = app.global::<SettingsForm>();
            if form.get_account_busy() || (password_change && form.get_password().is_empty()) {
                return;
            }
            let password = form.get_password().to_string();
            form.set_account_busy(true);
            form.set_account_error("".into());
            form.set_account_raw_error("".into());
            form.set_password_changed(false);
            if let Some((state, handle)) = &account_core {
                let state = state.clone();
                let weak = app.as_weak();
                handle.spawn(async move {
                    let base = state
                        .pack
                        .read()
                        .await
                        .as_ref()
                        .and_then(|p| p.info.skins.clone());
                    let result = if password_change {
                        launcher_net::account::change_password(
                            &state.config,
                            &state.app_data_dir,
                            base,
                            password,
                        )
                        .await
                    } else {
                        launcher_net::account::logout(&state.config, &state.app_data_dir, base)
                            .await
                    };
                    let _ = weak.upgrade_in_event_loop(move |app| {
                        account_result(&app, password_change, result.map(|_| ()))
                    });
                });
            } else {
                let result = if password_change && password.len() < 8 {
                    Err("Drasl: Invalid password".into())
                } else {
                    Ok(())
                };
                account_result(&app, password_change, result);
            }
        };
        if password_change {
            app.global::<SettingsForm>().on_change_password(callback);
        } else {
            app.global::<SettingsForm>().on_logout(callback);
        }
    }
    // Shared footer callbacks remain the public app seam for screen consumers.
    let weak = app.as_weak();
    app.on_settings_save(move || {
        if let Some(app) = weak.upgrade() {
            app.global::<SettingsForm>().invoke_save();
        }
    });
    let weak = app.as_weak();
    app.on_settings_reset(move || {
        if let Some(app) = weak.upgrade() {
            app.global::<SettingsForm>().invoke_reset();
        }
    });
}

fn account_result(app: &AppWindow, password_change: bool, result: Result<(), String>) {
    let form = app.global::<SettingsForm>();
    form.set_account_busy(false);
    match result {
        Ok(()) => {
            form.set_password("".into());
            form.set_password_open(false);
            form.set_password_changed(password_change);
            if !password_change {
                // The login screen unit consumes the account-expired event; never retain a password in UI state.
                app.set_username("".into());
                crate::bridge::apply(app, launcher_app::LauncherEvent::AccountExpired);
            }
        }
        Err(error) => {
            let friendly = friendly_error(&error, ErrorContext::Account);
            form.set_account_error(friendly.message.into());
            form.set_account_raw_error(friendly.raw.into());
        }
    }
}
