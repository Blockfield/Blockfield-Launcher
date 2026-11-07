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
    /// `[index] file`: the packwiz index, home of the `preserve` flags.
    pub index_file: String,
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
    let (mut version, mut index_hash, mut index_file, mut minecraft, mut loader) =
        (None, None, None, None, None);
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
            ("index", "file") => index_file = Some(value),
            ("versions", "minecraft") => minecraft = Some(value),
            ("versions", "fabric") => loader = Some(value),
            _ => {}
        }
    }
    Ok(PackMeta {
        version: version.ok_or("pack.toml has no version")?,
        index_hash: index_hash.ok_or("pack.toml has no [index].hash")?,
        index_file: index_file.ok_or("pack.toml has no [index].file")?,
        minecraft: minecraft.ok_or("pack.toml has no [versions].minecraft")?,
        loader,
    })
}

/// Game-dir-relative paths of the `[[files]]` entries a packwiz `index.toml`
/// marks `preserve = true`: the game and the player edit these by design, so
/// their hashes never match the installer cache.
pub fn parse_index_preserved(text: &str) -> Vec<String> {
    let mut preserved = Vec::new();
    let mut in_files = false;
    let mut file: Option<String> = None;
    let mut keep = false;
    let mut commit = |file: &mut Option<String>, keep: &mut bool| {
        if std::mem::replace(keep, false) {
            if let Some(path) = file.take().filter(|path| !path.is_empty()) {
                preserved.push(path);
            }
        } else {
            file.take();
        }
    };
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            commit(&mut file, &mut keep);
            in_files = line == "[[files]]";
            continue;
        }
        if !in_files {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "file" => file = Some(value.trim().trim_matches('"').to_string()),
            "preserve" => keep = value.trim() == "true",
            _ => {}
        }
    }
    commit(&mut file, &mut keep);
    preserved
}

/// Index URL next to the pack manifest: `<pack-dir>/<index file>`.
fn index_url(pack_toml_url: &str, index_file: &str) -> String {
    match pack_toml_url.trim_end_matches('/').rsplit_once('/') {
        Some((root, _)) => format!("{root}/{index_file}"),
        None => index_file.to_string(),
    }
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
    serde_json::from_str(&fetch_text(&url).await?).map_err(|e| format!("{url}: {e}"))
}

pub async fn fetch_pack_meta(url: &str) -> Result<PackMeta, String> {
    parse_pack_toml(&fetch_text(url).await?)
}

/// Preserved paths for a pack, with the pack `[index] hash`: fetches the
/// published index next to `pack.toml` and trusts it only when its sha256
/// matches the manifest, so a tampered or truncated mirror cannot silently
/// exempt files from the launch check.
pub async fn fetch_preserved(pack_toml_url: &str) -> Result<(String, Vec<String>), String> {
    let meta = parse_pack_toml(&fetch_text(pack_toml_url).await?)?;
    let url = index_url(pack_toml_url, &meta.index_file);
    let index = fetch_text(&url).await?;
    use sha2::Digest;
    let digest = crate::download::hex_digest(&sha2::Sha256::digest(index.as_bytes()));
    if !digest.eq_ignore_ascii_case(&meta.index_hash) {
        return Err(format!("{url}: index hash mismatch"));
    }
    Ok((meta.index_hash, parse_index_preserved(&index)))
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
                index_file: "index.toml".into(),
                minecraft: "1.21.1".into(),
                loader: Some("0.19.3".into())
            }
        );
        assert!(parse_pack_toml("name = \"x\"").is_err());
        assert!(parse_pack_toml("version = \"1\"\n[versions]\nminecraft = \"1.21.1\"").is_err());
        assert!(parse_pack_toml(
            "version = \"1\"\n[index]\nhash = \"abc\"\n[versions]\nminecraft = \"1.21.1\""
        )
        .is_err());
    }

    #[test]
    fn parses_index_preserved_entries() {
        let index = "hash-format = \"sha256\"\n\
            \n[[files]]\nfile = \"config/a.toml\"\nhash = \"00\"\npreserve = true\n\
            \n[[files]]\npreserve = true\nfile = \"options.txt\" # player name in a comment: \"x\"\nhash = \"11\"\n\
            \n[[files]]\nfile = \"mods/a.jar\"\nhash = \"22\"\n\
            \n[[files]]\nfile = \"servers.dat\"\nhash = \"33\"\n";
        assert_eq!(
            parse_index_preserved(index),
            ["config/a.toml", "options.txt"]
        );
        assert!(parse_index_preserved("hash-format = \"sha256\"\n").is_empty());
    }

    #[test]
    fn index_url_sits_next_to_pack_toml() {
        assert_eq!(
            index_url("https://blockfield.pro/pack.toml", "index.toml"),
            "https://blockfield.pro/index.toml"
        );
    }

    #[test]
    fn platform_key_is_stable() {
        assert!(platform_key().contains('-'));
    }
}
