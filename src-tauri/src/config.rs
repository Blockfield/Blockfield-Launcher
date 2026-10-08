use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClientProfile {
    #[default]
    Game,
    Workshop,
}

/// Launcher configuration persisted in Tauri's app data directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherConfig {
    /// Absolute path to the game directory where modpack files are installed
    pub game_dir: String,
    #[serde(default)]
    pub active_profile: ClientProfile,
    #[serde(default)]
    pub workshop_game_dir: String,
    #[serde(default)]
    pub workshop_java_path: String,
    #[serde(default = "default_workshop_server")]
    pub workshop_server: String,
    /// Absolute path to the Java runtime executable
    pub java_path: String,
    /// Allocated RAM in megabytes
    pub ram_mb: u32,
    /// Whether to auto-check for modpack updates on launch
    pub auto_update: bool,
    /// UI language code ("en", "ru", "uk")
    pub lang: String,
    /// Drasl player name, exact case as returned by the auth server. Kept after logout to prefill login.
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub player_uuid: String,
    /// Yggdrasil session; empty when logged out.
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub client_token: String,
    /// Drasl API v3 token for skin uploads and password changes.
    #[serde(default)]
    pub api_token: String,
    #[serde(default)]
    pub pre_launch_command: String,
    #[serde(default)]
    pub post_exit_command: String,
    #[serde(default)]
    pub hide_while_playing: bool,
    #[serde(default = "presence_enabled")]
    pub discord_presence: bool,
    /// Legacy launcher-generated Drasl password; only read to migrate the account, then cleared.
    #[serde(default)]
    pub skin_password: String,
    /// Account `skin_password` belongs to, set once another account signs in; empty means `username`.
    #[serde(default)]
    pub skin_username: String,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        // Runtime first (dotenvy for local dev), then compile-time (CI build), then hardcoded.
        let game_dir = default_game_dir();
        let workshop_game_dir = workshop_directory(&game_dir);
        Self {
            game_dir,
            active_profile: ClientProfile::Game,
            workshop_game_dir,
            workshop_java_path: String::new(),
            workshop_server: default_workshop_server(),
            java_path: String::new(),
            ram_mb: std::env::var("BLOCKFIELD_DEFAULT_RAM_MB")
                .ok()
                .and_then(|v| v.parse().ok())
                .or_else(|| option_env!("BLOCKFIELD_DEFAULT_RAM_MB").and_then(|v| v.parse().ok()))
                .unwrap_or(4096),
            auto_update: std::env::var("BLOCKFIELD_AUTO_UPDATE")
                .ok()
                .map(|v| v == "true" || v == "1")
                .or_else(|| option_env!("BLOCKFIELD_AUTO_UPDATE").map(|v| v == "true" || v == "1"))
                .unwrap_or(true),
            lang: std::env::var("BLOCKFIELD_DEFAULT_LANG")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .or_else(|| option_env!("BLOCKFIELD_DEFAULT_LANG").map(String::from))
                .unwrap_or_else(|| "en".to_string()),
            username: String::new(),
            player_uuid: String::new(),
            access_token: String::new(),
            client_token: String::new(),
            api_token: String::new(),
            pre_launch_command: String::new(),
            post_exit_command: String::new(),
            hide_while_playing: false,
            discord_presence: true,
            skin_password: String::new(),
            skin_username: String::new(),
        }
    }
}

impl LauncherConfig {
    /// Keep the legacy game directory in persisted settings; only operation snapshots use the
    /// selected directory. Accounts and launcher preferences remain shared.
    pub fn active(&self) -> Self {
        let mut config = self.clone();
        if self.active_profile == ClientProfile::Workshop {
            config.game_dir = self.workshop_game_dir.clone();
            config.java_path = self.workshop_java_path.clone();
        }
        config
    }

    pub fn validated_active(&self) -> Result<Self, String> {
        validate_config(self)?;
        Ok(self.active())
    }

    pub fn set_active_paths(&mut self, directory: String, java: String) {
        match self.active_profile {
            ClientProfile::Game => {
                self.game_dir = directory;
                self.java_path = java;
            }
            ClientProfile::Workshop => {
                self.workshop_game_dir = directory;
                self.workshop_java_path = java;
            }
        }
    }

    pub fn bridge_dir(&self, app_data_dir: &Path) -> PathBuf {
        match self.active_profile {
            ClientProfile::Game => app_data_dir.to_path_buf(),
            ClientProfile::Workshop => app_data_dir.join("workshop"),
        }
    }

    pub fn target(&self, game_server: &str) -> Result<Option<String>, String> {
        if self.active_profile == ClientProfile::Game {
            return Ok((!game_server.trim().is_empty()).then(|| game_server.trim().to_owned()));
        }
        let server = self.workshop_server.trim();
        if server.is_empty() {
            return Err("Укажите адрес мастерской в настройках выбранного профиля.".into());
        }
        if server_host(server)? == server_host(game_server)? {
            return Err(
                "Мастерской нужен отдельный адрес: смена порта игрового адреса не меняет профиль."
                    .into(),
            );
        }
        Ok(Some(server.to_owned()))
    }
}

fn default_workshop_server() -> String {
    std::env::var("BLOCKFIELD_WORKSHOP_SERVER")
        .ok()
        .or_else(|| option_env!("BLOCKFIELD_WORKSHOP_SERVER").map(str::to_owned))
        .unwrap_or_else(|| "workshop.blockfield.pro:25565".to_string())
}

fn workshop_directory(game_dir: &str) -> String {
    let path = Path::new(game_dir);
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!("{name}-Workshop"))
        .to_string_lossy()
        .into_owned()
}

pub fn server_host(address: &str) -> Result<String, String> {
    let address = address
        .trim()
        .strip_prefix("raknet;")
        .unwrap_or(address.trim());
    let invalid =
        || "Нужен адрес сервера: хост и необязательный порт, без ссылки или пробелов.".to_string();
    if address.is_empty()
        || address
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '/' | '?' | '#' | '@' | '%' | ';'))
    {
        return Err(invalid());
    }
    let url = tauri::Url::parse(&format!("minecraft://{address}")).map_err(|_| invalid())?;
    if url.port() == Some(0) {
        return Err(invalid());
    }
    let host = url.host_str().ok_or_else(invalid)?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let host = host.strip_suffix('.').unwrap_or(host).to_ascii_lowercase();
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return Ok(ip.to_string());
    }
    if host.len() > 253
        || host.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || !label.starts_with(|c: char| c.is_ascii_alphanumeric())
                || !label.ends_with(|c: char| c.is_ascii_alphanumeric())
                || !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
    {
        return Err(invalid());
    }
    Ok(host)
}

/// Platform-appropriate default game directory. A dev build keeps its own, so a developer's
/// player installation is never touched by an experiment.
fn default_game_dir() -> String {
    if let Some(dir) = crate::dev::game_dir_override() {
        return dir;
    }
    let name = if crate::dev::is_dev_build() {
        "BlockField-Dev"
    } else {
        "BlockField"
    };
    if let Some(base) = directories::BaseDirs::new() {
        base.data_dir().join(name).to_string_lossy().to_string()
    } else {
        // Fallback for edge cases
        String::from("./BlockField")
    }
}

/// Path to the launcher config file inside the app data directory.
pub fn config_path(app_data_dir: &Path) -> PathBuf {
    // Dev and production builds share the app data directory; separate files keep their
    // settings (game directory above all) apart.
    if crate::dev::is_dev_build() {
        return app_data_dir.join(crate::dev::CONFIG_FILE);
    }
    app_data_dir.join("launcher-config.json")
}

/// Load the launcher config from disk, or return defaults.
pub fn load_config(app_data_dir: &Path) -> LauncherConfig {
    let path = config_path(app_data_dir);
    let mut config: LauncherConfig = match std::fs::read_to_string(&path) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_else(|e| {
            log::warn!("Failed to parse launcher config, using defaults: {e}");
            LauncherConfig::default()
        }),
        Err(e) => {
            log::info!("No existing launcher config ({e}), using defaults");
            LauncherConfig::default()
        }
    };
    if config.workshop_game_dir.is_empty() {
        config.workshop_game_dir = workshop_directory(&config.game_dir);
    }
    if config.workshop_server.trim().is_empty() {
        config.workshop_server = default_workshop_server();
    }
    config
}

/// Save the launcher config to disk.
pub fn save_config(app_data_dir: &PathBuf, config: &LauncherConfig) -> Result<(), String> {
    validate_config(config)?;
    std::fs::create_dir_all(app_data_dir)
        .map_err(|e| format!("Failed to create app data dir: {e}"))?;
    let path = config_path(app_data_dir);
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {e}"))?;
    #[cfg(unix)]
    let written = {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .and_then(|mut file| {
                // `mode` only applies on create; configs written by older versions stay 0644.
                file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
                file.write_all(json.as_bytes())
            })
    };
    #[cfg(not(unix))]
    let written = std::fs::write(&path, json);
    written.map_err(|e| format!("Failed to write config: {e}"))?;
    log::info!("Launcher config saved to {}", path.display());
    Ok(())
}

pub fn validate_config(config: &LauncherConfig) -> Result<(), String> {
    let game = configured_directory(&config.game_dir)?;
    let workshop = configured_directory(&config.workshop_game_dir)?;
    if game.starts_with(&workshop) || workshop.starts_with(&game) {
        return Err(
            "Для игры и мастерской нужны отдельные папки, без вложения одной в другую.".into(),
        );
    }
    if !config.workshop_server.trim().is_empty() {
        server_host(&config.workshop_server)?;
    }
    validate_directory(match config.active_profile {
        ClientProfile::Game => &config.game_dir,
        ClientProfile::Workshop => &config.workshop_game_dir,
    })?;
    validate_runtime(&config.active())
}

fn validate_directory(directory: &str) -> Result<PathBuf, String> {
    let game_dir = absolute_directory(directory)?;
    std::fs::create_dir_all(&game_dir)
        .map_err(|e| format!("Game directory cannot be created: {e}"))?;
    let probe = game_dir.join(".blockfield-write-test");
    std::fs::write(&probe, b"")
        .and_then(|_| std::fs::remove_file(&probe))
        .map_err(|e| format!("Game directory is not writable: {e}"))?;
    game_dir.canonicalize().map_err(|e| e.to_string())
}

fn absolute_directory(directory: &str) -> Result<PathBuf, String> {
    let game_dir = PathBuf::from(directory);
    if !game_dir.is_absolute() || game_dir.parent().is_none() {
        return Err(
            "Game directory must be a safe absolute path, not a filesystem root".to_string(),
        );
    }
    Ok(game_dir)
}

// Resolve existing symlinks without creating or write-probing the inactive installation.
fn configured_directory(directory: &str) -> Result<PathBuf, String> {
    let mut ancestor = absolute_directory(directory)?;
    let mut missing = Vec::new();
    loop {
        match ancestor.canonicalize() {
            Ok(mut path) => {
                for name in missing.into_iter().rev() {
                    path.push(name);
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if std::fs::symlink_metadata(&ancestor).is_ok() {
                    return Err(error.to_string());
                }
                missing.push(
                    ancestor
                        .file_name()
                        .ok_or_else(|| error.to_string())?
                        .to_owned(),
                );
                ancestor.pop();
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

fn validate_runtime(config: &LauncherConfig) -> Result<(), String> {
    let max_ram = max_ram_mb();
    if config.ram_mb < 2_048 || config.ram_mb > max_ram {
        return Err(format!("RAM must be between 2048 and {max_ram} MB"));
    }

    if !config.username.trim().is_empty()
        && !crate::commands::valid_username(config.username.trim())
    {
        return Err("Username must be 3-16 characters: letters, digits or _".to_string());
    }

    if !config.java_path.trim().is_empty() {
        let path = Path::new(&config.java_path);
        if !path.is_file() {
            return Err("Selected Java executable does not exist".to_string());
        }
        let output = crate::host_env::command(path)
            .arg("-version")
            .output()
            .map_err(|e| format!("Failed to run selected Java: {e}"))?;
        let version = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() || !(version.contains("\"17") || version.contains("\"21")) {
            return Err("Selected Java must be a working Java 17 or 21 runtime".to_string());
        }
    }
    Ok(())
}

pub fn max_ram_mb() -> u32 {
    total_memory_mb().map_or(32_768, |total| total.saturating_mul(3) / 4)
}

#[cfg(windows)]
fn total_memory_mb() -> Option<u32> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..unsafe { std::mem::zeroed() }
    };
    (unsafe { GlobalMemoryStatusEx(&mut status) } != 0)
        .then_some((status.ullTotalPhys / 1024 / 1024).min(u64::from(u32::MAX)) as u32)
}

#[cfg(not(windows))]
fn total_memory_mb() -> Option<u32> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    let kb = meminfo.lines().find_map(|line| {
        line.strip_prefix("MemTotal:")?
            .split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()
    })?;
    Some((kb / 1024).min(u64::from(u32::MAX)) as u32)
}

fn presence_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_settings_load_with_empty_commands() {
        let mut value = serde_json::to_value(LauncherConfig::default()).unwrap();
        value.as_object_mut().unwrap().remove("preLaunchCommand");
        value.as_object_mut().unwrap().remove("postExitCommand");
        value.as_object_mut().unwrap().remove("hideWhilePlaying");
        value["ramMb"] = serde_json::json!(6144);
        let config: LauncherConfig = serde_json::from_value(value).unwrap();
        assert!(config.pre_launch_command.is_empty());
        assert!(config.post_exit_command.is_empty());
        assert!(!config.hide_while_playing);
        assert_eq!(config.ram_mb, 6144);
    }

    #[test]
    fn rejects_relative_root_and_unsafe_ram_settings() {
        let mut config = LauncherConfig {
            game_dir: ".".to_string(),
            ..LauncherConfig::default()
        };
        assert!(validate_config(&config).is_err());

        config.game_dir = std::env::temp_dir()
            .join(format!("blockfield-config-{}", std::process::id()))
            .to_string_lossy()
            .to_string();
        config.workshop_game_dir = format!("{}-workshop", config.game_dir);
        config.ram_mb = 1;
        assert!(validate_config(&config).is_err());
        let _ = std::fs::remove_dir_all(&config.game_dir);
        let _ = std::fs::remove_dir_all(&config.workshop_game_dir);
    }

    #[test]
    fn legacy_game_files_and_account_stay_in_place() {
        let scratch = tempfile::tempdir().unwrap();
        let game = scratch.path().join("legacy-game");
        std::fs::create_dir_all(&game).unwrap();
        let options = game.join("options.txt");
        std::fs::write(&options, "key_key.attack:key.mouse.left\n").unwrap();
        let mut legacy = serde_json::to_value(LauncherConfig::default()).unwrap();
        for field in [
            "activeProfile",
            "workshopGameDir",
            "workshopJavaPath",
            "workshopServer",
        ] {
            legacy.as_object_mut().unwrap().remove(field);
        }
        legacy["gameDir"] = game.to_string_lossy().as_ref().into();
        legacy["accessToken"] = "fake-test-session".into();
        std::fs::write(config_path(scratch.path()), legacy.to_string()).unwrap();
        let mut config = load_config(scratch.path());
        assert_eq!(config.active_profile, ClientProfile::Game);
        assert_eq!(Path::new(&config.game_dir), game);
        assert_eq!(
            config.workshop_game_dir,
            workshop_directory(&config.game_dir)
        );
        assert_eq!(config.workshop_server, "workshop.blockfield.pro:25565");
        config.active_profile = ClientProfile::Workshop;
        let active = config.active();
        assert_eq!(active.game_dir, config.workshop_game_dir);
        assert_eq!(active.access_token, "fake-test-session");
        assert_eq!(Path::new(&config.game_dir), game);
        assert_eq!(
            std::fs::read_to_string(options).unwrap(),
            "key_key.attack:key.mouse.left\n"
        );
        assert!(!Path::new(&active.game_dir).exists());
    }

    #[test]
    fn profile_paths_java_and_bridge_are_independent() {
        let mut config = LauncherConfig::default();
        let game = config.game_dir.clone();
        config.active_profile = ClientProfile::Workshop;
        config.set_active_paths("workshop-directory".into(), "workshop-java".into());
        assert_eq!(config.game_dir, game);
        assert_eq!(config.active().java_path, "workshop-java");
        let app_data = Path::new("app-data");
        assert_eq!(config.bridge_dir(app_data), app_data.join("workshop"));
        let round_trip: LauncherConfig =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        assert_eq!(round_trip.active_profile, ClientProfile::Workshop);
        config.active_profile = ClientProfile::Game;
        assert_eq!(config.active().game_dir, game);
        assert_eq!(config.bridge_dir(app_data), app_data);
    }

    #[test]
    fn profile_directories_cannot_overlap_or_alias() {
        let scratch = tempfile::tempdir().unwrap();
        let game = scratch.path().join("game");
        let mut config = LauncherConfig {
            game_dir: game.to_string_lossy().into_owned(),
            workshop_game_dir: game.to_string_lossy().into_owned(),
            ..LauncherConfig::default()
        };
        assert!(validate_config(&config).is_err());
        config.workshop_game_dir = game.join("inside").to_string_lossy().into_owned();
        assert!(validate_config(&config).is_err());
        #[cfg(unix)]
        {
            std::fs::create_dir_all(&game).unwrap();
            let alias = scratch.path().join("alias");
            std::os::unix::fs::symlink(&game, &alias).unwrap();
            config.workshop_game_dir = alias.to_string_lossy().into_owned();
            assert!(validate_config(&config).is_err());
        }
        config.workshop_game_dir = scratch
            .path()
            .join("workshop")
            .to_string_lossy()
            .into_owned();
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn validating_one_profile_does_not_create_the_other_installation() {
        let scratch = tempfile::tempdir().unwrap();
        let game = scratch.path().join("game");
        let workshop = scratch.path().join("not-created").join("workshop");
        let mut config = LauncherConfig {
            game_dir: game.to_string_lossy().into_owned(),
            workshop_game_dir: workshop.to_string_lossy().into_owned(),
            ..LauncherConfig::default()
        };
        validate_config(&config).unwrap();
        assert!(game.is_dir());
        assert!(!workshop.parent().unwrap().exists());
        config.active_profile = ClientProfile::Workshop;
        config.game_dir = scratch
            .path()
            .join("absent-game")
            .to_string_lossy()
            .into_owned();
        validate_config(&config).unwrap();
        assert!(workshop.is_dir());
        assert!(!Path::new(&config.game_dir).exists());
    }

    #[test]
    fn default_workshop_server_migrates_empty_persisted_value_and_preserves_custom() {
        let scratch = tempfile::tempdir().unwrap();
        let mut empty_config = serde_json::to_value(LauncherConfig::default()).unwrap();
        empty_config["workshopServer"] = "".into();
        std::fs::write(config_path(scratch.path()), empty_config.to_string()).unwrap();
        let loaded = load_config(scratch.path());
        assert_eq!(loaded.workshop_server, "workshop.blockfield.pro:25565");

        let mut custom_config = serde_json::to_value(LauncherConfig::default()).unwrap();
        custom_config["workshopServer"] = "custom.example:25565".into();
        std::fs::write(config_path(scratch.path()), custom_config.to_string()).unwrap();
        let loaded_custom = load_config(scratch.path());
        assert_eq!(loaded_custom.workshop_server, "custom.example:25565");
    }

    #[test]
    fn workshop_requires_a_distinct_valid_handshake_host() {
        let mut config = LauncherConfig {
            active_profile: ClientProfile::Workshop,
            workshop_server: String::new(),
            ..LauncherConfig::default()
        };
        assert!(config.target("raknet;game.example:25566").is_err());
        for address in ["GAME.example.:25570", "raknet;game.example:25566"] {
            config.workshop_server = address.into();
            assert!(config.target("game.example:25565").is_err());
        }
        config.workshop_server = "raknet;workshop.example:25566".into();
        assert_eq!(
            config.target("game.example:25565").unwrap().as_deref(),
            Some("raknet;workshop.example:25566")
        );
        for address in [
            "https://workshop.example",
            "a b:25565",
            "user@host",
            "host:0",
            "host:65536",
            "host?query",
            "host#fragment",
            "host..",
            "-host.example",
            "host_.example",
        ] {
            assert!(server_host(address).is_err(), "{address}");
        }
        assert_eq!(server_host("raknet;[::1]:25566").unwrap(), "::1");
        config.workshop_server = "[0:0:0:0:0:0:0:1]:25566".into();
        assert!(config.target("raknet;[::1]:25565").is_err());
        config.active_profile = ClientProfile::Game;
        assert_eq!(
            config
                .target("raknet;game.example:25566")
                .unwrap()
                .as_deref(),
            Some("raknet;game.example:25566")
        );
    }
}
