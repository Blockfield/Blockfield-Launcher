use crate::manifest::ModpackManifest;
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
        Self {
            client: Client::new(),
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

    /// Set the grand total of all expected bytes across all download phases.
    pub fn set_grand_total(&self, total: u64) {
        self.grand_total.store(total, Ordering::SeqCst);
    }

    /// Add to the grand total (useful when phases discover their size dynamically).
    pub fn add_to_grand_total(&self, delta: u64) {
        self.grand_total.fetch_add(delta, Ordering::SeqCst);
    }

    /// Check whether cancellation has been requested.
    async fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::SeqCst)
    }

    /// Fetch the remote modpack manifest.
    pub async fn fetch_manifest(&self, url: &str) -> Result<ModpackManifest, DownloadError> {
        log::info!("Fetching manifest from {url}");
        let response = self.client.get(url).send().await?.error_for_status()?;
        let manifest: ModpackManifest = response.json().await?;
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

        let file_count = manifest.files.len();
        let mut total_downloaded: u64 = 0;
        let mut new_installed: std::collections::HashMap<String, String> = installed_sha256.clone();
        let mut downloads = Vec::new();

        for (i, entry) in manifest.files.iter().enumerate() {
            if self.is_cancelled().await {
                log::info!("Download cancelled by user");
                return Err(DownloadError::Other("Download cancelled".to_string()));
            }

            let dest = game_dir.join(&entry.path);

            // Skip files that already match the expected hash (unchanged)
            if let Some(existing_hash) = installed_sha256.get(&entry.path) {
                if existing_hash == &entry.sha256 && dest.exists() {
                    match Self::sha256_file(&dest) {
                        Ok(actual_hash) if actual_hash == entry.sha256 => {
                            log::info!("Skipping unchanged file: {}", entry.path);
                            total_downloaded += entry.size;
                            // Advance the atomic cumulative counter for skipped files too
                            self.cumulative_downloaded
                                .fetch_add(entry.size, Ordering::Relaxed);
                            let cumulative = self.cumulative_downloaded.load(Ordering::Relaxed);
                            let grand = self.grand_total.load(Ordering::Relaxed);
                            self.emit_progress(
                                &entry.path,
                                i + 1,
                                file_count,
                                entry.size,
                                entry.size,
                                cumulative,
                                grand,
                                0,
                            );
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
            }

            downloads.push(DownloadJob {
                url: entry.url.clone(),
                dest,
                file_path: entry.path.clone(),
                file_size: entry.size,
                file_index: i + 1,
                file_count,
                sha256: entry.sha256.clone(),
            });
            new_installed.insert(entry.path.clone(), entry.sha256.clone());
        }

        total_downloaded += self.download_jobs(downloads).await?;

        // Write the updated installed manifest
        let installed = crate::manifest::InstalledManifest {
            version: manifest.version.clone(),
            files: new_installed,
        };
        installed
            .save(&game_dir.to_string_lossy())
            .map_err(|e| DownloadError::Other(e))?;

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
        let mut downloads = futures_util::stream::iter(jobs)
            .map(|job| async move {
                let mut downloaded = 0;
                self.download_one(
                    &job.url,
                    &job.dest,
                    &job.file_path,
                    job.file_size,
                    job.file_index,
                    job.file_count,
                    &mut downloaded,
                    0,
                )
                .await?;

                let actual_hash = Self::sha256_file(&job.dest)?;
                if actual_hash != job.sha256 {
                    let _ = std::fs::remove_file(&job.dest);
                    return Err(DownloadError::Other(format!(
                        "SHA256 mismatch for {}: expected {}, got {}",
                        job.file_path, job.sha256, actual_hash
                    )));
                }

                Ok(downloaded)
            })
            .buffer_unordered(PARALLEL_FILE_DOWNLOADS);

        let mut total_downloaded = 0;
        while let Some(result) = downloads.next().await {
            total_downloaded += result?;
        }
        Ok(total_downloaded)
    }

    /// Download a single file with streaming, emitting progress events.
    /// Progress uses the Downloader's atomic `cumulative_downloaded` and `grand_total`
    /// so the frontend sees one continuous progress bar across all download phases.
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
        log::info!("Downloading: {file_path} ({file_size} bytes)");

        // Ensure parent directory exists
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let response = self.client.get(url).send().await?.error_for_status()?;
        let mut stream = response.bytes_stream();

        let mut file = std::fs::File::create(dest)?;
        let mut file_bytes: u64 = 0;
        let start = Instant::now();

        while let Some(chunk) = stream.next().await {
            if self.is_cancelled().await {
                let _ = std::fs::remove_file(dest);
                return Err(DownloadError::Other("Download cancelled".to_string()));
            }

            let chunk = chunk?;
            std::io::Write::write_all(&mut file, &chunk)?;
            file_bytes += chunk.len() as u64;
            *_total_downloaded += chunk.len() as u64;

            // Update the atomic cumulative counter (never resets across phases)
            self.cumulative_downloaded
                .fetch_add(chunk.len() as u64, Ordering::Relaxed);

            // Calculate speed
            let elapsed = start.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                (file_bytes as f64 / elapsed) as u64
            } else {
                0
            };

            // Use atomic cumulative values so the frontend sees continuous progress
            let cumulative = self.cumulative_downloaded.load(Ordering::Relaxed);
            let grand = self.grand_total.load(Ordering::Relaxed);

            self.emit_progress(
                file_path, file_index, file_count, file_bytes, file_size, cumulative, grand, speed,
            );
        }

        Ok(())
    }

    /// Emit a progress event to the frontend.
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
        let bytes = std::fs::read(path)?;
        let mut hasher = sha2::Sha256::new();
        hasher.update(&bytes);
        Ok(format!("{:x}", hasher.finalize()))
    }
}

#[cfg(test)]
mod tests {
    use super::DownloadProgressPayload;

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
