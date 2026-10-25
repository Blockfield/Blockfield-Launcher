//! Tauri-independent home of the launcher's shared state and event seam.
//!
//! Phase 2 of the native migration: [`LauncherAppState`] and its payload types
//! moved here unchanged from `src-tauri/src/commands.rs`. The Tauri adapter
//! (`src-tauri`) re-exports them, so every call site compiles unchanged, and maps
//! [`LauncherEvent`] back to today's exact event names and payloads. The later
//! Slint app uses the same state and sink with a different adapter.

use launcher_core::config::LauncherConfig;
use launcher_core::game::{GameState, GameStatus};
use launcher_net::download::{DownloadProgressPayload, Downloader};
use launcher_net::pack::{JavaInfo, LauncherInfo, PackMeta};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::{Mutex, MutexGuard, RwLock};

/// Shared application state, moved unchanged from the Tauri adapter except for
/// the added [`EventSink`]: core and shell observe the same events.
pub struct LauncherAppState {
    /// No outer mutex — Downloader uses atomics internally for cancel flag and progress.
    pub downloader: Arc<Downloader>,
    pub pack: RwLock<Option<PackState>>,
    pub config: RwLock<LauncherConfig>,
    pub app_data_dir: PathBuf,
    /// Direct handle to the atomic cancel flag so cancel_download can set it
    /// without any lock contention.
    pub cancel_flag: Arc<AtomicBool>,
    pub game: Arc<GameState>,
    pub operation: Mutex<()>,
    /// Where status/progress/game/room/account events go; the shell maps each
    /// variant back to today's exact event name and payload.
    pub events: EventSink,
}

/// What `check_modpack_version` learned about the hosted pack.
#[derive(Clone)]
pub struct PackState {
    pub info: LauncherInfo,
    pub meta: PackMeta,
}

pub use launcher_net::account::GameIdentity;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionCheckResult {
    pub needs_update: bool,
    pub remote_version: String,
    pub installed_version: String,
    pub mirror: String,
    pub file_count: usize,
    pub total_size: u64,
    pub java: Option<JavaInfo>,
    pub java_ok: bool,
    pub loader_ok: bool,
}

/// Payload of today's `launcher://status` event, identical serde shape.
#[derive(Clone, Debug, Serialize)]
pub struct LauncherStatusPayload {
    pub phase: &'static str,
    pub message: String,
    pub cancelable: bool,
}

/// Mirror line forwarded from the pack installer; today's `download://mirror`
/// payload is `{"name", "speed"}` with null speed when unknown.
#[derive(Clone, Debug, Serialize)]
pub struct DownloadMirrorPayload {
    pub name: String,
    pub speed: Option<String>,
}

/// One backend-to-frontend event, a variant per event name emitted today.
#[derive(Clone, Debug)]
pub enum LauncherEvent {
    Status(LauncherStatusPayload),
    DownloadProgress(DownloadProgressPayload),
    DownloadMirror(DownloadMirrorPayload),
    GameStatus(GameStatus),
    RoomPending,
    AccountExpired,
}

/// Event observer; the shell adapter maps each variant to today's exact event
/// name and payload, the native app to its own UI updates.
pub type EventSink = Arc<dyn Fn(LauncherEvent) + Send + Sync>;

/// Error when a second launcher operation starts while one runs. Byte-identical
/// to today's `download_modpack`/`verify_files`/`launch_game` error.
pub const OPERATION_BUSY_MESSAGE: &str = "Another launcher operation is in progress";

impl LauncherAppState {
    /// Holds the single-operation lock; fails with today's exact busy string.
    pub fn begin_operation(&self) -> Result<MutexGuard<'_, ()>, String> {
        self.operation
            .try_lock()
            .map_err(|_| OPERATION_BUSY_MESSAGE.to_string())
    }

    /// Builds a status event with today's exact payload shape.
    pub fn status_event(
        phase: &'static str,
        message: impl Into<String>,
        cancelable: bool,
    ) -> LauncherEvent {
        LauncherEvent::Status(LauncherStatusPayload {
            phase,
            message: message.into(),
            cancelable,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use launcher_net::download::noop_guard_sink;
    use std::sync::Mutex as StdMutex;

    fn test_state(events: EventSink) -> LauncherAppState {
        let downloader = Arc::new(Downloader::new(
            Arc::new(|_| {}),
            Arc::new(|_, _| {}),
            noop_guard_sink(),
        ));
        LauncherAppState {
            cancel_flag: downloader.cancel_flag.clone(),
            downloader,
            pack: RwLock::new(None),
            config: RwLock::new(LauncherConfig::default()),
            app_data_dir: PathBuf::from("/tmp/blockfield-launcher-app-test"),
            game: Arc::new(GameState::new(|_| {})),
            operation: Mutex::new(()),
            events,
        }
    }

    #[test]
    fn status_event_serializes_like_todays_launcher_status_payload() {
        let received = Arc::new(StdMutex::new(Vec::new()));
        let sink: EventSink = Arc::new({
            let received = received.clone();
            move |event| received.lock().unwrap().push(event)
        });
        let state = test_state(sink);
        (state.events)(LauncherAppState::status_event(
            "setup",
            "Подготовка установки",
            false,
        ));

        let received = received.lock().unwrap();
        assert_eq!(received.len(), 1);
        let LauncherEvent::Status(payload) = &received[0] else {
            panic!("expected a Status event");
        };
        assert_eq!(
            serde_json::to_value(payload).unwrap(),
            serde_json::json!({
                "phase": "setup",
                "message": "Подготовка установки",
                "cancelable": false,
            })
        );
    }

    #[test]
    fn operation_gate_rejects_a_second_operation_and_frees_on_drop() {
        let state = test_state(Arc::new(|_| {}));
        let _first = state.begin_operation().unwrap();
        assert_eq!(
            state.begin_operation().unwrap_err(),
            "Another launcher operation is in progress"
        );
        drop(_first);
        assert!(state.begin_operation().is_ok());
    }

    #[test]
    fn version_check_result_keeps_its_camel_case_snapshot() {
        let result = VersionCheckResult {
            needs_update: true,
            remote_version: "2.0.0".to_string(),
            installed_version: "1.9.0".to_string(),
            mirror: "https://example.com/pack".to_string(),
            file_count: 3,
            total_size: 42,
            java: Some(JavaInfo {
                version: "21.0.5".to_string(),
                url: "https://example.com/java.zip".to_string(),
                sha256: "ab".repeat(32),
                size: 1,
                platform: "linux-x86_64".to_string(),
            }),
            java_ok: true,
            loader_ok: false,
        };
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            serde_json::json!({
                "needsUpdate": true,
                "remoteVersion": "2.0.0",
                "installedVersion": "1.9.0",
                "mirror": "https://example.com/pack",
                "fileCount": 3,
                "totalSize": 42,
                "java": {
                    "version": "21.0.5",
                    "url": "https://example.com/java.zip",
                    "sha256": "ab".repeat(32),
                    "size": 1,
                    "platform": "linux-x86_64",
                },
                "javaOk": true,
                "loaderOk": false,
            })
        );
    }
}
