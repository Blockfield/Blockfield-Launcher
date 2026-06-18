use serde::{Deserialize, Serialize};

/// A single file entry in the modpack manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFileEntry {
    /// Relative path within the game directory, e.g. "mods/battlefield-core.jar"
    pub path: String,
    /// File size in bytes
    pub size: u64,
    /// Hex-encoded SHA-256 hash of the file
    pub sha256: String,
    /// Fully-qualified download URL for this file
    pub url: String,
}

/// Info about required Java runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaInfo {
    /// Java version, e.g. "21.0.5"
    pub version: String,
    /// Target platform, e.g. "windows-x86_64"
    pub platform: String,
    /// Download URL for the Java archive
    pub url: String,
    /// SHA-256 checksum of the archive
    pub sha256: String,
    /// Archive size in bytes
    pub size: u64,
}

/// Info about required Forge installer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeInfo {
    /// Forge version, e.g. "1.20.1-47.4.10"
    pub version: String,
    /// Download URL for the installer JAR
    pub url: String,
    /// SHA-256 checksum of the installer JAR
    pub sha256: String,
    /// File size in bytes
    pub size: u64,
}

/// The remote modpack manifest served by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModpackManifest {
    /// Semantic version of the modpack, e.g. "0.1.43"
    pub version: String,
    /// Minecraft game version this modpack targets, e.g. "1.20.1"
    #[serde(rename = "minecraftVersion")]
    pub minecraft_version: String,
    /// All files that constitute the modpack
    pub files: Vec<ManifestFileEntry>,
    /// Sum of all file sizes in bytes
    #[serde(rename = "totalSize")]
    pub total_size: u64,
    /// ISO 8601 timestamp of the release
    #[serde(rename = "releaseDate")]
    pub release_date: String,
    /// Optional: glob-like paths to delete for this version
    pub prune: Option<Vec<String>>,
    /// Required Java runtime info (auto-downloaded if not installed)
    pub java: Option<JavaInfo>,
    /// Forge installer info (auto-downloaded + run if not installed)
    pub forge: Option<ForgeInfo>,
}

/// Result returned to the frontend after checking for modpack updates.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionCheckResult {
    pub needs_update: bool,
    pub remote_version: String,
    pub installed_version: String,
    pub file_count: usize,
    pub total_size: u64,
    /// Required Java info from the manifest
    pub java: Option<JavaInfo>,
    /// Whether the installed Java matches the required version
    pub java_ok: bool,
}
