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
    /// Release builds require `https://`, non-local hosts, and trusted pack
    /// origins; debug builds also permit `http://` for local test fixtures.
    pub fn pack_sources(&self) -> Vec<&str> {
        let mut out = Vec::new();
        let primary = self.pack.trim();
        let primary_origin = if is_allowed_pack_url(primary, None) {
            out.push(primary);
            Some(primary)
        } else {
            None
        };
        for mirror in &self.pack_mirrors {
            let url = mirror.trim();
            if out.contains(&url) {
                continue;
            }
            if is_allowed_pack_url(url, primary_origin) {
                out.push(url);
            }
        }
        out
    }
}

fn is_allowed_pack_url(url: &str, primary_origin: Option<&str>) -> bool {
    is_allowed_pack_url_with(url, primary_origin, cfg!(debug_assertions))
}

/// Validate a pack or mirror URL. When `allow_http` is false (release builds),
/// enforces `https://`, rejects local/private/link-local/loopback hosts, and
/// restricts hosts to the primary origin or trusted Blockfield/GitHub origins.
pub(crate) fn is_allowed_pack_url_with(
    url: &str,
    primary_origin: Option<&str>,
    allow_http: bool,
) -> bool {
    if url.is_empty() || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let scheme_ok = match parsed.scheme() {
        "https" => true,
        "http" => allow_http,
        _ => false,
    };
    if !scheme_ok {
        return false;
    }
    if !parsed.username().is_empty() || parsed.password().is_some() || parsed.fragment().is_some() {
        return false;
    }
    if parsed.host().is_none() {
        return false;
    }
    if !allow_http {
        if is_local_host(&parsed) {
            return false;
        }
        if !is_trusted_pack_host(&parsed, primary_origin) {
            return false;
        }
    }
    true
}

fn is_trusted_pack_host(parsed: &url::Url, primary_origin: Option<&str>) -> bool {
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host == "blockfield.pro"
        || (host.ends_with(".blockfield.pro") && !host.starts_with('.'))
        || matches!(
            host.as_str(),
            "raw.githubusercontent.com"
                | "github.com"
                | "objects.githubusercontent.com"
                | "release-assets.githubusercontent.com"
        )
    {
        return true;
    }
    primary_origin
        .and_then(|origin| url::Url::parse(origin).ok())
        .and_then(|origin| {
            origin
                .host_str()
                .map(|h| h.trim_end_matches('.').to_ascii_lowercase())
        })
        .is_some_and(|primary_host| primary_host == host)
}

fn is_local_host(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Ipv4(ip)) => is_non_global_ipv4(ip.octets()),
        Some(url::Host::Ipv6(ip)) => {
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
        Some(url::Host::Domain(host)) => {
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

fn pack_redirect_policy(enforce_https: bool) -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() > 10 {
            return attempt.error("too many redirects");
        }
        if enforce_https {
            let next = attempt.url();
            if next.scheme() != "https" {
                return attempt.error("refused redirect to non-https URL");
            }
            if !next.username().is_empty() || next.password().is_some() {
                return attempt.error("refused redirect with credentials");
            }
            if is_local_host(next) {
                return attempt.error("refused redirect to local or private host");
            }
        }
        attempt.follow()
    })
}

pub fn primary_failover_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(PRIMARY_CONNECT_TIMEOUT)
            .timeout(PRIMARY_TOTAL_TIMEOUT)
            .redirect(pack_redirect_policy(!cfg!(debug_assertions)))
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
            .redirect(pack_redirect_policy(!cfg!(debug_assertions)))
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

/// Fetch the first reachable candidate. Candidates followed by another healthy
/// candidate (or already cooled-down hosts probed as a last resort) get one
/// fast attempt (`primary_client`, 3s connect / 5s total); the last healthy
/// candidate gets 2 attempts with 250ms backoff on `fallback_client`.
/// On success after earlier failures, every earlier failed host is cooled
/// down; a first-candidate success clears its cooldown.
#[cfg(test)]
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
/// answered.
#[cfg(test)]
pub async fn fetch_reachable_text_with_candidates(
    candidates: &[&str],
    health: &HostHealth,
    primary_client: &reqwest::Client,
    fallback_client: &reqwest::Client,
) -> Result<(String, String), String> {
    fetch_parsed_with_candidates(
        candidates,
        health,
        primary_client,
        fallback_client,
        |text| Ok(text.to_string()),
    )
    .await
}

/// Fetch and validate candidates in `HostHealth` order. A `200 OK` body that
/// fails `parse` (e.g. a captive portal or ISP block page) fails over to the
/// next candidate instead of marking the broken host healthy.
pub async fn fetch_parsed_with_candidates<T, F>(
    candidates: &[&str],
    health: &HostHealth,
    primary_client: &reqwest::Client,
    fallback_client: &reqwest::Client,
    parse: F,
) -> Result<(String, T), String>
where
    F: Fn(&str) -> Result<T, String>,
{
    if candidates.is_empty() {
        return Err("no pack candidates".to_string());
    }
    let ordered = health.order_urls(candidates);
    let mut errors: Vec<(String, String)> = Vec::new();
    for (index, url) in ordered.iter().enumerate() {
        let use_fast = ordered.len() > 1
            && (health.is_cooled_down(url)
                || ordered[index + 1..]
                    .iter()
                    .any(|next| !health.is_cooled_down(next)));
        let fetched = if use_fast {
            fetch_once(primary_client, url).await
        } else {
            fetch_with_retries(fallback_client, url, 2).await
        };
        let result = fetched.and_then(|text| parse(&text).map_err(|e| format!("{url}: {e}")));
        match result {
            Ok(parsed) => {
                if index == 0 {
                    health.mark_healthy(url);
                } else {
                    for (failed_url, _) in &errors {
                        health.mark_unhealthy(failed_url);
                    }
                    health.mark_healthy(url);
                }
                return Ok(((*url).to_string(), parsed));
            }
            Err(error) => {
                if index + 1 < ordered.len() {
                    log::warn!("[pack] {url} failed ({error}); trying fallback");
                } else {
                    log::warn!("[pack] {url} failed ({error})");
                }
                errors.push(((*url).to_string(), error));
            }
        }
    }
    // Surface the first (primary) error so the UI names the pack host, with
    // fallback detail appended when present.
    let (_primary_url, primary_error) = errors.remove(0);
    if errors.is_empty() {
        return Err(primary_error);
    }
    let rest = errors
        .iter()
        .map(|(_, error)| format!("fallback {error}"))
        .collect::<Vec<_>>()
        .join("; ");
    Err(format!("{primary_error}; {rest}"))
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
    let fallback = github_manifest_fallback(&url);
    let mut candidates: Vec<&str> = vec![url.as_str()];
    if let Some(fallback) = fallback.as_deref() {
        candidates.push(fallback);
    }
    fetch_parsed_with_candidates(
        &candidates,
        health,
        primary_client,
        fallback_client,
        |text| serde_json::from_str::<LauncherInfo>(text).map_err(|e| e.to_string()),
    )
    .await
    .map(|(_, info)| info)
}

/// Fetch `pack.toml` metadata for a `LauncherInfo` using the shared
/// `HostHealth` and failover clients.
pub async fn fetch_pack_meta_for(info: &LauncherInfo) -> Result<PackMeta, String> {
    let sources = info.pack_sources();
    fetch_pack_meta_with_sources(
        &sources,
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
    let github = sources
        .first()
        .and_then(|first| github_manifest_fallback(first));
    let mut candidates: Vec<&str> = sources.to_vec();
    if let Some(github) = github.as_deref() {
        if !candidates.contains(&github) {
            candidates.push(github);
        }
    }
    fetch_parsed_with_candidates(
        &candidates,
        health,
        primary_client,
        fallback_client,
        parse_pack_toml,
    )
    .await
    .map(|(_, meta)| meta)
}

/// Resolve the first reachable `pack.toml` URL from `pack_sources()` so
/// `packwiz-installer` never gets a dead pack host URL that would stall the
/// JVM. Never falls back to the flat GitHub `manifest` release (which lacks
/// `index.toml` and `mods/*.pw.toml`).
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
    fetch_parsed_with_candidates(
        &sources,
        health,
        primary_client,
        fallback_client,
        parse_pack_toml,
    )
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
        let launcher_json = launcher_json_for("https://blockfield.pro/pack.toml");
        let (fallback_base, server) =
            serve_pack(launcher_json, PACK_TOML.into(), fallback_hits.clone());
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
                "  https://r2.blockfield.pro/pack.toml  ",
                "https://r2.blockfield.pro/pack.toml",
                "https://blockfield.pro/pack.toml",
                "gopher://evil.example/pack.toml",
                "https://raw.githubusercontent.com/Blockfield/blockfield-pack-mirror/main/pack.toml",
                "",
            ],
        );
        assert_eq!(
            info.pack_sources(),
            [
                "https://blockfield.pro/pack.toml",
                "https://r2.blockfield.pro/pack.toml",
                "https://raw.githubusercontent.com/Blockfield/blockfield-pack-mirror/main/pack.toml",
            ]
        );
        // http mirrors only in debug builds (never gated on cfg!(test)).
        let http = test_info(
            "https://blockfield.pro/pack.toml",
            &["http://127.0.0.1:1/pack.toml"],
        );
        if cfg!(debug_assertions) {
            assert_eq!(http.pack_sources().len(), 2);
        } else {
            assert_eq!(http.pack_sources().len(), 1);
        }
        // Invalid primary is excluded too.
        let bad_primary = test_info(
            "gopher://evil.example/pack.toml",
            &["https://r2.blockfield.pro/pack.toml"],
        );
        assert_eq!(
            bad_primary.pack_sources(),
            ["https://r2.blockfield.pro/pack.toml"]
        );
        // Cooled-down primary sinks to the back.
        let health = HostHealth::new();
        assert_eq!(health.order_urls(&info.pack_sources()), info.pack_sources());
        health.mark_unhealthy("https://blockfield.pro/pack.toml");
        let ordered = health.order_urls(&info.pack_sources());
        assert_eq!(ordered[0], "https://r2.blockfield.pro/pack.toml");
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
        // Default ports normalize away so https://host:443 matches https://host.
        assert_eq!(
            HostHealth::host_key("https://blockfield.pro:443/pack.toml").as_deref(),
            Some("blockfield.pro")
        );
        assert_eq!(
            HostHealth::host_key("http://127.0.0.1:80/pack.toml").as_deref(),
            Some("127.0.0.1")
        );
        // IPv6 literals retain brackets with or without explicit ports.
        assert_eq!(
            HostHealth::host_key("http://[::1]:8080/pack.toml").as_deref(),
            Some("[::1]:8080")
        );
        assert_eq!(
            HostHealth::host_key("https://[2001:DB8::1]/pack.toml").as_deref(),
            Some("[2001:db8::1]")
        );
        assert_eq!(
            HostHealth::host_key("https://[2001:DB8::1]:443/pack.toml").as_deref(),
            Some("[2001:db8::1]")
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

    #[test]
    fn host_health_cooldown_expires_and_recovers_primary() {
        let health = HostHealth::with_cooldown(Duration::from_millis(50));
        let primary = "https://blockfield.pro/pack.toml";
        let fallback = "https://r2.blockfield.pro/pack.toml";
        health.mark_unhealthy(primary);
        assert!(health.is_cooled_down(primary));
        assert_eq!(health.order_urls(&[primary, fallback]), [fallback, primary]);
        std::thread::sleep(Duration::from_millis(75));
        assert!(!health.is_cooled_down(primary));
        assert_eq!(health.order_urls(&[primary, fallback]), [primary, fallback]);
    }

    #[test]
    fn all_candidates_failing_leaves_primary_uncooled() {
        let primary_hits = std::sync::Arc::new(AtomicUsize::new(0));
        let fallback_hits = std::sync::Arc::new(AtomicUsize::new(0));
        let (primary_url, _primary) =
            serve_status("502 Bad Gateway", b"bad".to_vec(), primary_hits.clone());
        let (fallback_url, _fallback) = serve_status(
            "503 Service Unavailable",
            b"bad".to_vec(),
            fallback_hits.clone(),
        );
        let (primary_client, fallback_client) = test_clients();
        let health = HostHealth::new();
        let err = block_on(fetch_parsed_with_candidates(
            &[primary_url.as_str(), fallback_url.as_str()],
            &health,
            &primary_client,
            &fallback_client,
            parse_pack_toml,
        ))
        .unwrap_err();
        assert!(err.contains(&primary_url), "{err}");
        assert!(err.contains(&fallback_url), "{err}");
        assert_eq!(primary_hits.load(Ordering::SeqCst), 1);
        assert_eq!(fallback_hits.load(Ordering::SeqCst), 2);
        // Offline / all-fail must not penalize the primary host.
        assert!(!health.is_cooled_down(&primary_url));
        assert!(!health.is_cooled_down(&fallback_url));
    }

    fn serve_delayed_status(
        delay: Duration,
        status: &str,
        body: Vec<u8>,
        hits: std::sync::Arc<AtomicUsize>,
    ) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let status = status.to_string();
        let server = std::thread::spawn(move || {
            for stream in listener.incoming().take(8) {
                let Ok(mut stream) = stream else { break };
                let mut head = [0u8; 1024];
                let _ = stream.read(&mut head);
                hits.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(delay);
                let response = format!(
                    "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        (format!("http://{address}/pack.toml"), server)
    }

    #[test]
    fn cooled_primary_preserves_resilient_fallback_budget() {
        let primary_client = reqwest::Client::builder()
            .connect_timeout(Duration::from_millis(100))
            .timeout(Duration::from_millis(150))
            .build()
            .unwrap();
        let fallback_client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();

        // When primary is cooled down, the sole healthy fallback must get
        // fallback_client (3s timeout), NOT primary_client (150ms timeout).
        let primary_hits = std::sync::Arc::new(AtomicUsize::new(0));
        let fallback_hits = std::sync::Arc::new(AtomicUsize::new(0));
        let (primary_url, _primary) = serve_blackhole(primary_hits.clone());
        let primary_url = format!("{primary_url}/pack.toml");
        let (fallback_url, _fallback) = serve_delayed_status(
            Duration::from_millis(300),
            "200 OK",
            PACK_TOML.as_bytes().to_vec(),
            fallback_hits.clone(),
        );
        let health = HostHealth::new();
        health.mark_unhealthy(&primary_url);
        let (winning_url, meta) = block_on(fetch_parsed_with_candidates(
            &[primary_url.as_str(), fallback_url.as_str()],
            &health,
            &primary_client,
            &fallback_client,
            parse_pack_toml,
        ))
        .expect("healthy fallback must use resilient fallback_client");
        assert_eq!(winning_url, fallback_url);
        assert_eq!(meta.version, "9.9.9");
        assert_eq!(primary_hits.load(Ordering::SeqCst), 0);
        assert_eq!(fallback_hits.load(Ordering::SeqCst), 1);

        // And when the healthy fallback also fails, probing the cooled-down
        // primary as a last resort gets 1 fast attempt on primary_client, not
        // multi-retry fallback budgets.
        let dead_fallback_hits = std::sync::Arc::new(AtomicUsize::new(0));
        let (dead_fallback_url, _dead_fallback) = serve_status(
            "502 Bad Gateway",
            b"bad".to_vec(),
            dead_fallback_hits.clone(),
        );
        let err = block_on(fetch_parsed_with_candidates(
            &[primary_url.as_str(), dead_fallback_url.as_str()],
            &health,
            &primary_client,
            &fallback_client,
            parse_pack_toml,
        ))
        .unwrap_err();
        assert!(err.contains(&dead_fallback_url), "{err}");
        assert_eq!(dead_fallback_hits.load(Ordering::SeqCst), 2);
        assert_eq!(primary_hits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn lone_cooled_candidate_retries_with_fallback_budget() {
        let hits = std::sync::Arc::new(AtomicUsize::new(0));
        let hits_clone = hits.clone();
        let pack_toml = PACK_TOML.as_bytes().to_vec();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let _server = std::thread::spawn(move || {
            for stream in listener.incoming().take(8) {
                let Ok(mut stream) = stream else { break };
                let mut head = [0u8; 1024];
                let _ = stream.read(&mut head);
                let hit = hits_clone.fetch_add(1, Ordering::SeqCst) + 1;
                if hit == 1 {
                    let _ = stream.write_all(
                        b"HTTP/1.1 502 Bad Gateway\r\ncontent-length: 3\r\nconnection: close\r\n\r\nbad",
                    );
                } else {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                        pack_toml.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.write_all(&pack_toml);
                }
            }
        });
        let url = format!("http://{address}/pack.toml");
        let (primary_client, fallback_client) = test_clients();
        let health = HostHealth::new();
        health.mark_unhealthy(&url);
        let (winning_url, meta) = block_on(fetch_parsed_with_candidates(
            &[url.as_str()],
            &health,
            &primary_client,
            &fallback_client,
            parse_pack_toml,
        ))
        .expect("lone cooled candidate must retry and succeed");
        assert_eq!(winning_url, url);
        assert_eq!(meta.version, "9.9.9");
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn captive_portal_html_triggers_failover_without_marking_host_healthy() {
        let html = b"<html><body>Sign in to Wi-Fi</body></html>".to_vec();
        let (primary_client, fallback_client) = test_clients();

        // 1) fetch_launcher_info_with rejects 200 OK HTML and attributes the URL.
        let html_hits = std::sync::Arc::new(AtomicUsize::new(0));
        let (html_base, _html_server) = serve_pack(
            String::from_utf8(html.clone()).unwrap(),
            String::from_utf8(html.clone()).unwrap(),
            html_hits.clone(),
        );
        let err = block_on(fetch_launcher_info_with(
            &html_base,
            &HostHealth::new(),
            &primary_client,
            &fallback_client,
        ))
        .unwrap_err();
        assert!(err.contains(&format!("{html_base}/launcher.json")), "{err}");

        // 2) LauncherInfo candidate failover skips 200 OK HTML and uses valid JSON.
        let valid_hits = std::sync::Arc::new(AtomicUsize::new(0));
        let (valid_base, _valid_server) = serve_pack(
            launcher_json_for("https://blockfield.pro/pack.toml"),
            PACK_TOML.into(),
            valid_hits.clone(),
        );
        let health = HostHealth::new();
        let primary_launcher = format!("{html_base}/launcher.json");
        let fallback_launcher = format!("{valid_base}/launcher.json");
        let (winner, info) = block_on(fetch_parsed_with_candidates(
            &[primary_launcher.as_str(), fallback_launcher.as_str()],
            &health,
            &primary_client,
            &fallback_client,
            |text| serde_json::from_str::<LauncherInfo>(text).map_err(|e| e.to_string()),
        ))
        .expect("must fail over past 200 OK HTML to valid launcher.json");
        assert_eq!(winner, fallback_launcher);
        assert_eq!(info.pack, "https://blockfield.pro/pack.toml");
        assert!(health.is_cooled_down(&primary_launcher));
        assert!(!health.is_cooled_down(&fallback_launcher));

        // 3) resolve_reachable_pack_url_with skips 200 OK HTML and uses valid pack.toml.
        let pack_health = HostHealth::new();
        let primary_pack = format!("{html_base}/pack.toml");
        let fallback_pack = format!("{valid_base}/pack.toml");
        let resolved = block_on(resolve_reachable_pack_url_with(
            &test_info(&primary_pack, &[&fallback_pack]),
            &pack_health,
            &primary_client,
            &fallback_client,
        ))
        .expect("must fail over past 200 OK HTML to valid pack.toml");
        assert_eq!(resolved, fallback_pack);
        assert!(pack_health.is_cooled_down(&primary_pack));
        assert!(!pack_health.is_cooled_down(&fallback_pack));
    }

    #[test]
    fn resolve_reachable_pack_url_never_falls_back_to_flat_github_manifest() {
        // Confirm the flat manifest release fallback exists for metadata, so
        // this test meaningfully verifies resolve_reachable_pack_url excludes it.
        assert!(github_manifest_fallback("https://blockfield.pro/pack.toml").is_some());

        let dead_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let dead_addr = dead_listener.local_addr().unwrap();
        drop(dead_listener);

        let github_hits = std::sync::Arc::new(AtomicUsize::new(0));
        let github_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let github_addr = github_listener.local_addr().unwrap();
        let github_hits_clone = github_hits.clone();
        let _github = std::thread::spawn(move || {
            for stream in github_listener.incoming().take(4).flatten() {
                github_hits_clone.fetch_add(1, Ordering::SeqCst);
                drop(stream);
            }
        });

        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_millis(300))
            .timeout(Duration::from_millis(500))
            .resolve("blockfield.pro", dead_addr)
            .resolve("github.com", github_addr)
            .build()
            .unwrap();
        let health = HostHealth::new();
        let info = test_info("https://blockfield.pro/pack.toml", &[]);
        let err = block_on(resolve_reachable_pack_url_with(
            &info, &health, &client, &client,
        ))
        .unwrap_err();
        assert!(!err.contains("blockfield-pack-mirror"), "{err}");
        assert_eq!(github_hits.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn is_allowed_pack_url_enforces_release_ssrf_and_allowlist() {
        let primary = Some("https://blockfield.pro/pack.toml");
        for accepted in [
            "https://blockfield.pro/pack.toml",
            "https://r2.blockfield.pro/pack.toml",
            "https://raw.githubusercontent.com/Blockfield/blockfield-pack-mirror/main/pack.toml",
            "https://github.com/Blockfield/blockfield-pack-mirror/releases/download/manifest/pack.toml",
            "https://objects.githubusercontent.com/some-asset",
            "https://release-assets.githubusercontent.com/some-asset",
        ] {
            assert!(
                is_allowed_pack_url_with(accepted, primary, false),
                "expected allowed: {accepted}"
            );
        }
        assert!(is_allowed_pack_url_with(
            "https://cdn.example/v2/pack.toml",
            Some("https://cdn.example/pack.toml"),
            false
        ));

        for rejected in [
            "",
            "   ",
            "http://blockfield.pro/pack.toml",
            "https://127.0.0.1/pack.toml",
            "https://169.254.169.254/latest/meta-data",
            "https://10.0.0.1/pack.toml",
            "https://172.16.0.1/pack.toml",
            "https://192.168.1.1/pack.toml",
            "https://[::1]/pack.toml",
            "https://[fe80::1]/pack.toml",
            "https://[fc00::1]/pack.toml",
            "https://[::ffff:127.0.0.1]/pack.toml",
            "https://localhost/pack.toml",
            "https://printer.local/pack.toml",
            "https://intranet/pack.toml",
            "https://user:pass@blockfield.pro/pack.toml",
            "https://user@blockfield.pro/pack.toml",
            "https://:pass@blockfield.pro/pack.toml",
            "https://blockfield.pro/pack.toml#frag",
            "https://blockfield.pro/pack.toml#",
            "https://blockfield.pro/pack toml",
            "https://blockfield.pro/pack\ntoml",
            "https://evil.example/pack.toml",
            "https://evilblockfield.pro/pack.toml",
            "gopher://blockfield.pro/pack.toml",
            "file:///etc/passwd",
        ] {
            assert!(
                !is_allowed_pack_url_with(rejected, primary, false),
                "expected rejected: {rejected:?}"
            );
        }
    }

    #[test]
    fn release_redirect_policy_blocks_non_https_and_local_redirects() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let _server = std::thread::spawn(move || {
            for stream in listener.incoming().take(2) {
                let Ok(mut stream) = stream else { break };
                let mut head = [0u8; 1024];
                let _ = stream.read(&mut head);
                let response =
                    "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/secret\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";
                let _ = stream.write_all(response.as_bytes());
            }
        });
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .redirect(pack_redirect_policy(true))
            .build()
            .unwrap();
        let err = block_on(async { client.get(format!("http://{addr}/start")).send().await })
            .unwrap_err();
        assert!(err.is_redirect(), "{err:?}");
    }
}
