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
    pub unit: ProgressUnit,
    pub file_path: String,
    pub file_index: usize,
    pub file_count: usize,
    pub bytes_downloaded: u64,
    pub file_bytes_total: u64,
    pub total_bytes_downloaded: u64,
    pub total_bytes_all: u64,
    pub speed_bytes_per_sec: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProgressUnit {
    Bytes,
    Files,
}

impl DownloadProgressPayload {
    pub fn files(file_path: String, done: usize, total: usize) -> Self {
        Self {
            unit: ProgressUnit::Files,
            file_path,
            file_index: done,
            file_count: total,
            bytes_downloaded: 0,
            file_bytes_total: 0,
            total_bytes_downloaded: 0,
            total_bytes_all: 0,
            speed_bytes_per_sec: 0,
        }
    }
}

// ponytail: fixed small fan-out; make it configurable only if the mirror starts rate-limiting.
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
    /// Progress events are throttled: thousands of small asset chunks per second would
    /// flood the webview IPC and dominate download time.
    last_emit_ms: AtomicU64,
    epoch: Instant,
}

impl Downloader {
    pub fn new(app_handle: AppHandle) -> Self {
        let client_builder = || {
            Client::builder()
                .connect_timeout(std::time::Duration::from_secs(10))
                .read_timeout(std::time::Duration::from_secs(30))
                .timeout(std::time::Duration::from_secs(30 * 60))
        };
        Self {
            client: client_builder().build().expect("valid HTTP client"),
            app_handle,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            grand_total: AtomicU64::new(0),
            cumulative_downloaded: AtomicU64::new(0),
            last_emit_ms: AtomicU64::new(0),
            epoch: Instant::now(),
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
        crate::installed_files::cleanup_partials(game_dir);
    }

    /// Expected bytes still to come; the grand total becomes what is already downloaded plus this.
    pub fn set_grand_total(&self, remaining: u64) {
        let done = self.cumulative_downloaded.load(Ordering::SeqCst);
        self.grand_total.store(done + remaining, Ordering::SeqCst);
    }

    /// True at most once per 100ms, or when everything is downloaded.
    fn should_emit(&self, cumulative: u64, grand: u64) -> bool {
        if grand > 0 && cumulative >= grand {
            return true;
        }
        let now = self.epoch.elapsed().as_millis() as u64;
        let last = self.last_emit_ms.load(Ordering::Relaxed);
        now.saturating_sub(last) >= 100
            && self
                .last_emit_ms
                .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
    }

    /// Check whether cancellation has been requested.
    async fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::SeqCst)
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
        let owned = crate::installed_files::Write::new(dest);
        let result = self
            .download_one_with_client(
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
            .await;
        owned.finish();
        result
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
                if self.should_emit(cumulative, grand) {
                    self.emit_progress(
                        file_path, file_index, file_count, file_bytes, file_size, cumulative,
                        grand, speed,
                    );
                }
            }
            std::io::Write::flush(&mut file)?;
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
            unit: ProgressUnit::Bytes,
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
        Ok(hex_digest(&hasher.finalize()))
    }
}

pub(crate) fn hex_digest(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut hex, byte| {
            write!(hex, "{byte:02x}").expect("formatting into a String cannot fail");
            hex
        })
}

pub(crate) fn partial_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(".part");
    PathBuf::from(value)
}

pub(crate) fn activate_partial(partial: &Path, destination: &Path) -> Result<(), std::io::Error> {
    let owned = crate::installed_files::Write::new(destination);
    if !destination.exists() {
        std::fs::rename(partial, destination)?;
        owned.finish();
        return Ok(());
    }
    let backup = partial_path(destination).with_extension("backup");
    let _ = std::fs::remove_file(&backup);
    std::fs::rename(destination, &backup)?;
    if let Err(error) = std::fs::rename(partial, destination) {
        let _ = std::fs::rename(&backup, destination);
        return Err(error);
    }
    let _ = std::fs::remove_file(backup);
    owned.finish();
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
    fn sha256_streams_files_and_failed_activation_restores_the_previous_file() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("artifact.jar");
        std::fs::write(&destination, b"abc").unwrap();
        assert_eq!(
            Downloader::sha256_file(&destination).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let large = temp.path().join("large.jar");
        std::fs::write(&large, vec![b'a'; 1_000_000]).unwrap();
        assert_eq!(
            Downloader::sha256_file(&large).unwrap(),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        let partial = partial_path(&destination);
        assert!(activate_partial(&partial, &destination).is_err());
        assert_eq!(std::fs::read(&destination).unwrap(), b"abc");
        std::fs::write(&partial, b"new").unwrap();
        activate_partial(&partial, &destination).unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), b"new");
        assert!(!partial.exists());
    }

    #[test]
    fn cleanup_preserves_packwiz_resume_files() {
        let game = tempfile::tempdir().unwrap();
        let cache = game.path().join(".packwiz-downloads");
        std::fs::create_dir(&cache).unwrap();
        let resumable = cache.join("mod.partial");
        std::fs::write(&resumable, b"downloaded prefix").unwrap();
        let obsolete = game.path().join("runtime.part");
        std::fs::write(&obsolete, b"old runtime download").unwrap();
        Downloader::cleanup_partials(game.path());
        assert_eq!(std::fs::read(resumable).unwrap(), b"downloaded prefix");
        assert_eq!(std::fs::read(obsolete).unwrap(), b"old runtime download");
    }

    #[test]
    fn progress_payload_matches_frontend_keys() {
        let value = serde_json::to_value(DownloadProgressPayload {
            unit: ProgressUnit::Bytes,
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
        assert_eq!(value["unit"], "bytes");
        let files =
            serde_json::to_value(DownloadProgressPayload::files("large.jar".into(), 99, 100))
                .unwrap();
        assert_eq!(files["unit"], "files");
        assert_eq!(files["fileIndex"], 99);
        assert_eq!(files["fileCount"], 100);
        assert_eq!(files["totalBytesDownloaded"], 0);
        assert_eq!(files["totalBytesAll"], 0);
    }
}
