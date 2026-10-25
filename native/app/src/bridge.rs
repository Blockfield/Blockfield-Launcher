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
                data.progress = -1;
            }
        }
        LauncherEvent::DownloadProgress(progress) => {
            let (done, total) = match progress.unit {
                ProgressUnit::Bytes => (progress.total_bytes_downloaded, progress.total_bytes_all),
                ProgressUnit::Files => (progress.file_index as u64, progress.file_count as u64),
            };
            data.progress = percent(done, total);
            if !progress.file_path.is_empty() {
                data.launch_status = progress.file_path.into();
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
    #[test]
    fn progress_handles_unknown_totals_overflow_and_completion() {
        assert_eq!(super::percent(0, 0), -1);
        assert_eq!(super::percent(1, 3), 33);
        assert_eq!(super::percent(4, 3), 100);
        assert_eq!(super::percent(u64::MAX, u64::MAX), 100);
    }
}
