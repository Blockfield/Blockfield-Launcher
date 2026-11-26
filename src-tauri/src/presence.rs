// Presence session logic moved to launcher-app (phase 2, no behaviour change).
// Spawning stays here: `setup` calls `start` outside any tokio runtime, where
// `tokio::spawn` would panic, so `tauri::async_runtime::spawn` is still
// required. Re-exports keep every `crate::presence::…` call site unchanged.
pub use launcher_app::presence::application_id;

use crate::commands::LauncherAppState;
use crate::rooms::PendingRoom;
use tauri::Manager;

pub fn start(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            if app
                .state::<LauncherAppState>()
                .config
                .read()
                .await
                .discord_presence
            {
                let result = {
                    let state = app.state::<LauncherAppState>();
                    let pending = app.state::<PendingRoom>();
                    launcher_app::presence::session(&state, &pending).await
                };
                if let Err(error) = result {
                    log::debug!("Discord presence disconnected: {error}");
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    });
}
