//! Settings/profile commands, moved unchanged from `src-tauri/src/commands.rs`
//! (phase 2, no behaviour change). The Tauri-only input (the app data dir the
//! `save_settings` wrapper resolves via `AppHandle`) arrives as a plain path.

use super::java::bundled_java_path;
use crate::LauncherAppState;
use launcher_core::config::{ClientProfile, LauncherConfig};
use std::path::{Path, PathBuf};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsResponse {
    #[serde(flatten)]
    config: LauncherConfig,
    max_ram_mb: u32,
}

pub fn load_settings(state: &LauncherAppState) -> Result<SettingsResponse, String> {
    Ok(settings_response(state.config.blocking_read().active()))
}

pub async fn select_profile(
    state: &LauncherAppState,
    profile: ClientProfile,
) -> Result<SettingsResponse, String> {
    let _operation = state
        .operation
        .try_lock()
        .map_err(|_| "Дождитесь завершения текущей операции.")?;
    if state.game.is_busy() {
        return Err("Закройте игру перед сменой профиля.".into());
    }
    let mut current = state.config.write().await;
    let mut updated = current.clone();
    updated.active_profile = profile;
    launcher_core::config::save_config(&state.app_data_dir, &updated)?;
    *current = updated;
    Ok(settings_response(current.active()))
}

pub fn save_settings(
    state: &LauncherAppState,
    app_data_dir: &PathBuf,
    config: LauncherConfig,
) -> Result<(), String> {
    let _operation = state
        .operation
        .try_lock()
        .map_err(|_| "Дождитесь завершения текущей операции.")?;
    if state.game.is_busy() {
        return Err("Закройте игру перед изменением настроек.".into());
    }
    let mut current = state.config.blocking_write();
    if config.active_profile != current.active_profile {
        return Err("Профиль изменился. Откройте настройки заново.".into());
    }
    let directory = config.game_dir.clone();
    let java = config.java_path.clone();
    if java.trim() != current.active().java_path.trim()
        && !java.trim().is_empty()
        && !Path::new(java.trim()).is_file()
    {
        return Err("Selected Java executable does not exist".to_string());
    }
    // The settings form does not own the account.
    let mut config = LauncherConfig {
        workshop_server: config.workshop_server,
        ram_mb: config.ram_mb,
        auto_update: config.auto_update,
        lang: config.lang,
        pre_launch_command: config.pre_launch_command,
        post_exit_command: config.post_exit_command,
        hide_while_playing: config.hide_while_playing,
        discord_presence: config.discord_presence,
        ..current.clone()
    };
    config.set_active_paths(directory, java);
    launcher_core::config::save_config(app_data_dir, &config)?;
    *current = config;
    Ok(())
}

fn settings_response(mut config: LauncherConfig) -> SettingsResponse {
    for secret in [
        &mut config.access_token,
        &mut config.client_token,
        &mut config.api_token,
        &mut config.skin_password,
    ] {
        secret.clear();
    }
    // Auto-detect bundled Java if path is empty
    if config.java_path.is_empty() {
        if let Some(bundled) = bundled_java_path(&config.game_dir) {
            config.java_path = bundled.to_string_lossy().to_string();
        }
    }
    SettingsResponse {
        config,
        max_ram_mb: launcher_core::config::max_ram_mb(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_state;
    use super::*;
    use launcher_core::config::load_config;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("blockfield-{name}-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn load_settings_redacts_secrets_and_reports_max_ram() {
        let dir = temp_dir("settings");
        let config = LauncherConfig {
            access_token: "secret-a".into(),
            client_token: "secret-c".into(),
            api_token: "secret-t".into(),
            skin_password: "secret-s".into(),
            ..LauncherConfig::default()
        };
        let state = test_state(dir, config);
        let response = load_settings(&state).unwrap();
        assert_eq!(response.config.access_token, "");
        assert_eq!(response.config.client_token, "");
        assert_eq!(response.config.api_token, "");
        assert_eq!(response.config.skin_password, "");
        assert_eq!(response.max_ram_mb, launcher_core::config::max_ram_mb());
    }

    #[tokio::test]
    async fn select_profile_switches_profile_and_persists_it() {
        let dir = temp_dir("profile");
        let state = test_state(dir.clone(), LauncherConfig::default());
        assert_eq!(
            state.config.read().await.active_profile,
            ClientProfile::Game
        );
        let response = select_profile(&state, ClientProfile::Workshop)
            .await
            .unwrap();
        assert_eq!(response.config.active_profile, ClientProfile::Workshop);
        assert_eq!(
            state.config.read().await.active_profile,
            ClientProfile::Workshop
        );
        assert_eq!(load_config(&dir).active_profile, ClientProfile::Workshop);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn save_settings_validates_profile_and_java_then_applies() {
        let dir = temp_dir("save");
        let state = test_state(dir.clone(), LauncherConfig::default());

        // Stale form: the profile changed since the form was opened.
        let stale = LauncherConfig {
            active_profile: ClientProfile::Workshop,
            ..LauncherConfig::default()
        };
        assert_eq!(
            save_settings(&state, &dir, stale).unwrap_err(),
            "Профиль изменился. Откройте настройки заново."
        );

        // Java path that does not exist.
        let bad_java = LauncherConfig {
            java_path: "/nonexistent/java".into(),
            ..LauncherConfig::default()
        };
        assert_eq!(
            save_settings(&state, &dir, bad_java).unwrap_err(),
            "Selected Java executable does not exist"
        );

        // Valid change applies to the active profile and persists.
        let game_dir = dir.join("game");
        std::fs::create_dir_all(&game_dir).unwrap();
        let good = LauncherConfig {
            game_dir: game_dir.to_string_lossy().to_string(),
            ram_mb: 4096,
            ..LauncherConfig::default()
        };
        save_settings(&state, &dir, good).unwrap();
        let active = state.config.blocking_read().active();
        assert_eq!(active.game_dir, game_dir.to_string_lossy());
        assert_eq!(active.ram_mb, 4096);
        assert_eq!(load_config(&dir).ram_mb, 4096);
        let _ = std::fs::remove_dir_all(dir);
    }
}
