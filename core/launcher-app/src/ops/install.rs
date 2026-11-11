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
use super::pack_check::{installed_pack, require_pack_profile, INSTALLED_VERSION_FILE};
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
    std::sync::Arc::new(
        move |phase: &'static str, message: &str, cancelable: bool| {
            events(LauncherAppState::status_event(phase, message, cancelable));
        },
    )
}

/// The pack phase already completed for this index hash AND a real hash check
/// of the installed files passes: a retry after a runtime-phase failure
/// resumes at the runtime phase. A missing or same-size-corrupted mod fails
/// the check and forces a full installer run, so it heals without a manual
/// verify. Unchanged files are not re-hashed (mtime+size cache).
pub(crate) fn pack_phase_complete(app_data: &Path, game_dir: &Path, index_hash: &str) -> bool {
    installed_pack(game_dir).0 == index_hash
        && launcher_net::installed_files::pack_files_valid(game_dir, app_data)
}

/// Blocking pack hash check, designed for `spawn_blocking`: a corrupt pack
/// must not survive a launch, so the check runs off the async worker.
async fn pack_phase_done(app_data: PathBuf, game_dir: PathBuf, index_hash: String) -> bool {
    tokio::task::spawn_blocking(move || pack_phase_complete(&app_data, &game_dir, &index_hash))
        .await
        .unwrap_or(false)
}

async fn pack_files_valid_blocking(app_data: PathBuf, game_dir: PathBuf) -> bool {
    tokio::task::spawn_blocking(move || {
        launcher_net::installed_files::pack_files_valid(&game_dir, &app_data)
    })
    .await
    .unwrap_or(false)
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
    require_pack_profile(&pack, config.active_profile)?;
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
    if pack_phase_done(
        state.app_data_dir.clone(),
        game_dir.clone(),
        pack.meta.index_hash.clone(),
    )
    .await
    {
        emit_status(
            &state.events,
            "modpack",
            "Файлы модпака уже установлены",
            false,
        );
    } else {
        ensure_jar(state, &game_dir, BOOTSTRAP_JAR, &pack.info.bootstrap).await?;
        ensure_jar(state, &game_dir, INSTALLER_JAR, &pack.info.installer).await?;
        // packwiz trusts its own cached hashes and reports "already up to
        // date" without touching a corrupted file: delete the bad files and
        // drop their index entries first, so the installer downloads them
        // again. A no-op when the check fails only on the marker. A frozen dev
        // pack is never touched, like the installer itself.
        let frozen = launcher_core::dev::freeze_pack();
        let repaired = if frozen {
            (Vec::<String>::new(), Vec::<String>::new())
        } else {
            let (repair_data, repair_dir) = (state.app_data_dir.clone(), game_dir.clone());
            tokio::task::spawn_blocking(move || {
                launcher_net::installed_files::drop_bad_pack_files(&repair_dir, &repair_data)
            })
            .await
            .map_err(|e| format!("Pack repair task failed: {e}"))?
        };
        let (repaired, repair_failed) = repaired;
        if !repair_failed.is_empty() {
            log::error!(
                "[download_modpack] pack repair could not remove {} files: {}",
                repair_failed.len(),
                repair_failed.join(", ")
            );
        }
        if !repaired.is_empty() {
            // The dropped entries leave the launch check; without the marker an
            // interrupted install cannot pass as a complete pack.
            let _ = std::fs::remove_file(game_dir.join(INSTALLED_VERSION_FILE));
            log::info!("[download_modpack] repairing {} files", repaired.len());
            emit_status(
                &state.events,
                "modpack",
                format!("Восстановление {} файлов", repaired.len()),
                true,
            );
        }
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
        // Single pass, no loop: a pack that is still bad after a repair +
        // reinstall needs eyes, not another installer run.
        if !frozen && !pack_files_valid_blocking(state.app_data_dir.clone(), game_dir.clone()).await
        {
            return Err(
                "Pack files are still damaged after the repair. Open Updates to retry, and report the failing files from the log."
                    .to_string(),
            );
        }
        emit_status(&state.events, "modpack", "Файлы модпака готовы", false);
    }

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

/// "[blockfield-file] {\"name\":\"a.jar\",\"received\":123,\"total\":456,\"source\":\"Mirror\"}"
/// → (name, received bytes, total bytes, source). JSON so spaces, quotes and Unicode survive.
#[derive(Debug, serde::Deserialize)]
struct InstallerFileLine {
    name: String,
    received: u64,
    total: Option<u64>,
    source: Option<String>,
}

fn installer_file(line: &str) -> Option<(String, u64, Option<u64>, Option<String>)> {
    let payload = line.strip_prefix("[blockfield-file] ")?.trim();
    if payload.is_empty() {
        return None;
    }
    let parsed: InstallerFileLine = serde_json::from_str(payload).ok()?;
    if parsed.name.is_empty() {
        return None;
    }
    Some((
        parsed.name,
        parsed.received,
        parsed.total,
        parsed.source.filter(|source| !source.is_empty()),
    ))
}

/// What the update screen shows as the current file while a large jar is still downloading.
/// Bytes lead so progress stays visible when the stats cell truncates a long jar name.
fn installer_file_display(
    name: &str,
    received: u64,
    total: Option<u64>,
    source: Option<&str>,
) -> String {
    let bytes = match total {
        Some(total) => format!("{:.1}/{:.1} MB", received as f64 / 1e6, total as f64 / 1e6),
        None => format!("{:.1} MB", received as f64 / 1e6),
    };
    let short = if name.chars().count() > 44 {
        format!("{}…", name.chars().take(43).collect::<String>())
    } else {
        name.to_string()
    };
    match source {
        Some(source) => format!("{bytes} - {short} - {source}"),
        None => format!("{bytes} - {short}"),
    }
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
    let mut last_files: Option<(usize, usize)> = None;
    let mut handle_line = |line: String| {
        log::info!("[packwiz-installer] {line}");
        if let Some((name, speed)) = installer_mirror(&line) {
            events(LauncherEvent::DownloadMirror(DownloadMirrorPayload {
                name,
                speed,
            }));
        } else if let Some((done, total)) = installer_progress(&line) {
            last_files = Some((done, total));
            events(LauncherEvent::DownloadProgress(
                launcher_net::download::DownloadProgressPayload::files(line.clone(), done, total),
            ));
        } else if let Some((name, received, total, source)) = installer_file(&line) {
            // A large file is still downloading: keep the last file count, show the live file.
            let (done, total_files) = last_files.unwrap_or((0, 0));
            events(LauncherEvent::DownloadProgress(
                launcher_net::download::DownloadProgressPayload::files(
                    installer_file_display(&name, received, total, source.as_deref()),
                    done,
                    total_files,
                ),
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
    require_pack_profile(&pack, config.active_profile)?;
    let game_dir = PathBuf::from(&config.game_dir);
    let java = resolved_java_path(&config);
    std::fs::create_dir_all(&game_dir).map_err(|e| e.to_string())?;
    let _tracking = launcher_net::installed_files::begin(&state.app_data_dir, &game_dir)?;
    ensure_jar(state, &game_dir, BOOTSTRAP_JAR, &pack.info.bootstrap).await?;
    ensure_jar(state, &game_dir, INSTALLER_JAR, &pack.info.installer).await?;
    state.downloader.reset_cancel();
    let cancel = state.cancel_flag.clone();
    let events = state.events.clone();
    let java_for_verify = java.clone();
    let dir = game_dir.clone();
    let pack_url = pack.info.pack.clone();
    tokio::task::spawn_blocking(move || {
        run_packwiz_installer(&events, &cancel, &java_for_verify, &dir, &pack_url)
    })
    .await
    .map_err(|e| format!("Verification task failed: {e}"))??;
    // Runtime files were never re-checked: a same-size corrupt jar passed every
    // size probe while the game crashed at launch. Heal the manifests first so
    // the hash checks trust fresh metadata, then delete the corrupt files so
    // the dependency pass below re-downloads them against the metadata hashes.
    // The hash scan is blocking file IO: it runs on the blocking pool.
    let loader = pack.meta.loader.clone();
    if let Some(loader) = &loader {
        launcher_net::minecraft::ensure_loader_profile(&game_dir, &pack.meta.minecraft, loader)
            .await?;
    }
    launcher_net::minecraft::heal_runtime_manifests(&game_dir, &pack.meta.minecraft).await?;
    let loader_id = loader
        .as_ref()
        .map(|loader| launcher_net::minecraft::loader_version_id(&pack.meta.minecraft, loader));
    let loader_id = loader_id
        .as_deref()
        .filter(|id| launcher_net::minecraft::loader_profile_exists(&game_dir, id));
    let minecraft = pack.meta.minecraft.clone();
    let loader_id_owned = loader_id.map(String::from);
    let dir = game_dir.clone();
    let corrupt = tokio::task::spawn_blocking(move || {
        launcher_net::minecraft::heal_corrupt_runtime_files(
            &dir,
            &minecraft,
            loader_id_owned.as_deref(),
        )
    })
    .await
    .map_err(|e| format!("Verification task failed: {e}"))??;
    if !corrupt.is_empty()
        || !launcher_net::minecraft::has_launch_dependencies(
            &game_dir,
            &pack.meta.minecraft,
            loader_id,
        )
    {
        let runtime_status = minecraft_status_sink(&state.events);
        launcher_net::minecraft::ensure_launch_dependencies(
            &state.downloader,
            &runtime_status,
            &game_dir,
            &pack.meta.minecraft,
            loader_id,
        )
        .await?;
    }
    Ok(corrupt
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect())
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
        assert_eq!(
            installer_progress("[blockfield-file] {\"name\":\"a.jar\"}"),
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
    fn installer_file_lines_are_parsed() {
        for (line, expected) in [
            (
                "[blockfield-file] {\"name\":\"create.jar\",\"received\":131072,\"total\":22105900,\"source\":\"Gcore CDN\"}",
                Some((
                    "create.jar".to_string(),
                    131072,
                    Some(22105900),
                    Some("Gcore CDN".to_string()),
                )),
            ),
            (
                // Spaces, quotes and delimiter-like text survive the JSON format.
                "[blockfield-file] {\"name\":\"My \\\"Mod\\\" 1/2.jar\",\"received\":0,\"total\":null,\"source\":\"pack host\"}",
                Some((
                    "My \"Mod\" 1/2.jar".to_string(),
                    0,
                    None,
                    Some("pack host".to_string()),
                )),
            ),
            (
                "[blockfield-file] {\"name\":\"Мод.jar\",\"received\":1,\"total\":2,\"source\":\"Cloudflare R2\"}",
                Some((
                    "Мод.jar".to_string(),
                    1,
                    Some(2),
                    Some("Cloudflare R2".to_string()),
                )),
            ),
            (
                "[blockfield-file] {\"name\":\"a.jar\",\"received\":1,\"total\":2}",
                Some(("a.jar".to_string(), 1, Some(2), None)),
            ),
        ] {
            assert_eq!(installer_file(line), expected, "{line}");
        }
        for line in [
            "[blockfield-file] ",
            "[blockfield-file] not json",
            "[blockfield-file] {\"name\":\"\",\"received\":1,\"total\":2}",
            "[blockfield-file] {\"name\":\"a.jar\",\"received\":\"x\",\"total\":2}",
            // The previous space-separated format is no longer produced; it must not parse.
            "[blockfield-file] create.jar 131072/22105900 Gcore CDN",
            "Finished successfully!",
            "(12/137) Downloaded Create",
            "[blockfield-mirror] Gcore CDN 12.3 MB/s",
        ] {
            assert_eq!(installer_file(line), None, "{line}");
        }
    }

    #[test]
    fn installer_file_display_keeps_bytes_visible() {
        // Bytes lead so a truncated stats cell still shows progress.
        assert_eq!(
            installer_file_display(
                "blockfield-client-1.56.0+mc1.21.1.jar",
                12300000,
                Some(40088921),
                Some("Gcore CDN")
            ),
            "12.3/40.1 MB - blockfield-client-1.56.0+mc1.21.1.jar - Gcore CDN"
        );
        assert_eq!(
            installer_file_display(
                "create-fabric-6.0.0.0+mc1.21.1-bf6.jar",
                0,
                Some(22105900),
                Some("pack host")
            ),
            "0.0/22.1 MB - create-fabric-6.0.0.0+mc1.21.1-bf6.jar - pack host"
        );
        // Long names truncate; the byte count stays at the front.
        let long = installer_file_display(
            "a-quite-long-mod-name-that-keeps-going-forever.jar",
            1000000,
            None,
            None,
        );
        assert!(long.starts_with("1.0 MB - "), "{long}");
        assert!(long.ends_with("…"), "{long}");
        assert_eq!(long.chars().count(), "1.0 MB - ".len() + 44);
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
    fn pack_phase_skips_only_when_marker_current_and_files_verify() {
        let root = std::env::temp_dir().join(format!(
            "blockfield-pack-phase-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let data = root.join("app-data");
        let game = root.join("game");
        std::fs::create_dir_all(game.join("mods")).unwrap();
        // No marker: full installer run.
        assert!(!pack_phase_complete(&data, &game, "abc"));
        std::fs::write(game.join(INSTALLED_VERSION_FILE), "abc\n1.0\n").unwrap();
        // Marker current but no installer cache: full run.
        assert!(!pack_phase_complete(&data, &game, "abc"));
        let good = game.join("mods/a.jar");
        std::fs::write(&good, b"correct-bytes!").unwrap();
        std::fs::write(
            game.join("packwiz.json"),
            serde_json::to_vec(&serde_json::json!({ "cachedFiles": {
                "a": {
                    "cachedLocation": "mods/a.jar",
                    "hash": {
                        "type": "sha256",
                        "value": Downloader::sha256_file(&good).unwrap()
                    }
                }
            }}))
            .unwrap(),
        )
        .unwrap();
        assert!(pack_phase_complete(&data, &game, "abc"));
        // Unchanged files are stats-only on repeat checks (mtime+size cache).
        assert!(pack_phase_complete(&data, &game, "abc"));
        // Drop the cache so the corruption step is a fresh hash, not a race
        // with filesystem timestamp granularity on same-size rewrites.
        std::fs::remove_dir_all(&data).unwrap();
        // A same-size corrupted mod forces a full run so it heals.
        std::fs::write(&good, b"corrupt-bytes!").unwrap();
        assert!(!pack_phase_complete(&data, &game, "abc"));
        // Stale marker: full run even with verified files.
        std::fs::write(&good, b"correct-bytes!").unwrap();
        assert!(!pack_phase_complete(&data, &game, "other"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn blocking_pack_check_reports_corrupt_files() {
        // Covers the post-repair gate in download_modpack without a network run.
        let root = std::env::temp_dir().join(format!(
            "blockfield-pack-blocking-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let data = root.join("app-data");
        let game = root.join("game");
        std::fs::create_dir_all(game.join("mods")).unwrap();
        let file = game.join("mods/a.jar");
        std::fs::write(&file, b"correct-bytes!").unwrap();
        std::fs::write(
            game.join("packwiz.json"),
            serde_json::to_vec(&serde_json::json!({ "cachedFiles": {
                "a": {
                    "cachedLocation": "mods/a.jar",
                    "hash": {
                        "type": "sha256",
                        "value": Downloader::sha256_file(&file).unwrap()
                    }
                }
            }}))
            .unwrap(),
        )
        .unwrap();
        let run = |data: PathBuf, game: PathBuf| pack_files_valid_blocking(data, game);
        let block_on = |future| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(future)
        };
        assert!(block_on(run(data.clone(), game.clone())));
        // Same-size corruption is still caught after the mtime moves, as a
        // real edit later in time would.
        std::fs::write(&file, b"corrupt-bytes!").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(60))
            .unwrap();
        assert!(!block_on(run(data.clone(), game.clone())));
        std::fs::remove_file(&file).unwrap();
        assert!(!block_on(run(data.clone(), game.clone())));
        let _ = std::fs::remove_dir_all(root);
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
