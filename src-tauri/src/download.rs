use crate::manifest::ModpackManifest;
use blockfield_shared::{validate_manifest, validate_manifest_redirect, validate_manifest_source};
use futures_util::StreamExt;
use reqwest::Client;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter};

/// Serializable progress payload emitted to the frontend via `download://progress`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgressPayload {
    pub file_path: String,
    pub file_index: usize,
    pub file_count: usize,
    pub bytes_downloaded: u64,
    pub file_bytes_total: u64,
    pub total_bytes_downloaded: u64,
    pub total_bytes_all: u64,
    pub speed_bytes_per_sec: u64,
}

// ponytail: fixed small fan-out; make it configurable only if the mirror starts rate-limiting.
const PARALLEL_FILE_DOWNLOADS: usize = 6;

struct DownloadJob {
    url: String,
    dest: PathBuf,
    file_path: String,
    file_size: u64,
    file_index: usize,
    file_count: usize,
    sha256: String,
    game_dir: PathBuf,
}

/// Streaming download engine with progress event emission.
#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

impl From<DownloadError> for String {
    fn from(e: DownloadError) -> Self {
        e.to_string()
    }
}

/// Streaming download engine with progress event emission.
pub struct Downloader {
    client: Client,
    manifest_client: Client,
    manifest_policy: blockfield_shared::ManifestPolicy,
    app_handle: AppHandle,
    /// Atomic cancel flag — set from any thread without locking the downloader.
    pub cancel_flag: Arc<AtomicBool>,
    /// Grand total of bytes across all download phases (set before downloads begin).
    grand_total: AtomicU64,
    /// Cumulative bytes downloaded across all phases (never resets mid-operation).
    cumulative_downloaded: AtomicU64,
}

impl Downloader {
    pub fn new(app_handle: AppHandle) -> Self {
        let manifest_policy = manifest_policy();
        let redirect_hosts = manifest_policy.trusted_download_hosts.clone();
        let allow_insecure_localhost = manifest_policy.allow_insecure_localhost;
        let manifest_redirects = reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.error("too many manifest download redirects");
            }
            let Some(initial) = attempt.previous().first() else {
                return attempt.stop();
            };
            match validate_manifest_redirect(
                attempt.url(),
                initial,
                &redirect_hosts,
                allow_insecure_localhost,
            ) {
                Ok(()) => attempt.follow(),
                Err(error) => attempt.error(error),
            }
        });
        let client_builder = || {
            Client::builder()
                .connect_timeout(std::time::Duration::from_secs(10))
                .read_timeout(std::time::Duration::from_secs(30))
                .timeout(std::time::Duration::from_secs(30 * 60))
        };
        Self {
            client: client_builder().build().expect("valid HTTP client"),
            manifest_client: client_builder()
                .redirect(manifest_redirects)
                .build()
                .expect("valid manifest HTTP client"),
            manifest_policy,
            app_handle,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            grand_total: AtomicU64::new(0),
            cumulative_downloaded: AtomicU64::new(0),
        }
    }

    /// Signal the downloader to cancel the current operation.
    /// Non-async: uses atomic flag so it works even when a download is in progress.
    #[allow(dead_code)]
    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
        log::info!("Download cancel requested");
    }

    /// Reset the cancel flag and cumulative counters before starting a new download.
    pub fn reset_cancel(&self) {
        self.cancel_flag.store(false, Ordering::SeqCst);
        self.cumulative_downloaded.store(0, Ordering::SeqCst);
        self.grand_total.store(0, Ordering::SeqCst);
    }

    pub fn cleanup_partials(game_dir: &Path) {
        fn visit(dir: &Path) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_symlink() {
                    continue;
                }
                if kind.is_dir() {
                    visit(&path);
                } else if path.to_string_lossy().ends_with(".part") {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        visit(game_dir);
    }

    /// Set the grand total of all expected bytes across all download phases.
    pub fn set_grand_total(&self, total: u64) {
        self.grand_total.store(total, Ordering::SeqCst);
    }

    /// Check whether cancellation has been requested.
    async fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::SeqCst)
    }

    /// Fetch the remote modpack manifest.
    pub async fn fetch_manifest(&self, url: &str) -> Result<ModpackManifest, DownloadError> {
        validate_manifest_source(url, self.manifest_policy.allow_insecure_localhost)
            .map_err(DownloadError::Other)?;
        log::info!("Fetching manifest from {url}");
        let response = self
            .manifest_client
            .get(url)
            .send()
            .await?
            .error_for_status()?;
        let max_bytes = self.manifest_policy.limits.max_manifest_bytes;
        if response
            .content_length()
            .is_some_and(|length| length > max_bytes as u64)
        {
            return Err(DownloadError::Other(
                "Manifest exceeds the configured size limit".to_string(),
            ));
        }
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if bytes.len().saturating_add(chunk.len()) > max_bytes {
                return Err(DownloadError::Other(
                    "Manifest exceeds the configured size limit".to_string(),
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let manifest: ModpackManifest = serde_json::from_slice(&bytes)
            .map_err(|error| DownloadError::Other(format!("Invalid manifest JSON: {error}")))?;
        validate_manifest(&manifest, url, bytes.len(), &self.manifest_policy)
            .map_err(DownloadError::Other)?;
        log::info!(
            "Manifest fetched: version={}, {} files, {} bytes total",
            manifest.version,
            manifest.files.len(),
            manifest.total_size
        );
        Ok(manifest)
    }

    /// Download changed files from the manifest into `game_dir`.
    /// Files whose SHA256 matches the installed manifest are skipped (delta update).
    /// Progress events are emitted to the frontend.
    pub async fn download_files(
        &self,
        manifest: &ModpackManifest,
        game_dir: &str,
        installed_sha256: &std::collections::HashMap<String, String>,
    ) -> Result<(), DownloadError> {
        let game_dir = PathBuf::from(game_dir);
        std::fs::create_dir_all(&game_dir)?;

        let mut total_downloaded: u64 = 0;
        let mut new_installed: std::collections::HashMap<String, String> = installed_sha256.clone();
        let mut downloads = Vec::new();

        for entry in &manifest.files {
            if self.is_cancelled().await {
                log::info!("Download cancelled by user");
                return Err(DownloadError::Other("Download cancelled".to_string()));
            }

            let dest = prepare_manifest_target(&game_dir, &entry.path)?;

            // Check existing file on disk regardless of installed manifest.
            // Handles reinstalls, manifest loss, and partial downloads.
            if dest.exists() {
                match Self::sha256_file(&dest) {
                    Ok(actual_hash) if actual_hash == entry.sha256 => {
                        log::info!("Skipping unchanged file: {}", entry.path);
                        new_installed.insert(entry.path.clone(), entry.sha256.clone());
                        continue;
                    }
                    Ok(actual_hash) => {
                        log::info!(
                            "Redownloading changed file: {} (expected {}, got {})",
                            entry.path,
                            entry.sha256,
                            actual_hash
                        );
                    }
                    Err(e) => {
                        log::info!("Redownloading unreadable file: {} ({e})", entry.path);
                    }
                }
            }

            downloads.push(DownloadJob {
                url: entry.url.clone(),
                dest,
                file_path: entry.path.clone(),
                file_size: entry.size,
                file_index: 0,
                file_count: 0,
                sha256: entry.sha256.clone(),
                game_dir: game_dir.clone(),
            });
            new_installed.insert(entry.path.clone(), entry.sha256.clone());
        }

        let file_count = downloads.len();
        for (index, job) in downloads.iter_mut().enumerate() {
            job.file_index = index + 1;
            job.file_count = file_count;
        }

        total_downloaded += self.download_jobs(downloads).await?;

        // Write the updated installed manifest
        let installed = crate::manifest::InstalledManifest {
            version: manifest.version.clone(),
            files: new_installed,
        };
        installed
            .save(&game_dir.to_string_lossy())
            .map_err(DownloadError::Other)?;

        log::info!(
            "Download complete: {} files, {} bytes",
            file_count,
            total_downloaded
        );

        // Emit a final event with speed=0 to signal completion
        let cumulative = self.cumulative_downloaded.load(Ordering::Relaxed);
        let grand = self.grand_total.load(Ordering::Relaxed);
        self.emit_progress("", file_count, file_count, 0, 0, cumulative, grand, 0);

        Ok(())
    }

    async fn download_jobs(&self, jobs: Vec<DownloadJob>) -> Result<u64, DownloadError> {
        let partials = jobs
            .iter()
            .map(|job| partial_path(&job.dest))
            .collect::<Vec<_>>();
        let mut downloads = futures_util::stream::iter(jobs)
            .map(|job| async move {
                let mut downloaded = 0;
                let partial = partial_path(&job.dest);
                reject_symlink(&partial)?;
                self.download_one_with_client(
                    &self.manifest_client,
                    &job.url,
                    &partial,
                    &job.file_path,
                    job.file_size,
                    job.file_index,
                    job.file_count,
                    &mut downloaded,
                    0,
                )
                .await?;

                reject_symlink(&partial)?;
                let actual_hash = Self::sha256_file(&partial)?;
                if actual_hash != job.sha256 {
                    let _ = std::fs::remove_file(&partial);
                    return Err(DownloadError::Other(format!(
                        "SHA256 mismatch for {}: expected {}, got {}",
                        job.file_path, job.sha256, actual_hash
                    )));
                }
                if std::fs::metadata(&partial)?.len() != job.file_size {
                    let _ = std::fs::remove_file(&partial);
                    return Err(DownloadError::Other(format!(
                        "Size mismatch for {}",
                        job.file_path
                    )));
                }
                prepare_manifest_target(&job.game_dir, &job.file_path)?;
                activate_partial(&partial, &job.dest)?;

                Ok(downloaded)
            })
            .buffer_unordered(PARALLEL_FILE_DOWNLOADS);

        let mut total_downloaded = 0;
        while let Some(result) = downloads.next().await {
            match result {
                Ok(downloaded) => total_downloaded += downloaded,
                Err(error) => {
                    for partial in &partials {
                        let _ = std::fs::remove_file(partial);
                    }
                    return Err(error);
                }
            }
        }
        Ok(total_downloaded)
    }

    /// Download a single file with streaming, emitting progress events.
    /// Progress uses the Downloader's atomic `cumulative_downloaded` and `grand_total`
    /// so the frontend sees one continuous progress bar across all download phases.
    #[allow(clippy::too_many_arguments)]
    pub async fn download_one(
        &self,
        url: &str,
        dest: &Path,
        file_path: &str,
        file_size: u64,
        file_index: usize,
        file_count: usize,
        _total_downloaded: &mut u64,
        _total_bytes_all: u64,
    ) -> Result<(), DownloadError> {
        self.download_one_with_client(
            &self.client,
            url,
            dest,
            file_path,
            file_size,
            file_index,
            file_count,
            _total_downloaded,
            _total_bytes_all,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn download_manifest_artifact(
        &self,
        url: &str,
        dest: &Path,
        file_path: &str,
        file_size: u64,
        file_index: usize,
        file_count: usize,
        total_downloaded: &mut u64,
        total_bytes_all: u64,
    ) -> Result<(), DownloadError> {
        self.download_one_with_client(
            &self.manifest_client,
            url,
            dest,
            file_path,
            file_size,
            file_index,
            file_count,
            total_downloaded,
            total_bytes_all,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn download_one_with_client(
        &self,
        client: &Client,
        url: &str,
        dest: &Path,
        file_path: &str,
        file_size: u64,
        file_index: usize,
        file_count: usize,
        _total_downloaded: &mut u64,
        _total_bytes_all: u64,
    ) -> Result<(), DownloadError> {
        log::info!("Downloading: {file_path} ({file_size} bytes)");

        // Ensure parent directory exists
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file_bytes = std::fs::metadata(dest)
            .map(|value| value.len())
            .unwrap_or(0);
        if file_size > 0 && file_bytes > file_size {
            std::fs::remove_file(dest)?;
            file_bytes = 0;
        }
        if file_bytes > 0 {
            *_total_downloaded += file_bytes;
            self.cumulative_downloaded
                .fetch_add(file_bytes, Ordering::Relaxed);
        }
        let start = Instant::now();
        let mut last_error = "download failed".to_string();

        for attempt in 1..=3u64 {
            if self.is_cancelled().await {
                let _ = std::fs::remove_file(dest);
                return Err(DownloadError::Other("Download cancelled".to_string()));
            }
            if file_size > 0 && file_bytes == file_size {
                return Ok(());
            }

            let resume_from = file_bytes;
            let mut request = client.get(url);
            if resume_from > 0 {
                request = request.header(reqwest::header::RANGE, format!("bytes={resume_from}-"));
            }
            let response = match request.send().await {
                Ok(response) => response,
                Err(error) => {
                    last_error = error.to_string();
                    if attempt < 3 {
                        self.emit_retry(file_path, attempt + 1);
                        tokio::time::sleep(std::time::Duration::from_millis(250 * attempt)).await;
                        continue;
                    }
                    break;
                }
            };
            let status = response.status();
            if !status.is_success() {
                last_error = format!("HTTP status {status}");
                let transient = status.is_server_error()
                    || status == reqwest::StatusCode::REQUEST_TIMEOUT
                    || status == reqwest::StatusCode::TOO_MANY_REQUESTS;
                if transient && attempt < 3 {
                    self.emit_retry(file_path, attempt + 1);
                    tokio::time::sleep(std::time::Duration::from_millis(250 * attempt)).await;
                    continue;
                }
                break;
            }

            let resumed = resume_from > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT;
            if resumed {
                let expected = format!("bytes {resume_from}-");
                let valid = response
                    .headers()
                    .get(reqwest::header::CONTENT_RANGE)
                    .and_then(|value| value.to_str().ok())
                    .is_some_and(|value| value.starts_with(&expected));
                if !valid {
                    return Err(DownloadError::Other(format!(
                        "Invalid range response for {file_path}"
                    )));
                }
            } else if resume_from > 0 {
                *_total_downloaded = _total_downloaded.saturating_sub(resume_from);
                self.cumulative_downloaded
                    .fetch_sub(resume_from, Ordering::Relaxed);
                file_bytes = 0;
            }

            let mut options = std::fs::OpenOptions::new();
            options.create(true).write(true);
            if resumed {
                options.append(true);
            } else {
                options.truncate(true);
            }
            let mut file = options.open(dest)?;
            let mut stream = response.bytes_stream();
            let mut stream_failed = false;

            while let Some(chunk) = stream.next().await {
                if self.is_cancelled().await {
                    drop(file);
                    let _ = std::fs::remove_file(dest);
                    return Err(DownloadError::Other("Download cancelled".to_string()));
                }
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(error) => {
                        last_error = error.to_string();
                        stream_failed = true;
                        break;
                    }
                };
                std::io::Write::write_all(&mut file, &chunk)?;
                file_bytes += chunk.len() as u64;
                *_total_downloaded += chunk.len() as u64;
                self.cumulative_downloaded
                    .fetch_add(chunk.len() as u64, Ordering::Relaxed);

                let elapsed = start.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    (file_bytes as f64 / elapsed) as u64
                } else {
                    0
                };
                let cumulative = self.cumulative_downloaded.load(Ordering::Relaxed);
                let grand = self.grand_total.load(Ordering::Relaxed);
                self.emit_progress(
                    file_path, file_index, file_count, file_bytes, file_size, cumulative, grand,
                    speed,
                );
            }
            std::io::Write::flush(&mut file)?;
            file.sync_all()?;
            drop(file);

            if !stream_failed && (file_size == 0 || file_bytes == file_size) {
                return Ok(());
            }
            if !stream_failed {
                last_error = format!(
                    "Size mismatch for {file_path}: expected {file_size}, got {file_bytes}"
                );
            }
            if file_size > 0 && file_bytes > file_size {
                break;
            }
            if attempt < 3 {
                self.emit_retry(file_path, attempt + 1);
                tokio::time::sleep(std::time::Duration::from_millis(250 * attempt)).await;
            }
        }

        let _ = std::fs::remove_file(dest);
        Err(DownloadError::Other(format!(
            "Download failed for {file_path} after 3 attempts: {last_error}"
        )))
    }

    fn emit_retry(&self, file_path: &str, attempt: u64) {
        let _ = self.app_handle.emit(
            "launcher://status",
            serde_json::json!({
                "phase": "retry",
                "message": format!("Retrying {file_path} ({attempt}/3)"),
                "cancelable": true,
            }),
        );
    }

    /// Emit a progress event to the frontend.
    #[allow(clippy::too_many_arguments)]
    fn emit_progress(
        &self,
        file_path: &str,
        file_index: usize,
        file_count: usize,
        bytes_downloaded: u64,
        file_bytes_total: u64,
        total_bytes_downloaded: u64,
        total_bytes_all: u64,
        speed_bytes_per_sec: u64,
    ) {
        let payload = DownloadProgressPayload {
            file_path: file_path.to_string(),
            file_index,
            file_count,
            bytes_downloaded,
            file_bytes_total,
            total_bytes_downloaded,
            total_bytes_all,
            speed_bytes_per_sec,
        };

        if let Err(e) = self.app_handle.emit("download://progress", payload) {
            log::error!("Failed to emit progress event: {e}");
        }
    }

    /// Compute the SHA-256 hex digest of a file.
    pub fn sha256_file(path: &Path) -> Result<String, DownloadError> {
        use sha2::Digest;
        let mut file = std::fs::File::open(path)?;
        let mut hasher = sha2::Sha256::new();
        std::io::copy(&mut file, &mut HashWriter(&mut hasher))?;
        Ok(format!("{:x}", hasher.finalize()))
    }
}

pub(crate) fn partial_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(".part");
    PathBuf::from(value)
}

fn manifest_policy() -> blockfield_shared::ManifestPolicy {
    fn value(runtime: &str, compiled: Option<&str>) -> Option<String> {
        std::env::var(runtime)
            .ok()
            .or_else(|| compiled.map(str::to_string))
            .filter(|value| !value.trim().is_empty())
    }
    fn usize_limit(runtime: &str, compiled: Option<&str>, default: usize) -> usize {
        value(runtime, compiled)
            .and_then(|value| value.parse().ok())
            .unwrap_or(default)
    }
    fn u64_limit(runtime: &str, compiled: Option<&str>, default: u64) -> u64 {
        value(runtime, compiled)
            .and_then(|value| value.parse().ok())
            .unwrap_or(default)
    }

    let mut limits = blockfield_shared::ManifestLimits::default();
    limits.max_manifest_bytes = usize_limit(
        "BLOCKFIELD_MAX_MANIFEST_BYTES",
        option_env!("BLOCKFIELD_MAX_MANIFEST_BYTES"),
        limits.max_manifest_bytes,
    );
    limits.max_files = usize_limit(
        "BLOCKFIELD_MAX_MANIFEST_FILES",
        option_env!("BLOCKFIELD_MAX_MANIFEST_FILES"),
        limits.max_files,
    );
    limits.max_file_bytes = u64_limit(
        "BLOCKFIELD_MAX_MANIFEST_FILE_BYTES",
        option_env!("BLOCKFIELD_MAX_MANIFEST_FILE_BYTES"),
        limits.max_file_bytes,
    );
    limits.max_payload_bytes = u64_limit(
        "BLOCKFIELD_MAX_MANIFEST_PAYLOAD_BYTES",
        option_env!("BLOCKFIELD_MAX_MANIFEST_PAYLOAD_BYTES"),
        limits.max_payload_bytes,
    );

    blockfield_shared::ManifestPolicy {
        limits,
        trusted_download_hosts: value(
            "BLOCKFIELD_TRUSTED_DOWNLOAD_HOSTS",
            option_env!("BLOCKFIELD_TRUSTED_DOWNLOAD_HOSTS"),
        )
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|host| !host.is_empty())
        .map(str::to_string)
        .collect(),
        allow_insecure_localhost: cfg!(debug_assertions),
    }
}

fn prepare_manifest_target(game_dir: &Path, value: &str) -> Result<PathBuf, DownloadError> {
    let relative = blockfield_shared::safe_manifest_path(value).map_err(DownloadError::Other)?;
    let root = std::fs::canonicalize(game_dir)?;
    let mut parent = root.clone();
    if let Some(components) = relative.parent() {
        for component in components.components() {
            parent.push(component);
            match std::fs::symlink_metadata(&parent) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(DownloadError::Other(format!(
                        "Symlinked manifest parent rejected: {}",
                        parent.display()
                    )));
                }
                Ok(metadata) if !metadata.is_dir() => {
                    return Err(DownloadError::Other(format!(
                        "Manifest parent is not a directory: {}",
                        parent.display()
                    )));
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    std::fs::create_dir(&parent)?;
                }
                Err(error) => return Err(error.into()),
            }
            if !std::fs::canonicalize(&parent)?.starts_with(&root) {
                return Err(DownloadError::Other(
                    "Manifest target escapes the game directory".to_string(),
                ));
            }
        }
    }

    let destination = root.join(relative);
    if std::fs::symlink_metadata(&destination)
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(DownloadError::Other(format!(
            "Symlinked manifest target rejected: {}",
            destination.display()
        )));
    }
    Ok(destination)
}

fn reject_symlink(path: &Path) -> Result<(), DownloadError> {
    if std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        Err(DownloadError::Other(format!(
            "Symlinked download target rejected: {}",
            path.display()
        )))
    } else {
        Ok(())
    }
}

pub(crate) fn activate_partial(partial: &Path, destination: &Path) -> Result<(), std::io::Error> {
    if !destination.exists() {
        return std::fs::rename(partial, destination);
    }
    let backup = partial_path(destination).with_extension("backup");
    let _ = std::fs::remove_file(&backup);
    std::fs::rename(destination, &backup)?;
    if let Err(error) = std::fs::rename(partial, destination) {
        let _ = std::fs::rename(&backup, destination);
        return Err(error);
    }
    let _ = std::fs::remove_file(backup);
    Ok(())
}

struct HashWriter<'a>(&'a mut sha2::Sha256);

impl std::io::Write for HashWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        use sha2::Digest;
        self.0.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_target_stays_under_root_and_rejects_symlinked_parents() {
        let root = std::env::temp_dir().join(format!(
            "blockfield-manifest-target-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let outside = root.with_extension("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        assert_eq!(
            prepare_manifest_target(&root, "mods/core.jar").unwrap(),
            std::fs::canonicalize(&root).unwrap().join("mods/core.jar")
        );
        assert!(prepare_manifest_target(&root, "../outside").is_err());

        let link = root.join("linked");
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&outside, &link).is_ok();
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_dir(&outside, &link).is_ok();
        if linked {
            assert!(prepare_manifest_target(&root, "linked/escape.jar").is_err());
        }

        std::fs::remove_dir_all(&root).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
    }

    #[test]
    fn progress_payload_matches_frontend_keys() {
        let value = serde_json::to_value(DownloadProgressPayload {
            file_path: "mods/a.jar".into(),
            file_index: 1,
            file_count: 2,
            bytes_downloaded: 3,
            file_bytes_total: 4,
            total_bytes_downloaded: 5,
            total_bytes_all: 6,
            speed_bytes_per_sec: 7,
        })
        .unwrap();

        assert!(value.get("totalBytesAll").is_some());
        assert!(value.get("total_bytes_all").is_none());
    }
}
