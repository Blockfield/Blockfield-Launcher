use crate::config::LauncherConfig;
use crate::download::Downloader;
use crate::manifest::{InstalledManifest, ModpackManifest, VersionCheckResult};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::RwLock;

/// Shared application state managed by Tauri.
pub struct LauncherAppState {
    pub downloader: Arc<tokio::sync::Mutex<Downloader>>,
    pub manifest: RwLock<Option<ModpackManifest>>,
    pub config: RwLock<LauncherConfig>,
}

// ── Settings commands ──────────────────────────────────────────

#[tauri::command]
pub fn load_settings(state: State<'_, LauncherAppState>) -> Result<LauncherConfig, String> {
    let config = state.config.blocking_read().clone();
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

#[tauri::command]
pub fn get_default_game_dir() -> String {
    crate::config::LauncherConfig::default().game_dir
}

// ── Modpack commands ───────────────────────────────────────────

#[tauri::command]
pub async fn check_modpack_version(
    state: State<'_, LauncherAppState>,
) -> Result<VersionCheckResult, String> {
    let api_base = crate::commands::api_base_url();
    let manifest_url = format!("{api_base}/manifest.json");
    log::info!("[check_modpack_version] Fetching: {manifest_url}");

    let downloader = state.downloader.lock().await;
    let manifest = downloader
        .fetch_manifest(&manifest_url)
        .await
        .map_err(|e| {
            log::error!("[check_modpack_version] Fetch failed: {e}");
            format!("Failed to fetch manifest: {e}")
        })?;

    let config = state.config.read().await;
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

    let needs_update = installed.version != manifest.version;
    log::info!("[check_modpack_version] needs_update = {needs_update}");

    // Check Java version if required
    let java_info = manifest.java.clone();
    let java_ok = if let Some(ref java) = java_info {
        let java_ver_path = std::path::PathBuf::from(&config.game_dir)
            .join("java")
            .join(".version");
        match std::fs::read_to_string(&java_ver_path) {
            Ok(v) if v.trim() == java.version => {
                log::info!("[check_modpack_version] Java OK: {}", v.trim());
                true
            }
            Ok(v) => {
                log::info!("[check_modpack_version] Java mismatch: have {}, need {}", v.trim(), java.version);
                false
            }
            Err(_) => {
                log::info!("[check_modpack_version] Java not installed");
                false
            }
        }
    } else {
        true // No Java requirement
    };

    let result = VersionCheckResult {
        needs_update,
        remote_version: manifest.version.clone(),
        installed_version: installed.version.clone(),
        file_count: manifest.files.len(),
        total_size: manifest.total_size,
        java: java_info,
        java_ok,
    };

    // Cache the manifest for later use
    let mut cached = state.manifest.write().await;
    *cached = Some(manifest);

    Ok(result)
}

#[tauri::command]
pub async fn download_modpack(
    state: State<'_, LauncherAppState>,
) -> Result<(), String> {
    let manifest = {
        let cached = state.manifest.read().await;
        cached
            .clone()
            .ok_or_else(|| "No manifest cached – call check_modpack_version first".to_string())?
    };

    let config = state.config.read().await;
    let installed = InstalledManifest::load(&config.game_dir);

    let downloader = state.downloader.lock().await;
    downloader.reset_cancel().await;

    // Download Java runtime if needed
    if let Some(ref java) = manifest.java {
        let java_dir = PathBuf::from(&config.game_dir).join("java");
        let java_ver_path = java_dir.join(".version");
        let need_java = match std::fs::read_to_string(&java_ver_path) {
            Ok(v) if v.trim() == java.version => false,
            _ => true,
        };

        if need_java {
            log::info!("Downloading Java {} for {}", java.version, java.platform);
            let archive_path = java_dir.join("java-archive");
            std::fs::create_dir_all(&java_dir)
                .map_err(|e| format!("Failed to create java dir: {e}"))?;

            downloader
                .download_one(
                    &java.url,
                    &archive_path,
                    "java-runtime",
                    java.size,
                    1,
                    1,
                    &mut 0,
                    java.size,
                )
                .await
                .map_err(|e| format!("Java download failed: {e}"))?;

            // Verify SHA256 (skip if not provided — e.g. Adoptium redirect URLs)
            if !java.sha256.is_empty() {
                let actual = Downloader::sha256_file(&archive_path)
                    .map_err(|e| format!("Java checksum error: {e}"))?;
                if actual != java.sha256 {
                    let _ = std::fs::remove_file(&archive_path);
                    return Err(format!(
                        "Java SHA256 mismatch: expected {}, got {}",
                        java.sha256, actual
                    ));
                }
                log::info!("Java SHA256 verified");
            }

            // Extract the archive (supports .zip and .tar.gz)
            extract_archive(&archive_path, &java_dir)?;
            let _ = std::fs::remove_file(&archive_path);

            // Write version marker
            std::fs::write(&java_ver_path, &java.version)
                .map_err(|e| format!("Failed to write java version: {e}"))?;

            // Auto-update in-memory config to point to the downloaded Java
            let java_exe = java_dir.join("bin").join(if cfg!(windows) { "java.exe" } else { "java" });
            if java_exe.exists() {
                let mut cfg = state.config.write().await;
                cfg.java_path = java_exe.to_string_lossy().to_string();
                log::info!("Java installed, path set to: {}", java_exe.display());
            }
        }
    }

    // If the version changed entirely, start fresh (prune old files)
    let installed_sha256 = if installed.version != manifest.version {
        // Delete files listed in the prune array
        if let Some(ref prune_patterns) = manifest.prune {
            for pattern in prune_patterns {
                // Simple prefix/suffix matching for basic globs
                let game_dir = PathBuf::from(&config.game_dir);
                if pattern.ends_with("/*") {
                    let dir_path = game_dir.join(pattern.trim_end_matches("/*"));
                    if dir_path.exists() {
                        let _ = std::fs::remove_dir_all(&dir_path);
                        log::info!("Pruned directory: {}", dir_path.display());
                    }
                } else {
                    let file_path = game_dir.join(pattern);
                    if file_path.exists() {
                        let _ = std::fs::remove_file(&file_path);
                        log::info!("Pruned file: {}", file_path.display());
                    }
                }
            }
        }
        std::collections::HashMap::new()
    } else {
        installed.files
    };

    downloader
        .download_files(&manifest, &config.game_dir, &installed_sha256)
        .await
        .map_err(|e| {
            log::error!("Download failed: {e}");
            e.to_string()
        })
}

#[tauri::command]
pub async fn verify_files(
    state: State<'_, LauncherAppState>,
) -> Result<Vec<String>, String> {
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
    let downloader = state.downloader.lock().await;
    downloader.cancel().await;
    Ok(())
}

// ── Game launch command ────────────────────────────────────────

#[tauri::command]
pub async fn launch_game(
    app_handle: AppHandle,
    state: State<'_, LauncherAppState>,
) -> Result<(), String> {
    let config = state.config.read().await;

    // Resolve Java path: configured > auto-detected in game_dir > system "java"
    let java_exe_name = if cfg!(windows) { "java.exe" } else { "java" };
    let bundled_java = PathBuf::from(&config.game_dir)
        .join("java")
        .join("bin")
        .join(java_exe_name);

    let java = if !config.java_path.is_empty() {
        config.java_path.clone()
    } else if bundled_java.exists() {
        bundled_java.to_string_lossy().to_string()
    } else {
        "java".to_string()
    };

    let ram_mb = config.ram_mb;
    let game_dir = config.game_dir.clone();

    log::info!(
        "Launching game: java={java}, ram={ram_mb}MB, dir={game_dir}"
    );

    // Use tauri-plugin-shell to spawn the process
    // The sidecar/spawn approach requires shell permissions
    let args = vec![
        format!("-Xmx{ram_mb}M"),
        format!("-Xms{ram_mb}M"),
        "-Djava.library.path=natives".to_string(),
        "-jar".to_string(),
        "minecraft.jar".to_string(),
    ];

    // Log the command for debugging; actual spawn requires shell plugin setup
    log::info!("Would execute: {java} {}", args.join(" "));

    // TODO: When tauri-plugin-shell is fully configured, use:
    // use tauri_plugin_shell::ShellExt;
    // let shell = app_handle.shell();
    // let cmd = shell.command(&java).args(&args).current_dir(&game_dir);
    // let output = cmd.output().await.map_err(|e| e.to_string())?;

    // For now, emit a message that launching is a stub
    app_handle
        .emit("launch://status", "Launch stub: command prepared but not executed")
        .map_err(|e| e.to_string())?;

    Ok(())
}

// ── Helpers ────────────────────────────────────────────────────

/// Base URL for the launcher API.
fn api_base_url() -> String {
    std::env::var("BLOCKFIELD_API_URL")
        .unwrap_or_else(|_| "http://localhost:3000/api/launcher/v1".to_string())
}

/// Extract a ZIP archive to a target directory.
fn extract_archive(archive_path: &PathBuf, dest_dir: &PathBuf) -> Result<(), String> {
    let file = std::fs::File::open(archive_path)
        .map_err(|e| format!("Failed to open archive: {e}"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Failed to read archive: {e}"))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Archive entry {i} error: {e}"))?;

        let name = entry.name().to_string();
        if entry.is_dir() {
            continue;
        }

        // Strip top-level directory (e.g. "jdk-21.0.5+11/" → "")
        let relative = if let Some(pos) = name.find('/') {
            &name[pos + 1..]
        } else {
            &name
        };

        if relative.is_empty() {
            continue;
        }

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

    log::info!("Extracted {} entries to {}", archive.len(), dest_dir.display());
    Ok(())
}
