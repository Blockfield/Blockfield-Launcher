// Re-export shared types so the rest of the launcher can import from here.
pub use blockfield_shared::{ModpackManifest, VersionCheckResult};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Locally persisted record of what is installed.
/// Written to `.blockfield-manifest.json` inside the game directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledManifest {
    pub version: String,
    /// File path → sha256 mapping of installed files
    pub files: HashMap<String, String>,
}

impl InstalledManifest {
    /// Read from disk, or return a default empty manifest.
    pub fn load(game_dir: &str) -> Self {
        let path = std::path::PathBuf::from(game_dir).join(".blockfield-manifest.json");
        match std::fs::read_to_string(&path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_else(|_| Self::new("0.0.0")),
            Err(_) => Self::new("0.0.0"),
        }
    }

    /// Write to disk inside the game directory.
    pub fn save(&self, game_dir: &str) -> Result<(), String> {
        let dir = std::path::PathBuf::from(game_dir);
        std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create game dir: {e}"))?;
        let path = dir.join(".blockfield-manifest.json");
        let json =
            serde_json::to_string_pretty(self).map_err(|e| format!("Failed to serialize: {e}"))?;
        std::fs::write(&path, json).map_err(|e| format!("Failed to write manifest: {e}"))?;
        Ok(())
    }

    fn new(version: &str) -> Self {
        Self {
            version: version.to_string(),
            files: HashMap::new(),
        }
    }
}
