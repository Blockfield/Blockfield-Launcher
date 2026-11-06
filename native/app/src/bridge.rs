//! The sole core-event path into Slint's UI thread.
use std::sync::Arc;

use launcher_app::{EventSink, LauncherEvent};
use launcher_core::game::GamePhase;
use launcher_net::download::ProgressUnit;
use slint::ComponentHandle;

use crate::AppWindow;

pub fn sink(app: &AppWindow) -> EventSink {
    let weak = app.as_weak();
    Arc::new(move |event| {
        if let Err(error) = weak.upgrade_in_event_loop(move |app| apply(&app, event)) {
            eprintln!("Cannot deliver launcher event: {error}");
        }
    })
}

pub fn apply(app: &AppWindow, event: LauncherEvent) {
    let mut data = app.get_main_data();
    match event {
        LauncherEvent::Status(status) => data.launch_status = status.message.into(),
        LauncherEvent::GameStatus(status) => {
            data.phase = match status.phase {
                GamePhase::Idle => "idle",
                GamePhase::Launching => "launching",
                GamePhase::Running => "running",
                GamePhase::Finishing => "finishing",
            }
            .into();
            data.launching = status.phase == GamePhase::Launching;
            if status.phase == GamePhase::Idle {
                data.launch_status = "".into();
                data.inflight_file = "".into();
                data.progress = -1;
            }
        }
        LauncherEvent::DownloadProgress(progress) => {
            let (done, total) = match progress.unit {
                ProgressUnit::Bytes => (progress.total_bytes_downloaded, progress.total_bytes_all),
                ProgressUnit::Files => (progress.file_index as u64, progress.file_count as u64),
            };
            data.progress = percent(done, total);
            // PR #64 inflight file: the files payload carries the live
            // "bytes - name - source" display. It gets its own field so file
            // progress never clobbers the status text (see core_bridge); the
            // main screen leads with it while launching, the Update screen
            // owns a separate stat in U8.
            if matches!(progress.unit, ProgressUnit::Files) && !progress.file_path.is_empty() {
                data.inflight_file = progress.file_path.clone().into();
            }
        }
        LauncherEvent::RoomPending => data.pending_room = true,
        event => {
            eprintln!("Launcher event has no native UI target yet: {event:?}");
            return;
        }
    }
    app.set_main_data(data);
}

fn percent(done: u64, total: u64) -> i32 {
    if total == 0 {
        -1
    } else {
        ((done.min(total) as u128 * 100) / total as u128) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_progress_reports_the_live_file_without_touching_status() {
        use launcher_net::download::DownloadProgressPayload;
        let app = crate::AppWindow::new().unwrap();
        super::apply(
            &app,
            LauncherEvent::DownloadProgress(DownloadProgressPayload::files(
                "12.3/40.1 MB - create.jar - Gcore CDN".into(),
                12,
                137,
            )),
        );
        let data = app.get_main_data();
        assert_eq!(
            data.inflight_file.to_string(),
            "12.3/40.1 MB - create.jar - Gcore CDN"
        );
        assert_eq!(data.launch_status.to_string(), "");
        assert_eq!(data.progress, 8);
        // Bytes-unit payloads carry a file path too (contract golden
        // `download_progress.json`); it must not leak into the live file.
        super::apply(
            &app,
            LauncherEvent::DownloadProgress(DownloadProgressPayload {
                unit: ProgressUnit::Bytes,
                file_path: "mods/core.jar".into(),
                file_index: 2,
                file_count: 9,
                bytes_downloaded: 1048576,
                file_bytes_total: 2097152,
                total_bytes_downloaded: 12582912,
                total_bytes_all: 33554432,
                speed_bytes_per_sec: 8388608,
            }),
        );
        assert_eq!(
            app.get_main_data().inflight_file.to_string(),
            "12.3/40.1 MB - create.jar - Gcore CDN"
        );
        // An empty files path keeps the last file instead of blanking it.
        super::apply(
            &app,
            LauncherEvent::DownloadProgress(DownloadProgressPayload::files(String::new(), 13, 137)),
        );
        assert_eq!(
            app.get_main_data().inflight_file.to_string(),
            "12.3/40.1 MB - create.jar - Gcore CDN"
        );
        assert_eq!(app.get_main_data().progress, 9);
    }

    #[test]
    fn progress_handles_unknown_totals_overflow_and_completion() {
        assert_eq!(super::percent(0, 0), -1);
        assert_eq!(super::percent(1, 3), 33);
        assert_eq!(super::percent(4, 3), 100);
        assert_eq!(super::percent(u64::MAX, u64::MAX), 100);
    }
}
