use crate::config::LauncherConfig;
use crate::download::Downloader;
use crate::manifest::{InstalledManifest, ModpackManifest, VersionCheckResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::RwLock;

/// Shared application state managed by Tauri.
pub struct LauncherAppState {
    /// No outer mutex — Downloader uses atomics internally for cancel flag and progress.
    pub downloader: Arc<Downloader>,
    pub manifest: RwLock<Option<ModpackManifest>>,
    pub config: RwLock<LauncherConfig>,
    pub app_data_dir: PathBuf,
    /// Direct handle to the atomic cancel flag so cancel_download can set it
    /// without any lock contention.
    pub cancel_flag: Arc<AtomicBool>,
    pub game_identity: RwLock<Option<GameIdentity>>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameIdentity {
    pub username: String,
    pub uuid: String,
    pub access_token: String,
}

impl GameIdentity {
    fn is_valid(&self) -> bool {
        self.username.len() >= 3
            && self.username.len() <= 16
            && self
                .username
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            && !self.access_token.is_empty()
            && !self.uuid.is_empty()
            && self.uuid.len() <= 36
            && self
                .uuid
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    }
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
    std::process::Command::new("java")
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

#[tauri::command]
pub async fn check_modpack_version(
    state: State<'_, LauncherAppState>,
) -> Result<VersionCheckResult, String> {
    let api_base = crate::commands::api_base_url();
    let manifest_url = format!("{api_base}/manifest.json");
    log::info!("[check_modpack_version] Fetching: {manifest_url}");

    let manifest = state
        .downloader
        .fetch_manifest(&manifest_url)
        .await
        .map_err(|e| {
            log::error!("[check_modpack_version] Fetch failed: {e}");
            format!("Failed to fetch manifest: {e}")
        })?;

    let config = state.config.read().await.clone();
    let game_dir = PathBuf::from(&config.game_dir);
    let manifest_path =
        std::path::PathBuf::from(&config.game_dir).join(".blockfield-manifest.json");
    let manifest_exists = manifest_path.exists();
    let installed = InstalledManifest::load(&config.game_dir);

    log::info!(
        "[check_modpack_version] game_dir={}, manifest_path={}, manifest_exists={manifest_exists}",
        config.game_dir,
        manifest_path.display(),
    );
    log::info!(
        "[check_modpack_version] remote_version={}, installed_version={}, installed_files_count={}",
        manifest.version,
        installed.version,
        installed.files.len(),
    );

    let mut needs_update = installed.version != manifest.version;

    // Even if version matches, check for missing files (quick existence + spot SHA256)
    if !needs_update && !manifest.files.is_empty() {
        let game_dir = PathBuf::from(&config.game_dir);
        // First pass: quick existence check for all files
        for entry in &manifest.files {
            if !game_dir.join(&entry.path).exists() {
                log::info!(
                    "[check_modpack_version] Missing file: {} — forcing update",
                    entry.path
                );
                needs_update = true;
                break;
            }
        }
        // Second pass: SHA256 spot-check on 10 random files
        if !needs_update {
            for entry in manifest.files.iter().take(10) {
                let path = game_dir.join(&entry.path);
                match Downloader::sha256_file(&path) {
                    Ok(hash) if hash == entry.sha256 => { /* ok */ }
                    _ => {
                        log::info!(
                            "[check_modpack_version] File changed: {} — forcing update",
                            entry.path
                        );
                        needs_update = true;
                        break;
                    }
                }
            }
        }
    }

    log::info!("[check_modpack_version] needs_update = {needs_update}");

    // Check Java version if required
    let java_info = manifest.java.clone();
    let java_ok = if let Some(ref java) = java_info {
        if java_runtime_ready(&game_dir, &java.version) {
            log::info!("[check_modpack_version] Java OK: {}", java.version);
            true
        } else {
            log::info!("[check_modpack_version] Java not ready");
            false
        }
    } else {
        true // No Java requirement
    };

    // Check Forge installation
    let forge_ok = if let Some(ref forge) = manifest.forge {
        let forge_version_id = crate::minecraft::forge_version_id(&forge.version);
        let forge_json = game_dir
            .join("versions")
            .join(&forge_version_id)
            .join(format!("{}.json", forge_version_id));
        let forge_json_ok = forge_json.exists();
        let runtime_ok = crate::minecraft::has_launch_dependencies(
            &game_dir,
            &manifest.minecraft_version,
            Some(&forge_version_id),
        );
        let ok = forge_json_ok && runtime_ok;
        if ok {
            log::info!("[check_modpack_version] Forge/runtime OK: {forge_version_id}");
        } else if !forge_json_ok {
            log::info!(
                "[check_modpack_version] Forge not installed at {}",
                forge_json.display()
            );
        } else {
            log::info!("[check_modpack_version] Minecraft runtime files missing");
        }
        ok
    } else {
        crate::minecraft::has_launch_dependencies(&game_dir, &manifest.minecraft_version, None)
    };

    let result = VersionCheckResult {
        needs_update,
        remote_version: manifest.version.clone(),
        installed_version: installed.version.clone(),
        mirror: api_base,
        file_count: manifest.files.len(),
        total_size: manifest.total_size,
        java: java_info,
        java_ok,
        forge_ok,
    };

    // Cache the manifest for later use
    let mut cached = state.manifest.write().await;
    *cached = Some(manifest);

    Ok(result)
}

#[tauri::command]
pub async fn download_modpack(
    app_handle: AppHandle,
    state: State<'_, LauncherAppState>,
) -> Result<(), String> {
    let manifest = {
        let cached = state.manifest.read().await;
        cached
            .clone()
            .ok_or_else(|| "No manifest cached – call check_modpack_version first".to_string())?
    };

    let mut config = state.config.read().await.clone();
    let installed = InstalledManifest::load(&config.game_dir);

    state.downloader.reset_cancel();
    Downloader::cleanup_partials(Path::new(&config.game_dir));

    // Bootstrap artifacts are known immediately. Minecraft runtime artifacts are
    // planned after Forge has generated its version metadata; until then the UI
    // remains in an explicit indeterminate setup phase.
    let game_dir = PathBuf::from(&config.game_dir);
    let mut planned_bytes = planned_bootstrap_bytes(&manifest, &game_dir);

    emit_status(&app_handle, "setup", "Preparing install tasks", false);

    // Download Java runtime if needed
    if let Some(ref java) = manifest.java {
        let need_java = !java_runtime_ready(Path::new(&config.game_dir), &java.version);

        if need_java {
            log::info!("Downloading Java {} for {}", java.version, java.platform);
            emit_status(
                &app_handle,
                "java",
                format!("Downloading Java {}", java.version),
                true,
            );
            if java.sha256.len() != 64 || !java.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("Java runtime has no valid SHA-256 checksum".to_string());
            }

            let game_dir = PathBuf::from(&config.game_dir);
            std::fs::create_dir_all(&game_dir)
                .map_err(|e| format!("Failed to create game dir: {e}"))?;
            let archive_path = game_dir.join(".java-runtime.zip.part");

            let mut java_downloaded = 0;
            state
                .downloader
                .download_one(
                    &java.url,
                    &archive_path,
                    "java-runtime",
                    java.size,
                    1,
                    1,
                    &mut java_downloaded,
                    java.size,
                )
                .await
                .map_err(|e| format!("Java download failed: {e}"))?;

            let actual_size = std::fs::metadata(&archive_path)
                .map_err(|e| format!("Java archive metadata error: {e}"))?
                .len();
            if actual_size != java.size {
                let _ = std::fs::remove_file(&archive_path);
                return Err(format!(
                    "Java archive size mismatch: expected {}, got {actual_size}",
                    java.size
                ));
            }

            let actual = Downloader::sha256_file(&archive_path)
                .map_err(|e| format!("Java checksum error: {e}"))?;
            if !actual.eq_ignore_ascii_case(&java.sha256) {
                let _ = std::fs::remove_file(&archive_path);
                return Err("Java SHA-256 mismatch".to_string());
            }
            log::info!("Java SHA256 verified");

            emit_status(&app_handle, "java", "Extracting Java runtime", false);
            let java_exe = install_java_archive(&archive_path, &game_dir, &java.version)?;
            let _ = std::fs::remove_file(&archive_path);
            let java_path_str = java_exe.to_string_lossy().to_string();
            let mut cfg = state.config.write().await;
            cfg.java_path = java_path_str.clone();
            config.java_path = java_path_str.clone();
            let _ = crate::config::save_config(&state.app_data_dir, &cfg);
            log::info!("Java installed, path saved: {java_path_str}");
            emit_status(&app_handle, "java", "Java runtime ready", false);
        } else {
            emit_status(&app_handle, "java", "Java runtime ready", false);
        }
    }

    // Download & install Forge if needed
    if let Some(ref forge) = manifest.forge {
        let forge_version_id = crate::minecraft::forge_version_id(&forge.version);
        let forge_json = PathBuf::from(&config.game_dir)
            .join("versions")
            .join(&forge_version_id)
            .join(format!("{}.json", forge_version_id));
        if !forge_json.exists() {
            log::info!("Downloading Forge installer {}", forge.version);
            emit_status(
                &app_handle,
                "forge",
                format!("Downloading Forge {}", forge.version),
                true,
            );
            let installer_path = PathBuf::from(&config.game_dir).join("forge-installer.jar");
            let installer_partial = crate::download::partial_path(&installer_path);
            let mut forge_downloaded = 0;
            state
                .downloader
                .download_one(
                    &forge.url,
                    &installer_partial,
                    "forge-installer",
                    forge.size,
                    1,
                    1,
                    &mut forge_downloaded,
                    forge.size,
                )
                .await
                .map_err(|e| format!("Forge download failed: {e}"))?;

            if !forge.sha256.is_empty() {
                let actual = Downloader::sha256_file(&installer_partial)
                    .map_err(|e| format!("Forge checksum error: {e}"))?;
                if actual != forge.sha256 {
                    let _ = std::fs::remove_file(&installer_partial);
                    return Err(format!(
                        "Forge SHA256 mismatch: expected {}, got {}",
                        forge.sha256, actual
                    ));
                }
            }
            crate::download::activate_partial(&installer_partial, &installer_path)
                .map_err(|e| format!("Failed to activate Forge installer: {e}"))?;

            // Run Forge installer
            log::info!("Running Forge installer...");
            emit_status(&app_handle, "forge", "Installing Forge client", false);

            // Create minimal Minecraft launcher profile so Forge installer works
            let launcher_profiles = PathBuf::from(&config.game_dir).join("launcher_profiles.json");
            if !launcher_profiles.exists() {
                let minimal_profile = serde_json::json!({
                    "profiles": {},
                    "settings": {},
                    "version": 3
                });
                std::fs::write(
                    &launcher_profiles,
                    serde_json::to_string(&minimal_profile).unwrap_or_default(),
                )
                .map_err(|e| format!("Failed to create launcher profile: {e}"))?;
            }

            let java = resolved_java_path(&config);

            use tauri_plugin_shell::ShellExt;
            let output = app_handle
                .shell()
                .command(&java)
                .args([
                    "-jar",
                    "forge-installer.jar",
                    "--installClient",
                    &config.game_dir,
                ])
                .current_dir(&config.game_dir)
                .output()
                .await
                .map_err(|e| format!("Forge installer failed to start: {e}"))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                log::error!("Forge installer stderr: {stderr}");
                log::error!("Forge installer stdout: {stdout}");
                let _ = std::fs::remove_file(&installer_path);
                return Err(format!("Forge installer failed: {stderr} {stdout}"));
            }

            log::info!("Forge installed successfully");
            let _ = std::fs::remove_file(&installer_path);

            if !forge_json.exists() {
                return Err(format!(
                    "Forge installer completed but version JSON is missing: {}",
                    forge_json.display()
                ));
            }
        }
        emit_status(&app_handle, "forge", "Forge client ready", false);
    }

    let game_dir = PathBuf::from(&config.game_dir);
    let forge_version_id = manifest
        .forge
        .as_ref()
        .map(|forge| crate::minecraft::forge_version_id(&forge.version));
    emit_status(
        &app_handle,
        "setup",
        "Planning required runtime files",
        false,
    );
    planned_bytes += crate::minecraft::prepare_download_plan(
        &game_dir,
        &manifest.minecraft_version,
        forge_version_id.as_deref(),
    )
    .await?;
    state.downloader.set_grand_total(planned_bytes);

    // If the version changed entirely, start fresh (prune old files)
    let installed_sha256 = if installed.version != manifest.version {
        emit_status(&app_handle, "prune", "Pruning stale modpack files", false);
        // Delete files listed in the prune array
        if let Some(ref prune_patterns) = manifest.prune {
            let game_dir = PathBuf::from(&config.game_dir);
            for pattern in prune_patterns {
                let (path, directory) = safe_prune_target(&game_dir, pattern)?;
                if path.exists() {
                    if directory {
                        std::fs::remove_dir_all(&path)
                    } else {
                        std::fs::remove_file(&path)
                    }
                    .map_err(|e| format!("Failed to prune {}: {e}", path.display()))?;
                    log::info!("Pruned: {}", path.display());
                }
            }
        }
        emit_status(&app_handle, "prune", "Stale modpack files pruned", false);
        std::collections::HashMap::new()
    } else {
        installed.files
    };

    emit_status(
        &app_handle,
        "modpack",
        "Syncing modpack files + preparing runtime",
        true,
    );

    // Modpack files and Minecraft runtime are independent — run them concurrently.
    let (modpack_result, runtime_result) = tokio::join!(
        async {
            state
                .downloader
                .download_files(&manifest, &config.game_dir, &installed_sha256)
                .await
                .map_err(|e| {
                    log::error!("Download failed: {e}");
                    e.to_string()
                })
        },
        async {
            crate::minecraft::ensure_launch_dependencies(
                &state.downloader,
                &app_handle,
                &game_dir,
                &manifest.minecraft_version,
                forge_version_id.as_deref(),
            )
            .await
        },
    );

    modpack_result?;
    emit_status(&app_handle, "modpack", "Modpack files ready", false);

    if runtime_result.is_ok() {
        emit_status(&app_handle, "ready", "Minecraft runtime ready", false);
    }
    runtime_result
}

fn planned_bootstrap_bytes(manifest: &ModpackManifest, game_dir: &Path) -> u64 {
    let java = manifest
        .java
        .as_ref()
        .filter(|java| !java_runtime_ready(game_dir, &java.version))
        .map_or(0, |java| java.size);
    let forge = manifest
        .forge
        .as_ref()
        .filter(|forge| {
            let version_id = crate::minecraft::forge_version_id(&forge.version);
            !game_dir
                .join("versions")
                .join(&version_id)
                .join(format!("{version_id}.json"))
                .exists()
        })
        .map_or(0, |forge| forge.size);
    let modpack = manifest
        .files
        .iter()
        .filter(|entry| {
            let path = game_dir.join(&entry.path);
            !matches!(Downloader::sha256_file(&path), Ok(hash) if hash == entry.sha256)
        })
        .map(|entry| entry.size)
        .sum::<u64>();

    java + forge + modpack
}

#[tauri::command]
pub async fn verify_files(state: State<'_, LauncherAppState>) -> Result<Vec<String>, String> {
    let manifest = {
        let cached = state.manifest.read().await;
        cached
            .clone()
            .ok_or_else(|| "No manifest cached".to_string())?
    };

    let config = state.config.read().await;
    let mut failed: Vec<String> = Vec::new();

    for entry in &manifest.files {
        let path = PathBuf::from(&config.game_dir).join(&entry.path);
        if !path.exists() {
            failed.push(format!("{} (missing)", entry.path));
            continue;
        }
        match Downloader::sha256_file(&path) {
            Ok(hash) if hash == entry.sha256 => { /* ok */ }
            Ok(hash) => failed.push(format!("{} (hash mismatch: {})", entry.path, hash)),
            Err(e) => failed.push(format!("{} (read error: {})", entry.path, e)),
        }
    }

    Ok(failed)
}

#[tauri::command]
pub async fn cancel_download(state: State<'_, LauncherAppState>) -> Result<(), String> {
    // Use the atomic flag directly — no mutex, works even during active downloads.
    state.cancel_flag.store(true, Ordering::SeqCst);
    log::info!("Download cancel requested from frontend");
    Ok(())
}

// ── Game launch command ────────────────────────────────────────

#[tauri::command]
pub async fn set_game_identity(
    state: State<'_, LauncherAppState>,
    identity: Option<GameIdentity>,
) -> Result<(), String> {
    if identity
        .as_ref()
        .is_some_and(|identity| !identity.is_valid())
    {
        return Err("Invalid authenticated game identity".to_string());
    }
    *state.game_identity.write().await = identity;
    Ok(())
}

#[tauri::command]
pub async fn launch_game(
    app_handle: AppHandle,
    state: State<'_, LauncherAppState>,
) -> Result<(), String> {
    let identity = state
        .game_identity
        .read()
        .await
        .clone()
        .ok_or_else(|| "Authentication required before launch".to_string())?;
    verify_game_identity(&identity).await?;
    let config = state.config.read().await.clone();
    let manifest = state.manifest.read().await.clone();

    let java = resolved_java_path(&config);
    if java == "java" && !system_java_available() {
        let hint = if manifest.as_ref().and_then(|m| m.java.as_ref()).is_some() {
            "Java runtime is not installed. Open Updates to install it or select java.exe in Settings."
        } else {
            "Java runtime is not installed, and the deployment manifest does not provide a Java download."
        };
        return Err(hint.to_string());
    }

    let ram_mb = config.ram_mb;
    let game_dir = config.game_dir.clone();
    let game_dir_path = PathBuf::from(&game_dir);

    let forge_version_id = manifest
        .as_ref()
        .and_then(|m| {
            m.forge
                .as_ref()
                .map(|forge| crate::minecraft::forge_version_id(&forge.version))
        })
        .or_else(|| crate::minecraft::find_forge_version_id(&game_dir_path));

    let minecraft_version = manifest
        .as_ref()
        .map(|m| m.minecraft_version.clone())
        .or_else(|| {
            forge_version_id
                .as_ref()
                .and_then(|id| id.split_once("-forge-").map(|(mc, _)| mc.to_string()))
        })
        .unwrap_or_else(|| {
            std::env::var("BLOCKFIELD_MINECRAFT_VERSION")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .or_else(|| option_env!("BLOCKFIELD_MINECRAFT_VERSION").map(String::from))
                .unwrap_or_else(|| "1.20.1".to_string())
        });

    log::info!("Launching game: java={java}, ram={ram_mb}MB, dir={game_dir}");

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
            forge_version_id.as_deref(),
        )
        .await?;
    }

    emit_status(&app_handle, "launch", "Building launch command", false);
    let args = crate::minecraft::build_launch_args(
        &game_dir_path,
        ram_mb,
        &minecraft_version,
        forge_version_id.as_deref(),
        &identity,
    )?;

    log::info!("Spawning authenticated game process");
    emit_status(&app_handle, "launch", "Starting game process", false);

    let child = std::process::Command::new(&java)
        .args(&args)
        .current_dir(&game_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to spawn game process: {e}"))?;
    log::info!("Game process spawned (pid {})", child.id());
    emit_status(
        &app_handle,
        "launch",
        format!("Game process started (pid {})", child.id()),
        false,
    );

    Ok(())
}

// ── Helpers ────────────────────────────────────────────────────

/// Base URL for the launcher API.
fn api_base_url() -> String {
    // Runtime first (dotenvy for local dev), then compile-time (CI build),
    // then hardcoded fallback.
    let url = std::env::var("VITE_BLOCKFIELD_API_URL")
        .ok()
        .or_else(|| option_env!("VITE_BLOCKFIELD_API_URL").map(String::from))
        .unwrap_or_else(|| "http://localhost:3000/api/launcher/v1".to_string());
    log::info!("[api_base_url] VITE_BLOCKFIELD_API_URL={url}");
    url
}

async fn verify_game_identity(identity: &GameIdentity) -> Result<(), String> {
    #[derive(Deserialize)]
    struct MeResponse {
        user: MeUser,
    }
    #[derive(Deserialize)]
    struct MeUser {
        username: String,
        minecraft_uuid: String,
    }

    let response = reqwest::Client::new()
        .get(format!("{}/auth/me", api_base_url()))
        .bearer_auth(&identity.access_token)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|_| "Authentication service unavailable".to_string())?;
    if !response.status().is_success() {
        return Err("Authentication session expired".to_string());
    }
    let me = response
        .json::<MeResponse>()
        .await
        .map_err(|_| "Invalid authentication response".to_string())?;
    if me.user.username != identity.username || me.user.minecraft_uuid != identity.uuid {
        return Err("Authenticated game identity mismatch".to_string());
    }
    Ok(())
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

fn safe_prune_target(game_dir: &Path, pattern: &str) -> Result<(PathBuf, bool), String> {
    let (value, directory) = pattern
        .strip_suffix("/*")
        .map_or((pattern, false), |value| (value, true));
    if value.contains('*') || value.contains('?') {
        return Err(format!("Unsafe prune rule rejected: {pattern}"));
    }

    let relative =
        safe_relative_path(value).map_err(|_| format!("Unsafe prune rule rejected: {pattern}"))?;
    let target = game_dir.join(relative);
    if target.exists() {
        let metadata = std::fs::symlink_metadata(&target)
            .map_err(|e| format!("Failed to inspect prune target: {e}"))?;
        if metadata.file_type().is_symlink() {
            return Err(format!("Symlink prune rule rejected: {pattern}"));
        }
        let root = std::fs::canonicalize(game_dir)
            .map_err(|e| format!("Failed to resolve game directory: {e}"))?;
        let resolved = std::fs::canonicalize(&target)
            .map_err(|e| format!("Failed to resolve prune target: {e}"))?;
        if !resolved.starts_with(root) {
            return Err(format!("Unsafe prune rule rejected: {pattern}"));
        }
    }
    Ok((target, directory))
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

/// Extract a ZIP archive to a target directory.
fn extract_archive(archive_path: &Path, dest_dir: &Path) -> Result<(), String> {
    let file =
        std::fs::File::open(archive_path).map_err(|e| format!("Failed to open archive: {e}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Failed to read archive: {e}"))?;

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
        std::io::copy(&mut entry, &mut out)
            .map_err(|e| format!("Failed to extract {}: {e}", name))?;
    }

    log::info!(
        "Extracted {} entries to {}",
        archive.len(),
        dest_dir.display()
    );
    Ok(())
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
    fn rejects_unsafe_relative_paths() {
        for path in [
            "",
            "../x",
            "a/../../x",
            "C:/x",
            "/etc/passwd",
            r"\\server\x",
        ] {
            assert!(safe_relative_path(path).is_err(), "accepted {path}");
        }
        assert_eq!(
            safe_relative_path("mods/old.jar").unwrap(),
            Path::new("mods/old.jar")
        );
    }

    #[test]
    fn accepts_only_the_supported_prune_glob() {
        let root = std::env::temp_dir().join(format!("blockfield-prune-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();

        assert!(safe_prune_target(&root, "config/old/*").unwrap().1);
        assert!(!safe_prune_target(&root, "mods/old.jar").unwrap().1);
        assert!(safe_prune_target(&root, "mods/*.jar").is_err());
        assert!(safe_prune_target(&root, "../x").is_err());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn download_plan_counts_only_missing_bootstrap_and_changed_modpack_files() {
        use blockfield_shared::{ForgeInfo, JavaInfo, ManifestFileEntry};
        use sha2::Digest;

        let root = std::env::temp_dir().join(format!("blockfield-plan-{}", std::process::id()));
        let contents = b"same";
        let hash = format!("{:x}", sha2::Sha256::digest(contents));
        let manifest = ModpackManifest {
            version: "1.0.0".to_string(),
            minecraft_version: "1.20.1".to_string(),
            files: vec![ManifestFileEntry {
                path: "mods/test.jar".to_string(),
                size: contents.len() as u64,
                sha256: hash,
                url: "https://example.invalid/test.jar".to_string(),
            }],
            total_size: contents.len() as u64,
            prune: None,
            java: Some(JavaInfo {
                version: "17".to_string(),
                platform: "test".to_string(),
                url: "https://example.invalid/java.zip".to_string(),
                sha256: "0".repeat(64),
                size: 100,
            }),
            forge: Some(ForgeInfo {
                version: "1.20.1-47.4.10".to_string(),
                url: "https://example.invalid/forge.jar".to_string(),
                sha256: "0".repeat(64),
                size: 200,
            }),
        };

        assert_eq!(planned_bootstrap_bytes(&manifest, &root), 304);

        let java = root.join("java/bin").join(java_exe_name());
        std::fs::create_dir_all(java.parent().unwrap()).unwrap();
        std::fs::write(&java, b"").unwrap();
        std::fs::write(root.join("java/.version"), "17").unwrap();
        let forge_id = crate::minecraft::forge_version_id("1.20.1-47.4.10");
        let forge_json = root
            .join("versions")
            .join(&forge_id)
            .join(format!("{forge_id}.json"));
        std::fs::create_dir_all(forge_json.parent().unwrap()).unwrap();
        std::fs::write(forge_json, "{}").unwrap();

        assert_eq!(planned_bootstrap_bytes(&manifest, &root), 4);
        std::fs::create_dir_all(root.join("mods")).unwrap();
        std::fs::write(root.join("mods/test.jar"), contents).unwrap();
        assert_eq!(planned_bootstrap_bytes(&manifest, &root), 0);
        std::fs::write(root.join("mods/test.jar"), b"changed").unwrap();
        assert_eq!(planned_bootstrap_bytes(&manifest, &root), 4);

        std::fs::remove_dir_all(root).unwrap();
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
    fn game_identity_rejects_placeholder_or_invalid_values() {
        let valid = GameIdentity {
            username: "operator_1".to_string(),
            uuid: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            access_token: "secret".to_string(),
        };
        assert!(valid.is_valid());
        assert!(!GameIdentity {
            access_token: String::new(),
            ..valid.clone()
        }
        .is_valid());
        assert!(!GameIdentity {
            username: "bad name".to_string(),
            ..valid
        }
        .is_valid());
    }
}
