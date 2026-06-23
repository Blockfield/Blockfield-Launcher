use axum::{
    extract::{Path as AxumPath, State},
    http::{header, Extensions, HeaderMap, StatusCode, Version},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use blockfield_shared::{ForgeInfo, JavaInfo, ManifestFileEntry, ModpackManifest};
use futures_util::StreamExt;
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::Digest;
use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};
use tokio::{io::AsyncWriteExt, sync::RwLock};
use tower_http::{compression::CompressionLayer, cors::CorsLayer};

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
    fallback_launcher_version: String,
    fallback_update_url: String,
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
            forge_url: env_opt("FORGE_URL"),
            forge_version: env_opt("FORGE_VERSION"),
            forge_sha256: env_opt("FORGE_SHA256"),
            forge_size: env_u64("FORGE_SIZE"),
            cms_url: env_opt("CMS_URL").map(|url| url.trim_end_matches('/').to_string()),
            cms_token: env_opt("CMS_TOKEN"),
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
            fallback_launcher_version: env_string("FALLBACK_LAUNCHER_VERSION", "0.4.2"),
            fallback_update_url: env_string(
                "FALLBACK_UPDATE_URL",
                "https://play.blockfield.gg/downloads/blockfield-launcher_0.1.0_x64-setup.exe",
            ),
        }
    }
}

struct AppState {
    config: AppConfig,
    client: Client,
    manifest: RwLock<Option<CachedManifest>>,
}

#[derive(PartialEq, Eq)]
enum ManifestSourceKind {
    Cms,
    Fallback,
}

struct CachedManifest {
    manifest: ModpackManifest,
    source: ManifestSourceKind,
}

#[derive(Clone)]
struct ReleaseMetadata {
    version: String,
    minecraft_version: String,
    prune: Option<Vec<String>>,
    java: Option<JavaInfo>,
    forge: Option<ForgeInfo>,
    source: Option<ReleaseSource>,
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
            from_cms: true,
        }
    }
}

async fn regenerate_from(config: &AppConfig, client: &Client) -> Result<CachedManifest, String> {
    let release = fetch_release(config, client).await?;
    prepare_payload(config, client, &release).await?;
    let manifest = build_manifest(config, &release)?;
    Ok(CachedManifest {
        manifest,
        source: if release.from_cms {
            ManifestSourceKind::Cms
        } else {
            ManifestSourceKind::Fallback
        },
    })
}

async fn fetch_release(config: &AppConfig, client: &Client) -> Result<ReleaseMetadata, String> {
    match fetch_cms_release(config, client).await {
        Ok(Some(release)) => Ok(release),
        Ok(None) => Ok(fallback_release(config)),
        Err(e) if config.cms_required => Err(e),
        Err(e) => {
            eprintln!("CMS release fallback: {e}");
            Ok(fallback_release(config))
        }
    }
}

async fn fetch_cms_release(
    config: &AppConfig,
    client: &Client,
) -> Result<Option<ReleaseMetadata>, String> {
    if config.cms_url.is_none() {
        return Ok(None);
    }

    // ponytail: fields=* only — explicit field refs would error if not yet created
    let query = format!(
        "items/{}?sort=-id&limit=1&fields=*",
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
) -> Result<(), String> {
    match &release.source {
        Some(ReleaseSource::CmsAsset(id)) => {
            let url = cms_asset_url(config, id)?;
            let zip_path = cms_cache_path(config, &release.version);
            download_file(client, &url, config.cms_token.as_deref(), &zip_path).await?;
            reset_dir(&config.extracted_dir)?;
            extract_zip_archive(&zip_path, &config.extracted_dir)
        }
        Some(ReleaseSource::Url(url)) => {
            let zip_path = cms_cache_path(config, &release.version);
            download_file(client, url, None, &zip_path).await?;
            reset_dir(&config.extracted_dir)?;
            extract_zip_archive(&zip_path, &config.extracted_dir)
        }
        None => extract_zips(&config.files_dir, &config.extracted_dir),
    }
}

async fn download_file(
    client: &Client,
    url: &str,
    bearer: Option<&str>,
    dest: &Path,
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

    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| format!("Failed to create {}: {e}", dest.display()))?;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Failed to read {url}: {e}"))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Failed to write {}: {e}", dest.display()))?;
    }

    file.flush()
        .await
        .map_err(|e| format!("Failed to flush {}: {e}", dest.display()))
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
    reset_dir(extracted_dir)?;

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
            println!("Extracting: {}", path.display());
            extract_zip_archive(&path, extracted_dir)?;
        } else if path.is_file() {
            let name = path.file_name().unwrap_or_default();
            let dest = extracted_dir.join(name);
            std::fs::copy(&path, &dest)
                .map_err(|e| format!("Failed to copy {}: {e}", path.display()))?;
            println!("Copied: {}", name.to_string_lossy());
        }
    }

    Ok(())
}

fn reset_dir(dir: &Path) -> Result<(), String> {
    if !dir.exists() {
        return std::fs::create_dir_all(dir)
            .map_err(|e| format!("Failed to create {}: {e}", dir.display()));
    }

    if !dir.is_dir() {
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
        if entry.is_dir() {
            continue;
        }

        let Some(rel_path) = safe_zip_entry_path(entry.name()) else {
            continue;
        };
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

fn safe_zip_entry_path(name: &str) -> Option<PathBuf> {
    let normalized = name.replace('\\', "/");
    let path = Path::new(&normalized);
    if path.is_absolute() {
        return None;
    }

    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

fn build_manifest(
    config: &AppConfig,
    release: &ReleaseMetadata,
) -> Result<ModpackManifest, String> {
    let mut files = Vec::new();
    let mut total_size = 0;
    let base_url = config.base_url.trim_end_matches('/');

    for entry in walkdir::WalkDir::new(&config.extracted_dir)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let abs_path = entry.path();
        let rel_path = abs_path
            .strip_prefix(&config.extracted_dir)
            .map_err(|e| format!("Strip prefix error: {e}"))?
            .to_string_lossy()
            .replace('\\', "/");

        let data = std::fs::read(abs_path)
            .map_err(|e| format!("Failed to read {}: {e}", abs_path.display()))?;
        let size = data.len() as u64;
        let mut hasher = sha2::Sha256::new();
        hasher.update(&data);
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

    println!(
        "Manifest built: {} files, {} bytes total",
        files.len(),
        total_size
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
    let needs_regen = {
        let manifest = state.manifest.read().await;
        match manifest.as_ref() {
            None => true,
            Some(cached) => {
                state.config.cms_url.is_some() && cached.source == ManifestSourceKind::Fallback
            }
        }
    };
    if needs_regen {
        match regenerate_from(&state.config, &state.client).await {
            Ok(manifest) => *state.manifest.write().await = Some(manifest),
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Manifest generation error: {e}"),
                )
                    .into_response()
            }
        }
    }

    match state.manifest.read().await.as_ref() {
        Some(cached) => json_bytes_response(&cached.manifest),
        None => (StatusCode::INTERNAL_SERVER_ERROR, "No manifest available").into_response(),
    }
}

async fn serve_file(
    State(state): State<Arc<AppState>>,
    AxumPath(path): AxumPath<String>,
) -> Response {
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

    match tokio::fs::read(&resolved).await {
        Ok(data) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, file_content_type(&resolved))],
            data,
        )
            .into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            format!("File not found: {}", safe_path.display()),
        )
            .into_response(),
    }
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

async fn handle_reload(State(state): State<Arc<AppState>>) -> Response {
    match regenerate_from(&state.config, &state.client).await {
        Ok(manifest) => {
            let file_count = manifest.manifest.files.len();
            let total_mb = manifest.manifest.total_size as f64 / 1_048_576.0;
            *state.manifest.write().await = Some(manifest);
            (
                StatusCode::OK,
                format!("Reloaded: {file_count} files, {total_mb:.1} MB"),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Reload error: {e}"),
        )
            .into_response(),
    }
}

async fn serve_update(State(state): State<Arc<AppState>>) -> Response {
    let value = match fetch_github_update(&state.config, &state.client).await {
        Ok(Some(value)) => value,
        Ok(None) | Err(_) => fallback_update_json(&state.config),
    };
    json_value_response(value)
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

async fn serve_content(State(state): State<Arc<AppState>>) -> Response {
    let value = match fetch_cms_content(&state.config, &state.client).await {
        Ok(Some(value)) => value,
        Ok(None) => fallback_content_json(&state.config),
        Err(e) if state.config.cms_required => {
            return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response()
        }
        Err(e) => {
            eprintln!("CMS content fallback: {e}");
            fallback_content_json(&state.config)
        }
    };
    json_value_response(value)
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

async fn health() -> &'static str {
    "ok"
}

fn fallback_update_json(config: &AppConfig) -> Value {
    json!({
        "version": "0.1.0",
        "notes": "No launcher update available.",
        "pub_date": "2026-06-18T00:00:00Z",
        "platforms": {
            "windows-x86_64": {
                "url": config.fallback_update_url,
                "signature": ""
            }
        }
    })
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
        "server_region": "EU-WEST - 28ms",
        "operators": "142",
        "ping": "28",
        "region": "EU-W",
        "launcher_version": config.fallback_launcher_version,
        "coordinates": "LAT 47.3829 / LON 19.0402",
        "copyright": "2026 BLOCKFIELD COMMAND",
        "login_sector": "SECTOR 07 - NORTH RIDGE",
        "login_slogan": "DEPLOY. CAPTURE. DOMINATE.",
        "operator_handle": "KILO_7",
        "operator_initials": "K7",
        "operator_rank": "RANK - SERGEANT",
        "support_label": "SUPPORT",
        "network_status": "NETWORK NOMINAL",
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

#[tokio::main]
async fn main() {
    let config = AppConfig::from_env();
    let client = Client::new();

    println!("Blockfield API Server");
    println!("  Base URL:    {}", config.base_url);
    println!("  Files dir:   {}", config.files_dir.display());
    println!("  Extracted:   {}", config.extracted_dir.display());
    println!(
        "  CMS:         {}",
        config.cms_url.as_deref().unwrap_or("disabled")
    );

    let manifest = match regenerate_from(&config, &client).await {
        Ok(manifest) => {
            println!(
                "Startup OK: {} files, {:.1} MB",
                manifest.manifest.files.len(),
                manifest.manifest.total_size as f64 / 1_048_576.0
            );
            Some(manifest)
        }
        Err(e) => {
            eprintln!("Startup WARNING: {e} (server will retry on first request)");
            None
        }
    };

    let prefix = config.api_prefix.clone();
    let state = Arc::new(AppState {
        config,
        client,
        manifest: RwLock::new(manifest),
    });

    let app = Router::new()
        .route("/health", get(health))
        .route(&format!("/{prefix}/manifest.json"), get(serve_manifest))
        .route(&format!("/{prefix}/files/{{*path}}"), get(serve_file))
        .route(&format!("/{prefix}/reload"), post(handle_reload))
        .route(&format!("/{prefix}/update.json"), get(serve_update))
        .route(&format!("/{prefix}/content.json"), get(serve_content))
        .layer(
            CompressionLayer::new()
                .gzip(true)
                .compress_when(should_gzip),
        )
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    let addr = format!("{}:{}", state.config.bind_host, state.config.port);
    println!("Listening on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind");

    axum::serve(listener, app).await.expect("Server error");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zip_path_traversal() {
        assert!(safe_zip_entry_path("../evil.jar").is_none());
        assert!(safe_zip_entry_path("mods/ok.jar").is_some());
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
