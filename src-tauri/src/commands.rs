use crate::config::LauncherConfig;
use crate::download::Downloader;
use crate::manifest::{InstalledManifest, ModpackManifest, VersionCheckResult};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};
use tokio::sync::RwLock;

/// Shared application state managed by Tauri.
pub struct LauncherAppState {
    pub downloader: Arc<tokio::sync::Mutex<Downloader>>,
    pub manifest: RwLock<Option<ModpackManifest>>,
    pub config: RwLock<LauncherConfig>,
    pub app_data_dir: PathBuf,
}

// ── Settings commands ──────────────────────────────────────────

#[tauri::command]
pub fn load_settings(state: State<'_, LauncherAppState>) -> Result<LauncherConfig, String> {
    let mut config = state.config.blocking_read().clone();
    // Auto-detect bundled Java if path is empty
    if config.java_path.is_empty() {
        let java_exe = if cfg!(windows) { "java.exe" } else { "java" };
        let bundled = PathBuf::from(&config.game_dir)
            .join("java")
            .join("bin")
            .join(java_exe);
        if bundled.exists() {
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

    let config = state.config.read().await.clone();
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
        let java_ver_path = std::path::PathBuf::from(&config.game_dir)
            .join("java")
            .join(".version");
        match std::fs::read_to_string(&java_ver_path) {
            Ok(v) if v.trim() == java.version => {
                log::info!("[check_modpack_version] Java OK: {}", v.trim());
                true
            }
            Ok(v) => {
                log::info!(
                    "[check_modpack_version] Java mismatch: have {}, need {}",
                    v.trim(),
                    java.version
                );
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

    // Check Forge installation
    let game_dir = PathBuf::from(&config.game_dir);
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

            let mut java_downloaded = 0;
            downloader
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

            extract_archive(&archive_path, &java_dir)?;
            let _ = std::fs::remove_file(&archive_path);
            std::fs::write(&java_ver_path, &java.version)
                .map_err(|e| format!("Failed to write java version: {e}"))?;

            let java_exe =
                java_dir
                    .join("bin")
                    .join(if cfg!(windows) { "java.exe" } else { "java" });
            if java_exe.exists() {
                let java_path_str = java_exe.to_string_lossy().to_string();
                let mut cfg = state.config.write().await;
                cfg.java_path = java_path_str.clone();
                config.java_path = java_path_str.clone();
                let _ = crate::config::save_config(&state.app_data_dir, &cfg);
                log::info!("Java installed, path saved: {java_path_str}");
            }
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
            let installer_path = PathBuf::from(&config.game_dir).join("forge-installer.jar");
            let mut forge_downloaded = 0;
            downloader
                .download_one(
                    &forge.url,
                    &installer_path,
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
                let actual = Downloader::sha256_file(&installer_path)
                    .map_err(|e| format!("Forge checksum error: {e}"))?;
                if actual != forge.sha256 {
                    let _ = std::fs::remove_file(&installer_path);
                    return Err(format!(
                        "Forge SHA256 mismatch: expected {}, got {}",
                        forge.sha256, actual
                    ));
                }
            }

            // Run Forge installer
            log::info!("Running Forge installer...");

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

            let java_bin = if cfg!(windows) { "java.exe" } else { "java" };
            let java = PathBuf::from(&config.game_dir)
                .join("java")
                .join("bin")
                .join(java_bin);
            let java = if java.exists() {
                java.to_string_lossy().to_string()
            } else {
                "java".to_string()
            };

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
        })?;

    let forge_version_id = manifest
        .forge
        .as_ref()
        .map(|forge| crate::minecraft::forge_version_id(&forge.version));
    crate::minecraft::ensure_launch_dependencies(
        &downloader,
        &PathBuf::from(&config.game_dir),
        &manifest.minecraft_version,
        forge_version_id.as_deref(),
    )
    .await
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
    let downloader = state.downloader.lock().await;
    downloader.cancel().await;
    Ok(())
}

// ── Game launch command ────────────────────────────────────────

#[tauri::command]
pub async fn launch_game(
    _app_handle: AppHandle,
    state: State<'_, LauncherAppState>,
) -> Result<(), String> {
    let config = state.config.read().await.clone();
    let manifest = state.manifest.read().await.clone();

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
        .unwrap_or_else(|| "1.20.1".to_string());

    log::info!("Launching game: java={java}, ram={ram_mb}MB, dir={game_dir}");

    {
        let downloader = state.downloader.lock().await;
        downloader.reset_cancel().await;
        crate::minecraft::ensure_launch_dependencies(
            &downloader,
            &game_dir_path,
            &minecraft_version,
            forge_version_id.as_deref(),
        )
        .await?;
    }

    let args = crate::minecraft::build_launch_args(
        &game_dir_path,
        ram_mb,
        &minecraft_version,
        forge_version_id.as_deref(),
    )?;

    log::info!("Spawning: {java} {}", args.join(" "));

    let child = std::process::Command::new(&java)
        .args(&args)
        .current_dir(&game_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to spawn game process: {e}"))?;
    log::info!("Game process spawned (pid {})", child.id());

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
    let file =
        std::fs::File::open(archive_path).map_err(|e| format!("Failed to open archive: {e}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Failed to read archive: {e}"))?;

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

    log::info!(
        "Extracted {} entries to {}",
        archive.len(),
        dest_dir.display()
    );
    Ok(())
}
