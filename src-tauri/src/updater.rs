use crate::commands::LauncherAppState;
use serde::Serialize;
use std::time::Duration;
#[cfg(target_os = "linux")]
use tauri::Manager;
use tauri::{ipc::Channel, AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex;

#[derive(Default)]
pub struct LauncherUpdater(Mutex<Option<Update>>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    current_version: String,
    version: String,
    body: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    phase: &'static str,
    downloaded: u64,
    total: Option<u64>,
}

#[tauri::command]
pub async fn check_launcher_update(
    app: AppHandle,
    state: State<'_, LauncherUpdater>,
) -> Result<Option<UpdateInfo>, String> {
    if cfg!(debug_assertions) {
        return Err("Обновления доступны в установленной release-версии лаунчера".into());
    }
    let builder = app.updater_builder().timeout(Duration::from_secs(30));
    #[cfg(target_os = "linux")]
    let builder = {
        if app.env().appimage.is_some() {
            builder
        } else {
            let path = tauri::process::current_binary(&app.env()).map_err(|e| e.to_string())?;
            if !crate::native_update::enabled(&path) {
                return Err("Эта установка не поддерживает встроенное обновление".into());
            }
            // This target is already signed and published. Its binary uses the host WebKitGTK.
            builder.target(format!("linux-{}-deb", std::env::consts::ARCH))
        }
    };
    let update = builder
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    let info = update.as_ref().map(|update| UpdateInfo {
        current_version: update.current_version.clone(),
        version: update.version.clone(),
        body: update.body.clone(),
    });
    *state.0.lock().await = update;
    Ok(info)
}

#[tauri::command]
pub async fn install_launcher_update(
    app: AppHandle,
    state: State<'_, LauncherUpdater>,
    launcher: State<'_, LauncherAppState>,
    on_progress: Channel<UpdateProgress>,
) -> Result<(), String> {
    let _operation = launcher
        .operation
        .try_lock()
        .map_err(|_| "Дождитесь завершения проверки или установки файлов игры")?;
    if launcher.game.is_busy() {
        return Err("Закройте игру перед обновлением лаунчера".into());
    }
    let mut pending = state.0.lock().await;
    let update = pending
        .as_ref()
        .ok_or("Сначала проверьте обновления лаунчера")?;
    let mut downloaded = 0u64;
    let mut last_progress = std::time::Instant::now();
    // download() checks the signature before the install phase can begin.
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                if last_progress.elapsed() >= Duration::from_millis(100)
                    || total == Some(downloaded)
                {
                    let _ = on_progress.send(UpdateProgress {
                        phase: "downloading",
                        downloaded,
                        total,
                    });
                    last_progress = std::time::Instant::now();
                }
            },
            || {
                let _ = on_progress.send(UpdateProgress {
                    phase: "verifying",
                    downloaded: 0,
                    total: None,
                });
            },
        )
        .await
        .map_err(|e| e.to_string())?;
    let _ = on_progress.send(UpdateProgress {
        phase: "installing",
        downloaded: bytes.len() as u64,
        total: Some(bytes.len() as u64),
    });
    #[cfg(target_os = "linux")]
    if app.env().appimage.is_none() {
        let path = tauri::process::current_binary(&app.env()).map_err(|e| e.to_string())?;
        if !crate::native_update::enabled(&path) {
            return Err("Этот способ установки не поддерживает встроенное обновление".into());
        }
        tokio::task::spawn_blocking(move || crate::native_update::install_deb(&bytes, &path))
            .await
            .map_err(|e| e.to_string())??;
        *pending = None;
        return Ok(());
    }
    let _ = app;
    let update = update.clone();
    tokio::task::spawn_blocking(move || update.install(bytes))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    *pending = None;
    Ok(())
}
