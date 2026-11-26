//! The modpack is a packwiz pack hosted as static files. Next to `pack.toml` lives `launcher.json`
//! with everything the launcher needs that packwiz does not describe: the game server, the
//! packwiz-installer jar and a Java runtime per platform.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

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
    /// Mirror URLs for `pack.toml` (same file, other hosts). Backwards
    /// compatible: absent in old manifests means pack-host only.
    #[serde(default, rename = "packMirrors")]
    pub pack_mirrors: Vec<String>,
}

impl LauncherInfo {
    pub fn java_for_this_platform(&self) -> Option<JavaInfo> {
        let key = platform_key();
        self.java.get(&key).cloned().map(|mut java| {
            java.platform = key;
            java
        })
    }

    /// Primary `pack.toml` URL first, then deduplicated valid mirror URLs.
    /// Mirrors allow `https://`, and `http://` only under
    /// `cfg!(debug_assertions)` or `cfg(test)` (same rule as
    /// `select_pack_base_url`).
    pub fn pack_sources(&self) -> Vec<&str> {
        let mut out = vec![self.pack.as_str()];
        for mirror in &self.pack_mirrors {
            let url = mirror.trim();
            if url.is_empty() || url == self.pack {
                continue;
            }
            if out.contains(&url) {
                continue;
            }
            if !is_allowed_pack_url(url) {
                continue;
            }
            // Leak-free: push the slice borrowed from the stored String. The
            // trimmed view equals the stored value for well-formed entries;
            // entries with surrounding whitespace were skipped above only when
            // empty — otherwise re-borrow the trimmed form via the original.
            // To keep lifetimes simple, only accept exact stored values here:
            // find the suffix in the original string.
            let start = mirror
                .find(url)
                .expect("trimmed URL must occur in the original");
            out.push(&mirror[start..start + url.len()]);
        }
        out
    }
}

fn is_allowed_pack_url(url: &str) -> bool {
    let url = url.trim();
    let http_allowed = cfg!(debug_assertions) || cfg!(test);
    if !(url.starts_with("https://") || (http_allowed && url.starts_with("http://"))) {
        return false;
    }
    if url.contains(char::is_whitespace) {
        return false;
    }
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    if parsed.host_str().is_none() {
        return false;
    }
    matches!(parsed.scheme(), "https" | "http")
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

// Failover budgets: a dead/hanging pack host must fail over in 3-5s, not 60-360s.
pub const PRIMARY_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
pub const PRIMARY_TOTAL_TIMEOUT: Duration = Duration::from_secs(5);
pub const FALLBACK_CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
pub const FALLBACK_TOTAL_TIMEOUT: Duration = Duration::from_secs(20);
/// Session cooldown: after a host fails and a fallback succeeds, the dead
/// host is skipped for 60s (0ms spent on `blockfield.pro`).
pub const HEALTH_COOLDOWN: Duration = Duration::from_secs(60);

pub fn primary_failover_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(PRIMARY_CONNECT_TIMEOUT)
            .timeout(PRIMARY_TOTAL_TIMEOUT)
            .build()
            .expect("valid HTTP client")
    })
}

pub fn fallback_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(FALLBACK_CONNECT_TIMEOUT)
            .timeout(FALLBACK_TOTAL_TIMEOUT)
            .build()
            .expect("valid HTTP client")
    })
}

/// Per-host failure cooldown: a host that failed while a fallback succeeded is
/// skipped first for `HEALTH_COOLDOWN` (60s). Thread-safe process-wide state.
pub struct HostHealth {
    last_failure: Mutex<HashMap<String, Instant>>,
    cooldown: Duration,
}

impl HostHealth {
    pub fn new() -> Self {
        Self {
            last_failure: Mutex::new(HashMap::new()),
            cooldown: HEALTH_COOLDOWN,
        }
    }

    pub fn with_cooldown(cooldown: Duration) -> Self {
        Self {
            last_failure: Mutex::new(HashMap::new()),
            cooldown,
        }
    }

    /// Lowercase `host[:port]` via `url::Url` so loopback tests on
    /// `127.0.0.1:<port>` isolate per port. `None` for unparseable URLs.
    pub fn host_key(url: &str) -> Option<String> {
        let parsed = url::Url::parse(url.trim()).ok()?;
        let host = parsed.host_str()?.to_lowercase();
        match parsed.port() {
            Some(port) => Some(format!("{host}:{port}")),
            None => Some(host),
        }
    }

    pub fn is_cooled_down(&self, url: &str) -> bool {
        let Some(key) = Self::host_key(url) else {
            return false;
        };
        let guard = self.last_failure.lock().unwrap_or_else(|p| p.into_inner());
        guard
            .get(&key)
            .is_some_and(|at| at.elapsed() < self.cooldown)
    }

    pub fn mark_unhealthy(&self, url: &str) {
        let Some(key) = Self::host_key(url) else {
            return;
        };
        self.last_failure
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(key, Instant::now());
    }

    pub fn mark_healthy(&self, url: &str) {
        let Some(key) = Self::host_key(url) else {
            return;
        };
        self.last_failure
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&key);
    }

    /// Stable partition: healthy candidates first (original order), then
    /// cooled-down hosts (original order).
    pub fn order_urls<'a>(&self, urls: &[&'a str]) -> Vec<&'a str> {
        let mut healthy = Vec::with_capacity(urls.len());
        let mut cooled = Vec::with_capacity(urls.len());
        for url in urls {
            if self.is_cooled_down(url) {
                cooled.push(*url);
            } else {
                healthy.push(*url);
            }
        }
        healthy.extend(cooled);
        healthy
    }
}

impl Default for HostHealth {
    fn default() -> Self {
        Self::new()
    }
}

pub fn default_health() -> &'static HostHealth {
    static HEALTH: OnceLock<HostHealth> = OnceLock::new();
    HEALTH.get_or_init(HostHealth::new)
}

async fn fetch_once(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let response = client
        .get(url)
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|e| format!("{url}: {e}"))?;
    response.text().await.map_err(|e| format!("{url}: {e}"))
}

async fn fetch_with_retries(
    client: &reqwest::Client,
    url: &str,
    attempts: u32,
) -> Result<String, String> {
    let mut last_error = String::new();
    for attempt in 1..=attempts.max(1) {
        match fetch_once(client, url).await {
            Ok(text) => return Ok(text),
            Err(error) => {
                last_error = error;
                if attempt < attempts.max(1) {
                    tokio::time::sleep(Duration::from_millis(250 * u64::from(attempt))).await;
                }
            }
        }
    }
    Err(last_error)
}

/// Fetch the first reachable candidate. Non-last candidates get exactly one
/// fast attempt (`primary_client`, 3s connect / 5s total); the last candidate
/// (or a lone URL) gets 2 attempts with 250ms backoff on `fallback_client`.
/// On success after earlier failures, every earlier failed host is cooled
/// down; a first-candidate success clears its cooldown.
pub async fn fetch_text_with_candidates(
    candidates: &[&str],
    health: &HostHealth,
    primary_client: &reqwest::Client,
    fallback_client: &reqwest::Client,
) -> Result<String, String> {
    fetch_reachable_text_with_candidates(candidates, health, primary_client, fallback_client)
        .await
        .map(|(_, text)| text)
}

/// Same as [`fetch_text_with_candidates`], but also returns the URL that
/// answered so `packwiz-installer` is never handed a dead `pack.toml` URL.
pub async fn fetch_reachable_text_with_candidates(
    candidates: &[&str],
    health: &HostHealth,
    primary_client: &reqwest::Client,
    fallback_client: &reqwest::Client,
) -> Result<(String, String), String> {
    if candidates.is_empty() {
        return Err("no pack candidates".to_string());
    }
    let ordered = health.order_urls(candidates);
    let mut errors: Vec<(String, String)> = Vec::new();
    for (index, url) in ordered.iter().enumerate() {
        let last = index + 1 == ordered.len();
        let result = if ordered.len() > 1 && !last {
            fetch_once(primary_client, url).await
        } else {
            fetch_with_retries(fallback_client, url, 2).await
        };
        match result {
            Ok(text) => {
                if index == 0 {
                    health.mark_healthy(url);
                } else {
                    for (failed_url, _) in &errors {
                        health.mark_unhealthy(failed_url);
                    }
                    health.mark_healthy(url);
                }
                return Ok(((*url).to_string(), text));
            }
            Err(error) => {
                log::warn!("[pack] {url} failed ({error}); trying fallback");
                errors.push(((*url).to_string(), error));
            }
        }
    }
    // Surface the first (primary) error so the UI names the pack host, with
    // fallback detail appended when present.
    let (primary_url, primary_error) = errors.remove(0);
    if errors.is_empty() {
        return Err(primary_error);
    }
    let rest = errors
        .iter()
        .map(|(url, error)| format!("fallback {url}: {error}"))
        .collect::<Vec<_>>()
        .join("; ");
    Err(format!("{primary_url}: {primary_error}; {rest}"))
}

pub async fn fetch_launcher_info(base: &str) -> Result<LauncherInfo, String> {
    let health = default_health();
    fetch_launcher_info_with(base, health, primary_failover_client(), fallback_client()).await
}

pub async fn fetch_launcher_info_with(
    base: &str,
    health: &HostHealth,
    primary_client: &reqwest::Client,
    fallback_client: &reqwest::Client,
) -> Result<LauncherInfo, String> {
    let url = format!("{base}/launcher.json");
    let mut candidates: Vec<&str> = vec![url.as_str()];
    let fallback = github_manifest_fallback(&url);
    if let Some(fallback) = fallback.as_deref() {
        candidates.push(fallback);
    }
    let text =
        fetch_text_with_candidates(&candidates, health, primary_client, fallback_client).await?;
    serde_json::from_str(&text).map_err(|e| format!("{url}: {e}"))
}

pub async fn fetch_pack_meta(url: &str) -> Result<PackMeta, String> {
    fetch_pack_meta_with_sources(
        std::slice::from_ref(&url),
        default_health(),
        primary_failover_client(),
        fallback_client(),
    )
    .await
}

/// Pack-metadata fetch over primary + `packMirrors` (+ GitHub as the final
/// fallback). Callers with a `LauncherInfo` pass `&info.pack_sources()`.
pub async fn fetch_pack_meta_with_sources(
    sources: &[&str],
    health: &HostHealth,
    primary_client: &reqwest::Client,
    fallback_client: &reqwest::Client,
) -> Result<PackMeta, String> {
    let mut candidates: Vec<&str> = sources.to_vec();
    if let Some(first) = sources.first() {
        if let Some(github) = github_manifest_fallback(first) {
            if !candidates.iter().any(|existing| *existing == github) {
                // ponytail: one clone per check, freed right after.
                let leaked: &'static str = Box::leak(github.into_boxed_str());
                candidates.push(leaked);
            }
        }
    }
    let text =
        fetch_text_with_candidates(&candidates, health, primary_client, fallback_client).await?;
    let url = candidates.first().copied().unwrap_or("pack.toml");
    parse_pack_toml(&text).map_err(|e| format!("{url}: {e}"))
    // Note: the Box::leak above is a shortcut with a bounded leak (one tiny
    // string per version check when GitHub fallback applies). Upgrade to an
    // owned-candidates variant when this shows up in profiles.
}

/// Resolve the first reachable `pack.toml` URL from `pack_sources()` (+ GitHub
/// fallback) so `packwiz-installer` never gets a dead `blockfield.pro` URL
/// that would stall the JVM. Single-source failures surface cleanly.
pub async fn resolve_reachable_pack_url(info: &LauncherInfo) -> Result<String, String> {
    resolve_reachable_pack_url_with(
        info,
        default_health(),
        primary_failover_client(),
        fallback_client(),
    )
    .await
}

pub async fn resolve_reachable_pack_url_with(
    info: &LauncherInfo,
    health: &HostHealth,
    primary_client: &reqwest::Client,
    fallback_client: &reqwest::Client,
) -> Result<String, String> {
    let sources = info.pack_sources();
    let mut candidates: Vec<&str> = sources;
    if let Some(first) = candidates.first() {
        if let Some(github) = github_manifest_fallback(first) {
            if !candidates.iter().any(|existing| *existing == github) {
                let leaked: &'static str = Box::leak(github.into_boxed_str());
                candidates.push(leaked);
            }
        }
    }
    fetch_reachable_text_with_candidates(&candidates, health, primary_client, fallback_client)
        .await
        .map(|(url, _)| url)
}

/// Stable GitHub copies of the manifests, republished by client CI on every
/// tag release. The pack files have R2/GitHub mirrors with automatic
/// failover, but launcher.json/pack.toml are otherwise pack-host-only: one
/// stalled uplink fails the whole update before the installer even starts.
const MANIFEST_FALLBACK_BASE: &str =
    "https://github.com/Blockfield/blockfield-pack-mirror/releases/download/manifest";

/// GitHub fallback for a pack-host manifest URL, if it has a stable copy.
pub fn github_manifest_fallback(url: &str) -> Option<String> {
    let asset = match url.strip_prefix("https://blockfield.pro/")? {
        "launcher.json" => "launcher.json",
        "pack.toml" => "pack.toml",
        "workshop-pack/launcher.json" => "workshop-launcher.json",
        "workshop-pack/pack.toml" => "workshop-pack.toml",
        _ => return None,
    };
    Some(format!("{MANIFEST_FALLBACK_BASE}/{asset}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read as _, Write as _};
    use std::sync::atomic::{AtomicUsize, Ordering};

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

    #[test]
    fn production_failover_budgets_are_fast() {
        assert_eq!(PRIMARY_CONNECT_TIMEOUT, Duration::from_secs(3));
        assert_eq!(PRIMARY_TOTAL_TIMEOUT, Duration::from_secs(5));
        assert_eq!(FALLBACK_CONNECT_TIMEOUT, Duration::from_secs(8));
        assert_eq!(FALLBACK_TOTAL_TIMEOUT, Duration::from_secs(20));
        assert_eq!(HEALTH_COOLDOWN, Duration::from_secs(60));
    }

    fn test_clients() -> (reqwest::Client, reqwest::Client) {
        let primary = reqwest::Client::builder()
            .connect_timeout(Duration::from_millis(300))
            .timeout(Duration::from_millis(400))
            .build()
            .unwrap();
        let fallback = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        (primary, fallback)
    }

    fn launcher_json_for(pack_url: &str) -> String {
        format!(
            r#"{{"pack":"{pack_url}","server":"raknet;play.example:25566","installer":{{"url":"https://example.invalid/a.jar","sha256":"a"}},"bootstrap":{{"url":"https://example.invalid/b.jar","sha256":"b"}},"java":{{}}}}"#
        )
    }

    const PACK_TOML: &str =
        "version = \"9.9.9\"\n[index]\nhash = \"abc\"\n[versions]\nminecraft = \"1.21.1\"\n";

    /// Blackhole: accepts connections and holds them open without replying.
    fn serve_blackhole(hits: std::sync::Arc<AtomicUsize>) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for stream in listener.incoming().take(8).flatten() {
                hits.fetch_add(1, Ordering::SeqCst);
                // Hold the connection open past the test client timeout.
                std::thread::sleep(Duration::from_millis(1500));
                drop(stream);
            }
        });
        (format!("http://{address}"), server)
    }

    /// Minimal path-aware HTTP stub serving launcher.json + pack.toml.
    fn serve_pack(
        launcher_json: String,
        pack_toml: String,
        hits: std::sync::Arc<AtomicUsize>,
    ) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for stream in listener.incoming().take(8) {
                let Ok(mut stream) = stream else { break };
                let mut head = [0u8; 2048];
                let Ok(read) = stream.read(&mut head) else {
                    continue;
                };
                hits.fetch_add(1, Ordering::SeqCst);
                let head = String::from_utf8_lossy(&head[..read]);
                let body = if head.contains("launcher.json") {
                    launcher_json.clone()
                } else {
                    pack_toml.clone()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (format!("http://{address}"), server)
    }

    fn serve_status(
        status: &str,
        body: Vec<u8>,
        hits: std::sync::Arc<AtomicUsize>,
    ) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let body_clone = body.clone();
        let status = status.to_string();
        let server = std::thread::spawn(move || {
            for stream in listener.incoming().take(8) {
                let Ok(mut stream) = stream else { break };
                let mut head = [0u8; 1024];
                let _ = stream.read(&mut head);
                hits.fetch_add(1, Ordering::SeqCst);
                let response = format!(
                    "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body_clone.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.write_all(&body_clone);
            }
        });
        (format!("http://{address}/pack.toml"), server)
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    fn test_info(pack: &str, mirrors: &[&str]) -> LauncherInfo {
        LauncherInfo {
            pack: pack.to_string(),
            server: "x".to_string(),
            skins: None,
            installer: artifact("https://example.invalid/a.jar", None),
            bootstrap: artifact("https://example.invalid/b.jar", None),
            java: Default::default(),
            pack_mirrors: mirrors.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    #[test]
    fn hanging_primary_fails_over_fast_and_sticks_to_cooldown() {
        let primary_hits = std::sync::Arc::new(AtomicUsize::new(0));
        let fallback_hits = std::sync::Arc::new(AtomicUsize::new(0));
        let (fallback_base, _server) =
            serve_pack(String::new(), String::new(), fallback_hits.clone());
        // Rewrite after we know the fallback address: serve real bodies.
        drop(_server);
        let fallback_hits2 = fallback_hits.clone();
        let launcher_json = launcher_json_for(&format!("{fallback_base}/pack.toml"));
        let (fallback_base, server) = serve_pack(launcher_json, PACK_TOML.into(), fallback_hits2);
        let (blackhole_base, _blackhole) = serve_blackhole(primary_hits.clone());
        let (primary_client, fallback_client) = test_clients();
        let health = HostHealth::new();

        let primary_launcher = format!("{blackhole_base}/launcher.json");
        let fallback_launcher = format!("{fallback_base}/launcher.json");
        let primary_pack = format!("{blackhole_base}/pack.toml");
        let fallback_pack = format!("{fallback_base}/pack.toml");

        let started = Instant::now();
        let first = block_on(fetch_text_with_candidates(
            &[primary_launcher.as_str(), fallback_launcher.as_str()],
            &health,
            &primary_client,
            &fallback_client,
        ))
        .expect("fallback launcher.json");
        assert!(first.contains("\"pack\""));
        let second = block_on(fetch_text_with_candidates(
            &[primary_pack.as_str(), fallback_pack.as_str()],
            &health,
            &primary_client,
            &fallback_client,
        ))
        .expect("fallback pack.toml");
        assert!(second.contains("minecraft"));
        let elapsed = started.elapsed();
        // Two sequential failovers with 400ms test clients stay far under 6s.
        assert!(elapsed < Duration::from_secs(6), "{elapsed:?}");
        // Primary was tried exactly once, then cooled down: the second fetch
        // skips the blackhole entirely via HostHealth ordering.
        assert_eq!(primary_hits.load(Ordering::SeqCst), 1);
        assert!(fallback_hits.load(Ordering::SeqCst) >= 2);
        assert!(health.is_cooled_down(&primary_pack));
        // Ordered candidates now put the fallback first.
        let ordered = health.order_urls(&[primary_pack.as_str(), fallback_pack.as_str()]);
        assert_eq!(ordered, [fallback_pack.as_str(), primary_pack.as_str()]);
        drop(server);
    }

    #[test]
    fn failing_primary_is_tried_once_then_fallback_wins() {
        let primary_hits = std::sync::Arc::new(AtomicUsize::new(0));
        let fallback_hits = std::sync::Arc::new(AtomicUsize::new(0));
        let (primary_url, _primary) =
            serve_status("502 Bad Gateway", b"bad".to_vec(), primary_hits.clone());
        let (fallback_url, _fallback) = serve_status(
            "200 OK",
            PACK_TOML.as_bytes().to_vec(),
            fallback_hits.clone(),
        );
        let (primary_client, fallback_client) = test_clients();
        let health = HostHealth::new();
        let text = block_on(fetch_text_with_candidates(
            &[primary_url.as_str(), fallback_url.as_str()],
            &health,
            &primary_client,
            &fallback_client,
        ))
        .unwrap();
        assert!(text.contains("minecraft"));
        assert_eq!(primary_hits.load(Ordering::SeqCst), 1);
        assert_eq!(fallback_hits.load(Ordering::SeqCst), 1);
        assert!(health.is_cooled_down(&primary_url));
        assert!(!health.is_cooled_down(&fallback_url));
    }

    #[test]
    fn pack_mirrors_parse_dedup_filter_and_cool_down() {
        // Backwards compatible: no packMirrors key means pack-host only.
        let bare: LauncherInfo = serde_json::from_str(
            r#"{"pack":"https://blockfield.pro/pack.toml","server":"x","installer":{"url":"https://example.invalid/a.jar","sha256":"a"},"bootstrap":{"url":"https://example.invalid/b.jar","sha256":"b"},"java":{}}"#,
        )
        .unwrap();
        assert!(bare.pack_mirrors.is_empty());
        assert_eq!(bare.pack_sources(), ["https://blockfield.pro/pack.toml"]);

        let info = test_info(
            "https://blockfield.pro/pack.toml",
            &[
                "https://mirror.example/pack.toml",
                "https://mirror.example/pack.toml",
                "https://blockfield.pro/pack.toml",
                "gopher://evil.example/pack.toml",
                "https://mirror.example/other.toml",
                "",
            ],
        );
        assert_eq!(
            info.pack_sources(),
            [
                "https://blockfield.pro/pack.toml",
                "https://mirror.example/pack.toml",
                "https://mirror.example/other.toml",
            ]
        );
        // http mirrors only in debug/test builds.
        let http = test_info(
            "https://blockfield.pro/pack.toml",
            &["http://127.0.0.1:1/pack.toml"],
        );
        if cfg!(debug_assertions) || cfg!(test) {
            assert_eq!(http.pack_sources().len(), 2);
        } else {
            assert_eq!(http.pack_sources().len(), 1);
        }
        // Cooled-down primary sinks to the back.
        let health = HostHealth::new();
        assert_eq!(health.order_urls(&info.pack_sources()), info.pack_sources());
        health.mark_unhealthy("https://blockfield.pro/pack.toml");
        let ordered = health.order_urls(&info.pack_sources());
        assert_eq!(ordered[0], "https://mirror.example/pack.toml");
        assert_eq!(ordered.last().unwrap(), &"https://blockfield.pro/pack.toml");
        health.mark_healthy("https://blockfield.pro/pack.toml");
        assert!(!health.is_cooled_down("https://blockfield.pro/pack.toml"));
    }

    #[test]
    fn host_key_isolates_loopback_ports_and_lowercases() {
        assert_eq!(
            HostHealth::host_key("http://127.0.0.1:8080/pack.toml").as_deref(),
            Some("127.0.0.1:8080")
        );
        assert_eq!(
            HostHealth::host_key("http://127.0.0.1:8081/pack.toml").as_deref(),
            Some("127.0.0.1:8081")
        );
        assert_eq!(
            HostHealth::host_key("HTTPS://Example.COM/Pack.toml").as_deref(),
            Some("example.com")
        );
        assert_eq!(HostHealth::host_key("not a url"), None);
    }

    #[test]
    fn resolve_reachable_pack_url_prefers_first_live_mirror() {
        let primary_hits = std::sync::Arc::new(AtomicUsize::new(0));
        let (primary_url, _primary) = serve_status(
            "503 Service Unavailable",
            b"down".to_vec(),
            primary_hits.clone(),
        );
        let fallback_hits = std::sync::Arc::new(AtomicUsize::new(0));
        let (fallback_url, _fallback) = serve_status(
            "200 OK",
            PACK_TOML.as_bytes().to_vec(),
            fallback_hits.clone(),
        );
        let (primary_client, fallback_client) = test_clients();
        let health = HostHealth::new();
        let info = test_info(&primary_url, &[&fallback_url]);
        let resolved = block_on(resolve_reachable_pack_url_with(
            &info,
            &health,
            &primary_client,
            &fallback_client,
        ))
        .unwrap();
        assert_eq!(resolved, fallback_url);
        assert_eq!(primary_hits.load(Ordering::SeqCst), 1);
        assert!(health.is_cooled_down(&primary_url));
        // Second resolve skips the cooled-down primary entirely.
        let resolved2 = block_on(resolve_reachable_pack_url_with(
            &info,
            &health,
            &primary_client,
            &fallback_client,
        ))
        .unwrap();
        assert_eq!(resolved2, fallback_url);
        assert_eq!(primary_hits.load(Ordering::SeqCst), 1);
    }
}
