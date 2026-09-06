use crate::config::LauncherConfig;
use crate::download::Downloader;
use crate::pack::{self, JavaInfo, LauncherInfo, PackMeta};
use crate::status::ServerStatus;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::RwLock;

const INSTALLED_VERSION_FILE: &str = ".blockfield-pack-version";
const INSTALLER_JAR: &str = "packwiz-installer.jar";
const BOOTSTRAP_JAR: &str = "packwiz-installer-bootstrap.jar";

/// Shared application state managed by Tauri.
pub struct LauncherAppState {
    /// No outer mutex — Downloader uses atomics internally for cancel flag and progress.
    pub downloader: Arc<Downloader>,
    pub pack: RwLock<Option<PackState>>,
    pub config: RwLock<LauncherConfig>,
    pub app_data_dir: PathBuf,
    /// Direct handle to the atomic cancel flag so cancel_download can set it
    /// without any lock contention.
    pub cancel_flag: Arc<AtomicBool>,
    pub game: Arc<crate::game::GameState>,
    pub operation: tokio::sync::Mutex<()>,
}

/// What `check_modpack_version` learned about the hosted pack.
#[derive(Clone)]
pub struct PackState {
    pub info: LauncherInfo,
    pub meta: PackMeta,
}

/// Offline-mode identity. The proxy runs online-mode=false and derives the UUID from the
/// username itself, so the launcher-side check is only about a well-formed name.
#[derive(Clone)]
pub struct GameIdentity {
    pub username: String,
    pub uuid: String,
    pub access_token: String,
}

impl GameIdentity {
    pub fn offline(username: &str) -> Result<Self, String> {
        let username = username.trim();
        if !valid_username(username) {
            return Err(
                "Set a Minecraft username in Settings (3-16 letters, digits or _)".to_string(),
            );
        }
        Ok(Self {
            username: username.to_string(),
            uuid: offline_uuid(username),
            access_token: "0".to_string(),
        })
    }
}

pub fn valid_username(name: &str) -> bool {
    (3..=16).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// Vanilla offline UUID: UUIDv3 of "OfflinePlayer:<name>".
fn offline_uuid(username: &str) -> String {
    use md5::Digest;
    let mut hash: [u8; 16] =
        md5::Md5::digest(format!("OfflinePlayer:{username}").as_bytes()).into();
    hash[6] = (hash[6] & 0x0f) | 0x30;
    hash[8] = (hash[8] & 0x3f) | 0x80;
    let hex = hash.iter().map(|b| format!("{b:02x}")).collect::<String>();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

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

#[derive(Clone, Serialize)]
struct LauncherStatusPayload {
    phase: &'static str,
    message: String,
    cancelable: bool,
}

fn emit_status(
    app_handle: &AppHandle,
    phase: &'static str,
    message: impl Into<String>,
    cancelable: bool,
) {
    let payload = LauncherStatusPayload {
        phase,
        message: message.into(),
        cancelable,
    };

    if let Err(e) = app_handle.emit("launcher://status", payload) {
        log::error!("Failed to emit launcher status: {e}");
    }
}

fn java_exe_name() -> &'static str {
    if cfg!(windows) {
        "javaw.exe"
    } else {
        "java"
    }
}

fn bundled_java_path(game_dir: impl AsRef<Path>) -> Option<PathBuf> {
    let java_dir = game_dir.as_ref().join("java");
    let direct = java_dir.join("bin").join(java_exe_name());
    if direct.exists() {
        return Some(direct);
    }

    find_bin_java(&java_dir)
}

fn find_bin_java(dir: &Path) -> Option<PathBuf> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if is_bin_java(&path) {
                return Some(path);
            }
        }
    }
    None
}

fn is_bin_java(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(java_exe_name()))
        && path
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("bin"))
}

fn java_runtime_ready(game_dir: &Path, version: &str) -> bool {
    let java_ver_path = game_dir.join("java").join(".version");
    matches!(std::fs::read_to_string(&java_ver_path), Ok(v) if v.trim() == version)
        && bundled_java_path(game_dir).is_some()
}

fn resolved_java_path(config: &LauncherConfig) -> String {
    let configured = config.java_path.trim();
    if !configured.is_empty() {
        let mut path = PathBuf::from(configured);
        if !path.exists() {
            // User pointed at java.exe — try javaw.exe next to it
            if let Some(javaw) = try_javaw(&path) {
                path = javaw;
            }
        }
        if path.exists() {
            return path.to_string_lossy().to_string();
        }
    }

    bundled_java_path(&config.game_dir)
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| java_exe_name().to_string())
}

/// If `path` ends with `java.exe`, return `javaw.exe` in the same directory.
fn try_javaw(path: &Path) -> Option<PathBuf> {
    if cfg!(windows)
        && path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("java.exe"))
    {
        let javaw = path.with_file_name("javaw.exe");
        if javaw.exists() {
            return Some(javaw);
        }
    }
    None
}

fn system_java_available() -> bool {
    crate::host_env::command("java")
        .arg("-version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

// ── Settings commands ──────────────────────────────────────────

#[tauri::command]
pub fn load_settings(state: State<'_, LauncherAppState>) -> Result<LauncherConfig, String> {
    let mut config = state.config.blocking_read().clone();
    // Auto-detect bundled Java if path is empty
    if config.java_path.is_empty() {
        if let Some(bundled) = bundled_java_path(&config.game_dir) {
            config.java_path = bundled.to_string_lossy().to_string();
        }
    }
    Ok(config)
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
    crate::config::save_config(&app_data_dir, &config)?;

    // Update in-memory state
    let mut current = state.config.blocking_write();
    *current = config;
    Ok(())
}

// ── Modpack commands ───────────────────────────────────────────

fn installed_pack_version(game_dir: &Path) -> String {
    std::fs::read_to_string(game_dir.join(INSTALLED_VERSION_FILE))
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "none".to_string())
}

#[tauri::command]
pub async fn check_modpack_version(
    state: State<'_, LauncherAppState>,
) -> Result<VersionCheckResult, String> {
    let base = pack_base_url()?;
    let info = pack::fetch_launcher_info(&base).await?;
    let meta = pack::fetch_pack_meta(&info.pack).await?;

    let config = state.config.read().await.clone();
    let game_dir = PathBuf::from(&config.game_dir);
    let installed = installed_pack_version(&game_dir);
    let needs_update = installed != meta.version;
    log::info!(
        "[check_modpack_version] pack={} remote={} installed={} needs_update={needs_update}",
        info.pack,
        meta.version,
        installed
    );

    let java = info.java_for_this_platform();
    let java_ok = java
        .as_ref()
        .is_none_or(|java| java_runtime_ready(&game_dir, &java.version));

    let loader_ok = match &meta.loader {
        Some(loader) => {
            let id = crate::minecraft::loader_version_id(&meta.minecraft, loader);
            crate::minecraft::loader_profile_exists(&game_dir, &id)
                && crate::minecraft::has_launch_dependencies(&game_dir, &meta.minecraft, Some(&id))
        }
        None => crate::minecraft::has_launch_dependencies(&game_dir, &meta.minecraft, None),
    };

    let result = VersionCheckResult {
        needs_update,
        remote_version: meta.version.clone(),
        installed_version: installed,
        mirror: base,
        file_count: 0,
        total_size: 0,
        java,
        java_ok,
        loader_ok,
    };
    *state.pack.write().await = Some(PackState { info, meta });
    Ok(result)
}

#[tauri::command]
pub async fn download_modpack(
    app_handle: AppHandle,
    state: State<'_, LauncherAppState>,
) -> Result<(), String> {
    let _operation = state
        .operation
        .try_lock()
        .map_err(|_| "Another launcher operation is in progress")?;
    if state.game.is_busy() {
        return Err("Close the game before updating files".to_string());
    }
    let pack = state
        .pack
        .read()
        .await
        .clone()
        .ok_or_else(|| "No pack info cached – call check_modpack_version first".to_string())?;
    let mut config = state.config.read().await.clone();
    let game_dir = PathBuf::from(&config.game_dir);
    std::fs::create_dir_all(&game_dir).map_err(|e| format!("Failed to create game dir: {e}"))?;

    state.downloader.reset_cancel();
    Downloader::cleanup_partials(&game_dir);
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
    emit_status(&app_handle, "setup", "Preparing install tasks", false);

    if let Some(java) = java {
        if java_runtime_ready(&game_dir, &java.version) {
            emit_status(&app_handle, "java", "Java runtime ready", false);
        } else {
            let java_exe = install_java(&app_handle, &state, &game_dir, &java).await?;
            let java_path = java_exe.to_string_lossy().to_string();
            let mut cfg = state.config.write().await;
            cfg.java_path = java_path.clone();
            config.java_path = java_path.clone();
            let _ = crate::config::save_config(&state.app_data_dir, &cfg);
            log::info!("Java installed, path saved: {java_path}");
            emit_status(&app_handle, "java", "Java runtime ready", false);
        }
    }

    let java = resolved_java_path(&config);
    if java == "java" && !system_java_available() {
        return Err("No Java runtime available for this platform".to_string());
    }

    let loader_version_id = match &pack.meta.loader {
        Some(loader) => {
            emit_status(
                &app_handle,
                "loader",
                format!("Fetching Fabric {loader} profile"),
                false,
            );
            crate::minecraft::ensure_loader_profile(&game_dir, &pack.meta.minecraft, loader)
                .await?;
            emit_status(&app_handle, "loader", "Fabric loader ready", false);
            Some(crate::minecraft::loader_version_id(
                &pack.meta.minecraft,
                loader,
            ))
        }
        None => None,
    };

    emit_status(&app_handle, "modpack", "Syncing modpack files", true);
    ensure_jar(&state, &game_dir, BOOTSTRAP_JAR, &pack.info.bootstrap).await?;
    ensure_jar(&state, &game_dir, INSTALLER_JAR, &pack.info.installer).await?;
    let cancel = state.cancel_flag.clone();
    let app = app_handle.clone();
    let (java_for_installer, dir, pack_url) =
        (java.clone(), game_dir.clone(), pack.info.pack.clone());
    tokio::task::spawn_blocking(move || {
        run_packwiz_installer(&app, &cancel, &java_for_installer, &dir, &pack_url)
    })
    .await
    .map_err(|e| format!("packwiz-installer task failed: {e}"))??;
    std::fs::write(game_dir.join(INSTALLED_VERSION_FILE), &pack.meta.version)
        .map_err(|e| format!("Failed to record installed version: {e}"))?;
    emit_status(&app_handle, "modpack", "Modpack files ready", false);

    emit_status(
        &app_handle,
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
    crate::minecraft::ensure_launch_dependencies(
        &state.downloader,
        &app_handle,
        &game_dir,
        &pack.meta.minecraft,
        loader_version_id.as_deref(),
    )
    .await?;
    emit_status(&app_handle, "ready", "Minecraft runtime ready", false);
    Ok(())
}

async fn install_java(
    app_handle: &AppHandle,
    state: &State<'_, LauncherAppState>,
    game_dir: &Path,
    java: &JavaInfo,
) -> Result<PathBuf, String> {
    log::info!("Downloading Java {} for {}", java.version, java.platform);
    emit_status(
        app_handle,
        "java",
        format!("Downloading Java {}", java.version),
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
    emit_status(app_handle, "java", "Extracting Java runtime", false);
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

/// Runs packwiz-installer (client side, via its bootstrap) in the game dir; it downloads, verifies and prunes the
/// pack contents itself. Output lines are forwarded to the UI as status/progress events.
fn run_packwiz_installer(
    app_handle: &AppHandle,
    cancel: &AtomicBool,
    java: &str,
    game_dir: &Path,
    pack_url: &str,
) -> Result<(), String> {
    use std::io::BufRead;
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
        if let Some((done, total)) = installer_progress(&line) {
            let _ = app_handle.emit(
                "download://progress",
                crate::download::DownloadProgressPayload {
                    file_path: line.clone(),
                    file_index: done,
                    file_count: total,
                    bytes_downloaded: 0,
                    file_bytes_total: 0,
                    total_bytes_downloaded: done as u64,
                    total_bytes_all: total as u64,
                    speed_bytes_per_sec: 0,
                },
            );
        } else {
            emit_status(app_handle, "modpack", line.clone(), true);
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
) -> Result<Vec<String>, String> {
    let _operation = state
        .operation
        .try_lock()
        .map_err(|_| "Another launcher operation is in progress")?;
    if state.game.is_busy() {
        return Err("Close the game before verifying files".to_string());
    }
    let pack = state
        .pack
        .read()
        .await
        .clone()
        .ok_or("Check the modpack first")?;
    let config = state.config.read().await.clone();
    let game_dir = PathBuf::from(&config.game_dir);
    let java = resolved_java_path(&config);
    ensure_jar(&state, &game_dir, BOOTSTRAP_JAR, &pack.info.bootstrap).await?;
    ensure_jar(&state, &game_dir, INSTALLER_JAR, &pack.info.installer).await?;
    state.downloader.reset_cancel();
    let cancel = state.cancel_flag.clone();
    tokio::task::spawn_blocking(move || {
        run_packwiz_installer(&app_handle, &cancel, &java, &game_dir, &pack.info.pack)
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
    let target = match state.pack.read().await.as_ref() {
        Some(pack) => pack.info.server.clone(),
        None => pack::fetch_launcher_info(&pack_base_url()?).await?.server,
    };
    let (host, port) = crate::status::split_host_port(&target);
    let ping_host = host.clone();
    let (status, (region_code, location_name)) = tokio::join!(
        tokio::task::spawn_blocking(move || crate::status::ping(&ping_host, port)),
        crate::status::locate(&host, port),
    );
    let mut status = status.map_err(|e| format!("status task failed: {e}"))?;
    status.region_code = region_code;
    status.location_name = location_name;
    Ok(status)
}

#[tauri::command]
pub fn game_status(state: State<'_, LauncherAppState>) -> crate::game::GameStatus {
    state.game.snapshot()
}

/// Opens a link in the user's browser. The URL can come from remote launcher content, so only
/// plain https links are accepted — never a local path, a `file:`/`javascript:` URL or an option.
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") || url.contains(char::is_whitespace) {
        return Err(format!("Refusing to open {url}"));
    }
    #[cfg(windows)]
    let mut command = {
        // Empty title argument: `start` treats a lone quoted argument as the window title.
        let mut command = crate::host_env::command("cmd.exe");
        command.args(["/D", "/S", "/C", "start", "", url.as_str()]);
        command
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut command = crate::host_env::command("xdg-open");
        command.arg(&url);
        command
    };
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Cannot open link: {e}"))
}

// ── Game launch command ────────────────────────────────────────

#[tauri::command]
pub async fn launch_game(
    app_handle: AppHandle,
    state: State<'_, LauncherAppState>,
) -> Result<(), String> {
    let _operation = state
        .operation
        .try_lock()
        .map_err(|_| "Another launcher operation is in progress")?;
    let running = state.game.begin()?;
    let config = state.config.read().await.clone();
    let identity = GameIdentity::offline(&config.username)?;
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
    let quick_play = pack
        .as_ref()
        .map(|p| p.info.server.trim().to_string())
        .filter(|s| !s.is_empty());

    log::info!(
        "Launching game: java={java}, ram={ram_mb}MB, dir={game_dir}, user={}",
        identity.username
    );

    {
        state.downloader.reset_cancel();
        // Floor so the frontend always has a non-zero denominator for progress.
        state.downloader.set_grand_total(16 * 1024 * 1024);
        emit_status(
            &app_handle,
            "launch",
            "Preparing launch dependencies",
            false,
        );
        crate::minecraft::ensure_launch_dependencies(
            &state.downloader,
            &app_handle,
            &game_dir_path,
            &minecraft_version,
            loader_version_id.as_deref(),
        )
        .await?;
    }

    emit_status(&app_handle, "launch", "Building launch command", false);
    let args = crate::minecraft::build_launch_args(
        &game_dir_path,
        ram_mb,
        &minecraft_version,
        loader_version_id.as_deref(),
        &identity,
        quick_play.as_deref(),
    )?;

    if !config.pre_launch_command.trim().is_empty() {
        emit_status(&app_handle, "launch", "Running pre-launch command", false);
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
    emit_status(&app_handle, "launch", "Starting game process", false);
    let log_dir = game_dir_path.join("logs");
    std::fs::create_dir_all(&log_dir).map_err(|e| format!("Cannot create log directory: {e}"))?;
    let output = std::fs::File::create(log_dir.join("launcher-game.log"))
        .map_err(|e| format!("Cannot create game log: {e}"))?;
    let mut child = crate::host_env::command(&java)
        .args(&args)
        .current_dir(&game_dir)
        .stdin(std::process::Stdio::null())
        .stdout(output.try_clone().map_err(|e| e.to_string())?)
        .stderr(output)
        .spawn()
        .map_err(|e| format!("Failed to spawn game process: {e}"))?;
    running.started();
    if config.hide_while_playing {
        if let Some(window) = app_handle.get_webview_window("main") {
            if let Err(error) = window.hide() {
                log::warn!("Cannot hide launcher: {error}");
            }
        }
    }
    log::info!("Game process spawned (pid {})", child.id());
    emit_status(
        &app_handle,
        "launch",
        format!("Game process started (pid {})", child.id()),
        false,
    );
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
                        emit_status(&app_handle, "hook-error", &error, false);
                        format!("Post-exit command failed: {error}")
                    }
                };
                emit_status(&app_handle, "exited", message, false);
            }
            Err(error) => emit_status(
                &app_handle,
                "exited",
                format!("Failed to wait for game: {error}"),
                false,
            ),
        }
    });
    Ok(())
}

// ── Helpers ────────────────────────────────────────────────────

/// Base URL of the hosted packwiz pack (directory containing pack.toml and launcher.json).
fn pack_base_url() -> Result<String, String> {
    let url = std::env::var("VITE_BLOCKFIELD_PACK_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| option_env!("VITE_BLOCKFIELD_PACK_URL").map(String::from))
        .ok_or_else(|| "VITE_BLOCKFIELD_PACK_URL is required".to_string())?;
    let url = url.trim().trim_end_matches('/').to_string();
    if !url.starts_with("https://") && !(cfg!(debug_assertions) && url.starts_with("http://")) {
        return Err("VITE_BLOCKFIELD_PACK_URL must use https".to_string());
    }
    Ok(url)
}

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
        if entry.encrypted()
            || !matches!(
                entry.compression(),
                zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
            )
        {
            return Err(format!("Unsupported archive entry: {}", entry.name()));
        }
        if entry.name().split('/').count() > limits.max_path_depth {
            return Err(format!("Archive path is too deep: {}", entry.name()));
        }
        if entry.size() > limits.max_entry_bytes {
            return Err(format!("Archive entry is too large: {}", entry.name()));
        }
        if entry.size() > 0
            && (entry.compressed_size() == 0
                || entry.size()
                    > entry
                        .compressed_size()
                        .saturating_mul(limits.max_compression_ratio))
        {
            return Err(format!("Suspicious compression ratio: {}", entry.name()));
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

        let name = entry.name().to_string();
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
    use std::io::Write;

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
    fn bundled_java_path_finds_nested_bin_java() {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("blockfield-java-test-{}-{id}", std::process::id()));
        let exe = root
            .join("java")
            .join("jdk-21")
            .join("bin")
            .join(java_exe_name());

        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"").unwrap();

        assert_eq!(bundled_java_path(&root), Some(exe));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn java_install_is_staged_and_rejects_unsafe_archives() {
        let root =
            std::env::temp_dir().join(format!("blockfield-java-install-{}", std::process::id()));
        let old_java = root.join("java").join("bin").join(java_exe_name());
        std::fs::create_dir_all(old_java.parent().unwrap()).unwrap();
        std::fs::write(&old_java, b"old").unwrap();

        let malicious = root.join("malicious.zip");
        write_zip(&malicious, &[("jdk/../../evil", b"bad")]);
        assert!(install_java_archive(&malicious, &root, "2").is_err());
        assert_eq!(std::fs::read(&old_java).unwrap(), b"old");
        assert!(!root.join("evil").exists());

        let missing_java = root.join("missing-java.zip");
        write_zip(&missing_java, &[("jdk/readme.txt", b"no java")]);
        assert!(install_java_archive(&missing_java, &root, "2").is_err());
        assert_eq!(std::fs::read(&old_java).unwrap(), b"old");

        let valid = root.join("valid.zip");
        let java_entry = format!("jdk/bin/{}", java_exe_name());
        write_zip(&valid, &[(java_entry.as_str(), b"new")]);
        let installed = install_java_archive(&valid, &root, "2").unwrap();
        assert_eq!(std::fs::read(installed).unwrap(), b"new");
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
    fn offline_identity_validates_username_and_derives_uuid() {
        assert!(GameIdentity::offline("ab").is_err());
        assert!(GameIdentity::offline("bad name").is_err());
        let identity = GameIdentity::offline(" Steve ").unwrap();
        assert_eq!(identity.username, "Steve");
        assert_eq!(identity.uuid.len(), 36);
        assert_eq!(identity.uuid, GameIdentity::offline("Steve").unwrap().uuid);
        assert_ne!(identity.uuid, GameIdentity::offline("Alex").unwrap().uuid);
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
    }
}
