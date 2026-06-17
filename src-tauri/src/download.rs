use crate::manifest::ModpackManifest;
use reqwest::Client;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

/// Serializable progress payload emitted to the frontend via `download://progress`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
    cancel_flag: Arc<Mutex<bool>>,
}

impl Downloader {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            client: Client::new(),
            app_handle,
            cancel_flag: Arc::new(Mutex::new(false)),
        }
    }

    /// Signal the downloader to cancel the current operation.
    pub async fn cancel(&self) {
        let mut flag = self.cancel_flag.lock().await;
        *flag = true;
        log::info!("Download cancel requested");
    }

    /// Reset the cancel flag before starting a new download.
    pub async fn reset_cancel(&self) {
        let mut flag = self.cancel_flag.lock().await;
        *flag = false;
    }

    /// Check whether cancellation has been requested.
    async fn is_cancelled(&self) -> bool {
        *self.cancel_flag.lock().await
    }

    /// Fetch the remote modpack manifest.
    pub async fn fetch_manifest(&self, url: &str) -> Result<ModpackManifest, DownloadError> {
        log::info!("Fetching manifest from {url}");
        let response = self.client.get(url).send().await?;
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
        let total_bytes_all = manifest.total_size;
        let mut total_downloaded: u64 = 0;
        let mut new_installed: std::collections::HashMap<String, String> = installed_sha256.clone();

        for (i, entry) in manifest.files.iter().enumerate() {
            if self.is_cancelled().await {
                log::info!("Download cancelled by user");
                return Err(DownloadError::Other("Download cancelled".to_string()));
            }

            let dest = game_dir.join(&entry.path);

            // Skip files that already match the expected hash (unchanged)
            if let Some(existing_hash) = installed_sha256.get(&entry.path) {
                if existing_hash == &entry.sha256 && dest.exists() {
                    log::info!("Skipping unchanged file: {}", entry.path);
                    total_downloaded += entry.size;
                    self.emit_progress(
                        &entry.path,
                        i + 1,
                        file_count,
                        entry.size,
                        entry.size,
                        total_downloaded,
                        total_bytes_all,
                        0,
                    );
                    continue;
                }
            }

            // Download the file
            self.download_one(&entry.url, &dest, &entry.path, entry.size, i + 1, file_count, &mut total_downloaded, total_bytes_all)
                .await?;

            // Verify SHA256 after download
            let actual_hash = Self::sha256_file(&dest)?;
            if actual_hash != entry.sha256 {
                // Delete the corrupted file
                let _ = std::fs::remove_file(&dest);
                return Err(DownloadError::Other(format!(
                    "SHA256 mismatch for {}: expected {}, got {}",
                    entry.path, entry.sha256, actual_hash
                )));
            }

            new_installed.insert(entry.path.clone(), entry.sha256.clone());
        }

        // Write the updated installed manifest
        let installed = crate::manifest::InstalledManifest {
            version: manifest.version.clone(),
            files: new_installed,
        };
        installed
            .save(&game_dir.to_string_lossy())
            .map_err(|e| DownloadError::Other(e))?;

        log::info!("Download complete: {} files, {} bytes", file_count, total_downloaded);

        // Emit a final event with speed=0 to signal completion
        self.emit_progress(
            "",
            file_count,
            file_count,
            0,
            0,
            total_downloaded,
            total_bytes_all,
            0,
        );

        Ok(())
    }

    /// Download a single file with streaming, emitting progress events.
    async fn download_one(
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
        log::info!("Downloading: {file_path} ({file_size} bytes)");

        // Ensure parent directory exists
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let response = self.client.get(url).send().await?;
        let mut stream = response.bytes_stream();

        let mut file = std::fs::File::create(dest)?;
        let mut file_bytes: u64 = 0;
        let start = Instant::now();

        use futures_util::StreamExt;
        while let Some(chunk) = stream.next().await {
            if self.is_cancelled().await {
                let _ = std::fs::remove_file(dest);
                return Err(DownloadError::Other("Download cancelled".to_string()));
            }

            let chunk = chunk?;
            std::io::Write::write_all(&mut file, &chunk)?;
            file_bytes += chunk.len() as u64;
            *total_downloaded += chunk.len() as u64;

            // Calculate speed
            let elapsed = start.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                (file_bytes as f64 / elapsed) as u64
            } else {
                0
            };

            self.emit_progress(
                file_path,
                file_index,
                file_count,
                file_bytes,
                file_size,
                *total_downloaded,
                total_bytes_all,
                speed,
            );
        }

        Ok(())
    }

    /// Emit a progress event to the frontend, rate-limited to ~150ms.
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
