//! The modpack is a packwiz pack hosted as static files. Next to `pack.toml` lives `launcher.json`
//! with everything the launcher needs that packwiz does not describe: the game server, the
//! packwiz-installer jar and a Java runtime per platform.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaInfo {
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    #[serde(default)]
    pub platform: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    pub url: String,
    pub sha256: String,
    /// Fallback download URL (GitHub release asset); used when `url` fails.
    #[serde(default)]
    pub source: Option<String>,
}

impl Artifact {
    /// Primary URL first, then the fallback when it differs and is non-empty.
    pub fn sources(&self) -> Vec<&str> {
        let mut out = vec![self.url.as_str()];
        if let Some(source) = self.source.as_deref().filter(|s| !s.trim().is_empty()) {
            if source != self.url {
                out.push(source);
            }
        }
        out
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LauncherInfo {
    /// Absolute URL of `pack.toml`.
    pub pack: String,
    /// `host:port` the game auto-connects to.
    pub server: String,
    /// Drasl skin server base URL; defaults to `skins::DEFAULT_SERVER`.
    #[serde(default)]
    pub skins: Option<String>,
    pub installer: Artifact,
    /// packwiz-installer refuses to start without its bootstrap, so both jars are shipped.
    pub bootstrap: Artifact,
    /// Keyed by `platform_key()`.
    pub java: HashMap<String, JavaInfo>,
}

impl LauncherInfo {
    pub fn java_for_this_platform(&self) -> Option<JavaInfo> {
        let key = platform_key();
        self.java.get(&key).cloned().map(|mut java| {
            java.platform = key;
            java
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackMeta {
    /// Display only; updates are detected by `index_hash`.
    pub version: String,
    /// `[index] hash`: changes whenever any client file changes.
    pub index_hash: String,
    pub minecraft: String,
    /// Fabric loader version from `[versions] fabric`.
    pub loader: Option<String>,
}

pub fn platform_key() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Minimal TOML reader for the few keys we need; avoids a TOML dependency.
pub fn parse_pack_toml(text: &str) -> Result<PackMeta, String> {
    let mut section = String::new();
    let (mut version, mut index_hash, mut minecraft, mut loader) = (None, None, None, None);
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.trim().to_string();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').to_string();
        match (section.as_str(), key.trim()) {
            ("", "version") => version = Some(value),
            ("index", "hash") => index_hash = Some(value),
            ("versions", "minecraft") => minecraft = Some(value),
            ("versions", "fabric") => loader = Some(value),
            _ => {}
        }
    }
    Ok(PackMeta {
        version: version.ok_or("pack.toml has no version")?,
        index_hash: index_hash.ok_or("pack.toml has no [index].hash")?,
        minecraft: minecraft.ok_or("pack.toml has no [versions].minecraft")?,
        loader,
    })
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .expect("valid HTTP client")
}

async fn fetch_text(url: &str) -> Result<String, String> {
    // The update check is a single user click: one lost packet must not fail it.
    let mut last_error = String::new();
    for attempt in 1..=3 {
        let result = client()
            .get(url)
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .map_err(|e| format!("{url}: {e}"));
        match result {
            Ok(response) => {
                return response.text().await.map_err(|e| format!("{url}: {e}"));
            }
            Err(error) => {
                last_error = error;
                if attempt < 3 {
                    tokio::time::sleep(std::time::Duration::from_millis(250 * attempt)).await;
                }
            }
        }
    }
    Err(format!("{last_error} (after 3 attempts)"))
}

pub async fn fetch_launcher_info(base: &str) -> Result<LauncherInfo, String> {
    let url = format!("{base}/launcher.json");
    serde_json::from_str(&fetch_text_with_fallback(&url).await?).map_err(|e| format!("{url}: {e}"))
}

pub async fn fetch_pack_meta(url: &str) -> Result<PackMeta, String> {
    parse_pack_toml(&fetch_text_with_fallback(url).await?)
}

/// Stable GitHub copies of the manifests, republished by client CI on every
/// tag release. The pack files have R2/GitHub mirrors with automatic
/// failover, but launcher.json/pack.toml are otherwise pack-host-only: one
/// stalled uplink fails the whole update before the installer even starts.
const MANIFEST_FALLBACK_BASE: &str =
    "https://github.com/Blockfield/blockfield-pack-mirror/releases/download/manifest";

/// GitHub fallback for a pack-host manifest URL, if it has a stable copy.
fn github_manifest_fallback(url: &str) -> Option<String> {
    let asset = match url.strip_prefix("https://blockfield.pro/")? {
        "launcher.json" => "launcher.json",
        "pack.toml" => "pack.toml",
        "workshop-pack/launcher.json" => "workshop-launcher.json",
        "workshop-pack/pack.toml" => "workshop-pack.toml",
        _ => return None,
    };
    Some(format!("{MANIFEST_FALLBACK_BASE}/{asset}"))
}

async fn fetch_text_with_fallback(url: &str) -> Result<String, String> {
    match fetch_text(url).await {
        Ok(text) => Ok(text),
        Err(primary) => {
            let Some(fallback) = github_manifest_fallback(url) else {
                return Err(primary);
            };
            // Same pattern as the installer-jar GitHub fallback: the pack
            // host stays primary, GitHub is tried once before failing.
            log::warn!("[manifest] {url} failed ({primary}); trying {fallback}");
            fetch_text(&fallback)
                .await
                .map_err(|error| format!("{url}: {primary}; fallback {fallback}: {error}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pack_toml_versions() {
        let meta = parse_pack_toml(
            "name = \"Blockfield\"\nversion = \"1.2.3\" # comment\n[index]\nfile = \"index.toml\"\nhash = \"abc123\"\n[versions]\nfabric = \"0.19.3\"\nminecraft = \"1.21.1\"\n",
        )
        .unwrap();
        assert_eq!(
            meta,
            PackMeta {
                version: "1.2.3".into(),
                index_hash: "abc123".into(),
                minecraft: "1.21.1".into(),
                loader: Some("0.19.3".into())
            }
        );
        assert!(parse_pack_toml("name = \"x\"").is_err());
        assert!(parse_pack_toml("version = \"1\"\n[versions]\nminecraft = \"1.21.1\"").is_err());
    }

    #[test]
    fn platform_key_is_stable() {
        assert!(platform_key().contains('-'));
    }

    #[test]
    fn manifest_fallback_maps_pack_host_manifests_to_github() {
        for (url, asset) in [
            ("https://blockfield.pro/launcher.json", "launcher.json"),
            ("https://blockfield.pro/pack.toml", "pack.toml"),
            (
                "https://blockfield.pro/workshop-pack/launcher.json",
                "workshop-launcher.json",
            ),
            (
                "https://blockfield.pro/workshop-pack/pack.toml",
                "workshop-pack.toml",
            ),
        ] {
            assert_eq!(
                github_manifest_fallback(url),
                Some(format!("{MANIFEST_FALLBACK_BASE}/{asset}"))
            );
        }
        // No stable copy, no fallback: dev overrides and non-manifest paths
        // keep the old pack-host-only behavior.
        assert_eq!(
            github_manifest_fallback("https://blockfield.pro/workshop-pack/index.toml"),
            None
        );
        assert_eq!(
            github_manifest_fallback("http://localhost:8080/launcher.json"),
            None
        );
    }

    fn artifact(url: &str, source: Option<&str>) -> Artifact {
        Artifact {
            url: url.into(),
            sha256: "0".repeat(64),
            source: source.map(str::to_string),
        }
    }

    #[test]
    fn artifact_sources_prefers_pack_host_then_github_fallback() {
        assert_eq!(
            artifact("https://blockfield.pro/a.jar", None).sources(),
            ["https://blockfield.pro/a.jar"]
        );
        assert_eq!(
            artifact(
                "https://blockfield.pro/packwiz-installer.jar",
                Some("https://github.com/Blockfield/blockfield-client/releases/download/x/packwiz-installer.jar"),
            )
            .sources(),
            [
                "https://blockfield.pro/packwiz-installer.jar",
                "https://github.com/Blockfield/blockfield-client/releases/download/x/packwiz-installer.jar",
            ]
        );
        // Empty or duplicate fallbacks add no second attempt.
        assert_eq!(
            artifact("https://blockfield.pro/a.jar", Some("  ")).sources(),
            ["https://blockfield.pro/a.jar"]
        );
        assert_eq!(
            artifact(
                "https://blockfield.pro/a.jar",
                Some("https://blockfield.pro/a.jar"),
            )
            .sources(),
            ["https://blockfield.pro/a.jar"]
        );
    }

    #[test]
    fn launcher_info_parses_live_shape_with_and_without_source() {
        let with: LauncherInfo = serde_json::from_str(
            r#"{"pack":"https://blockfield.pro/pack.toml","server":"raknet;play.blockfield.pro:25566",
                "installer":{"url":"https://blockfield.pro/packwiz-installer.jar","sha256":"a",
                "source":"https://github.com/Blockfield/blockfield-client/releases/download/i/packwiz-installer.jar"},
                "bootstrap":{"url":"https://blockfield.pro/packwiz-installer-bootstrap.jar","sha256":"b",
                "source":"https://github.com/packwiz/packwiz-installer-bootstrap/releases/download/v0.0.3/packwiz-installer-bootstrap.jar"},
                "java":{}}"#,
        )
        .unwrap();
        assert_eq!(with.installer.sources().len(), 2);
        let without: LauncherInfo = serde_json::from_str(
            r#"{"pack":"https://blockfield.pro/pack.toml","server":"x",
                "installer":{"url":"https://blockfield.pro/a.jar","sha256":"a"},
                "bootstrap":{"url":"https://blockfield.pro/b.jar","sha256":"b"},"java":{}}"#,
        )
        .unwrap();
        assert_eq!(without.bootstrap.sources().len(), 1);
    }
}
