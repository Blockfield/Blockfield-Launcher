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
}

#[derive(Debug, Clone, Deserialize)]
pub struct LauncherInfo {
    /// Absolute URL of `pack.toml`.
    pub pack: String,
    /// `host:port` the game auto-connects to.
    pub server: String,
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
    pub version: String,
    pub minecraft: String,
    /// Fabric loader version from `[versions] fabric`.
    pub loader: Option<String>,
}

pub fn platform_key() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Minimal TOML reader for the three keys we need; avoids a TOML dependency.
pub fn parse_pack_toml(text: &str) -> Result<PackMeta, String> {
    let mut section = String::new();
    let (mut version, mut minecraft, mut loader) = (None, None, None);
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
            ("versions", "minecraft") => minecraft = Some(value),
            ("versions", "fabric") => loader = Some(value),
            _ => {}
        }
    }
    Ok(PackMeta {
        version: version.ok_or("pack.toml has no version")?,
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
    client()
        .get(url)
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|e| format!("{url}: {e}"))?
        .text()
        .await
        .map_err(|e| format!("{url}: {e}"))
}

pub async fn fetch_launcher_info(base: &str) -> Result<LauncherInfo, String> {
    let url = format!("{base}/launcher.json");
    serde_json::from_str(&fetch_text(&url).await?).map_err(|e| format!("{url}: {e}"))
}

pub async fn fetch_pack_meta(url: &str) -> Result<PackMeta, String> {
    parse_pack_toml(&fetch_text(url).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pack_toml_versions() {
        let meta = parse_pack_toml(
            "name = \"Blockfield\"\nversion = \"1.2.3\" # comment\n[index]\nfile = \"index.toml\"\n[versions]\nfabric = \"0.19.3\"\nminecraft = \"1.21.1\"\n",
        )
        .unwrap();
        assert_eq!(
            meta,
            PackMeta {
                version: "1.2.3".into(),
                minecraft: "1.21.1".into(),
                loader: Some("0.19.3".into())
            }
        );
        assert!(parse_pack_toml("name = \"x\"").is_err());
    }

    #[test]
    fn platform_key_is_stable() {
        assert!(platform_key().contains('-'));
    }
}
