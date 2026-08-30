use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use url::{Host, Url};

pub const MAX_MANIFEST_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ManifestLimits {
    pub max_manifest_bytes: usize,
    pub max_files: usize,
    pub max_path_bytes: usize,
    pub max_component_bytes: usize,
    pub max_file_bytes: u64,
    pub max_payload_bytes: u64,
}

impl Default for ManifestLimits {
    fn default() -> Self {
        Self {
            max_manifest_bytes: MAX_MANIFEST_BYTES,
            max_files: 100_000,
            max_path_bytes: 1_024,
            max_component_bytes: 255,
            max_file_bytes: 16 * 1024 * 1024 * 1024,
            max_payload_bytes: 64 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ManifestPolicy {
    pub limits: ManifestLimits,
    pub trusted_download_hosts: Vec<String>,
    pub allow_insecure_localhost: bool,
}

#[derive(Debug, Clone)]
pub struct ArchiveLimits {
    pub max_compressed_bytes: u64,
    pub max_uncompressed_bytes: u64,
    pub max_entry_bytes: u64,
    pub max_entries: usize,
    pub max_compression_ratio: u64,
    pub max_path_depth: usize,
    pub min_free_space_bytes: u64,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_compressed_bytes: 2 * 1024 * 1024 * 1024,
            max_uncompressed_bytes: 16 * 1024 * 1024 * 1024,
            max_entry_bytes: 4 * 1024 * 1024 * 1024,
            max_entries: 200_000,
            max_compression_ratio: 1_000,
            max_path_depth: 32,
            min_free_space_bytes: 2 * 1024 * 1024 * 1024,
        }
    }
}

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
    /// Optional: glob-like paths to delete for this version
    pub prune: Option<Vec<String>>,
    /// Required Java runtime info (auto-downloaded if not installed)
    pub java: Option<JavaInfo>,
    /// Forge installer info (auto-downloaded + run if not installed)
    pub forge: Option<ForgeInfo>,
}

/// Validates the complete untrusted manifest before it is cached or used.
/// `totalSize` is exactly the checked sum of `files`; Java and Forge sizes are
/// additionally included when enforcing the total payload ceiling.
pub fn validate_manifest(
    manifest: &ModpackManifest,
    manifest_url: &str,
    manifest_bytes: usize,
    policy: &ManifestPolicy,
) -> Result<(), String> {
    if manifest_bytes > policy.limits.max_manifest_bytes {
        return Err("Manifest exceeds the maximum encoded size".to_string());
    }
    if manifest.files.len() > policy.limits.max_files {
        return Err("Manifest contains too many files".to_string());
    }

    let source = validate_manifest_source(manifest_url, policy.allow_insecure_localhost)?;
    let trusted = policy
        .trusted_download_hosts
        .iter()
        .map(|host| host.trim().trim_end_matches('.').to_ascii_lowercase())
        .filter(|host| !host.is_empty())
        .collect::<HashSet<_>>();
    let mut paths = HashSet::with_capacity(manifest.files.len());
    let mut files_size = 0u64;

    for entry in &manifest.files {
        validate_relative_path(&entry.path, &policy.limits)?;
        if !paths.insert(entry.path.to_ascii_lowercase()) {
            return Err(format!("Duplicate manifest path: {}", entry.path));
        }
        validate_hash(&entry.sha256, &entry.path)?;
        validate_size(entry.size, &entry.path, &policy.limits)?;
        files_size = files_size
            .checked_add(entry.size)
            .ok_or_else(|| "Manifest file-size sum overflow".to_string())?;
        validate_download_url(&entry.url, &source, &trusted, policy, true)?;
    }

    if files_size != manifest.total_size {
        return Err(format!(
            "Manifest totalSize mismatch: expected {files_size}, got {}",
            manifest.total_size
        ));
    }

    let mut payload_size = files_size;
    if let Some(java) = &manifest.java {
        validate_hash(&java.sha256, "Java runtime")?;
        validate_size(java.size, "Java runtime", &policy.limits)?;
        validate_download_url(&java.url, &source, &trusted, policy, false)?;
        payload_size = payload_size
            .checked_add(java.size)
            .ok_or_else(|| "Manifest payload-size sum overflow".to_string())?;
    }
    if let Some(forge) = &manifest.forge {
        validate_hash(&forge.sha256, "Forge installer")?;
        validate_size(forge.size, "Forge installer", &policy.limits)?;
        validate_download_url(&forge.url, &source, &trusted, policy, false)?;
        payload_size = payload_size
            .checked_add(forge.size)
            .ok_or_else(|| "Manifest payload-size sum overflow".to_string())?;
    }
    if payload_size > policy.limits.max_payload_bytes {
        return Err("Manifest payload exceeds the configured limit".to_string());
    }

    if let Some(prune) = &manifest.prune {
        for pattern in prune {
            let value = pattern.strip_suffix("/*").unwrap_or(pattern);
            if value.contains(['*', '?']) {
                return Err(format!("Unsafe prune rule: {pattern}"));
            }
            validate_relative_path(value, &policy.limits)
                .map_err(|_| format!("Unsafe prune rule: {pattern}"))?;
        }
    }

    Ok(())
}

pub fn validate_manifest_source(url: &str, allow_insecure_localhost: bool) -> Result<Url, String> {
    let url = Url::parse(url).map_err(|_| "Manifest URL is invalid".to_string())?;
    validate_url_syntax(&url)?;
    let local = is_local_host(&url);
    if url.scheme() != "https" && !(allow_insecure_localhost && url.scheme() == "http" && local) {
        return Err("Manifest URL must use HTTPS".to_string());
    }
    if local && !allow_insecure_localhost {
        return Err("Manifest URL must not target a local or private host".to_string());
    }
    Ok(url)
}

pub fn validate_manifest_redirect(
    destination: &Url,
    initial: &Url,
    trusted_download_hosts: &[String],
    allow_insecure_localhost: bool,
) -> Result<(), String> {
    validate_url_syntax(destination)?;
    let local = is_local_host(destination);
    if destination.scheme() != "https"
        && !(allow_insecure_localhost && destination.scheme() == "http" && local)
    {
        return Err("Redirect must use HTTPS".to_string());
    }
    if local && !(allow_insecure_localhost && same_origin(destination, initial)) {
        return Err("Redirect targets a local or private host".to_string());
    }
    let destination_host = host_name(destination)?;
    let trusted = trusted_download_hosts.iter().any(|host| {
        host.trim()
            .trim_end_matches('.')
            .eq_ignore_ascii_case(destination_host)
    });
    if !same_origin(destination, initial) && !trusted {
        return Err("Redirect target is not trusted".to_string());
    }
    Ok(())
}

pub fn safe_manifest_path(value: &str) -> Result<PathBuf, String> {
    validate_relative_path(value, &ManifestLimits::default())?;
    Ok(value.split('/').collect())
}

fn validate_relative_path(value: &str, limits: &ManifestLimits) -> Result<(), String> {
    if value.is_empty()
        || value.len() > limits.max_path_bytes
        || value.starts_with(['/', '\\'])
        || value.contains('\\')
    {
        return Err("Path must be a bounded relative path using '/' separators".to_string());
    }
    for component in value.split('/') {
        if component.is_empty()
            || component == "."
            || component == ".."
            || component.len() > limits.max_component_bytes
            || component.contains(':')
            || component.ends_with(['.', ' '])
            || component.chars().any(char::is_control)
            || is_windows_device_name(component)
        {
            return Err(format!("Unsafe path component in {value}"));
        }
    }
    Ok(())
}

fn is_windows_device_name(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or(component);
    matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

fn validate_hash(hash: &str, label: &str) -> Result<(), String> {
    if hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(format!("Invalid SHA-256 for {label}"))
    }
}

fn validate_size(size: u64, label: &str, limits: &ManifestLimits) -> Result<(), String> {
    if size == 0 || size > limits.max_file_bytes {
        Err(format!("Invalid or oversized file size for {label}"))
    } else {
        Ok(())
    }
}

fn validate_download_url(
    value: &str,
    source: &Url,
    trusted: &HashSet<String>,
    policy: &ManifestPolicy,
    require_same_origin: bool,
) -> Result<(), String> {
    let url = Url::parse(value).map_err(|_| format!("Invalid download URL: {value}"))?;
    validate_url_syntax(&url)?;
    let local = is_local_host(&url);
    if url.scheme() != "https"
        && !(policy.allow_insecure_localhost && url.scheme() == "http" && local)
    {
        return Err(format!("Download URL must use HTTPS: {value}"));
    }
    if local && !(policy.allow_insecure_localhost && same_origin(&url, source)) {
        return Err(format!(
            "Download URL targets a local or private host: {value}"
        ));
    }
    let host = host_name(&url)?.trim_end_matches('.').to_ascii_lowercase();
    if require_same_origin && !same_origin(&url, source) {
        return Err(format!("File URL must use the manifest origin: {value}"));
    }
    if !require_same_origin && !same_origin(&url, source) && !trusted.contains(&host) {
        return Err(format!("Download host is not trusted: {host}"));
    }
    Ok(())
}

fn validate_url_syntax(url: &Url) -> Result<(), String> {
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err("URL credentials and fragments are forbidden".to_string());
    }
    host_name(url)?;
    Ok(())
}

fn host_name(url: &Url) -> Result<&str, String> {
    url.host_str()
        .ok_or_else(|| "URL host is required".to_string())
}

fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left.host_str().map(str::to_ascii_lowercase)
            == right.host_str().map(str::to_ascii_lowercase)
        && left.port_or_known_default() == right.port_or_known_default()
}

fn is_local_host(url: &Url) -> bool {
    match url.host() {
        Some(Host::Ipv4(ip)) => is_non_global_ipv4(ip.octets()),
        Some(Host::Ipv6(ip)) => {
            let octets = ip.octets();
            ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_multicast()
                || octets[0] & 0xfe == 0xfc
                || (octets[0] == 0xfe && octets[1] & 0xc0 == 0x80)
                || (octets[..10] == [0; 10]
                    && octets[10] == 0xff
                    && octets[11] == 0xff
                    && is_non_global_ipv4([octets[12], octets[13], octets[14], octets[15]]))
        }
        Some(Host::Domain(host)) => {
            let host = host.trim_end_matches('.').to_ascii_lowercase();
            host == "localhost"
                || host.ends_with(".localhost")
                || host.ends_with(".local")
                || !host.contains('.')
        }
        None => true,
    }
}

fn is_non_global_ipv4([a, b, c, _]: [u8; 4]) -> bool {
    a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 192 && b == 0 && c == 0)
        || (a == 198 && (b == 18 || b == 19))
        || (224..=255).contains(&a)
}

/// Result returned to the frontend after checking for modpack updates.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionCheckResult {
    pub needs_update: bool,
    pub remote_version: String,
    pub installed_version: String,
    pub mirror: String,
    pub file_count: usize,
    pub total_size: u64,
    /// Required Java info from the manifest
    pub java: Option<JavaInfo>,
    /// Whether the installed Java matches the required version
    pub java_ok: bool,
    /// Whether Forge is installed (true if not required)
    pub forge_ok: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, size: u64, url: &str) -> ManifestFileEntry {
        ManifestFileEntry {
            path: path.to_string(),
            size,
            sha256: "a".repeat(64),
            url: url.to_string(),
        }
    }

    fn manifest(files: Vec<ManifestFileEntry>) -> ModpackManifest {
        ModpackManifest {
            version: "1.0.0".to_string(),
            minecraft_version: "1.20.1".to_string(),
            total_size: files.iter().map(|file| file.size).sum(),
            files,
            prune: None,
            java: None,
            forge: None,
        }
    }

    fn policy() -> ManifestPolicy {
        ManifestPolicy {
            trusted_download_hosts: vec!["artifacts.example.com".to_string()],
            ..ManifestPolicy::default()
        }
    }

    fn validate(value: &ModpackManifest) -> Result<(), String> {
        validate_manifest(
            value,
            "https://play.example.com/api/launcher/v1/manifest.json",
            1_024,
            &policy(),
        )
    }

    #[test]
    fn accepts_a_normal_manifest() {
        let mut value = manifest(vec![file(
            "mods/core.jar",
            123,
            "https://play.example.com/api/launcher/v1/files/mods/core.jar",
        )]);
        value.java = Some(JavaInfo {
            version: "21".to_string(),
            platform: "windows-x86_64".to_string(),
            url: "https://artifacts.example.com/java.zip".to_string(),
            sha256: "b".repeat(64),
            size: 456,
        });
        assert_eq!(validate(&value), Ok(()));
    }

    #[test]
    fn rejects_unsafe_and_colliding_paths() {
        for path in [
            "../outside",
            "mods/../../outside",
            "/absolute/path",
            "C:/Windows/file",
            r"\\server\share",
            r"mods\file.jar",
            "mods//file.jar",
            "mods/./file.jar",
            "mods/NUL.txt",
            "mods/file. ",
        ] {
            let value = manifest(vec![file(
                path,
                1,
                "https://play.example.com/api/launcher/v1/files/file",
            )]);
            assert!(validate(&value).is_err(), "accepted {path}");
        }

        let duplicate = manifest(vec![
            file(
                "mods/Core.jar",
                1,
                "https://play.example.com/api/launcher/v1/files/one",
            ),
            file(
                "mods/core.jar",
                1,
                "https://play.example.com/api/launcher/v1/files/two",
            ),
        ]);
        assert!(validate(&duplicate).unwrap_err().contains("Duplicate"));
    }

    #[test]
    fn rejects_invalid_hash_sizes_counts_and_totals() {
        let mut invalid_hash = manifest(vec![file(
            "mods/core.jar",
            1,
            "https://play.example.com/api/launcher/v1/files/core.jar",
        )]);
        invalid_hash.files[0].sha256 = "xyz".to_string();
        assert!(validate(&invalid_hash).is_err());

        let mut zero = manifest(vec![file(
            "mods/core.jar",
            0,
            "https://play.example.com/api/launcher/v1/files/core.jar",
        )]);
        assert!(validate(&zero).is_err());
        zero.files[0].size = 1;
        zero.total_size = 2;
        assert!(validate(&zero).is_err());

        let mut low_limits = policy();
        low_limits.limits.max_files = 1;
        low_limits.limits.max_file_bytes = 2;
        low_limits.limits.max_payload_bytes = 2;
        let two_files = manifest(vec![
            file("a", 1, "https://play.example.com/api/launcher/v1/files/a"),
            file("b", 1, "https://play.example.com/api/launcher/v1/files/b"),
        ]);
        assert!(validate_manifest(
            &two_files,
            "https://play.example.com/api/launcher/v1/manifest.json",
            100,
            &low_limits,
        )
        .is_err());

        let oversized = manifest(vec![file(
            "large",
            3,
            "https://play.example.com/api/launcher/v1/files/large",
        )]);
        assert!(validate_manifest(
            &oversized,
            "https://play.example.com/api/launcher/v1/manifest.json",
            100,
            &low_limits,
        )
        .is_err());
        assert!(validate_manifest(
            &manifest(Vec::new()),
            "https://play.example.com/api/launcher/v1/manifest.json",
            MAX_MANIFEST_BYTES + 1,
            &policy(),
        )
        .is_err());

        let mut overflow = manifest(Vec::new());
        overflow.files = vec![
            file(
                "a",
                u64::MAX,
                "https://play.example.com/api/launcher/v1/files/a",
            ),
            file("b", 1, "https://play.example.com/api/launcher/v1/files/b"),
        ];
        overflow.total_size = 0;
        let mut overflow_policy = policy();
        overflow_policy.limits.max_file_bytes = u64::MAX;
        overflow_policy.limits.max_payload_bytes = u64::MAX;
        assert!(validate_manifest(
            &overflow,
            "https://play.example.com/api/launcher/v1/manifest.json",
            100,
            &overflow_policy,
        )
        .unwrap_err()
        .contains("overflow"));
    }

    #[test]
    fn rejects_untrusted_or_local_download_urls() {
        for url in [
            "http://play.example.com/file",
            "https://127.0.0.1/file",
            "https://10.0.0.1/file",
            "https://[::1]/file",
            "https://evil.example/file",
            "https://user:pass@play.example.com/file",
        ] {
            let value = manifest(vec![file("mods/core.jar", 1, url)]);
            assert!(validate(&value).is_err(), "accepted {url}");
        }

        let mut bootstrap = manifest(vec![file(
            "mods/core.jar",
            1,
            "https://play.example.com/file",
        )]);
        bootstrap.java = Some(JavaInfo {
            version: "21".to_string(),
            platform: "windows".to_string(),
            url: "https://127.0.0.1/java.zip".to_string(),
            sha256: "c".repeat(64),
            size: 1,
        });
        assert!(validate(&bootstrap).is_err());
    }

    #[test]
    fn redirects_cannot_escape_to_private_or_untrusted_hosts() {
        let initial = Url::parse("https://artifacts.example.com/java.zip").unwrap();
        assert!(validate_manifest_redirect(
            &Url::parse("https://artifacts.example.com/cdn/java.zip").unwrap(),
            &initial,
            &policy().trusted_download_hosts,
            false,
        )
        .is_ok());
        for redirect in [
            "http://artifacts.example.com/java.zip",
            "https://127.0.0.1/java.zip",
            "https://10.0.0.1/java.zip",
            "https://evil.example/java.zip",
        ] {
            assert!(validate_manifest_redirect(
                &Url::parse(redirect).unwrap(),
                &initial,
                &policy().trusted_download_hosts,
                false,
            )
            .is_err());
        }
    }
}
