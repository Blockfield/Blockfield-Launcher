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
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            game_dir: default_game_dir(),
            java_path: String::new(),
            ram_mb: env_u32("BLOCKFIELD_DEFAULT_RAM_MB", 8192),
            auto_update: env_bool("BLOCKFIELD_AUTO_UPDATE", true),
            lang: env_string("BLOCKFIELD_DEFAULT_LANG", "en"),
        }
    }
}

fn env_string(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn env_u32(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .map(|v| v == "true" || v == "1")
        .unwrap_or(default)
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
    std::fs::create_dir_all(app_data_dir)
        .map_err(|e| format!("Failed to create app data dir: {e}"))?;
    let path = config_path(app_data_dir);
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("Failed to write config: {e}"))?;
    log::info!("Launcher config saved to {}", path.display());
    Ok(())
}
