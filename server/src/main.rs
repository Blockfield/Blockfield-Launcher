use axum::{
    body::{Body, Bytes},
    extract::{Path as AxumPath, State},
    http::{header, Extensions, HeaderMap, HeaderValue, Method, StatusCode, Version},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use blockfield_shared::{ForgeInfo, JavaInfo, ManifestFileEntry, ModpackManifest};
use futures_util::StreamExt;
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::Digest;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    sync::{Mutex, RwLock},
};
use tower_http::{
    compression::CompressionLayer,
    cors::{AllowOrigin, CorsLayer},
};

const DEFAULT_FORGE_VERSION: &str = "1.20.1-47.4.10";
const DEFAULT_FORGE_URL: &str =
    "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.4.10/forge-1.20.1-47.4.10-installer.jar";
const DEFAULT_FORGE_SHA256: &str =
    "1912760b4cb6b803d8a826de603c9076b1da71ec2765e9a1f8c1ca78f65278e3";
const DEFAULT_FORGE_SIZE: u64 = 6_078_070;
static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

struct AppConfig {
    base_url: String,
    files_dir: PathBuf,
    extracted_dir: PathBuf,
    modpack_version: String,
    minecraft_version: String,
    bind_host: String,
    port: u16,
    api_prefix: String,
    java_version: Option<String>,
    java_platform: Option<String>,
    java_url: Option<String>,
    java_sha256: Option<String>,
    java_size: Option<u64>,
    forge_url: Option<String>,
    forge_version: Option<String>,
    forge_sha256: Option<String>,
    forge_size: Option<u64>,
    cms_url: Option<String>,
    cms_token: Option<String>,
    reload_token: Option<String>,
    cms_required: bool,
    modpack_collection: String,
    content_collection: String,
    // GitHub Releases — launcher update source
    github_repo: Option<String>,
    github_update_tag: String,
    github_token: Option<String>,
    // Fallback JSON values (used when CMS is unavailable)
    fallback_brand: String,
    fallback_brand_subtitle: String,
    fallback_chrome_title: String,
    fallback_operation_name: String,
    fallback_server_ip: String,
    server_host: String,
    server_port: u16,
    server_region_code: String,
    server_location_name: String,
}

impl AppConfig {
    fn from_env() -> Self {
        Self {
            base_url: env_string("BASE_URL", "http://localhost:3000"),
            files_dir: env_string("FILES_DIR", "server/files").into(),
            extracted_dir: env_string("EXTRACTED_DIR", "server/extracted").into(),
            modpack_version: env_string("MODPACK_VERSION", "0.1.43"),
            minecraft_version: env_string("MINECRAFT_VERSION", "1.20.1"),
            bind_host: env_string("BIND_HOST", "0.0.0.0"),
            port: env_string("PORT", "3000").parse().unwrap_or(3000),
            api_prefix: env_string("API_PREFIX", "api/launcher/v1"),
            java_version: env_opt("JAVA_VERSION"),
            java_platform: env_opt("JAVA_PLATFORM"),
            java_url: env_opt("JAVA_URL"),
            java_sha256: env_opt("JAVA_SHA256"),
            java_size: env_u64("JAVA_SIZE"),
            forge_version: Some(env_string("FORGE_VERSION", DEFAULT_FORGE_VERSION)),
            forge_url: Some(env_string("FORGE_URL", DEFAULT_FORGE_URL)),
            forge_sha256: Some(env_string("FORGE_SHA256", DEFAULT_FORGE_SHA256)),
            forge_size: Some(env_u64("FORGE_SIZE").unwrap_or(DEFAULT_FORGE_SIZE)),
            cms_url: env_opt("CMS_URL").map(|url| url.trim_end_matches('/').to_string()),
            cms_token: env_opt("CMS_TOKEN"),
            reload_token: env_opt("RELOAD_TOKEN"),
            cms_required: env_string("CMS_REQUIRED", "false") == "true",
            modpack_collection: env_string("CMS_MODPACK_COLLECTION", "modpack_releases"),
            content_collection: env_string("CMS_CONTENT_COLLECTION", "launcher_content"),
            github_repo: env_opt("GITHUB_REPO"),
            github_update_tag: env_string("GITHUB_UPDATE_TAG", "develop"),
            github_token: env_opt("GITHUB_TOKEN"),
            fallback_brand: env_string("FALLBACK_BRAND", "BLOCKFIELD"),
            fallback_brand_subtitle: env_string("FALLBACK_BRAND_SUBTITLE", "TACTICAL OPS"),
            fallback_chrome_title: env_string("FALLBACK_CHROME_TITLE", "BLOCKFIELD LAUNCHER"),
            fallback_operation_name: env_string("FALLBACK_OPERATION_NAME", "IRON FRONT"),
            fallback_server_ip: env_string("FALLBACK_SERVER_IP", "play.blockfield.gg:25565"),
            server_host: env_string("MINECRAFT_SERVER_HOST", "play.blockfield.gg"),
            server_port: env_string("MINECRAFT_SERVER_PORT", "25565")
                .parse()
                .unwrap_or(25565),
            server_region_code: env_string("SERVER_REGION_CODE", "UNKNOWN"),
            server_location_name: env_string("SERVER_LOCATION_NAME", "Unknown"),
        }
    }
}

struct AppState {
    config: AppConfig,
    client: Client,
    manifest: RwLock<Option<CachedManifest>>,
    reload_lock: Mutex<()>,
    release_lock: RwLock<()>,
    server_status: Mutex<Option<(std::time::Instant, Value)>>,
    content_cache: Mutex<Option<(std::time::Instant, Value)>>,
}

struct CachedManifest {
    manifest: ModpackManifest,
}

#[derive(Clone)]
struct ReleaseMetadata {
    version: String,
    minecraft_version: String,
    prune: Option<Vec<String>>,
    java: Option<JavaInfo>,
    forge: Option<ForgeInfo>,
    source: Option<ReleaseSource>,
    archive_size: Option<u64>,
    archive_sha256: Option<String>,
    from_cms: bool,
}

#[derive(Clone)]
enum ReleaseSource {
    CmsAsset(String),
    Url(String),
}

#[derive(Deserialize)]
struct CmsList<T> {
    data: Vec<T>,
}

#[derive(Clone, Deserialize)]
#[serde(untagged)]
enum CmsFileField {
    Id(String),
    Object { id: String },
}

impl CmsFileField {
    fn id(&self) -> &str {
        match self {
            Self::Id(id) => id,
            Self::Object { id } => id,
        }
    }
}

#[derive(Deserialize)]
struct CmsRelease {
    version: Option<String>,
    #[serde(alias = "minecraftVersion")]
    minecraft_version: Option<String>,
    prune: Option<Value>,
    java: Option<JavaInfo>,
    forge: Option<ForgeInfo>,
    #[serde(alias = "javaVersion")]
    java_version: Option<String>,
    #[serde(alias = "javaPlatform")]
    java_platform: Option<String>,
    #[serde(alias = "javaUrl")]
    java_url: Option<String>,
    #[serde(alias = "javaSha256")]
    java_sha256: Option<String>,
    #[serde(alias = "javaSize")]
    java_size: Option<u64>,
    #[serde(alias = "forgeVersion")]
    forge_version: Option<String>,
    #[serde(alias = "forgeUrl")]
    forge_url: Option<String>,
    #[serde(alias = "forgeSha256")]
    forge_sha256: Option<String>,
    #[serde(alias = "forgeSize")]
    forge_size: Option<u64>,
    modpack_zip: Option<CmsFileField>,
    build_zip: Option<CmsFileField>,
    build_file: Option<CmsFileField>,
    zip_file: Option<CmsFileField>,
    file: Option<CmsFileField>,
    zip_url: Option<String>,
    build_url: Option<String>,
    archive_size: Option<u64>,
    archive_sha256: Option<String>,
}

impl CmsRelease {
    fn into_metadata(self, config: &AppConfig) -> ReleaseMetadata {
        let java = self.java.or_else(|| {
            Some(JavaInfo {
                version: self.java_version?,
                platform: self.java_platform?,
                url: self.java_url?,
                sha256: self.java_sha256?,
                size: self.java_size?,
            })
        });
        let forge = self.forge.or_else(|| {
            Some(ForgeInfo {
                version: self.forge_version?,
                url: self.forge_url?,
                sha256: self.forge_sha256?,
                size: self.forge_size?,
            })
        });
        let file = self
            .modpack_zip
            .or(self.build_zip)
            .or(self.build_file)
            .or(self.zip_file)
            .or(self.file);
        let source = file
            .map(|file| ReleaseSource::CmsAsset(file.id().to_string()))
            .or_else(|| self.zip_url.or(self.build_url).map(ReleaseSource::Url));

        ReleaseMetadata {
            version: self
                .version
                .unwrap_or_else(|| config.modpack_version.clone()),
            minecraft_version: self
                .minecraft_version
                .unwrap_or_else(|| config.minecraft_version.clone()),
            prune: parse_prune(self.prune),
            java: java.or_else(|| java_from_env(config)),
            forge: forge.or_else(|| forge_from_env(config)),
            source,
            archive_size: self.archive_size,
            archive_sha256: self.archive_sha256,
            from_cms: true,
        }
    }
}

async fn regenerate_from(
    config: &AppConfig,
    client: &Client,
    release_id: Option<u64>,
) -> Result<CachedManifest, String> {
    let release = fetch_release(config, client, release_id).await?;
    validate_release(&release)?;
    let staging = staging_dir(config, &release.version);
    prepare_payload(config, client, &release, &staging).await?;
    let manifest = build_manifest(config, &release, &staging)?;
    persist_manifest(&staging, &manifest)?;
    activate_payload(&staging, &config.extracted_dir)?;
    cleanup_release_storage(config);
    Ok(CachedManifest { manifest })
}

const PERSISTED_MANIFEST: &str = ".blockfield-manifest.json";

fn persist_manifest(release_dir: &Path, manifest: &ModpackManifest) -> Result<(), String> {
    let bytes = serde_json::to_vec(manifest)
        .map_err(|e| format!("Failed to serialize active manifest: {e}"))?;
    std::fs::write(release_dir.join(PERSISTED_MANIFEST), bytes)
        .map_err(|e| format!("Failed to persist active manifest: {e}"))
}

fn load_persisted_manifest(config: &AppConfig) -> Result<CachedManifest, String> {
    let path = config.extracted_dir.join(PERSISTED_MANIFEST);
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("Failed to read persisted manifest {}: {e}", path.display()))?;
    let manifest: ModpackManifest = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Failed to parse persisted manifest: {e}"))?;
    for file in &manifest.files {
        let relative = safe_zip_entry_path(&file.path)?;
        let path = config.extracted_dir.join(relative);
        if !path.is_file() {
            return Err(format!("Persisted manifest file is missing: {}", file.path));
        }
    }
    Ok(CachedManifest { manifest })
}

async fn fetch_release(
    config: &AppConfig,
    client: &Client,
    release_id: Option<u64>,
) -> Result<ReleaseMetadata, String> {
    match fetch_cms_release(config, client, release_id).await {
        Ok(Some(release)) => Ok(release),
        Ok(None) => Ok(fallback_release(config)),
        Err(e) if config.cms_required => Err(e),
        Err(e) => {
            log_event("warn", "cms.release_fallback", json!({ "error": e }));
            Ok(fallback_release(config))
        }
    }
}

async fn fetch_cms_release(
    config: &AppConfig,
    client: &Client,
    release_id: Option<u64>,
) -> Result<Option<ReleaseMetadata>, String> {
    if config.cms_url.is_none() {
        return Ok(None);
    }

    // ponytail: fields=* only — explicit field refs would error if not yet created
    let selector = release_id.map_or_else(|| "active=1".to_string(), |id| format!("id={id}"));
    let query = format!(
        "items/{}?{selector}&sort=-id&limit=1&fields=*",
        config.modpack_collection
    );
    let response: CmsList<CmsRelease> = cms_get_json(config, client, &query).await?;
    Ok(response
        .data
        .into_iter()
        .next()
        .map(|item| item.into_metadata(config)))
}

fn fallback_release(config: &AppConfig) -> ReleaseMetadata {
    ReleaseMetadata {
        version: config.modpack_version.clone(),
        minecraft_version: config.minecraft_version.clone(),
        prune: None,
        java: java_from_env(config),
        forge: forge_from_env(config),
        source: None,
        archive_size: None,
        archive_sha256: None,
        from_cms: false,
    }
}

fn java_from_env(config: &AppConfig) -> Option<JavaInfo> {
    Some(JavaInfo {
        version: config.java_version.clone()?,
        platform: config.java_platform.clone()?,
        url: config.java_url.clone()?,
        sha256: config.java_sha256.clone()?,
        size: config.java_size?,
    })
}

fn forge_from_env(config: &AppConfig) -> Option<ForgeInfo> {
    Some(ForgeInfo {
        version: config.forge_version.clone()?,
        url: config.forge_url.clone()?,
        sha256: config.forge_sha256.clone()?,
        size: config.forge_size?,
    })
}

async fn prepare_payload(
    config: &AppConfig,
    client: &Client,
    release: &ReleaseMetadata,
    staging: &Path,
) -> Result<(), String> {
    reset_dir(staging)?;
    match &release.source {
        Some(ReleaseSource::CmsAsset(id)) => {
            let url = cms_asset_url(config, id)?;
            let zip_path = cms_cache_path(config, &release.version);
            download_file(
                client,
                &url,
                config.cms_token.as_deref(),
                &zip_path,
                release.archive_size,
                release.archive_sha256.as_deref(),
            )
            .await?;
            extract_zip_archive(&zip_path, staging)
        }
        Some(ReleaseSource::Url(url)) => {
            let zip_path = cms_cache_path(config, &release.version);
            download_file(
                client,
                url,
                None,
                &zip_path,
                release.archive_size,
                release.archive_sha256.as_deref(),
            )
            .await?;
            extract_zip_archive(&zip_path, staging)
        }
        None => extract_zips(&config.files_dir, staging),
    }
}

fn validate_release(release: &ReleaseMetadata) -> Result<(), String> {
    if release.version.trim().is_empty() || release.minecraft_version.trim().is_empty() {
        return Err("Release version and Minecraft version are required".to_string());
    }
    if !release
        .version
        .split('.')
        .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err("Release version must use numeric dot-separated components".to_string());
    }
    if release.from_cms && release.source.is_none() {
        return Err("Release archive source is required".to_string());
    }
    if release.from_cms
        && (release.archive_size.unwrap_or(0) == 0
            || !matches!(release.archive_sha256.as_ref(), Some(hash)
                if hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())))
    {
        return Err("Release archive requires a size and valid SHA-256".to_string());
    }
    if let Some(ReleaseSource::Url(url)) = &release.source {
        if !url.starts_with("https://") && !url.starts_with("http://") {
            return Err("Release URL must use HTTP or HTTPS".to_string());
        }
    }

    let java = release
        .java
        .as_ref()
        .ok_or_else(|| "Release must define an immutable Java runtime".to_string())?;
    if !java.url.starts_with("https://") || java.url.contains("/latest/") {
        return Err("Java URL must be an immutable HTTPS artifact URL".to_string());
    }
    if java.size == 0
        || java.sha256.len() != 64
        || !java.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("Java runtime requires a size and a valid SHA-256 checksum".to_string());
    }
    if let Some(forge) = &release.forge {
        if !forge.url.starts_with("https://")
            || forge.size == 0
            || forge.sha256.len() != 64
            || !forge.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("Forge requires an immutable HTTPS URL, size and SHA-256".to_string());
        }
    }
    if let Some(prune) = &release.prune {
        for pattern in prune {
            let value = pattern.strip_suffix("/*").unwrap_or(pattern);
            if value.contains(['*', '?']) || safe_zip_entry_path(value).is_err() {
                return Err(format!("Unsafe prune rule rejected: {pattern}"));
            }
        }
    }
    Ok(())
}

fn staging_dir(config: &AppConfig, version: &str) -> PathBuf {
    config
        .extracted_dir
        .parent()
        .unwrap_or(Path::new("."))
        .join(format!(".staging-{}", safe_file_part(version)))
}

fn activate_payload(staging: &Path, active: &Path) -> Result<(), String> {
    let parent = active
        .parent()
        .ok_or_else(|| "Active release directory has no parent".to_string())?;
    let previous = parent.join("previous");
    if previous.exists() {
        std::fs::remove_dir_all(&previous)
            .map_err(|e| format!("Failed to clear previous release: {e}"))?;
    }
    if active.exists() {
        std::fs::rename(active, &previous)
            .map_err(|e| format!("Failed to preserve active release: {e}"))?;
    }
    if let Err(error) = std::fs::rename(staging, active) {
        if previous.exists() {
            let _ = std::fs::rename(&previous, active);
        }
        return Err(format!("Failed to activate release: {error}"));
    }
    Ok(())
}

fn cleanup_release_storage(config: &AppConfig) {
    if let Some(parent) = config.extracted_dir.parent() {
        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if name.to_string_lossy().starts_with(".staging-") {
                    let _ = std::fs::remove_dir_all(entry.path());
                }
            }
        }
    }
    let cache = config.files_dir.join(".cms-cache");
    let Ok(entries) = std::fs::read_dir(cache) else {
        return;
    };
    let mut files = entries
        .flatten()
        .filter(|entry| entry.path().is_file())
        .collect::<Vec<_>>();
    files.sort_by_key(|entry| entry.metadata().and_then(|value| value.modified()).ok());
    let remove_count = files.len().saturating_sub(3);
    for entry in files.into_iter().take(remove_count) {
        let _ = std::fs::remove_file(entry.path());
    }
}

async fn download_file(
    client: &Client,
    url: &str,
    bearer: Option<&str>,
    dest: &Path,
    expected_size: Option<u64>,
    expected_sha256: Option<&str>,
) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }

    let mut request = client.get(url);
    if let Some(token) = bearer {
        request = request.bearer_auth(token);
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Failed to download {url}: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Failed to download {url}: {e}"))?;

    let partial = dest.with_extension("part");
    let mut file = tokio::fs::File::create(&partial)
        .await
        .map_err(|e| format!("Failed to create {}: {e}", dest.display()))?;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(error) => {
                drop(file);
                let _ = tokio::fs::remove_file(&partial).await;
                return Err(format!("Failed to read {url}: {error}"));
            }
        };
        if let Err(error) = file.write_all(&chunk).await {
            drop(file);
            let _ = tokio::fs::remove_file(&partial).await;
            return Err(format!("Failed to write {}: {error}", dest.display()));
        }
    }

    file.flush()
        .await
        .map_err(|e| format!("Failed to flush {}: {e}", partial.display()))?;
    file.sync_all()
        .await
        .map_err(|e| format!("Failed to sync {}: {e}", partial.display()))?;
    drop(file);
    let actual_size = std::fs::metadata(&partial)
        .map_err(|e| format!("Failed to inspect {}: {e}", partial.display()))?
        .len();
    if expected_size.is_some_and(|size| size != actual_size) {
        let _ = std::fs::remove_file(&partial);
        return Err(format!(
            "Archive size mismatch: expected {}, got {actual_size}",
            expected_size.unwrap_or_default()
        ));
    }
    if let Some(expected) = expected_sha256 {
        let actual = sha256_file(&partial)?;
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = std::fs::remove_file(&partial);
            return Err("Archive SHA-256 mismatch".to_string());
        }
    }
    activate_file(&partial, dest)
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to open {} for hashing: {e}", path.display()))?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = std::io::Read::read(&mut file, &mut buffer)
            .map_err(|e| format!("Failed to hash {}: {e}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn activate_file(partial: &Path, destination: &Path) -> Result<(), String> {
    if !destination.exists() {
        return std::fs::rename(partial, destination)
            .map_err(|e| format!("Failed to activate {}: {e}", destination.display()));
    }
    let backup = destination.with_extension("backup");
    let _ = std::fs::remove_file(&backup);
    std::fs::rename(destination, &backup)
        .map_err(|e| format!("Failed to preserve {}: {e}", destination.display()))?;
    if let Err(error) = std::fs::rename(partial, destination) {
        let _ = std::fs::rename(&backup, destination);
        return Err(format!(
            "Failed to activate {}: {error}",
            destination.display()
        ));
    }
    let _ = std::fs::remove_file(backup);
    Ok(())
}

fn cms_cache_path(config: &AppConfig, version: &str) -> PathBuf {
    config
        .files_dir
        .join(".cms-cache")
        .join(format!("{}.zip", safe_file_part(version)))
}

fn cms_asset_url(config: &AppConfig, id: &str) -> Result<String, String> {
    let base = config
        .cms_url
        .as_ref()
        .ok_or_else(|| "CMS_URL is not configured".to_string())?;
    Ok(format!("{base}/assets/{id}"))
}

fn extract_zips(files_dir: &Path, extracted_dir: &Path) -> Result<(), String> {
    for entry in std::fs::read_dir(files_dir)
        .map_err(|e| format!("Failed to read files dir {}: {e}", files_dir.display()))?
    {
        let path = entry
            .map_err(|e| format!("Read dir entry error: {e}"))?
            .path();
        let is_zip = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"));

        if is_zip {
            extract_zip_archive(&path, extracted_dir)?;
        } else if path.is_file() {
            let name = path.file_name().unwrap_or_default();
            let dest = extracted_dir.join(name);
            std::fs::copy(&path, &dest)
                .map_err(|e| format!("Failed to copy {}: {e}", path.display()))?;
        }
    }

    Ok(())
}

fn reset_dir(dir: &Path) -> Result<(), String> {
    if !dir.exists() {
        return std::fs::create_dir_all(dir)
            .map_err(|e| format!("Failed to create {}: {e}", dir.display()));
    }

    let metadata = std::fs::symlink_metadata(dir)
        .map_err(|e| format!("Failed to inspect {}: {e}", dir.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("Refusing to clear symlink: {}", dir.display()));
    }

    if !metadata.is_dir() {
        std::fs::remove_file(dir).map_err(|e| format!("Failed to clear {}: {e}", dir.display()))?;
        return std::fs::create_dir_all(dir)
            .map_err(|e| format!("Failed to create {}: {e}", dir.display()));
    }

    for entry in
        std::fs::read_dir(dir).map_err(|e| format!("Failed to read {}: {e}", dir.display()))?
    {
        let entry = entry.map_err(|e| format!("Failed to read {} entry: {e}", dir.display()))?;
        let path = entry.path();
        if entry
            .file_type()
            .map_err(|e| format!("Failed to inspect {}: {e}", path.display()))?
            .is_dir()
        {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        }
        .map_err(|e| format!("Failed to clear {}: {e}", path.display()))?;
    }

    Ok(())
}

fn extract_zip_archive(zip_path: &Path, extracted_dir: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path)
        .map_err(|e| format!("Failed to open zip {}: {e}", zip_path.display()))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Failed to read zip: {e}"))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Zip entry {i} error: {e}"))?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(format!("Zip symlink rejected: {}", entry.name()));
        }
        if entry.is_dir() {
            continue;
        }

        let rel_path = safe_zip_entry_path(entry.name())?;
        let dest = extracted_dir.join(rel_path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
        }

        let mut out = std::fs::File::create(&dest)
            .map_err(|e| format!("Failed to create {}: {e}", dest.display()))?;
        std::io::copy(&mut entry, &mut out)
            .map_err(|e| format!("Failed to extract {}: {e}", dest.display()))?;
    }

    Ok(())
}

fn safe_zip_entry_path(name: &str) -> Result<PathBuf, String> {
    if name.is_empty() || name.starts_with(['/', '\\']) || name.contains('\\') {
        return Err(format!("Unsafe ZIP path rejected: {name}"));
    }

    let mut out = PathBuf::new();
    for part in name.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains(':') {
            return Err(format!("Unsafe ZIP path rejected: {name}"));
        }
        out.push(part);
    }
    Ok(out)
}

fn build_manifest(
    config: &AppConfig,
    release: &ReleaseMetadata,
    extracted_dir: &Path,
) -> Result<ModpackManifest, String> {
    let mut files = Vec::new();
    let mut total_size = 0;
    let base_url = config.base_url.trim_end_matches('/');

    for entry in walkdir::WalkDir::new(extracted_dir).sort_by_file_name() {
        let entry = entry.map_err(|e| format!("Failed to inspect extracted release: {e}"))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let abs_path = entry.path();
        if abs_path.file_name().and_then(|name| name.to_str()) == Some(PERSISTED_MANIFEST) {
            continue;
        }
        let rel_path = abs_path
            .strip_prefix(extracted_dir)
            .map_err(|e| format!("Strip prefix error: {e}"))?
            .to_string_lossy()
            .replace('\\', "/");

        let mut file = std::fs::File::open(abs_path)
            .map_err(|e| format!("Failed to read {}: {e}", abs_path.display()))?;
        let size = file.metadata().map_err(|e| e.to_string())?.len();
        let mut hasher = sha2::Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = std::io::Read::read(&mut file, &mut buffer)
                .map_err(|e| format!("Failed to hash {}: {e}", abs_path.display()))?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        let sha256 = format!("{:x}", hasher.finalize());
        let encoded = percent_encode_path(&rel_path);

        total_size += size;
        files.push(ManifestFileEntry {
            path: rel_path,
            size,
            sha256,
            url: format!("{base_url}/api/launcher/v1/files/{encoded}"),
        });
    }

    if files.is_empty() {
        return Err("Release archive contains no files".to_string());
    }

    log_event(
        "info",
        "release.manifest_built",
        json!({
            "version": release.version, "phase": "manifest", "files": files.len(), "bytes": total_size
        }),
    );

    Ok(ModpackManifest {
        version: release.version.clone(),
        minecraft_version: release.minecraft_version.clone(),
        files,
        total_size,
        prune: release.prune.clone(),
        java: release.java.clone(),
        forge: release.forge.clone(),
    })
}

fn percent_encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for b in path.bytes() {
        match b {
            b'/' | b'-' | b'_' | b'.' | b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

async fn serve_manifest(State(state): State<Arc<AppState>>) -> Response {
    let _release_guard = state.release_lock.read().await;
    match state.manifest.read().await.as_ref() {
        Some(cached) => json_bytes_response(&cached.manifest),
        None => json_error(StatusCode::SERVICE_UNAVAILABLE, "no active release"),
    }
}

async fn serve_file(
    State(state): State<Arc<AppState>>,
    AxumPath(path): AxumPath<String>,
    headers: HeaderMap,
) -> Response {
    let _release_guard = state.release_lock.read().await;
    let mut safe_path = PathBuf::new();
    for part in path.split('/') {
        if !part.is_empty() && part != "." && part != ".." {
            safe_path.push(part);
        }
    }

    let file_path = state.config.extracted_dir.join(&safe_path);
    let resolved = match std::fs::canonicalize(&file_path) {
        Ok(path) => path,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                format!("File not found: {}", safe_path.display()),
            )
                .into_response()
        }
    };
    let extracted_root = match std::fs::canonicalize(&state.config.extracted_dir) {
        Ok(path) => path,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Server misconfiguration: extracted dir not accessible",
            )
                .into_response()
        }
    };

    if !resolved.starts_with(extracted_root) {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }

    match tokio::fs::File::open(&resolved).await {
        Ok(mut file) => {
            let size = match file.metadata().await {
                Ok(metadata) => metadata.len(),
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            };
            let range = match parse_byte_range(&headers, size) {
                Ok(range) => range,
                Err(()) => {
                    return Response::builder()
                        .status(StatusCode::RANGE_NOT_SATISFIABLE)
                        .header(header::CONTENT_RANGE, format!("bytes */{size}"))
                        .body(Body::empty())
                        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
                }
            };
            let (status, start, end) = range
                .map(|(start, end)| (StatusCode::PARTIAL_CONTENT, start, end))
                .unwrap_or((StatusCode::OK, 0, size.saturating_sub(1)));
            if start > 0 && file.seek(std::io::SeekFrom::Start(start)).await.is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            let length = if size == 0 { 0 } else { end - start + 1 };
            let mut builder = Response::builder()
                .status(status)
                .header(header::CONTENT_TYPE, file_content_type(&resolved))
                .header(header::CONTENT_LENGTH, length)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable");
            if status == StatusCode::PARTIAL_CONTENT {
                builder =
                    builder.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{size}"));
            }
            builder
                .body(Body::from_stream(tokio_util::io::ReaderStream::new(
                    file.take(length),
                )))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        Err(_) => (
            StatusCode::NOT_FOUND,
            format!("File not found: {}", safe_path.display()),
        )
            .into_response(),
    }
}

fn parse_byte_range(headers: &HeaderMap, size: u64) -> Result<Option<(u64, u64)>, ()> {
    let Some(value) = headers.get(header::RANGE) else {
        return Ok(None);
    };
    let value = value.to_str().map_err(|_| ())?;
    let range = value.strip_prefix("bytes=").ok_or(())?;
    if range.contains(',') || size == 0 {
        return Err(());
    }
    let (start, end) = range.split_once('-').ok_or(())?;
    if start.is_empty() {
        let suffix = end.parse::<u64>().map_err(|_| ())?;
        if suffix == 0 {
            return Err(());
        }
        let start = size.saturating_sub(suffix);
        return Ok(Some((start, size - 1)));
    }
    let start = start.parse::<u64>().map_err(|_| ())?;
    if start >= size {
        return Err(());
    }
    let end = if end.is_empty() {
        size - 1
    } else {
        end.parse::<u64>().map_err(|_| ())?.min(size - 1)
    };
    if end < start {
        return Err(());
    }
    Ok(Some((start, end)))
}

fn file_content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "json" | "mcmeta" => "application/json",
        "cfg" | "properties" | "toml" | "txt" | "yaml" | "yml" => "text/plain",
        _ => "application/octet-stream",
    }
}

fn should_gzip(
    status: StatusCode,
    _version: Version,
    headers: &HeaderMap,
    _extensions: &Extensions,
) -> bool {
    status.is_success()
        && headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|content_type| {
                content_type.starts_with("application/json") || content_type.starts_with("text/")
            })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReloadRequest {
    release_id: Option<u64>,
}

async fn handle_reload(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Option<Json<ReloadRequest>>,
) -> Response {
    let request_id = next_request_id();
    let Some(expected) = state.config.reload_token.as_deref() else {
        log_event(
            "warn",
            "release.reload_rejected",
            json!({
                "request_id": request_id, "phase": "authorize", "reason": "not_configured"
            }),
        );
        return json_error(StatusCode::SERVICE_UNAVAILABLE, "reload is not configured");
    };
    if expected.len() < 32 {
        return json_error(StatusCode::SERVICE_UNAVAILABLE, "reload is not configured");
    }
    let supplied = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    if !secure_eq(expected.as_bytes(), supplied.as_bytes()) {
        log_event(
            "warn",
            "release.reload_rejected",
            json!({
                "request_id": request_id, "phase": "authorize", "reason": "unauthorized"
            }),
        );
        return json_error(StatusCode::UNAUTHORIZED, "unauthorized");
    }
    let Ok(_guard) = state.reload_lock.try_lock() else {
        log_event(
            "warn",
            "release.reload_rejected",
            json!({
                "request_id": request_id, "phase": "lock", "reason": "concurrent_publish"
            }),
        );
        return json_error(StatusCode::CONFLICT, "reload already in progress");
    };
    let _release_guard = state.release_lock.write().await;

    let release_id = body.and_then(|Json(body)| body.release_id);
    log_event(
        "info",
        "release.reload_started",
        json!({
            "request_id": request_id, "release_id": release_id, "phase": "prepare"
        }),
    );
    match regenerate_from(&state.config, &state.client, release_id).await {
        Ok(manifest) => {
            let file_count = manifest.manifest.files.len();
            let total_mb = manifest.manifest.total_size as f64 / 1_048_576.0;
            let version = manifest.manifest.version.clone();
            *state.manifest.write().await = Some(manifest);
            log_event(
                "info",
                "release.reload_succeeded",
                json!({
                    "request_id": request_id, "release_id": release_id, "version": version,
                    "phase": "active", "files": file_count
                }),
            );
            json_bytes_response(&json!({
                "status": "ok",
                "version": version,
                "files": file_count,
                "totalMb": total_mb,
            }))
        }
        Err(e) => {
            log_event(
                "error",
                "release.reload_failed",
                json!({
                    "request_id": request_id, "release_id": release_id,
                    "phase": "prepare_or_activate", "error": e
                }),
            );
            json_error(StatusCode::INTERNAL_SERVER_ERROR, &e)
        }
    }
}

async fn handle_content_reload(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let Some(expected) = state.config.reload_token.as_deref() else {
        return json_error(StatusCode::SERVICE_UNAVAILABLE, "reload is not configured");
    };
    let supplied = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    if expected.len() < 32 || !secure_eq(expected.as_bytes(), supplied.as_bytes()) {
        return json_error(StatusCode::UNAUTHORIZED, "unauthorized");
    }
    *state.content_cache.lock().await = None;
    json_bytes_response(&json!({ "status": "ok" }))
}

async fn proxy_auth_post(
    State(state): State<Arc<AppState>>,
    AxumPath(action): AxumPath<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    proxy_auth(&state, reqwest::Method::POST, &action, &headers, Some(body)).await
}

async fn proxy_auth_me(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    proxy_auth(&state, reqwest::Method::GET, "me", &headers, None).await
}

async fn proxy_auth(
    state: &AppState,
    method: reqwest::Method,
    action: &str,
    headers: &HeaderMap,
    body: Option<Bytes>,
) -> Response {
    if !matches!(action, "login" | "refresh" | "logout" | "me") {
        return json_error(StatusCode::NOT_FOUND, "not found");
    }
    let Some(base) = state.config.cms_url.as_ref() else {
        return json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "authentication service unavailable",
        );
    };
    let mut request = state
        .client
        .request(method, format!("{base}/launcher/v1/auth/{action}"));
    if let Some(value) = headers.get(header::AUTHORIZATION) {
        request = request.header(header::AUTHORIZATION, value.as_bytes());
    }
    if let Some(body) = body.filter(|body| !body.is_empty()) {
        if let Some(content_type) = headers.get(header::CONTENT_TYPE) {
            request = request.header(header::CONTENT_TYPE, content_type.as_bytes());
        }
        request = request.body(body);
    }
    match request.send().await {
        Ok(response) => {
            let status =
                StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let body = response.bytes().await.unwrap_or_default();
            (status, [(header::CONTENT_TYPE, "application/json")], body).into_response()
        }
        Err(_) => json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "authentication service unavailable",
        ),
    }
}

fn secure_eq(expected: &[u8], supplied: &[u8]) -> bool {
    expected.len() == supplied.len()
        && expected
            .iter()
            .zip(supplied)
            .fold(0, |difference, (a, b)| difference | (a ^ b))
            == 0
}

fn next_request_id() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let sequence = REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{millis}-{sequence}")
}

fn log_event(level: &str, event: &str, fields: Value) {
    println!(
        "{}",
        json!({
            "timestamp_ms": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            "level": level,
            "event": event,
            "fields": fields,
        })
    );
}

fn json_error(status: StatusCode, message: &str) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        json!({ "error": message }).to_string(),
    )
        .into_response()
}

async fn serve_update(State(state): State<Arc<AppState>>) -> Response {
    match fetch_github_update(&state.config, &state.client).await {
        Ok(Some(value)) => json_value_response(value),
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => {
            log_event(
                "warn",
                "updater.metadata_unavailable",
                json!({ "error": error }),
            );
            StatusCode::NO_CONTENT.into_response()
        }
    }
}

async fn fetch_github_update(config: &AppConfig, client: &Client) -> Result<Option<Value>, String> {
    let repo = match config.github_repo.as_ref() {
        Some(r) => r,
        None => return Ok(None),
    };
    let tag = &config.github_update_tag;

    let url = format!("https://github.com/{repo}/releases/download/{tag}/update.json");

    let mut request = client
        .get(&url)
        .header("User-Agent", "blockfield-launcher-server");

    if let Some(token) = config.github_token.as_ref() {
        request = request.header("Authorization", format!("Bearer {token}"));
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("GitHub update fetch failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        eprintln!("GitHub update returned {status}: {url}");
        return Ok(None);
    }

    response
        .json()
        .await
        .map(Some)
        .map_err(|e| format!("GitHub update parse failed: {e}"))
}

async fn serve_content(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let mut cache = state.content_cache.lock().await;
    let value = if let Some((cached_at, value)) = cache.as_ref() {
        if cached_at.elapsed() < std::time::Duration::from_secs(60) {
            Ok(value.clone())
        } else {
            fetch_content_value(&state).await
        }
    } else {
        fetch_content_value(&state).await
    };
    let value = match value {
        Ok(value) => value,
        Err(error) => return json_error(StatusCode::SERVICE_UNAVAILABLE, &error),
    };
    *cache = Some((std::time::Instant::now(), value.clone()));
    let etag = format!(
        "\"{:x}\"",
        sha2::Sha256::digest(value.to_string().as_bytes())
    );
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        == Some(&etag)
    {
        return StatusCode::NOT_MODIFIED.into_response();
    }
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json".to_string()),
            (header::ETAG, etag),
        ],
        value.to_string(),
    )
        .into_response()
}

async fn fetch_content_value(state: &AppState) -> Result<Value, String> {
    match fetch_cms_content(&state.config, &state.client).await {
        Ok(Some(value)) => Ok(value),
        Ok(None) => Ok(fallback_content_json(&state.config)),
        Err(error) if state.config.cms_required => Err(error),
        Err(e) => {
            log_event("warn", "cms.content_fallback", json!({ "error": e }));
            Ok(fallback_content_json(&state.config))
        }
    }
}

async fn serve_server_status(State(state): State<Arc<AppState>>) -> Response {
    let mut cache = state.server_status.lock().await;
    if let Some((checked, value)) = cache.as_ref() {
        if checked.elapsed() < std::time::Duration::from_secs(10) {
            return json_value_response(value.clone());
        }
    }
    let checked_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let started = std::time::Instant::now();
    let value = match minecraft_status(&state.config).await {
        Ok(status) => json!({
            "online": true,
            "playersOnline": status["players"]["online"],
            "playersMax": status["players"]["max"],
            "minecraftVersion": status["version"]["name"],
            "motd": status["description"],
            "host": state.config.server_host,
            "port": state.config.server_port,
            "regionCode": state.config.server_region_code,
            "locationName": state.config.server_location_name,
            "serverLatencyMs": started.elapsed().as_millis(),
            "checkedAt": checked_at,
        }),
        Err(_) => json!({
            "online": false,
            "playersOnline": null,
            "playersMax": null,
            "minecraftVersion": null,
            "motd": null,
            "host": state.config.server_host,
            "port": state.config.server_port,
            "regionCode": state.config.server_region_code,
            "locationName": state.config.server_location_name,
            "serverLatencyMs": null,
            "checkedAt": checked_at,
        }),
    };
    *cache = Some((std::time::Instant::now(), value.clone()));
    json_value_response(value)
}

async fn minecraft_status(config: &AppConfig) -> Result<Value, String> {
    let mut stream = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        tokio::net::TcpStream::connect((&*config.server_host, config.server_port)),
    )
    .await
    .map_err(|_| "Minecraft status timeout".to_string())?
    .map_err(|e| format!("Minecraft status connection failed: {e}"))?;

    let mut handshake = vec![0];
    write_varint(&mut handshake, -1);
    write_varint(&mut handshake, config.server_host.len() as i32);
    handshake.extend_from_slice(config.server_host.as_bytes());
    handshake.extend_from_slice(&config.server_port.to_be_bytes());
    write_varint(&mut handshake, 1);
    let mut packet = Vec::new();
    write_varint(&mut packet, handshake.len() as i32);
    packet.extend(handshake);
    packet.extend_from_slice(&[1, 0]);
    stream.write_all(&packet).await.map_err(|e| e.to_string())?;

    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let _packet_len = read_varint(&mut stream).await?;
        if read_varint(&mut stream).await? != 0 {
            return Err("Unexpected Minecraft status packet".to_string());
        }
        let length = read_varint(&mut stream).await?;
        if !(1..=1_048_576).contains(&length) {
            return Err("Invalid Minecraft status response length".to_string());
        }
        let mut json = vec![0; length as usize];
        stream
            .read_exact(&mut json)
            .await
            .map_err(|e| e.to_string())?;
        serde_json::from_slice(&json).map_err(|e| format!("Malformed Minecraft status: {e}"))
    })
    .await
    .map_err(|_| "Minecraft status timeout".to_string())?
}

fn write_varint(output: &mut Vec<u8>, value: i32) {
    let mut value = value as u32;
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
        if value == 0 {
            break;
        }
    }
}

async fn read_varint(stream: &mut tokio::net::TcpStream) -> Result<i32, String> {
    let mut result = 0u32;
    for shift in (0..35).step_by(7) {
        let byte = stream.read_u8().await.map_err(|e| e.to_string())?;
        result |= u32::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(result as i32);
        }
    }
    Err("Minecraft VarInt is too large".to_string())
}

async fn fetch_cms_content(config: &AppConfig, client: &Client) -> Result<Option<Value>, String> {
    if config.cms_url.is_none() {
        return Ok(None);
    }

    let query = format!(
        "items/{}?sort=-id&limit=1&fields=*",
        config.content_collection
    );
    let response: CmsList<Value> = cms_get_json(config, client, &query).await?;
    Ok(response.data.into_iter().next())
}

async fn cms_get_json<T: DeserializeOwned>(
    config: &AppConfig,
    client: &Client,
    path: &str,
) -> Result<T, String> {
    let base = config
        .cms_url
        .as_ref()
        .ok_or_else(|| "CMS_URL is not configured".to_string())?;
    let url = format!("{base}/{}", path.trim_start_matches('/'));
    let mut request = client.get(&url);
    if let Some(token) = config.cms_token.as_ref() {
        request = request.bearer_auth(token);
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("CMS request failed: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("CMS returned {status}: {body}"));
    }

    response
        .json()
        .await
        .map_err(|e| format!("CMS JSON parse failed: {e}"))
}

async fn health_live() -> Response {
    json_bytes_response(&json!({ "status": "live" }))
}

async fn health_ready(State(state): State<Arc<AppState>>) -> Response {
    {
        let manifest = state.manifest.read().await;
        let Some(manifest) = manifest.as_ref() else {
            return json_error(StatusCode::SERVICE_UNAVAILABLE, "manifest unavailable");
        };
        if manifest
            .manifest
            .files
            .iter()
            .any(|file| !state.config.extracted_dir.join(&file.path).is_file())
        {
            return json_error(StatusCode::SERVICE_UNAVAILABLE, "release files unavailable");
        }
    }
    let probe = state.config.extracted_dir.join(".write-probe");
    if std::fs::write(&probe, b"")
        .and_then(|_| std::fs::remove_file(&probe))
        .is_err()
    {
        return json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "release storage is not writable",
        );
    }
    if state.config.cms_required
        && fetch_cms_content(&state.config, &state.client)
            .await
            .is_err()
    {
        return json_error(StatusCode::SERVICE_UNAVAILABLE, "CMS unavailable");
    }
    json_bytes_response(&json!({ "status": "ready" }))
}

fn fallback_content_json(config: &AppConfig) -> Value {
    json!({
        "brand": config.fallback_brand,
        "brand_subtitle": config.fallback_brand_subtitle,
        "chrome_title": config.fallback_chrome_title,
        "operation_name": config.fallback_operation_name,
        "season": "/ SEASON 01",
        "description": "Large-scale tactical PvP across contested terrain.",
        "server_name": "BLOCKFIELD - PRIMARY",
        "server_ip": config.fallback_server_ip,
        "copyright": "2026 BLOCKFIELD COMMAND",
        "login_sector": "SECTOR 07 - NORTH RIDGE",
        "login_slogan": "DEPLOY. CAPTURE. DOMINATE.",
        "support_label": "SUPPORT",
        "update_description": "Synchronizing modpack assets with the primary deployment server. Do not close the launcher until the operation completes.",
        "settings_preferences": "/ LAUNCHER PREFERENCES",
        "translations": {
            "en": {},
            "ru": {},
            "uk": {}
        },
        "features": [
            { "icon": "flag", "title": "CAPTURE POINTS", "desc": "Dynamic objective control across multiple sectors." },
            { "icon": "swords", "title": "6 CLASSES", "desc": "Assault, Recon, Engineer, Medic, Support, Pilot." },
            { "icon": "truck", "title": "ARMORED VEHICLES", "desc": "Tanks, APCs, light recon and air transport." },
            { "icon": "crosshair", "title": "TACTICAL BATTLES", "desc": "Squad-based 64v64 persistent warfare." }
        ],
        "feed": [
            { "tag": "PATCH", "tone": "amber", "date": "06.07", "title": "0.1.43 - Vehicle Balance", "body": "New modpack release is available." }
        ]
    })
}

fn json_bytes_response<T: serde::Serialize>(value: &T) -> Response {
    match serde_json::to_vec(value) {
        Ok(json) => (StatusCode::OK, [("content-type", "application/json")], json).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("JSON serialization error: {e}"),
        )
            .into_response(),
    }
}

fn json_value_response(value: Value) -> Response {
    json_bytes_response(&value)
}

fn parse_prune(value: Option<Value>) -> Option<Vec<String>> {
    let paths: Vec<String> = match value? {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|item| item.as_str().map(str::trim).map(str::to_string))
            .filter(|item| !item.is_empty())
            .collect(),
        Value::String(text) => text
            .split([',', '\n'])
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    };
    (!paths.is_empty()).then_some(paths)
}

fn safe_file_part(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn env_string(key: &str, default: &str) -> String {
    env_opt(key).unwrap_or_else(|| default.to_string())
}

fn env_opt(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn env_u64(key: &str) -> Option<u64> {
    env_opt(key).and_then(|value| value.parse().ok())
}

fn cors_layer() -> CorsLayer {
    let origins = env_string(
        "CORS_ORIGINS",
        "tauri://localhost,http://tauri.localhost,http://localhost:5173",
    )
    .split(',')
    .filter_map(|origin| origin.trim().parse::<HeaderValue>().ok())
    .collect::<Vec<_>>();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
}

#[tokio::main]
async fn main() {
    let config = AppConfig::from_env();
    let client = Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .expect("valid HTTP client");

    log_event(
        "info",
        "server.starting",
        json!({
            "base_url": config.base_url,
            "files_dir": config.files_dir,
            "extracted_dir": config.extracted_dir,
            "cms_configured": config.cms_url.is_some(),
        }),
    );

    let manifest = match regenerate_from(&config, &client, None).await {
        Ok(manifest) => {
            log_event(
                "info",
                "server.release_ready",
                json!({
                    "version": manifest.manifest.version,
                    "files": manifest.manifest.files.len(),
                    "bytes": manifest.manifest.total_size,
                }),
            );
            Some(manifest)
        }
        Err(e) => match load_persisted_manifest(&config) {
            Ok(manifest) => {
                log_event(
                    "warn",
                    "server.release_restored",
                    json!({
                        "version": manifest.manifest.version, "startup_error": e
                    }),
                );
                Some(manifest)
            }
            Err(persisted_error) => {
                log_event(
                    "warn",
                    "server.release_unavailable",
                    json!({
                        "error": e, "persisted_error": persisted_error
                    }),
                );
                None
            }
        },
    };

    let prefix = config.api_prefix.clone();
    let state = Arc::new(AppState {
        config,
        client,
        manifest: RwLock::new(manifest),
        reload_lock: Mutex::new(()),
        release_lock: RwLock::new(()),
        server_status: Mutex::new(None),
        content_cache: Mutex::new(None),
    });

    let app = Router::new()
        .route("/health", get(health_ready))
        .route("/health/live", get(health_live))
        .route("/health/ready", get(health_ready))
        .route(&format!("/{prefix}/manifest.json"), get(serve_manifest))
        .route(&format!("/{prefix}/files/{{*path}}"), get(serve_file))
        .route(&format!("/{prefix}/reload"), post(handle_reload))
        .route(&format!("/{prefix}/auth/me"), get(proxy_auth_me))
        .route(&format!("/{prefix}/auth/{{action}}"), post(proxy_auth_post))
        .route(&format!("/{prefix}/update.json"), get(serve_update))
        .route(&format!("/{prefix}/content.json"), get(serve_content))
        .route(
            &format!("/{prefix}/content/reload"),
            post(handle_content_reload),
        )
        .route(
            &format!("/{prefix}/server-status"),
            get(serve_server_status),
        )
        .layer(
            CompressionLayer::new()
                .gzip(true)
                .compress_when(should_gzip),
        )
        .layer(cors_layer())
        .with_state(state.clone());

    let addr = format!("{}:{}", state.config.bind_host, state.config.port);
    log_event("info", "server.listening", json!({ "address": addr }));

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind");

    axum::serve(listener, app).await.expect("Server error");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> AppConfig {
        AppConfig {
            base_url: "http://localhost:3000".to_string(),
            files_dir: PathBuf::new(),
            extracted_dir: PathBuf::new(),
            modpack_version: "0.1.0".to_string(),
            minecraft_version: "1.20.1".to_string(),
            bind_host: "127.0.0.1".to_string(),
            port: 3000,
            api_prefix: "api/launcher/v1".to_string(),
            java_version: Some("17.0.16+8".to_string()),
            java_platform: Some("windows-x64".to_string()),
            java_url: Some("https://example.com/jre-17.0.16.zip".to_string()),
            java_sha256: Some("a".repeat(64)),
            java_size: Some(123),
            forge_version: Some(DEFAULT_FORGE_VERSION.to_string()),
            forge_url: Some(DEFAULT_FORGE_URL.to_string()),
            forge_sha256: Some(DEFAULT_FORGE_SHA256.to_string()),
            forge_size: Some(DEFAULT_FORGE_SIZE),
            cms_url: None,
            cms_token: None,
            reload_token: Some("r".repeat(32)),
            cms_required: false,
            modpack_collection: "modpack_releases".to_string(),
            content_collection: "launcher_content".to_string(),
            github_repo: None,
            github_update_tag: "develop".to_string(),
            github_token: None,
            fallback_brand: "BLOCKFIELD".to_string(),
            fallback_brand_subtitle: "TACTICAL OPS".to_string(),
            fallback_chrome_title: "BLOCKFIELD LAUNCHER".to_string(),
            fallback_operation_name: "IRON FRONT".to_string(),
            fallback_server_ip: "play.blockfield.gg:25565".to_string(),
            server_host: "localhost".to_string(),
            server_port: 25565,
            server_region_code: "TEST".to_string(),
            server_location_name: "Test".to_string(),
        }
    }

    #[test]
    fn fallback_release_includes_java_runtime() {
        let release = fallback_release(&test_config());
        let java = release.java.unwrap();

        assert_eq!(java.version, "17.0.16+8");
        assert_eq!(java.platform, "windows-x64");
        assert_eq!(java.url, "https://example.com/jre-17.0.16.zip");
        assert_eq!(java.size, 123);
    }

    #[test]
    fn cms_release_without_java_uses_default_java_runtime() {
        let release = CmsRelease {
            version: Some("0.1.0".to_string()),
            minecraft_version: Some("1.20.1".to_string()),
            prune: None,
            java: None,
            forge: None,
            java_version: None,
            java_platform: None,
            java_url: None,
            java_sha256: None,
            java_size: None,
            forge_version: None,
            forge_url: None,
            forge_sha256: None,
            forge_size: None,
            modpack_zip: None,
            build_zip: None,
            build_file: None,
            zip_file: None,
            file: None,
            zip_url: None,
            build_url: None,
            archive_size: None,
            archive_sha256: None,
        }
        .into_metadata(&test_config());

        assert_eq!(
            release.java.unwrap().url,
            "https://example.com/jre-17.0.16.zip"
        );
    }

    #[test]
    fn fallback_release_includes_forge() {
        let release = fallback_release(&test_config());
        let forge = release.forge.unwrap();

        assert_eq!(forge.version, DEFAULT_FORGE_VERSION);
        assert_eq!(forge.url, DEFAULT_FORGE_URL);
        assert_eq!(forge.sha256, DEFAULT_FORGE_SHA256);
        assert_eq!(forge.size, DEFAULT_FORGE_SIZE);
    }

    #[test]
    fn rejects_zip_path_traversal() {
        for path in [
            "../evil.jar",
            "a/../../evil.jar",
            "/etc/passwd",
            "C:/x",
            r"\\server\x",
        ] {
            assert!(safe_zip_entry_path(path).is_err());
        }
        assert!(safe_zip_entry_path("mods/ok.jar").is_ok());
    }

    #[test]
    fn rejects_mutable_or_unverified_java_runtime() {
        let mut release = fallback_release(&test_config());
        assert!(validate_release(&release).is_ok());

        release.java.as_mut().unwrap().sha256.clear();
        assert!(validate_release(&release).is_err());

        release.java = java_from_env(&test_config());
        release.java.as_mut().unwrap().url =
            "https://api.example.com/binary/latest/17/runtime.zip".to_string();
        assert!(validate_release(&release).is_err());
    }

    #[test]
    fn activation_keeps_one_previous_release() {
        let root =
            std::env::temp_dir().join(format!("blockfield-activation-{}", std::process::id()));
        let active = root.join("active");
        let staging = root.join("staging");
        std::fs::create_dir_all(&active).unwrap();
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(active.join("version"), "old").unwrap();
        std::fs::write(staging.join("version"), "new").unwrap();

        activate_payload(&staging, &active).unwrap();

        assert_eq!(
            std::fs::read_to_string(active.join("version")).unwrap(),
            "new"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("previous/version")).unwrap(),
            "old"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn corrupt_archive_and_empty_manifest_are_rejected() {
        let root = std::env::temp_dir().join(format!(
            "blockfield-invalid-release-{}-{}",
            std::process::id(),
            REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let archive = root.join("corrupt.zip");
        std::fs::write(&archive, b"not a zip").unwrap();
        assert!(extract_zip_archive(&archive, &root.join("extracted")).is_err());

        let empty = root.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let config = test_config();
        assert!(build_manifest(&config, &fallback_release(&config), &empty).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_activation_restores_previous_release() {
        let root = std::env::temp_dir().join(format!(
            "blockfield-failed-activation-{}-{}",
            std::process::id(),
            REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let active = root.join("active");
        std::fs::create_dir_all(&active).unwrap();
        std::fs::write(active.join("version"), "old").unwrap();

        assert!(activate_payload(&root.join("missing-staging"), &active).is_err());
        assert_eq!(
            std::fs::read_to_string(active.join("version")).unwrap(),
            "old"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn persisted_active_manifest_survives_restart() {
        let root = std::env::temp_dir().join(format!(
            "blockfield-persisted-release-{}-{}",
            std::process::id(),
            REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("mods")).unwrap();
        std::fs::write(root.join("mods/example.jar"), b"mod").unwrap();
        let mut config = test_config();
        config.extracted_dir = root.clone();
        let manifest = ModpackManifest {
            version: "1.2.3".to_string(),
            minecraft_version: "1.20.1".to_string(),
            files: vec![ManifestFileEntry {
                path: "mods/example.jar".to_string(),
                size: 3,
                sha256: "a".repeat(64),
                url: "https://example.com/mod.jar".to_string(),
            }],
            total_size: 3,
            prune: None,
            java: None,
            forge: None,
        };

        persist_manifest(&root, &manifest).unwrap();
        let restored = load_persisted_manifest(&config).unwrap();
        assert_eq!(restored.manifest.version, "1.2.3");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reload_token_comparison_is_exact() {
        assert!(secure_eq(b"secret", b"secret"));
        assert!(!secure_eq(b"secret", b"Secret"));
        assert!(!secure_eq(b"secret", b"secret-longer"));
    }

    #[test]
    fn minecraft_varint_encoding_matches_protocol_examples() {
        let mut bytes = Vec::new();
        write_varint(&mut bytes, 300);
        assert_eq!(bytes, [0xac, 0x02]);
        bytes.clear();
        write_varint(&mut bytes, -1);
        assert_eq!(bytes, [0xff, 0xff, 0xff, 0xff, 0x0f]);
    }

    #[test]
    fn parses_single_http_byte_ranges() {
        let mut headers = HeaderMap::new();
        headers.insert(header::RANGE, HeaderValue::from_static("bytes=10-19"));
        assert_eq!(parse_byte_range(&headers, 100), Ok(Some((10, 19))));
        headers.insert(header::RANGE, HeaderValue::from_static("bytes=90-"));
        assert_eq!(parse_byte_range(&headers, 100), Ok(Some((90, 99))));
        headers.insert(header::RANGE, HeaderValue::from_static("bytes=-10"));
        assert_eq!(parse_byte_range(&headers, 100), Ok(Some((90, 99))));
        headers.insert(header::RANGE, HeaderValue::from_static("bytes=100-"));
        assert!(parse_byte_range(&headers, 100).is_err());
    }

    #[tokio::test]
    async fn minecraft_status_parses_server_list_ping() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 128];
            let _ = stream.read(&mut request).await.unwrap();
            let json = br#"{"players":{"online":42,"max":200},"version":{"name":"1.20.1"},"description":"Blockfield"}"#;
            let mut response = vec![0];
            write_varint(&mut response, json.len() as i32);
            response.extend_from_slice(json);
            let mut packet = Vec::new();
            write_varint(&mut packet, response.len() as i32);
            packet.extend(response);
            stream.write_all(&packet).await.unwrap();
        });
        let mut config = test_config();
        config.server_host = "127.0.0.1".to_string();
        config.server_port = port;

        let status = minecraft_status(&config).await.unwrap();
        assert_eq!(status["players"]["online"], 42);
        assert_eq!(status["version"]["name"], "1.20.1");
    }

    #[tokio::test]
    async fn minecraft_status_rejects_malformed_and_reports_offline() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 128];
            let _ = stream.read(&mut request).await.unwrap();
            let json = b"not-json";
            let mut response = vec![0];
            write_varint(&mut response, json.len() as i32);
            response.extend_from_slice(json);
            let mut packet = Vec::new();
            write_varint(&mut packet, response.len() as i32);
            packet.extend(response);
            stream.write_all(&packet).await.unwrap();
        });
        let mut config = test_config();
        config.server_host = "127.0.0.1".to_string();
        config.server_port = port;
        assert!(minecraft_status(&config).await.is_err());

        let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        config.server_port = closed.local_addr().unwrap().port();
        drop(closed);
        assert!(minecraft_status(&config).await.is_err());
    }

    #[tokio::test]
    async fn minecraft_status_times_out_when_server_stalls() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(4)).await;
        });
        let mut config = test_config();
        config.server_host = "127.0.0.1".to_string();
        config.server_port = port;

        let started = std::time::Instant::now();
        assert!(minecraft_status(&config).await.is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(4));
    }

    #[tokio::test]
    async fn server_status_uses_ten_second_cache() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let requests = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let request_counter = requests.clone();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            request_counter.fetch_add(1, Ordering::SeqCst);
            let mut request = [0u8; 128];
            let _ = stream.read(&mut request).await.unwrap();
            let json = br#"{"players":{"online":1,"max":2},"version":{"name":"1.20.1"},"description":"Test"}"#;
            let mut response = vec![0];
            write_varint(&mut response, json.len() as i32);
            response.extend_from_slice(json);
            let mut packet = Vec::new();
            write_varint(&mut packet, response.len() as i32);
            packet.extend(response);
            stream.write_all(&packet).await.unwrap();
        });
        let mut config = test_config();
        config.server_host = "127.0.0.1".to_string();
        config.server_port = port;
        let state = Arc::new(AppState {
            config,
            client: Client::new(),
            manifest: RwLock::new(None),
            reload_lock: Mutex::new(()),
            release_lock: RwLock::new(()),
            server_status: Mutex::new(None),
            content_cache: Mutex::new(None),
        });

        let first = serve_server_status(State(state.clone())).await;
        let second = serve_server_status(State(state)).await;
        let first = axum::body::to_bytes(first.into_body(), usize::MAX)
            .await
            .unwrap();
        let second = axum::body::to_bytes(second.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(requests.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn manifest_waits_for_atomic_release_switch() {
        let state = Arc::new(AppState {
            config: test_config(),
            client: Client::new(),
            manifest: RwLock::new(None),
            reload_lock: Mutex::new(()),
            release_lock: RwLock::new(()),
            server_status: Mutex::new(None),
            content_cache: Mutex::new(None),
        });

        let switch = state.release_lock.write().await;
        assert!(tokio::time::timeout(
            std::time::Duration::from_millis(10),
            serve_manifest(State(state.clone()))
        )
        .await
        .is_err());
        drop(switch);

        assert_eq!(
            serve_manifest(State(state)).await.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[tokio::test]
    async fn auth_proxy_accepts_empty_logout_body() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 1024];
            let read = stream.read(&mut request).await.unwrap();
            assert!(String::from_utf8_lossy(&request[..read])
                .starts_with("POST /launcher/v1/auth/logout "));
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 15\r\nConnection: close\r\n\r\n{\"status\":\"ok\"}",
                )
                .await
                .unwrap();
        });

        let mut config = test_config();
        config.cms_url = Some(format!("http://{address}"));
        let state = Arc::new(AppState {
            config,
            client: Client::new(),
            manifest: RwLock::new(None),
            reload_lock: Mutex::new(()),
            release_lock: RwLock::new(()),
            server_status: Mutex::new(None),
            content_cache: Mutex::new(None),
        });

        assert_eq!(
            proxy_auth_post(
                State(state),
                AxumPath("logout".to_string()),
                HeaderMap::new(),
                Bytes::new()
            )
            .await
            .status(),
            StatusCode::OK
        );
    }

    #[test]
    fn encodes_url_path_bytes() {
        assert_eq!(percent_encode_path("mods/a b.jar"), "mods/a%20b.jar");
        assert_eq!(percent_encode_path("mods/[x].jar"), "mods/%5Bx%5D.jar");
    }

    #[test]
    fn parses_prune_from_text_or_json() {
        assert_eq!(
            parse_prune(Some(json!(["mods/old.jar", "config/*"]))).unwrap(),
            vec!["mods/old.jar", "config/*"]
        );
        assert_eq!(
            parse_prune(Some(json!("mods/old.jar\nconfig/*"))).unwrap(),
            vec!["mods/old.jar", "config/*"]
        );
    }

    #[test]
    fn marks_only_text_assets_as_compressible() {
        assert_eq!(
            file_content_type(Path::new("assets/index.json")),
            "application/json"
        );
        assert_eq!(
            file_content_type(Path::new("mods/big.jar")),
            "application/octet-stream"
        );
    }

    #[test]
    fn reset_dir_keeps_root() {
        let root =
            std::env::temp_dir().join(format!("blockfield-reset-dir-{}", std::process::id()));
        let nested = root.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(root.join("file.txt"), "x").unwrap();
        std::fs::write(nested.join("file.txt"), "x").unwrap();

        reset_dir(&root).unwrap();

        assert!(root.is_dir());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        std::fs::remove_dir_all(root).unwrap();
    }
}
