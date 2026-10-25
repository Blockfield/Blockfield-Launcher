//! Modpack install/verify/cancel orchestration, moved unchanged from
//! `src-tauri/src/commands.rs` (phase 2, no behaviour change).
//!
//! Tauri-only inputs arrive computed: the wrappers pass `&LauncherAppState`
//! straight through (no `State` wrapper), and the old direct
//! `download://mirror` / `download://progress` emits go through
//! [`crate::LauncherEvent`] so the shell adapter maps them back to today's
//! exact event names and payloads. Lock scopes, cancel handling and the order
//! of steps are exactly as in `commands.rs`.

use super::java::{java_runtime_ready, resolved_java_path, system_java_available};
use super::pack_check::INSTALLED_VERSION_FILE;
use crate::{DownloadMirrorPayload, EventSink, LauncherAppState, LauncherEvent};
use launcher_core::config::ClientProfile;
use launcher_net::download::Downloader;
use launcher_net::pack::{Artifact, JavaInfo};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) const INSTALLER_JAR: &str = "packwiz-installer.jar";
pub(crate) const BOOTSTRAP_JAR: &str = "packwiz-installer-bootstrap.jar";

fn emit_status(
    events: &EventSink,
    phase: &'static str,
    message: impl Into<String>,
    cancelable: bool,
) {
    events(LauncherAppState::status_event(phase, message, cancelable));
}

/// Status sink for the core minecraft runtime: produces today's
/// `launcher://status` payload (`{"phase", "message", "cancelable"}`).
fn minecraft_status_sink(events: &EventSink) -> launcher_net::minecraft::StatusSink {
    let events = events.clone();
    std::sync::Arc::new(move |phase: &str, message: &str, cancelable: bool| {
        // StatusSink hands out `&str`, today's status payload keys the phase as
        // `&'static str`; every phase the runtime emits today is listed here.
        // shortcut: a new core phase needs one line here, else one tiny leak per event.
        let phase: &'static str = match phase {
            "runtime" => "runtime",
            other => Box::leak(other.to_owned().into_boxed_str()),
        };
        events(LauncherAppState::status_event(phase, message, cancelable));
    })
}

pub async fn download_modpack(state: &LauncherAppState) -> Result<(), String> {
    let _operation = state.begin_operation()?;
    if state.game.is_busy() {
        return Err("Close the game before updating files".to_string());
    }
    let pack = state
        .pack
        .read()
        .await
        .clone()
        .ok_or_else(|| "No pack info cached – call check_modpack_version first".to_string())?;
    let mut config = state.config.read().await.validated_active()?;
    let game_dir = PathBuf::from(&config.game_dir);
    std::fs::create_dir_all(&game_dir).map_err(|e| format!("Failed to create game dir: {e}"))?;
    let _tracking = launcher_net::installed_files::begin(&state.app_data_dir, &game_dir)?;

    state.downloader.reset_cancel();
    launcher_net::download::cleanup_partials(&game_dir);
    let java = pack.info.java_for_this_platform();
    let java_bytes = java
        .as_ref()
        .filter(|java| !java_runtime_ready(&game_dir, &java.version))
        .map_or(0, |java| java.size);
    // The packwiz installer jar and Minecraft runtime sizes are unknown up front;
    // the runtime plan adds its own bytes below.
    state
        .downloader
        .set_grand_total(java_bytes + 64 * 1024 * 1024);
    emit_status(&state.events, "setup", "Подготовка установки", false);

    if let Some(java) = java {
        if java_runtime_ready(&game_dir, &java.version) {
            emit_status(&state.events, "java", "Java установлена", false);
        } else {
            let java_exe = install_java(state, &game_dir, &java).await?;
            let java_path = java_exe.to_string_lossy().to_string();
            let mut cfg = state.config.write().await;
            match cfg.active_profile {
                ClientProfile::Game => cfg.java_path = java_path.clone(),
                ClientProfile::Workshop => cfg.workshop_java_path = java_path.clone(),
            }
            config.java_path = java_path.clone();
            let _ = launcher_core::config::save_config(&state.app_data_dir, &cfg);
            log::info!("Java installed, path saved: {java_path}");
            emit_status(&state.events, "java", "Java установлена", false);
        }
    }

    let java = resolved_java_path(&config);
    if java == "java" && !system_java_available() {
        return Err("No Java runtime available for this platform".to_string());
    }

    let loader_version_id = match &pack.meta.loader {
        Some(loader) => {
            emit_status(
                &state.events,
                "loader",
                format!("Fetching Fabric {loader} profile"),
                false,
            );
            launcher_net::minecraft::ensure_loader_profile(&game_dir, &pack.meta.minecraft, loader)
                .await?;
            emit_status(&state.events, "loader", "Fabric установлен", false);
            Some(launcher_net::minecraft::loader_version_id(
                &pack.meta.minecraft,
                loader,
            ))
        }
        None => None,
    };

    emit_status(&state.events, "modpack", "Обновление файлов модпака", true);
    ensure_jar(state, &game_dir, BOOTSTRAP_JAR, &pack.info.bootstrap).await?;
    ensure_jar(state, &game_dir, INSTALLER_JAR, &pack.info.installer).await?;
    let cancel = state.cancel_flag.clone();
    let events = state.events.clone();
    let (java_for_installer, dir, pack_url) =
        (java.clone(), game_dir.clone(), pack.info.pack.clone());
    tokio::task::spawn_blocking(move || {
        run_packwiz_installer(&events, &cancel, &java_for_installer, &dir, &pack_url)
    })
    .await
    .map_err(|e| format!("packwiz-installer task failed: {e}"))??;
    launcher_net::installed_files::write(
        &game_dir.join(INSTALLED_VERSION_FILE),
        format!("{}\n{}\n", pack.meta.index_hash, pack.meta.version),
    )
    .map_err(|e| format!("Failed to record installed version: {e}"))?;
    emit_status(&state.events, "modpack", "Файлы модпака готовы", false);

    emit_status(
        &state.events,
        "setup",
        "Planning required runtime files",
        false,
    );
    let runtime_bytes = launcher_net::minecraft::prepare_download_plan(
        &game_dir,
        &pack.meta.minecraft,
        loader_version_id.as_deref(),
    )
    .await?;
    state.downloader.set_grand_total(runtime_bytes);
    let runtime_status = minecraft_status_sink(&state.events);
    launcher_net::minecraft::ensure_launch_dependencies(
        &state.downloader,
        &runtime_status,
        &game_dir,
        &pack.meta.minecraft,
        loader_version_id.as_deref(),
    )
    .await?;
    emit_status(&state.events, "ready", "Игра готова к запуску", false);
    Ok(())
}

async fn install_java(
    state: &LauncherAppState,
    game_dir: &Path,
    java: &JavaInfo,
) -> Result<PathBuf, String> {
    log::info!("Downloading Java {} for {}", java.version, java.platform);
    emit_status(
        &state.events,
        "java",
        format!("Загрузка Java {}", java.version),
        true,
    );
    if java.sha256.len() != 64 || !java.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Java runtime has no valid SHA-256 checksum".to_string());
    }
    let extension = if java.url.ends_with(".tar.gz") {
        "tar.gz"
    } else {
        "zip"
    };
    let archive_path = game_dir.join(format!(".java-runtime.{extension}"));
    let partial = launcher_net::download::partial_path(&archive_path);
    let mut downloaded = 0;
    state
        .downloader
        .download_one(
            &java.url,
            &partial,
            "java-runtime",
            java.size,
            1,
            1,
            &mut downloaded,
            java.size,
        )
        .await
        .map_err(|e| format!("Java download failed: {e}"))?;
    let actual_size = std::fs::metadata(&partial)
        .map_err(|e| format!("Java archive metadata error: {e}"))?
        .len();
    if actual_size != java.size {
        let _ = std::fs::remove_file(&partial);
        return Err(format!(
            "Java archive size mismatch: expected {}, got {actual_size}",
            java.size
        ));
    }
    let actual =
        Downloader::sha256_file(&partial).map_err(|e| format!("Java checksum error: {e}"))?;
    if !actual.eq_ignore_ascii_case(&java.sha256) {
        let _ = std::fs::remove_file(&partial);
        return Err("Java SHA-256 mismatch".to_string());
    }
    launcher_net::download::activate_partial(&partial, &archive_path)
        .map_err(|e| format!("Failed to activate Java archive: {e}"))?;
    emit_status(&state.events, "java", "Распаковка Java", false);
    let java_exe = super::archive::install_java_archive(&archive_path, game_dir, &java.version);
    let _ = std::fs::remove_file(&archive_path);
    java_exe
}

async fn ensure_jar(
    state: &LauncherAppState,
    game_dir: &Path,
    name: &str,
    artifact: &Artifact,
) -> Result<(), String> {
    let jar = game_dir.join(name);
    if matches!(Downloader::sha256_file(&jar), Ok(hash) if hash.eq_ignore_ascii_case(&artifact.sha256))
    {
        return Ok(());
    }
    let partial = launcher_net::download::partial_path(&jar);
    let mut downloaded = 0;
    state
        .downloader
        .download_one(&artifact.url, &partial, name, 0, 1, 1, &mut downloaded, 0)
        .await
        .map_err(|e| format!("{name} download failed: {e}"))?;
    let actual = Downloader::sha256_file(&partial).map_err(|e| e.to_string())?;
    if !actual.eq_ignore_ascii_case(&artifact.sha256) {
        let _ = std::fs::remove_file(&partial);
        return Err(format!("{name} SHA-256 mismatch"));
    }
    launcher_net::download::activate_partial(&partial, &jar).map_err(|e| e.to_string())
}

/// "(12/137) Downloaded X" → (12, 137)
pub(crate) fn installer_progress(line: &str) -> Option<(usize, usize)> {
    let inner = line.strip_prefix('(')?.split_once(')')?.0;
    let (done, total) = inner.split_once('/')?;
    Some((done.parse().ok()?, total.parse().ok()?))
}

pub(crate) fn installer_mirror(line: &str) -> Option<(String, Option<String>)> {
    let name = line.strip_prefix("[blockfield-mirror] ")?.trim();
    if name.is_empty() {
        return None;
    }
    if let Some((name, speed)) = name
        .strip_suffix(" MB/s")
        .and_then(|value| value.rsplit_once(' '))
        .filter(|(name, speed)| {
            !name.is_empty()
                && speed
                    .parse::<f64>()
                    .is_ok_and(|speed| speed.is_finite() && speed >= 0.0)
        })
    {
        return Some((name.to_string(), Some(speed.to_string())));
    }
    Some((name.to_string(), None))
}

/// Runs packwiz-installer (client side, via its bootstrap) in the game dir; it downloads, verifies and prunes the
/// pack contents itself. Output lines are forwarded to the UI as status/progress events.
fn run_packwiz_installer(
    events: &EventSink,
    cancel: &AtomicBool,
    java: &str,
    game_dir: &Path,
    pack_url: &str,
) -> Result<(), String> {
    use std::io::BufRead;
    if launcher_core::dev::freeze_pack() {
        log::warn!(
            "dev: BLOCKFIELD_DEV_FREEZE_PACK is set, leaving {} untouched",
            game_dir.display()
        );
        emit_status(
            events,
            "modpack",
            "Dev: пак заморожен, файлы не трогаем",
            false,
        );
        return Ok(());
    }
    let tracked = launcher_net::installed_files::ExternalInstall::new(game_dir, "")?;
    let mut child = launcher_core::host_env::command(java)
        // The bootstrap normally fetches the installer from GitHub; both jars come from the pack host instead.
        .args([
            "-jar",
            BOOTSTRAP_JAR,
            "--bootstrap-no-update",
            "--bootstrap-main-jar",
            INSTALLER_JAR,
            "-g",
            "-s",
            "client",
            "--pack-folder",
            ".",
        ])
        .arg(pack_url)
        .current_dir(game_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("packwiz-installer failed to start: {e}"))?;
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    for reader in [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(reader)
                .lines()
                .map_while(Result::ok)
            {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);

    let mut tail = std::collections::VecDeque::with_capacity(20);
    let mut handle_line = |line: String| {
        log::info!("[packwiz-installer] {line}");
        if let Some((name, speed)) = installer_mirror(&line) {
            events(LauncherEvent::DownloadMirror(DownloadMirrorPayload {
                name,
                speed,
            }));
        } else if let Some((done, total)) = installer_progress(&line) {
            events(LauncherEvent::DownloadProgress(
                launcher_net::download::DownloadProgressPayload::files(line.clone(), done, total),
            ));
        } else {
            emit_status(events, "modpack", line.clone(), true);
        }
        if tail.len() == 20 {
            tail.pop_front();
        }
        tail.push_back(line);
    };
    let status = loop {
        while let Ok(line) = rx.try_recv() {
            handle_line(line);
        }
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            tracked.finish_pack();
            return Err("Download cancelled".to_string());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(e) => return Err(format!("packwiz-installer wait failed: {e}")),
        }
    };
    for line in rx {
        handle_line(line);
    }
    tracked.finish_pack();
    if !status.success() {
        return Err(format!(
            "packwiz-installer failed ({status}):\n{}",
            tail.iter().cloned().collect::<Vec<_>>().join("\n")
        ));
    }
    Ok(())
}

pub async fn verify_files(
    state: &LauncherAppState,
    profile: Option<ClientProfile>,
) -> Result<Vec<String>, String> {
    let _operation = state.begin_operation()?;
    if state.game.is_busy() {
        return Err("Close the game before verifying files".to_string());
    }
    let pack = state
        .pack
        .read()
        .await
        .clone()
        .ok_or("Check the modpack first")?;
    let config = {
        let config = state.config.read().await;
        if profile.is_some_and(|profile| profile != config.active_profile) {
            return Err("Профиль изменился. Повторите проверку файлов.".into());
        }
        config.validated_active()?
    };
    let game_dir = PathBuf::from(&config.game_dir);
    let java = resolved_java_path(&config);
    std::fs::create_dir_all(&game_dir).map_err(|e| e.to_string())?;
    let _tracking = launcher_net::installed_files::begin(&state.app_data_dir, &game_dir)?;
    ensure_jar(state, &game_dir, BOOTSTRAP_JAR, &pack.info.bootstrap).await?;
    ensure_jar(state, &game_dir, INSTALLER_JAR, &pack.info.installer).await?;
    state.downloader.reset_cancel();
    let cancel = state.cancel_flag.clone();
    let events = state.events.clone();
    tokio::task::spawn_blocking(move || {
        run_packwiz_installer(&events, &cancel, &java, &game_dir, &pack.info.pack)
    })
    .await
    .map_err(|e| format!("Verification task failed: {e}"))??;
    Ok(Vec::new())
}

pub async fn cancel_download(state: &LauncherAppState) -> Result<(), String> {
    // Use the atomic flag directly — no mutex, works even during active downloads.
    state.cancel_flag.store(true, Ordering::SeqCst);
    log::info!("Download cancel requested from frontend");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::test_state;
    use super::*;
    use launcher_core::config::LauncherConfig;

    #[test]
    fn installer_progress_lines_are_parsed() {
        assert_eq!(
            installer_progress("(12/137) Downloaded Create"),
            Some((12, 137))
        );
        assert_eq!(installer_progress("Finished successfully!"), None);
        assert_eq!(
            installer_progress("[blockfield-mirror] Gcore CDN 12.3 MB/s"),
            None
        );
    }

    #[test]
    fn installer_mirror_lines_are_parsed() {
        for (line, name, speed) in [
            (
                "[blockfield-mirror] Gcore CDN 12.3 MB/s",
                "Gcore CDN",
                Some("12.3"),
            ),
            ("[blockfield-mirror] Cloudflare R2", "Cloudflare R2", None),
            ("[blockfield-mirror] GitHub 0.0 MB/s", "GitHub", Some("0.0")),
            ("[blockfield-mirror] blockfield.pro", "blockfield.pro", None),
        ] {
            assert_eq!(
                installer_mirror(line),
                Some((name.to_string(), speed.map(str::to_string))),
                "{line}"
            );
        }
        for line in [
            "[blockfield-mirror] ",
            "Finished successfully!",
            "(12/137) Downloaded Create",
            "Mirror probe: Gcore CDN 12.3 MB/s",
            "mod.jar from Gcore CDN failed (timeout); trying the next source",
            "Mirror GitHub failed; the rest of this run skips it",
        ] {
            assert_eq!(installer_mirror(line), None, "{line}");
        }
    }

    #[test]
    fn mirror_payload_matches_todays_download_mirror_json() {
        // The Tauri sink maps DownloadMirror to
        // `json!({"name", "speed"})`; the struct must serialize byte-identical.
        let with_speed = DownloadMirrorPayload {
            name: "Gcore CDN".to_string(),
            speed: Some("12.3".to_string()),
        };
        assert_eq!(
            serde_json::to_string(&with_speed).unwrap(),
            r#"{"name":"Gcore CDN","speed":"12.3"}"#
        );
        let without_speed = DownloadMirrorPayload {
            name: "Cloudflare R2".to_string(),
            speed: None,
        };
        assert_eq!(
            serde_json::to_string(&without_speed).unwrap(),
            r#"{"name":"Cloudflare R2","speed":null}"#
        );
    }

    #[test]
    fn cancel_sets_flag_and_busy_download_fails_fast_without_network() {
        let dir = std::env::temp_dir().join(format!(
            "blockfield-install-gate-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let state = test_state(dir.clone(), LauncherConfig::default());
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                // Hold the gate: the second download must fail with today's exact
                // busy string before touching the network or disk.
                let _guard = state.operation.try_lock().expect("gate starts free");
                assert_eq!(
                    download_modpack(&state).await.unwrap_err(),
                    "Another launcher operation is in progress"
                );
                drop(_guard);
                cancel_download(&state).await.unwrap();
                assert!(state.cancel_flag.load(Ordering::SeqCst));
            });
        let _ = std::fs::remove_dir_all(dir);
    }
}
