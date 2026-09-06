use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Launcher configuration persisted in Tauri's app data directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherConfig {
    /// Absolute path to the game directory where modpack files are installed
    pub game_dir: String,
    /// Absolute path to the Java runtime executable
    pub java_path: String,
    /// Allocated RAM in megabytes
    pub ram_mb: u32,
    /// Whether to auto-check for modpack updates on launch
    pub auto_update: bool,
    /// UI language code ("en", "ru", "uk")
    pub lang: String,
    /// Offline-mode Minecraft username (3-16 chars: letters, digits, underscore)
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub pre_launch_command: String,
    #[serde(default)]
    pub post_exit_command: String,
    #[serde(default)]
    pub hide_while_playing: bool,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        // Runtime first (dotenvy for local dev), then compile-time (CI build), then hardcoded.
        Self {
            game_dir: default_game_dir(),
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
            pre_launch_command: String::new(),
            post_exit_command: String::new(),
            hide_while_playing: false,
        }
    }
}

/// Platform-appropriate default game directory.
fn default_game_dir() -> String {
    if let Some(base) = directories::BaseDirs::new() {
        base.data_dir()
            .join("BlockField")
            .to_string_lossy()
            .to_string()
    } else {
        // Fallback for edge cases
        String::from("./BlockField")
    }
}

/// Path to the launcher config file inside the app data directory.
pub fn config_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("launcher-config.json")
}

/// Load the launcher config from disk, or return defaults.
pub fn load_config(app_data_dir: &Path) -> LauncherConfig {
    let path = config_path(app_data_dir);
    match std::fs::read_to_string(&path) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_else(|e| {
            log::warn!("Failed to parse launcher config, using defaults: {e}");
            LauncherConfig::default()
        }),
        Err(e) => {
            log::info!("No existing launcher config ({e}), using defaults");
            LauncherConfig::default()
        }
    }
}

/// Save the launcher config to disk.
pub fn save_config(app_data_dir: &PathBuf, config: &LauncherConfig) -> Result<(), String> {
    validate_config(config)?;
    std::fs::create_dir_all(app_data_dir)
        .map_err(|e| format!("Failed to create app data dir: {e}"))?;
    let path = config_path(app_data_dir);
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("Failed to write config: {e}"))?;
    log::info!("Launcher config saved to {}", path.display());
    Ok(())
}

pub fn validate_config(config: &LauncherConfig) -> Result<(), String> {
    let game_dir = PathBuf::from(config.game_dir.trim());
    if !game_dir.is_absolute() || game_dir.parent().is_none() {
        return Err(
            "Game directory must be a safe absolute path, not a filesystem root".to_string(),
        );
    }
    std::fs::create_dir_all(&game_dir)
        .map_err(|e| format!("Game directory cannot be created: {e}"))?;
    let probe = game_dir.join(".blockfield-write-test");
    std::fs::write(&probe, b"")
        .and_then(|_| std::fs::remove_file(&probe))
        .map_err(|e| format!("Game directory is not writable: {e}"))?;

    let max_ram = total_memory_mb().map_or(32_768, |total| total.saturating_mul(3) / 4);
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
        let output = std::process::Command::new(path)
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
        config.ram_mb = 1;
        assert!(validate_config(&config).is_err());
        let _ = std::fs::remove_dir_all(&config.game_dir);
    }
}
