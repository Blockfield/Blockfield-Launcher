use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use blockfield_shared::{ManifestFileEntry, ModpackManifest};
use sha2::Digest;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;

// ── Config ──────────────────────────────────────────────────────

struct AppConfig {
    /// Base URL for constructing download URLs, e.g. "https://play.blockfield.gg"
    base_url: String,
    /// Path to the directory containing input ZIPs
    files_dir: PathBuf,
    /// Path to the directory where files are extracted and served from
    extracted_dir: PathBuf,
    /// Modpack version override (reads from env or defaults)
    modpack_version: String,
    /// Minecraft version override
    minecraft_version: String,
}

impl AppConfig {
    fn from_env() -> Self {
        Self {
            base_url: std::env::var("BASE_URL")
                .unwrap_or_else(|_| "http://localhost:3000".into()),
            files_dir: std::env::var("FILES_DIR")
                .unwrap_or_else(|_| "server/files".into())
                .into(),
            extracted_dir: std::env::var("EXTRACTED_DIR")
                .unwrap_or_else(|_| "server/extracted".into())
                .into(),
            modpack_version: std::env::var("MODPACK_VERSION")
                .unwrap_or_else(|_| "0.1.43".into()),
            minecraft_version: std::env::var("MINECRAFT_VERSION")
                .unwrap_or_else(|_| "1.20.1".into()),
        }
    }
}

// ── Server state ────────────────────────────────────────────────

struct AppState {
    config: AppConfig,
    /// Cached manifest, regenerated on reload or ZIP change
    manifest: RwLock<Option<ModpackManifest>>,
}

// ── Manifest generation ─────────────────────────────────────────

/// Extract the first ZIP found in `files_dir` to `extracted_dir`.
/// Returns the list of extracted file paths (relative to extracted_dir).
fn extract_zips(files_dir: &PathBuf, extracted_dir: &PathBuf) -> Result<Vec<PathBuf>, String> {
    // Clear previous extraction
    if extracted_dir.exists() {
        std::fs::remove_dir_all(extracted_dir)
            .map_err(|e| format!("Failed to clear extracted dir: {e}"))?;
    }
    std::fs::create_dir_all(extracted_dir)
        .map_err(|e| format!("Failed to create extracted dir: {e}"))?;

    let mut extracted = Vec::new();

    // Find and extract all ZIPs in the files directory
    let entries = std::fs::read_dir(files_dir)
        .map_err(|e| format!("Failed to read files dir: {e}"))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Read dir entry error: {e}"))?;
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "zip") {
            println!("Extracting: {}", path.display());
            let file = std::fs::File::open(&path)
                .map_err(|e| format!("Failed to open zip: {e}"))?;
            let mut archive = zip::ZipArchive::new(file)
                .map_err(|e| format!("Failed to open zip archive: {e}"))?;

            for i in 0..archive.len() {
                let mut entry = archive.by_index(i)
                    .map_err(|e| format!("Zip entry {i} error: {e}"))?;
                let name = entry.name().to_string();

                // Skip directories
                if entry.is_dir() {
                    continue;
                }

                let dest = extracted_dir.join(&name);
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("Failed to create dir {}: {e}", parent.display()))?;
                }

                let mut out = std::fs::File::create(&dest)
                    .map_err(|e| format!("Failed to create file {}: {e}", dest.display()))?;
                std::io::copy(&mut entry, &mut out)
                    .map_err(|e| format!("Failed to extract {}: {e}", name))?;

                extracted.push(PathBuf::from(&name));
            }
        }
    }

    println!("Extracted {} files to {}", extracted.len(), extracted_dir.display());
    Ok(extracted)
}

/// Percent-encode characters that are unsafe in URL paths.
fn percent_encode_path(path: &str) -> String {
    path.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b == b'/' || b == b'-' || b == b'_' || b == b'.' {
                b as char
            } else {
                b as char // keep as-is; the HTTP client will encode
            }
        })
        .collect::<String>()
        .replace(' ', "%20")
        .replace('[', "%5B")
        .replace(']', "%5D")
        .replace('+', "%2B")
}

/// Build a manifest by walking the extracted directory and computing SHA256.
fn build_manifest(config: &AppConfig) -> Result<ModpackManifest, String> {
    let mut files = Vec::new();
    let mut total_size: u64 = 0;

    for entry in walkdir::WalkDir::new(&config.extracted_dir)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }

        let abs_path = entry.path();
        let rel_path = abs_path
            .strip_prefix(&config.extracted_dir)
            .map_err(|e| format!("Strip prefix error: {e}"))?
            .to_string_lossy()
            .replace('\\', "/");

        let size = abs_path.metadata().map(|m| m.len()).unwrap_or(0);
        let data = std::fs::read(abs_path)
            .map_err(|e| format!("Failed to read {}: {e}", abs_path.display()))?;
        let mut hasher = sha2::Sha256::new();
        hasher.update(&data);
        let sha256 = format!("{:x}", hasher.finalize());
        let encoded = percent_encode_path(&rel_path);
        let url = format!("{}/api/launcher/v1/files/{}", config.base_url, encoded);

        total_size += size;
        files.push(ManifestFileEntry {
            path: rel_path,
            size,
            sha256,
            url,
        });
    }

    println!("Manifest built: {} files, {} bytes total", files.len(), total_size);

    Ok(ModpackManifest {
        version: config.modpack_version.clone(),
        minecraft_version: config.minecraft_version.clone(),
        files,
        total_size,
        release_date: chrono_like_now(),
        prune: None,
    })
}

/// ISO 8601 timestamp without pulling in chrono.
fn chrono_like_now() -> String {
    use std::time::SystemTime;
    let dur = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    // Simple formatting: YYYY-MM-DDTHH:MM:SSZ
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate date from days since Unix epoch
    let (y, m, d) = civil_from_days(days_since_epoch as i64 + 719_468);
    format!("{y:04}-{m:02}-{d:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

/// Convert days since a fixed epoch to (year, month, day).
/// Based on Howard Hinnant's algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Regenerate the manifest: extract ZIPs, rebuild.
async fn regenerate(config: &AppConfig) -> Result<ModpackManifest, String> {
    extract_zips(&config.files_dir, &config.extracted_dir)?;
    build_manifest(config)
}

// ── Endpoints ───────────────────────────────────────────────────

/// GET /api/launcher/v1/manifest.json
async fn serve_manifest(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Check if we need to regenerate (ZIP changed)
    let needs_regen = {
        let cached = state.manifest.read().await;
        cached.is_none()
    };

    if needs_regen {
        match regenerate(&state.config).await {
            Ok(manifest) => {
                let mut cached = state.manifest.write().await;
                *cached = Some(manifest);
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Manifest generation error: {e}"),
                )
                    .into_response();
            }
        }
    }

    let cached = state.manifest.read().await;
    match cached.as_ref() {
        Some(manifest) => match serde_json::to_vec(manifest) {
            Ok(json) => (
                StatusCode::OK,
                [("content-type", "application/json")],
                json,
            )
                .into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Manifest serialization error: {e}"),
            )
                .into_response(),
        },
        None => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "No manifest available",
        )
            .into_response(),
    }
}

/// GET /api/launcher/v1/files/{*path}
async fn serve_file(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> impl IntoResponse {
    // Sanitize — prevent directory traversal
    let safe_path: PathBuf = path
        .split('/')
        .filter(|c| !c.is_empty() && *c != ".." && *c != ".")
        .collect();

    let file_path = state.config.extracted_dir.join(&safe_path);

    // Verify the path is actually inside extracted_dir (resolve .. and symlinks)
    let resolved = match std::fs::canonicalize(&file_path) {
        Ok(r) => r,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                format!("File not found: {}", safe_path.display()),
            )
                .into_response();
        }
    };

    let extracted_root = match std::fs::canonicalize(&state.config.extracted_dir) {
        Ok(r) => r,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Server misconfiguration: extracted dir not accessible",
            )
                .into_response();
        }
    };

    if !resolved.starts_with(&extracted_root) {
        return (
            StatusCode::FORBIDDEN,
            "Access denied",
        )
            .into_response();
    }

    match tokio::fs::read(&resolved).await {
        Ok(data) => (
            StatusCode::OK,
            [("content-type", "application/octet-stream")],
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

/// POST /api/launcher/v1/reload
/// Hot-reload: re-extract ZIP and regenerate manifest without restarting.
async fn handle_reload(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match regenerate(&state.config).await {
        Ok(manifest) => {
            let file_count = manifest.files.len();
            let total_mb = manifest.total_size as f64 / 1_048_576.0;
            let mut cached = state.manifest.write().await;
            *cached = Some(manifest);
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

/// GET /api/launcher/v1/update.json
async fn serve_update() -> impl IntoResponse {
    let update_info = serde_json::json!({
        "version": "0.0.0",
        "notes": "No launcher update available.",
        "pub_date": "2026-06-18T00:00:00Z",
        "platforms": {
            "windows-x86_64": { "url": "", "signature": "" }
        }
    });
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        serde_json::to_vec(&update_info).unwrap_or_default(),
    )
        .into_response()
}

/// Health check.
async fn health() -> &'static str {
    "ok"
}

// ── Main ────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let config = AppConfig::from_env();

    println!("Blockfield API Server");
    println!("  Base URL:    {}", config.base_url);
    println!("  Files dir:   {}", config.files_dir.display());
    println!("  Extracted:   {}", config.extracted_dir.display());
    println!("  Version:     {} (MC {})", config.modpack_version, config.minecraft_version);

    // Bootstrap: extract ZIP and build manifest on startup
    let manifest = match regenerate(&config).await {
        Ok(m) => {
            println!("Startup OK: {} files, {:.1} MB",
                m.files.len(), m.total_size as f64 / 1_048_576.0);
            Some(m)
        }
        Err(e) => {
            eprintln!("Startup WARNING: {} (server will retry on first request)", e);
            None
        }
    };

    let state = Arc::new(AppState {
        config,
        manifest: RwLock::new(manifest),
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/launcher/v1/manifest.json", get(serve_manifest))
        .route("/api/launcher/v1/files/{*path}", get(serve_file))
        .route("/api/launcher/v1/reload", post(handle_reload))
        .route("/api/launcher/v1/update.json", get(serve_update))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = "0.0.0.0:3000";
    println!("Listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind");

    axum::serve(listener, app)
        .await
        .expect("Server error");
}
