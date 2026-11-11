//! Skin upload wiring (G2). Ports Tauri `SettingsScreen.tsx`
//! `handleSkinUpload`: busy guard, `upload_skin` off the UI thread, avatar
//! cache invalidation (`forgetFace` → `avatars::forget_face` + a fresh face
//! fetch so the shell repaints), success message, friendly error + raw text.
//!
//! Tauri disables upload while busy or when both paths are empty; the Slint
//! button mirrors that via `skin-busy`. `skin-message` carries the success
//! text, `skin-error` the friendly failure, `skin-raw-error` the raw detail
//! (displayed by the G10 diagnostics pass).
use std::sync::Arc;

use launcher_app::LauncherAppState;
use launcher_core::config::LauncherConfig;
use launcher_core::errors::{ErrorContext, friendly_error};
use slint::ComponentHandle;
use tokio::runtime::Handle;

use crate::AppWindow;

/// Tauri `settings.skinUploaded`.
pub const SKIN_UPLOADED: &str = "СКИН ОБНОВЛЁН — виден после перезахода на сервер.";

fn non_empty(path: String) -> Option<String> {
    (!path.trim().is_empty()).then_some(path)
}

/// Off-UI-thread upload so the [`Handle`] spawn below stays thin.
///
/// `started` is the account snapshot taken when the user clicked upload.
/// The task sends exactly those credentials and aborts when the active
/// account changed meanwhile, so one account's skin can never land on
/// another account's session.
pub async fn perform_upload(
    state: &LauncherAppState,
    started: &LauncherConfig,
    skin_path: Option<String>,
    cape_path: Option<String>,
    slim: bool,
) -> Result<launcher_net::skins::SkinUploadResult, String> {
    let current = state.config.read().await;
    if current.player_uuid != started.player_uuid || current.api_token != started.api_token {
        return Err(
            "Аккаунт сменился до загрузки, скин не отправлен. Выберите его ещё раз.".to_string(),
        );
    }
    drop(current);
    let base = state
        .pack
        .read()
        .await
        .as_ref()
        .and_then(|pack| pack.info.skins.clone());
    launcher_net::skins::upload_skin(started, base, skin_path, cape_path, slim).await
}

/// Wire the skin tab upload to the core runtime (same code path as
/// production). Browse callbacks stay in `wire_app` next to the preview host.
pub fn wire_upload(app: &AppWindow, state: Arc<LauncherAppState>, handle: Handle) {
    let weak = app.as_weak();
    app.on_upload_skin(move || {
        let Some(app) = weak.upgrade() else { return };
        if app.get_skin_busy() {
            return;
        }
        let skin = non_empty(app.get_skin_path().to_string());
        let cape = non_empty(app.get_cape_path().to_string());
        if skin.is_none() && cape.is_none() {
            return;
        }
        let slim = app.get_slim();
        app.set_skin_busy(true);
        app.set_skin_message("".into());
        app.set_skin_error("".into());
        app.set_skin_raw_error("".into());
        // Snapshot the account on the UI thread: the queued task must use
        // exactly these credentials, never whatever is active when it runs.
        let started = state.config.blocking_read().clone();
        let state = state.clone();
        let face_handle = handle.clone();
        let spawn = handle.clone();
        let weak = weak.clone();
        spawn.spawn(async move {
            let result = perform_upload(&state, &started, skin, cape, slim).await;
            let skins = state
                .pack
                .read()
                .await
                .as_ref()
                .and_then(|pack| pack.info.skins.clone());
            let _ = weak.upgrade_in_event_loop(move |app| {
                app.set_skin_busy(false);
                match result {
                    Ok(_) => {
                        // Tauri `forgetFace(username)`: drop the cached face so
                        // the next paint refetches, then repaint the shell now.
                        let username = app.get_username().to_string();
                        crate::avatars::forget_face(&username);
                        crate::avatars::request_faces(
                            &app.as_weak(),
                            &face_handle,
                            skins,
                            vec![username],
                        );
                        app.set_skin_message(SKIN_UPLOADED.into());
                    }
                    Err(error) => {
                        let friendly = friendly_error(&error, ErrorContext::SkinUpload);
                        app.set_skin_error(friendly.message.into());
                        app.set_skin_raw_error(friendly.raw.into());
                    }
                }
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime;
    use launcher_app::LauncherAppState;
    use launcher_core::game::GameState;
    use launcher_net::download::{Downloader, noop_guard_sink};
    use std::sync::Arc;
    use tokio::sync::{Mutex, RwLock};

    #[test]
    fn uploaded_message_matches_tauri_i18n() {
        let i18n = include_str!("../../../src/app/i18n.tsx");
        let key = "'settings.skinUploaded': '";
        let start = i18n.find(key).expect("i18n key") + key.len();
        let end = i18n[start..].find('\'').expect("i18n value") + start;
        assert_eq!(&i18n[start..end], SKIN_UPLOADED);
    }

    #[test]
    fn empty_paths_rejected_before_network() {
        let runtime = runtime::new().expect("tokio runtime");
        let downloader = Arc::new(Downloader::new(
            Arc::new(|_| {}),
            Arc::new(|_, _| {}),
            noop_guard_sink(),
        ));
        let authed = || LauncherAppState {
            cancel_flag: downloader.cancel_flag.clone(),
            downloader: downloader.clone(),
            config: RwLock::new(LauncherConfig {
                api_token: "token".into(),
                player_uuid: "uuid".into(),
                ..LauncherConfig::default()
            }),
            app_data_dir: std::env::temp_dir(),
            pack: RwLock::new(None),
            game: Arc::new(GameState::new(|_| {})),
            operation: Mutex::new(()),
            events: Arc::new(|_| {}),
        };
        // Empty paths refuse before any network, even when authenticated.
        let authed = authed();
        let started = authed.config.blocking_read().clone();
        match runtime.block_on(perform_upload(&authed, &started, None, None, false)) {
            Err(err) => assert_eq!(err, "Choose a skin or cape PNG to upload"),
            Ok(_) => panic!("empty upload accepted"),
        }
        // Without tokens the engine refuses before touching the PNG, like
        // Tauri's expired-session path (no network in either case).
        let png = std::env::temp_dir().join(format!("skin-upload-{}.png", std::process::id()));
        std::fs::write(
            &png,
            [b"\x89PNG\r\n\x1a\n".as_slice(), b"data".as_slice()].concat(),
        )
        .expect("temp png");
        let anonymous = LauncherAppState {
            cancel_flag: downloader.cancel_flag.clone(),
            downloader,
            config: RwLock::new(LauncherConfig::default()),
            app_data_dir: std::env::temp_dir(),
            pack: RwLock::new(None),
            game: Arc::new(GameState::new(|_| {})),
            operation: Mutex::new(()),
            events: Arc::new(|_| {}),
        };
        match runtime.block_on(perform_upload(
            &anonymous,
            &anonymous.config.blocking_read().clone(),
            Some(png.to_string_lossy().into_owned()),
            None,
            true,
        )) {
            Err(err) => assert!(err.contains("SESSION_EXPIRED"), "got: {err}"),
            Ok(_) => panic!("anonymous upload accepted"),
        }
        let _ = std::fs::remove_file(&png);
        runtime.shutdown_timeout(std::time::Duration::from_secs(2));
    }

    #[test]
    fn account_switch_between_queue_and_run_aborts_without_upload() {
        let runtime = runtime::new().expect("tokio runtime");
        let downloader = Arc::new(Downloader::new(
            Arc::new(|_| {}),
            Arc::new(|_, _| {}),
            noop_guard_sink(),
        ));
        let state = LauncherAppState {
            cancel_flag: downloader.cancel_flag.clone(),
            downloader,
            config: RwLock::new(LauncherConfig {
                api_token: "token-a".into(),
                player_uuid: "uuid-a".into(),
                ..LauncherConfig::default()
            }),
            app_data_dir: std::env::temp_dir(),
            pack: RwLock::new(None),
            game: Arc::new(GameState::new(|_| {})),
            operation: Mutex::new(()),
            events: Arc::new(|_| {}),
        };
        // Snapshot at click time, like `wire_upload` does on the UI thread.
        let started = state.config.blocking_read().clone();
        // The user switches account before the queued task runs.
        *state.config.blocking_write() = LauncherConfig {
            api_token: "token-b".into(),
            player_uuid: "uuid-b".into(),
            ..LauncherConfig::default()
        };
        // Abort precedes any file/network touch: even a valid PNG refuses.
        let png = std::env::temp_dir().join(format!("skin-switch-{}.png", std::process::id()));
        std::fs::write(
            &png,
            [b"\x89PNG\r\n\x1a\n".as_slice(), b"data".as_slice()].concat(),
        )
        .expect("temp png");
        for paths in [
            (None, None),
            (Some(png.to_string_lossy().into_owned()), None),
        ] {
            match runtime.block_on(perform_upload(&state, &started, paths.0, paths.1, false)) {
                Err(err) => assert!(err.contains("Аккаунт сменился"), "got: {err}"),
                Ok(_) => panic!("upload ran with another account's session"),
            }
        }
        let _ = std::fs::remove_file(&png);
        runtime.shutdown_timeout(std::time::Duration::from_secs(2));
    }
}
