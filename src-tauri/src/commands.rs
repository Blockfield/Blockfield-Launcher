use crate::config::{ClientProfile, LauncherConfig};
use crate::download::Downloader;
use crate::pack::{self, JavaInfo};
use crate::status::ServerStatus;
// Moved to launcher-app (phase 2, no behaviour change); re-export keeps every
// `crate::commands::…` call site unchanged.
use launcher_app::ops::java::{
    find_bin_java, java_runtime_ready, resolved_java_path, system_java_available,
};
use launcher_app::ops::pack_check::{pack_base_url, INSTALLED_VERSION_FILE};
pub use launcher_app::ops::settings::SettingsResponse;
use launcher_app::EventSink;
pub use launcher_app::{GameIdentity, LauncherAppState, VersionCheckResult};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, State};

const INSTALLER_JAR: &str = "packwiz-installer.jar";
const BOOTSTRAP_JAR: &str = "packwiz-installer-bootstrap.jar";

fn emit_status(
    events: &EventSink,
    phase: &'static str,
    message: impl Into<String>,
    cancelable: bool,
) {
    events(LauncherAppState::status_event(phase, message, cancelable));
}

/// Status sink for the core minecraft runtime: produces exactly today's
/// `launcher://status` payload (`{"phase", "message", "cancelable"}`).
fn minecraft_status_sink(app_handle: &AppHandle) -> crate::minecraft::StatusSink {
    let app_handle = app_handle.clone();
    std::sync::Arc::new(move |phase: &str, message: &str, cancelable: bool| {
        let _ = app_handle.emit(
            "launcher://status",
            serde_json::json!({
                "phase": phase,
                "message": message,
                "cancelable": cancelable,
            }),
        );
    })
}

// ── Settings commands ──────────────────────────────────────────

#[tauri::command]
pub fn load_settings(state: State<'_, LauncherAppState>) -> Result<SettingsResponse, String> {
    launcher_app::ops::settings::load_settings(&state)
}

#[tauri::command]
pub async fn select_profile(
    profile: ClientProfile,
    state: State<'_, LauncherAppState>,
) -> Result<SettingsResponse, String> {
    launcher_app::ops::settings::select_profile(&state, profile).await
}

#[tauri::command]
pub fn save_settings(
    app_handle: AppHandle,
    state: State<'_, LauncherAppState>,
    config: LauncherConfig,
) -> Result<(), String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data dir: {e}"))?;
    launcher_app::ops::settings::save_settings(&state, &app_data_dir, config)
}

// ── Modpack commands ───────────────────────────────────────────

#[tauri::command]
pub async fn check_modpack_version(
    state: State<'_, LauncherAppState>,
) -> Result<VersionCheckResult, String> {
    launcher_app::ops::pack_check::check_modpack_version(&state, |game_dir, meta| {
        match &meta.loader {
            Some(loader) => {
                let id = crate::minecraft::loader_version_id(&meta.minecraft, loader);
                crate::minecraft::loader_profile_exists(game_dir, &id)
                    && crate::minecraft::has_launch_dependencies(
                        game_dir,
                        &meta.minecraft,
                        Some(&id),
                    )
            }
            None => crate::minecraft::has_launch_dependencies(game_dir, &meta.minecraft, None),
        }
    })
    .await
}

#[tauri::command]
pub async fn download_modpack(
    app_handle: AppHandle,
    state: State<'_, LauncherAppState>,
) -> Result<(), String> {
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
    let _tracking = crate::installed_files::begin(&state.app_data_dir, &game_dir)?;

    state.downloader.reset_cancel();
    crate::download::cleanup_partials(&game_dir);
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
            let java_exe = install_java(&state, &game_dir, &java).await?;
            let java_path = java_exe.to_string_lossy().to_string();
            let mut cfg = state.config.write().await;
            match cfg.active_profile {
                ClientProfile::Game => cfg.java_path = java_path.clone(),
                ClientProfile::Workshop => cfg.workshop_java_path = java_path.clone(),
            }
            config.java_path = java_path.clone();
            let _ = crate::config::save_config(&state.app_data_dir, &cfg);
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
            crate::minecraft::ensure_loader_profile(&game_dir, &pack.meta.minecraft, loader)
                .await?;
            emit_status(&state.events, "loader", "Fabric установлен", false);
            Some(crate::minecraft::loader_version_id(
                &pack.meta.minecraft,
                loader,
            ))
        }
        None => None,
    };

    emit_status(&state.events, "modpack", "Обновление файлов модпака", true);
    ensure_jar(&state, &game_dir, BOOTSTRAP_JAR, &pack.info.bootstrap).await?;
    ensure_jar(&state, &game_dir, INSTALLER_JAR, &pack.info.installer).await?;
    let cancel = state.cancel_flag.clone();
    let app = app_handle.clone();
    let events = state.events.clone();
    let (java_for_installer, dir, pack_url) =
        (java.clone(), game_dir.clone(), pack.info.pack.clone());
    tokio::task::spawn_blocking(move || {
        run_packwiz_installer(&app, events, &cancel, &java_for_installer, &dir, &pack_url)
    })
    .await
    .map_err(|e| format!("packwiz-installer task failed: {e}"))??;
    crate::installed_files::write(
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
    let runtime_bytes = crate::minecraft::prepare_download_plan(
        &game_dir,
        &pack.meta.minecraft,
        loader_version_id.as_deref(),
    )
    .await?;
    state.downloader.set_grand_total(runtime_bytes);
    let runtime_status = minecraft_status_sink(&app_handle);
    crate::minecraft::ensure_launch_dependencies(
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
    state: &State<'_, LauncherAppState>,
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
    let partial = crate::download::partial_path(&archive_path);
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
    crate::download::activate_partial(&partial, &archive_path)
        .map_err(|e| format!("Failed to activate Java archive: {e}"))?;
    emit_status(&state.events, "java", "Распаковка Java", false);
    let java_exe = install_java_archive(&archive_path, game_dir, &java.version);
    let _ = std::fs::remove_file(&archive_path);
    java_exe
}

async fn ensure_jar(
    state: &State<'_, LauncherAppState>,
    game_dir: &Path,
    name: &str,
    artifact: &pack::Artifact,
) -> Result<(), String> {
    let jar = game_dir.join(name);
    if matches!(Downloader::sha256_file(&jar), Ok(hash) if hash.eq_ignore_ascii_case(&artifact.sha256))
    {
        return Ok(());
    }
    let partial = crate::download::partial_path(&jar);
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
    crate::download::activate_partial(&partial, &jar).map_err(|e| e.to_string())
}

/// "(12/137) Downloaded X" → (12, 137)
fn installer_progress(line: &str) -> Option<(usize, usize)> {
    let inner = line.strip_prefix('(')?.split_once(')')?.0;
    let (done, total) = inner.split_once('/')?;
    Some((done.parse().ok()?, total.parse().ok()?))
}

fn installer_mirror(line: &str) -> Option<(String, Option<String>)> {
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
    app_handle: &AppHandle,
    events: EventSink,
    cancel: &AtomicBool,
    java: &str,
    game_dir: &Path,
    pack_url: &str,
) -> Result<(), String> {
    use std::io::BufRead;
    if crate::dev::freeze_pack() {
        log::warn!(
            "dev: BLOCKFIELD_DEV_FREEZE_PACK is set, leaving {} untouched",
            game_dir.display()
        );
        emit_status(
            &events,
            "modpack",
            "Dev: пак заморожен, файлы не трогаем",
            false,
        );
        return Ok(());
    }
    let tracked = crate::installed_files::ExternalInstall::new(game_dir, "")?;
    let mut child = crate::host_env::command(java)
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
            let _ = app_handle.emit(
                "download://mirror",
                serde_json::json!({ "name": name, "speed": speed }),
            );
        } else if let Some((done, total)) = installer_progress(&line) {
            let _ = app_handle.emit(
                "download://progress",
                crate::download::DownloadProgressPayload::files(line.clone(), done, total),
            );
        } else {
            emit_status(&events, "modpack", line.clone(), true);
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

#[tauri::command]
pub async fn verify_files(
    app_handle: AppHandle,
    state: State<'_, LauncherAppState>,
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
    let _tracking = crate::installed_files::begin(&state.app_data_dir, &game_dir)?;
    ensure_jar(&state, &game_dir, BOOTSTRAP_JAR, &pack.info.bootstrap).await?;
    ensure_jar(&state, &game_dir, INSTALLER_JAR, &pack.info.installer).await?;
    state.downloader.reset_cancel();
    let cancel = state.cancel_flag.clone();
    let events = state.events.clone();
    tokio::task::spawn_blocking(move || {
        run_packwiz_installer(
            &app_handle,
            events,
            &cancel,
            &java,
            &game_dir,
            &pack.info.pack,
        )
    })
    .await
    .map_err(|e| format!("Verification task failed: {e}"))??;
    Ok(Vec::new())
}

#[tauri::command]
pub async fn cancel_download(state: State<'_, LauncherAppState>) -> Result<(), String> {
    // Use the atomic flag directly — no mutex, works even during active downloads.
    state.cancel_flag.store(true, Ordering::SeqCst);
    log::info!("Download cancel requested from frontend");
    Ok(())
}

#[tauri::command]
pub async fn server_status(state: State<'_, LauncherAppState>) -> Result<ServerStatus, String> {
    launcher_app::status_ops::server_status(&state, &pack_base_url()?).await
}

type StatsQuery = std::collections::BTreeMap<String, String>;

#[tauri::command]
pub async fn stats_leaderboard(query: StatsQuery) -> Result<Option<serde_json::Value>, String> {
    launcher_app::status_ops::stats_leaderboard(&pack_base_url()?, &query).await
}

#[tauri::command]
pub async fn stats_seasons() -> Result<Option<serde_json::Value>, String> {
    launcher_app::status_ops::stats_seasons(&pack_base_url()?).await
}

#[tauri::command]
pub async fn stats_player(
    uuid: String,
    query: StatsQuery,
) -> Result<Option<serde_json::Value>, String> {
    launcher_app::status_ops::stats_player(&pack_base_url()?, &uuid, &query).await
}

#[tauri::command]
pub fn game_status(state: State<'_, LauncherAppState>) -> crate::game::GameStatus {
    launcher_app::status_ops::game_status(&state)
}

/// Opens a link in the user's browser. The URL can come from remote launcher content, so only
/// plain https links are accepted — never a local path, a `file:`/`javascript:` URL or an option.
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    launcher_app::status_ops::open_url(&url)
}

/// Server defaults for a first launch. Minecraft fills in every other option itself and rewrites
/// the whole file on exit, so a partial file is enough. An existing file is the player's own
/// settings and is never touched — and the modpack cannot carry these, the pack host treats
/// options.txt as a private file.
fn seed_game_options(game_dir: &Path) -> std::io::Result<()> {
    let path = game_dir.join("options.txt");
    if path.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(game_dir)?;
    std::fs::write(
        path,
        "fullscreen:true\nrenderDistance:20\nsimulationDistance:20\n",
    )
}

// ── Game launch command ────────────────────────────────────────

#[tauri::command]
pub async fn launch_game(
    app_handle: AppHandle,
    state: State<'_, LauncherAppState>,
) -> Result<(), String> {
    let _operation = state.begin_operation()?;
    let running = state.game.begin()?;
    let config = state.config.read().await.validated_active()?;
    let identity = crate::account::game_identity(&app_handle, &state).await?;
    let pack = state.pack.read().await.clone();

    let java = resolved_java_path(&config);
    if java == "java" && !system_java_available() {
        return Err(
            "Java runtime is not installed. Open Updates to install it or select a Java executable in Settings."
                .to_string(),
        );
    }

    let ram_mb = config.ram_mb;
    let game_dir = config.game_dir.clone();
    let game_dir_path = PathBuf::from(&game_dir);
    std::fs::create_dir_all(&game_dir_path).map_err(|e| e.to_string())?;
    let _tracking = crate::installed_files::begin(&state.app_data_dir, &game_dir_path)?;

    let loader_version_id = pack
        .as_ref()
        .and_then(|p| {
            p.meta
                .loader
                .as_deref()
                .map(|loader| crate::minecraft::loader_version_id(&p.meta.minecraft, loader))
        })
        .or_else(|| crate::minecraft::find_loader_version_id(&game_dir_path));
    let minecraft_version = pack
        .as_ref()
        .map(|p| p.meta.minecraft.clone())
        .or_else(|| {
            loader_version_id.as_ref().and_then(|id| {
                id.strip_prefix("fabric-loader-")?
                    .split_once('-')
                    .map(|(_, mc)| mc.to_string())
            })
        })
        .unwrap_or_else(|| {
            std::env::var("BLOCKFIELD_MINECRAFT_VERSION")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .or_else(|| option_env!("BLOCKFIELD_MINECRAFT_VERSION").map(String::from))
                .unwrap_or_else(|| "1.21.1".to_string())
        });
    let game_server = match &pack {
        Some(pack) => pack.info.server.clone(),
        None if config.active_profile == ClientProfile::Workshop => {
            pack::fetch_launcher_info(&pack_base_url()?).await?.server
        }
        None => String::new(),
    };
    let quick_play = config.target(&game_server)?;
    // Dev build: another server, or none at all so the game opens on the title screen.
    let quick_play = crate::dev::server_override().unwrap_or(quick_play);

    log::info!(
        "Launching game: java={java}, ram={ram_mb}MB, dir={game_dir}, user={}",
        identity.username
    );

    {
        state.downloader.reset_cancel();
        // Floor so the frontend always has a non-zero denominator for progress.
        state.downloader.set_grand_total(16 * 1024 * 1024);
        emit_status(&state.events, "launch", "Проверка компонентов игры", false);
        let launch_status = minecraft_status_sink(&app_handle);
        crate::minecraft::ensure_launch_dependencies(
            &state.downloader,
            &launch_status,
            &game_dir_path,
            &minecraft_version,
            loader_version_id.as_deref(),
        )
        .await?;
    }

    if let Err(error) = seed_game_options(&game_dir_path) {
        log::warn!("Cannot write default game options: {error}");
    }

    emit_status(&state.events, "launch", "Подготовка запуска", false);
    let args = crate::minecraft::build_launch_args(
        &game_dir_path,
        ram_mb,
        &minecraft_version,
        loader_version_id.as_deref(),
        &identity,
        quick_play.as_deref(),
        // Launcher crate version, not launcher-net's internal 0.0.0.
        env!("CARGO_PKG_VERSION"),
    )?;

    if !config.pre_launch_command.trim().is_empty() {
        emit_status(
            &state.events,
            "launch",
            "Выполнение команды перед запуском",
            false,
        );
        let hook_config = config.clone();
        let hook_java = java.clone();
        tokio::task::spawn_blocking(move || {
            crate::launch_hooks::run_hook(
                &hook_config.pre_launch_command,
                &hook_config,
                &hook_java,
                None,
            )
        })
        .await
        .map_err(|e| format!("Pre-launch task failed: {e}"))??;
    }
    emit_status(&state.events, "launch", "Запуск игры", false);
    let log_dir = game_dir_path.join("logs");
    std::fs::create_dir_all(&log_dir).map_err(|e| format!("Cannot create log directory: {e}"))?;
    let output = std::fs::File::create(log_dir.join("launcher-game.log"))
        .map_err(|e| format!("Cannot create game log: {e}"))?;
    let pending_room = app_handle
        .state::<crate::rooms::PendingRoom>()
        .0
        .lock()
        .unwrap()
        .clone();
    let pending_room = pending_room.filter(|_| config.active_profile == ClientProfile::Game);
    let bridge_dir = config.bridge_dir(&state.app_data_dir);
    std::fs::create_dir_all(&bridge_dir).map_err(|e| e.to_string())?;
    if let Some(id) = pending_room.as_ref() {
        crate::rooms::write_request(&state, id)?;
    } else {
        let _ = std::fs::remove_file(bridge_dir.join("room-request.json"));
    }
    let _ = std::fs::remove_file(bridge_dir.join("game-presence.json"));
    let mut child = crate::host_env::command(&java)
        .env("BLOCKFIELD_BRIDGE_DIR", &bridge_dir)
        .env("BLOCKFIELD_SERVER", quick_play.as_deref().unwrap_or(""))
        .args(&args)
        .current_dir(&game_dir)
        .stdin(std::process::Stdio::null())
        .stdout(output.try_clone().map_err(|e| e.to_string())?)
        .stderr(output)
        .spawn()
        .map_err(|e| format!("Failed to spawn game process: {e}"))?;
    running.started();
    {
        let pending = app_handle.state::<crate::rooms::PendingRoom>();
        let mut value = pending.0.lock().unwrap();
        if *value == pending_room {
            *value = None;
        }
    }
    if config.hide_while_playing {
        if let Some(window) = app_handle.get_webview_window("main") {
            if let Err(error) = window.hide() {
                log::warn!("Cannot hide launcher: {error}");
            }
        }
    }
    log::info!("Game process spawned (pid {})", child.id());
    emit_status(&state.events, "launch", "Игра запущена", false);
    let events = state.events.clone();
    std::thread::spawn(move || {
        let _running = running;
        let exit = child.wait();
        _running.finishing();
        crate::game::restore_hidden_launcher(&app_handle);
        match exit {
            Ok(exit) => {
                let result = crate::launch_hooks::run_hook(
                    &config.post_exit_command,
                    &config,
                    &java,
                    exit.code(),
                );
                let message = match result {
                    Ok(()) => format!("Game exited ({exit})"),
                    Err(error) => {
                        emit_status(&events, "hook-error", &error, false);
                        format!("Post-exit command failed: {error}")
                    }
                };
                emit_status(&events, "exited", message, false);
            }
            Err(error) => emit_status(
                &events,
                "exited",
                format!("Failed to wait for game: {error}"),
                false,
            ),
        }
    });
    Ok(())
}

/// Dev-build overrides in effect, so the developer can see what the launcher will actually do.
/// Always `None` in a production build.
#[tauri::command]
pub fn dev_status() -> Option<crate::dev::DevStatus> {
    launcher_app::status_ops::dev_status(&pack_base_url().unwrap_or_default())
}

// ── Helpers ────────────────────────────────────────────────────

fn safe_relative_path(value: &str) -> Result<PathBuf, String> {
    if value.is_empty() || value.starts_with(['/', '\\']) || value.contains('\\') {
        return Err("path must be a non-empty relative path using '/' separators".to_string());
    }

    let mut path = PathBuf::new();
    for part in value.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains(':') {
            return Err("path contains an unsafe component".to_string());
        }
        path.push(part);
    }
    Ok(path)
}

fn install_java_archive(
    archive_path: &Path,
    game_dir: &Path,
    version: &str,
) -> Result<PathBuf, String> {
    let tracked = crate::installed_files::ExternalInstall::new(game_dir, "java")?;
    let java_dir = game_dir.join("java");
    let staging = game_dir.join(".java-staging");
    let backup = game_dir.join(".java-backup");
    remove_path(&staging)?;
    remove_path(&backup)?;
    std::fs::create_dir_all(&staging)
        .map_err(|e| format!("Failed to create Java staging directory: {e}"))?;

    if let Err(error) = extract_archive(archive_path, &staging) {
        let _ = remove_path(&staging);
        return Err(error);
    }
    if find_bin_java(&staging).is_none() {
        let _ = remove_path(&staging);
        return Err("Java archive does not contain a bin/java executable".to_string());
    }
    std::fs::write(staging.join(".version"), version)
        .map_err(|e| format!("Failed to stage Java version: {e}"))?;

    if java_dir.exists() {
        std::fs::rename(&java_dir, &backup)
            .map_err(|e| format!("Failed to preserve previous Java runtime: {e}"))?;
    }
    if let Err(error) = std::fs::rename(&staging, &java_dir) {
        if backup.exists() {
            let _ = std::fs::rename(&backup, &java_dir);
        }
        return Err(format!("Failed to activate Java runtime: {error}"));
    }
    remove_path(&backup)?;
    tracked.finish_tree(&java_dir)?;
    find_bin_java(&java_dir)
        .ok_or_else(|| "Activated Java runtime is missing its executable".to_string())
}

fn remove_path(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|e| format!("Failed to inspect {}: {e}", path.display()))?;
    if metadata.file_type().is_symlink() {
        std::fs::remove_file(path)
    } else if metadata.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
    .map_err(|e| format!("Failed to remove {}: {e}", path.display()))
}

/// Extract a Java runtime archive: ZIP (Windows builds) or tar.gz (Linux/macOS Temurin).
fn extract_archive(archive_path: &Path, dest_dir: &Path) -> Result<(), String> {
    if archive_path.to_string_lossy().ends_with(".tar.gz") {
        return extract_tar_gz(archive_path, dest_dir);
    }
    extract_archive_with_limits(archive_path, dest_dir, &archive_limits())
}

/// System `tar` keeps the executable bits; it exists on every Linux/macOS install.
fn extract_tar_gz(archive_path: &Path, dest_dir: &Path) -> Result<(), String> {
    let output = crate::host_env::command("tar")
        .arg("-xzf")
        .arg(archive_path)
        .arg("-C")
        .arg(dest_dir)
        .output()
        .map_err(|e| format!("Failed to run tar: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "tar failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn extract_archive_with_limits(
    archive_path: &Path,
    dest_dir: &Path,
    limits: &blockfield_shared::ArchiveLimits,
) -> Result<(), String> {
    let compressed_bytes = std::fs::metadata(archive_path)
        .map_err(|e| format!("Failed to inspect archive: {e}"))?
        .len();
    if compressed_bytes == 0 || compressed_bytes > limits.max_compressed_bytes {
        return Err("Java archive is empty or exceeds the compressed-size limit".to_string());
    }
    let file =
        std::fs::File::open(archive_path).map_err(|e| format!("Failed to open archive: {e}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Failed to read archive: {e}"))?;

    if archive.len() > limits.max_entries {
        return Err("Java archive contains too many entries".to_string());
    }
    let mut declared_total = 0u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| format!("Archive entry {index} error: {e}"))?;
        let name = entry
            .name()
            .map_err(|e| format!("Archive entry {index} name error: {e}"))?;
        if entry.encrypted()
            || !matches!(
                entry.compression(),
                zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
            )
        {
            return Err(format!("Unsupported archive entry: {name}"));
        }
        if name.split('/').count() > limits.max_path_depth {
            return Err(format!("Archive path is too deep: {name}"));
        }
        if entry.size() > limits.max_entry_bytes {
            return Err(format!("Archive entry is too large: {name}"));
        }
        if entry.size() > 0
            && (entry.compressed_size() == 0
                || entry.size()
                    > entry
                        .compressed_size()
                        .saturating_mul(limits.max_compression_ratio))
        {
            return Err(format!("Suspicious compression ratio: {name}"));
        }
        declared_total = declared_total
            .checked_add(entry.size())
            .filter(|total| *total <= limits.max_uncompressed_bytes)
            .ok_or_else(|| "Java archive exceeds the uncompressed-size limit".to_string())?;
    }
    let required_space = declared_total.saturating_add(limits.min_free_space_bytes);
    if fs2::available_space(dest_dir).map_err(|e| format!("Failed to inspect free space: {e}"))?
        < required_space
    {
        return Err("Not enough free space to extract the Java archive safely".to_string());
    }

    let mut actual_total = 0u64;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Archive entry {i} error: {e}"))?;

        let name = entry
            .name()
            .map_err(|e| format!("Archive entry {i} name error: {e}"))?
            .into_owned();
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(format!("Archive symlink rejected: {name}"));
        }
        if entry.is_dir() {
            continue;
        }
        safe_relative_path(&name).map_err(|_| format!("Unsafe archive entry rejected: {name}"))?;

        // Strip top-level directory (e.g. "jdk-21.0.5+11/" → "")
        let relative = if let Some(pos) = name.find('/') {
            &name[pos + 1..]
        } else {
            &name
        };

        if relative.is_empty() {
            continue;
        }

        let relative = safe_relative_path(relative)
            .map_err(|_| format!("Unsafe archive entry rejected: {name}"))?;
        let dest = dest_dir.join(relative);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create dir {}: {e}", parent.display()))?;
        }

        let mut out = std::fs::File::create(&dest)
            .map_err(|e| format!("Failed to create {}: {e}", dest.display()))?;
        let expected = entry.size();
        let copied = std::io::copy(&mut entry, &mut out)
            .map_err(|e| format!("Failed to extract {}: {e}", name))?;
        actual_total = actual_total
            .checked_add(copied)
            .filter(|total| *total <= declared_total)
            .ok_or_else(|| "Java archive produced more data than declared".to_string())?;
        if copied != expected {
            return Err(format!("Archive entry size mismatch: {name}"));
        }
    }

    log::info!(
        "Extracted {} entries to {}",
        archive.len(),
        dest_dir.display()
    );
    Ok(())
}

fn archive_limits() -> blockfield_shared::ArchiveLimits {
    fn limit(name: &str, compiled: Option<&str>, default: u64) -> u64 {
        std::env::var(name)
            .ok()
            .or_else(|| compiled.map(str::to_string))
            .and_then(|value| value.parse().ok())
            .unwrap_or(default)
    }

    let mut limits = blockfield_shared::ArchiveLimits::default();
    limits.max_compressed_bytes = limit(
        "BLOCKFIELD_MAX_ARCHIVE_BYTES",
        option_env!("BLOCKFIELD_MAX_ARCHIVE_BYTES"),
        limits.max_compressed_bytes,
    );
    limits.max_uncompressed_bytes = limit(
        "BLOCKFIELD_MAX_EXTRACTED_BYTES",
        option_env!("BLOCKFIELD_MAX_EXTRACTED_BYTES"),
        limits.max_uncompressed_bytes,
    );
    limits.max_entry_bytes = limit(
        "BLOCKFIELD_MAX_ARCHIVE_ENTRY_BYTES",
        option_env!("BLOCKFIELD_MAX_ARCHIVE_ENTRY_BYTES"),
        limits.max_entry_bytes,
    );
    limits.max_entries = limit(
        "BLOCKFIELD_MAX_ARCHIVE_ENTRIES",
        option_env!("BLOCKFIELD_MAX_ARCHIVE_ENTRIES"),
        limits.max_entries as u64,
    ) as usize;
    limits.max_compression_ratio = limit(
        "BLOCKFIELD_MAX_ARCHIVE_RATIO",
        option_env!("BLOCKFIELD_MAX_ARCHIVE_RATIO"),
        limits.max_compression_ratio,
    );
    limits.max_path_depth = limit(
        "BLOCKFIELD_MAX_ARCHIVE_DEPTH",
        option_env!("BLOCKFIELD_MAX_ARCHIVE_DEPTH"),
        limits.max_path_depth as u64,
    ) as usize;
    limits.min_free_space_bytes = limit(
        "BLOCKFIELD_MIN_FREE_SPACE_BYTES",
        option_env!("BLOCKFIELD_MIN_FREE_SPACE_BYTES"),
        limits.min_free_space_bytes,
    );
    limits
}

#[cfg(test)]
mod tests {
    use super::*;
    use launcher_app::ops::java::java_exe_name;
    use std::io::Write;

    #[test]
    fn game_options_are_seeded_once_and_never_overwrite_the_player() {
        let dir = std::env::temp_dir().join(format!("blockfield-options-{}", std::process::id()));
        let options = dir.join("options.txt");

        seed_game_options(&dir).unwrap();
        let seeded = std::fs::read_to_string(&options).unwrap();
        assert!(seeded.contains("fullscreen:true"));
        assert!(seeded.contains("renderDistance:20"));
        assert!(seeded.contains("simulationDistance:20"));

        std::fs::write(&options, "renderDistance:8\n").unwrap();
        seed_game_options(&dir).unwrap();
        assert_eq!(
            std::fs::read_to_string(&options).unwrap(),
            "renderDistance:8\n"
        );

        std::fs::remove_dir_all(dir).unwrap();
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        for (name, contents) in entries {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(contents).unwrap();
        }
        writer.finish().unwrap();
    }

    fn write_deflated_zip(path: &Path, name: &str, contents: &[u8]) {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(contents).unwrap();
        writer.finish().unwrap();
    }

    #[test]
    fn java_install_is_staged_and_rejects_unsafe_archives() {
        let root =
            std::env::temp_dir().join(format!("blockfield-java-install-{}", std::process::id()));
        let old_java = root.join("java").join("bin").join(java_exe_name());
        std::fs::create_dir_all(old_java.parent().unwrap()).unwrap();
        std::fs::write(&old_java, b"old").unwrap();

        let malicious = root.join("malicious.zip");
        for entry in [
            "jdk/../../evil",
            "../evil",
            "jdk/../evil",
            "C:/evil",
            "jdk/evil:stream",
            "jdk\\evil",
        ] {
            write_zip(&malicious, &[(entry, b"bad")]);
            assert!(
                install_java_archive(&malicious, &root, "2")
                    .unwrap_err()
                    .contains("Unsafe archive entry"),
                "{entry}"
            );
            assert_eq!(std::fs::read(&old_java).unwrap(), b"old");
            assert!(!root.join("evil").exists());
        }

        let mut links = zip::ZipWriter::new(std::fs::File::create(&malicious).unwrap());
        links
            .add_symlink(
                "jdk/link",
                "../../evil",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        links.finish().unwrap();
        assert!(install_java_archive(&malicious, &root, "2")
            .unwrap_err()
            .contains("symlink"));
        assert_eq!(std::fs::read(&old_java).unwrap(), b"old");

        let missing_java = root.join("missing-java.zip");
        write_zip(&missing_java, &[("jdk/readme.txt", b"no java")]);
        assert!(install_java_archive(&missing_java, &root, "2").is_err());
        assert_eq!(std::fs::read(&old_java).unwrap(), b"old");

        let valid = root.join("valid.zip");
        let java_entry = format!("jdk/bin/{}", java_exe_name());
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&valid).unwrap());
        writer
            .start_file(
                java_entry.as_str(),
                zip::write::SimpleFileOptions::default().unix_permissions(0o6755),
            )
            .unwrap();
        writer.write_all(b"new").unwrap();
        writer.finish().unwrap();
        let installed = install_java_archive(&valid, &root, "2").unwrap();
        assert_eq!(std::fs::read(&installed).unwrap(), b"new");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&installed).unwrap().permissions().mode();
            assert_eq!(mode & 0o6000, 0, "SUID/SGID must not be applied");
        }
        assert_eq!(
            std::fs::read_to_string(root.join("java/.version")).unwrap(),
            "2"
        );
        assert!(!root.join(".java-backup").exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn java_archive_limits_reject_resource_exhaustion() {
        let root = std::env::temp_dir().join(format!(
            "blockfield-java-limits-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let dest = root.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        let archive = root.join("test.zip");
        write_zip(&archive, &[("jdk/a", b"aa"), ("jdk/b", b"b")]);

        let archive_size = std::fs::metadata(&archive).unwrap().len();
        let mut limits = blockfield_shared::ArchiveLimits {
            min_free_space_bytes: 0,
            max_compressed_bytes: archive_size - 1,
            ..Default::default()
        };
        assert!(extract_archive_with_limits(&archive, &dest, &limits)
            .unwrap_err()
            .contains("compressed-size"));
        limits.max_compressed_bytes = archive_size;
        limits.max_path_depth = 1;
        assert!(extract_archive_with_limits(&archive, &dest, &limits)
            .unwrap_err()
            .contains("too deep"));
        assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 0);

        let mut limits = blockfield_shared::ArchiveLimits {
            min_free_space_bytes: 0,
            max_entries: 1,
            ..Default::default()
        };
        assert!(extract_archive_with_limits(&archive, &dest, &limits).is_err());
        limits.max_entries = 2;
        limits.max_entry_bytes = 1;
        assert!(extract_archive_with_limits(&archive, &dest, &limits).is_err());
        limits.max_entry_bytes = 2;
        limits.max_uncompressed_bytes = 2;
        assert!(extract_archive_with_limits(&archive, &dest, &limits).is_err());
        limits.max_uncompressed_bytes = 3;
        limits.min_free_space_bytes = u64::MAX;
        assert!(extract_archive_with_limits(&archive, &dest, &limits).is_err());

        let bomb = root.join("bomb.zip");
        write_deflated_zip(&bomb, "jdk/bin/java.exe", &[0; 16_384]);
        limits = blockfield_shared::ArchiveLimits::default();
        limits.min_free_space_bytes = 0;
        limits.max_compression_ratio = 2;
        assert!(extract_archive_with_limits(&bomb, &dest, &limits).is_err());

        limits.max_compression_ratio = 10_000;
        assert!(extract_archive_with_limits(&bomb, &dest, &limits).is_ok());
        assert_eq!(
            std::fs::metadata(dest.join("bin/java.exe")).unwrap().len(),
            16_384
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_unsafe_relative_paths() {
        for value in ["", "/abs", "a\\b", "../x", "a/../b", "c:/x", "./a"] {
            assert!(safe_relative_path(value).is_err(), "{value}");
        }
        assert_eq!(
            safe_relative_path("mods/a.jar").unwrap(),
            PathBuf::from("mods/a.jar")
        );
    }

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
}
