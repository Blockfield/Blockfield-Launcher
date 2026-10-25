use crate::commands::LauncherAppState;
use serde::Serialize;
use std::time::Duration;
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
pub async fn check_launcher_update<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, LauncherUpdater>,
) -> Result<Option<UpdateInfo>, String> {
    if crate::dev::is_dev_build() {
        return Ok(None);
    }
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
pub async fn install_launcher_update<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, LauncherUpdater>,
    on_progress: Channel<UpdateProgress>,
) -> Result<(), String> {
    if crate::dev::is_dev_build() {
        return Err("Автоматические обновления недоступны в DEV-версии лаунчера".into());
    }
    let launcher = app.state::<LauncherAppState>();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name);
        let raw = std::fs::read_to_string(path).unwrap();
        raw.strip_suffix('\n').unwrap_or(&raw).to_string()
    }

    #[test]
    fn update_info_and_progress_keep_their_camel_case_snapshots() {
        assert_eq!(
            serde_json::to_string(&UpdateInfo {
                current_version: "1.0.19".into(),
                version: "1.0.20".into(),
                body: Some("Fixes and improvements".into()),
            })
            .unwrap(),
            fixture("update_info.json"),
        );
        assert_eq!(
            serde_json::to_string(&UpdateProgress {
                phase: "downloading",
                downloaded: 3,
                total: Some(9),
            })
            .unwrap(),
            fixture("update_progress.json"),
        );
    }

    #[tokio::test]
    async fn dev_build_refuses_stable_updates_before_accessing_updater_or_launcher() {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().plugins.0.insert(
            "updater".into(),
            serde_json::json!({ "pubkey": "", "endpoints": [] }),
        );
        let app = tauri::test::mock_builder()
            .manage(LauncherUpdater::default())
            .build(context)
            .unwrap();
        if crate::dev::is_dev_build() {
            assert!(check_launcher_update(app.handle().clone(), app.state())
                .await
                .unwrap()
                .is_none());
            let error = install_launcher_update(
                app.handle().clone(),
                app.state(),
                Channel::new(|_| panic!("DEV updater must not report progress")),
            )
            .await
            .unwrap_err();
            assert!(error.contains("DEV"));
        } else {
            // No endpoints or native-install marker: production still attempts its normal check.
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())
                .unwrap();
            assert!(check_launcher_update(app.handle().clone(), app.state())
                .await
                .is_err());
        }
    }
}
