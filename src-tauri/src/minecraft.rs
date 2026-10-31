use crate::download::Downloader;
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

// Runtime first (dotenvy for local dev), then compile-time (CI build), then hardcoded.
fn version_manifest_url() -> String {
    std::env::var("BLOCKFIELD_MOJANG_MANIFEST_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| option_env!("BLOCKFIELD_MOJANG_MANIFEST_URL").map(String::from))
        .unwrap_or_else(|| {
            "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json".to_string()
        })
}

fn asset_base_url() -> String {
    std::env::var("BLOCKFIELD_MOJANG_ASSET_BASE_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| option_env!("BLOCKFIELD_MOJANG_ASSET_BASE_URL").map(String::from))
        .unwrap_or_else(|| "https://resources.download.minecraft.net".to_string())
}

fn fabric_meta_url() -> String {
    std::env::var("BLOCKFIELD_FABRIC_META_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| option_env!("BLOCKFIELD_FABRIC_META_URL").map(String::from))
        .unwrap_or_else(|| "https://meta.fabricmc.net".to_string())
        .trim_end_matches('/')
        .to_string()
}

fn launcher_name() -> String {
    std::env::var("BLOCKFIELD_LAUNCHER_NAME")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| option_env!("BLOCKFIELD_LAUNCHER_NAME").map(String::from))
        .unwrap_or_else(|| "BlockfieldLauncher".to_string())
}

#[derive(Debug, Clone)]
struct Artifact {
    path: String,
    url: String,
    size: u64,
    /// Trusted checksum from the version metadata (Mojang ships SHA-1 for every
    /// library, the client jar and every asset object). `None` only for Fabric
    /// maven coordinates, which carry no hash.
    sha1: Option<String>,
}

/// Version id in the Fabric launch profile, e.g. `fabric-loader-0.19.3-1.21.1`.
pub fn loader_version_id(minecraft: &str, loader: &str) -> String {
    format!("fabric-loader-{loader}-{minecraft}")
}

pub fn find_loader_version_id(game_dir: &Path) -> Option<String> {
    let versions_dir = game_dir.join("versions");
    let entries = std::fs::read_dir(versions_dir).ok()?;

    entries.filter_map(Result::ok).find_map(|entry| {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("fabric-loader-") {
            return None;
        }

        let json = entry.path().join(format!("{name}.json"));
        json.exists().then_some(name)
    })
}

pub fn loader_profile_exists(game_dir: &Path, loader_version_id: &str) -> bool {
    version_json_path(game_dir, loader_version_id).exists()
}

/// A profile that exists but does not parse (killed launcher mid-download) must
/// be re-fetched, not accepted: every later update would fail on the same file.
pub fn loader_profile_valid(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(json) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    json.get("mainClass")
        .and_then(Value::as_str)
        .is_some_and(|main| !main.is_empty())
}

/// Fabric ships no installer: the launch profile is a plain version json served by the meta API.
pub async fn ensure_loader_profile(
    game_dir: &Path,
    minecraft: &str,
    loader: &str,
) -> Result<(), String> {
    let id = loader_version_id(minecraft, loader);
    let path = version_json_path(game_dir, &id);
    if loader_profile_valid(&path) {
        return Ok(());
    }
    let url = format!(
        "{}/v2/versions/loader/{minecraft}/{loader}/profile/json",
        fabric_meta_url()
    );
    download_text_to(&url, &path).await
}

pub fn has_launch_dependencies(
    game_dir: &Path,
    minecraft_version: &str,
    loader_version_id: Option<&str>,
) -> bool {
    let vanilla_json_path = version_json_path(game_dir, minecraft_version);
    let vanilla_json = match read_json_file(&vanilla_json_path) {
        Ok(json) => json,
        Err(_) => return false,
    };

    if !version_jar_path(game_dir, minecraft_version).exists() {
        return false;
    }

    if !artifacts_exist(game_dir, &[&vanilla_json]) {
        return false;
    }

    if !asset_objects_exist(game_dir, &vanilla_json) {
        return false;
    }

    if let Some(loader_version_id) = loader_version_id {
        let loader_json_path = version_json_path(game_dir, loader_version_id);
        let loader_json = match read_json_file(&loader_json_path) {
            Ok(json) => json,
            Err(_) => return false,
        };

        if !artifacts_exist(game_dir, &[&loader_json]) {
            return false;
        }
    }

    let (_, native_libraries) = collect_library_artifacts(&vanilla_json);
    if !native_libraries.is_empty() && !natives_dir(game_dir, minecraft_version).exists() {
        return false;
    }

    true
}

pub async fn ensure_launch_dependencies(
    downloader: &Downloader,
    app_handle: &AppHandle,
    game_dir: &Path,
    minecraft_version: &str,
    loader_version_id: Option<&str>,
) -> Result<(), String> {
    std::fs::create_dir_all(game_dir).map_err(|e| format!("Failed to create game dir: {e}"))?;

    emit(app_handle, "runtime", "Fetching version manifest", false);
    ensure_version_json(game_dir, minecraft_version).await?;
    let vanilla_json = read_json_file(&version_json_path(game_dir, minecraft_version))?;

    emit(app_handle, "runtime", "Downloading Minecraft client", true);
    ensure_client_jar(downloader, game_dir, minecraft_version, &vanilla_json).await?;

    let mut jsons = vec![vanilla_json.clone()];
    if let Some(loader_version_id) = loader_version_id {
        let loader_json_path = version_json_path(game_dir, loader_version_id);
        if !loader_json_path.exists() {
            return Err(format!(
                "Loader version JSON missing: {}",
                loader_json_path.display()
            ));
        }
        jsons.push(read_json_file(&loader_json_path)?);
    }

    let mut libraries = Vec::new();
    let mut native_libraries = Vec::new();
    for json in &jsons {
        let (mut libs, mut natives) = collect_library_artifacts(json);
        libraries.append(&mut libs);
        native_libraries.append(&mut natives);
    }

    libraries = dedupe_artifacts(libraries);
    native_libraries = dedupe_artifacts(native_libraries);

    // Libraries + assets are independent — run them concurrently.
    emit(
        app_handle,
        "runtime",
        &format!("Downloading {} libraries + game assets", libraries.len()),
        true,
    );

    let libs_dir = game_dir.join("libraries");
    let dest_natives_dir = natives_dir(game_dir, minecraft_version);
    let _ = std::fs::create_dir_all(&dest_natives_dir);
    let vanilla = &vanilla_json;

    let (lib_result, asset_result) = tokio::join!(
        async {
            download_artifacts(downloader, &libs_dir, "libraries", &libraries).await?;

            if !native_libraries.is_empty() {
                download_artifacts(downloader, &libs_dir, "native libraries", &native_libraries)
                    .await?;
                extract_native_libraries(game_dir, minecraft_version, &native_libraries)?;
            }
            Ok::<_, String>(())
        },
        async { ensure_assets(downloader, game_dir, vanilla).await },
    );

    lib_result?;
    asset_result?;

    Ok(())
}

pub async fn prepare_download_plan(
    game_dir: &Path,
    minecraft_version: &str,
    loader_version_id: Option<&str>,
) -> Result<u64, String> {
    ensure_version_json(game_dir, minecraft_version).await?;
    let vanilla_json = read_json_file(&version_json_path(game_dir, minecraft_version))?;

    let mut artifacts = Vec::new();
    let client = &vanilla_json["downloads"]["client"];
    if let (Some(url), Some(size)) = (client["url"].as_str(), client["size"].as_u64()) {
        let artifact = Artifact {
            path: format!("{minecraft_version}.jar"),
            url: url.to_string(),
            size,
            sha1: client["sha1"].as_str().map(String::from),
        };
        let root = game_dir.join("versions").join(minecraft_version);
        if artifact_needs_download(&root, &artifact) {
            artifacts.push((root, artifact));
        }
    }

    let mut jsons = vec![vanilla_json.clone()];
    if let Some(loader_version_id) = loader_version_id {
        jsons.push(read_json_file(&version_json_path(
            game_dir,
            loader_version_id,
        ))?);
    }
    let mut libraries = Vec::new();
    for json in &jsons {
        let (libs, natives) = collect_library_artifacts(json);
        libraries.extend(libs);
        libraries.extend(natives);
    }
    for artifact in dedupe_artifacts(libraries) {
        let root = game_dir.join("libraries");
        if artifact_needs_download(&root, &artifact) {
            artifacts.push((root, artifact));
        }
    }

    let asset_index = &vanilla_json["assetIndex"];
    let asset_id = asset_index["id"]
        .as_str()
        .ok_or_else(|| "Missing Minecraft asset index id".to_string())?;
    let asset_url = asset_index["url"]
        .as_str()
        .ok_or_else(|| "Missing Minecraft asset index URL".to_string())?;
    let index_path = game_dir
        .join("assets")
        .join("indexes")
        .join(format!("{asset_id}.json"));
    if !index_path.exists() {
        download_text_to(asset_url, &index_path).await?;
    }
    let index_json = read_json_file(&index_path)?;
    let asset_root = game_dir.join("assets").join("objects");
    for artifact in collect_asset_artifacts(&index_json) {
        if artifact_needs_download(&asset_root, &artifact) {
            artifacts.push((asset_root.clone(), artifact));
        }
    }

    Ok(artifacts
        .into_iter()
        .map(|(_, artifact)| artifact.size)
        .sum())
}

fn emit(app_handle: &AppHandle, phase: &str, message: &str, cancelable: bool) {
    #[derive(Serialize, Clone)]
    struct Payload<'a> {
        phase: &'a str,
        message: &'a str,
        cancelable: bool,
    }
    let _ = app_handle.emit(
        "launcher://status",
        Payload {
            phase,
            message,
            cancelable,
        },
    );
}

pub fn build_launch_args(
    game_dir: &Path,
    ram_mb: u32,
    minecraft_version: &str,
    loader_version_id: Option<&str>,
    identity: &crate::commands::GameIdentity,
    quick_play_server: Option<&str>,
) -> Result<Vec<String>, String> {
    let vanilla_json = read_json_file(&version_json_path(game_dir, minecraft_version))?;
    let loader_json = loader_version_id
        .map(|id| read_json_file(&version_json_path(game_dir, id)))
        .transpose()?;

    let version_name = loader_version_id.unwrap_or(minecraft_version);
    let main_class = loader_json
        .as_ref()
        .and_then(|json| json["mainClass"].as_str())
        .or_else(|| vanilla_json["mainClass"].as_str())
        .ok_or_else(|| "Missing Minecraft mainClass".to_string())?;

    let classpath = build_classpath(
        game_dir,
        minecraft_version,
        &vanilla_json,
        loader_json.as_ref(),
    );
    let asset_index = vanilla_json["assetIndex"]["id"]
        .as_str()
        .or_else(|| vanilla_json["assets"].as_str())
        .unwrap_or("legacy");

    let vars = launch_vars(
        game_dir,
        minecraft_version,
        version_name,
        asset_index,
        &classpath,
        identity,
    );

    // Let the selected JVM tune GC for the actual heap and hardware, as Prism does by default.
    let mut args = vec![
        format!("-Xmx{ram_mb}M"),
        format!("-Xms{}M", ram_mb.min(512)),
    ];

    args.extend(crate::dev::jvm_args()); // empty in a production build

    let mut jvm_args = collect_arguments(&vanilla_json, "jvm");
    if let Some(ref loader_json) = loader_json {
        jvm_args.extend(collect_arguments(loader_json, "jvm"));
    }

    for arg in jvm_args {
        args.push(expand_placeholders(&arg, &vars)?);
    }

    args.push(main_class.to_string());

    let mut game_args = collect_arguments(&vanilla_json, "game");
    if let Some(ref loader_json) = loader_json {
        game_args.extend(collect_arguments(loader_json, "game"));
    }

    for arg in game_args {
        args.push(expand_placeholders(&arg, &vars)?);
    }
    // Vanilla 1.20+ Quick Play: straight into the server, no server list needed.
    if let Some(server) = quick_play_server {
        args.push("--quickPlayMultiplayer".to_string());
        args.push(server.to_string());
    }

    Ok(args)
}

async fn ensure_version_json(game_dir: &Path, minecraft_version: &str) -> Result<(), String> {
    let path = version_json_path(game_dir, minecraft_version);
    if path.exists() {
        return Ok(());
    }

    let manifest = fetch_json(&version_manifest_url()).await?;
    let url = manifest["versions"]
        .as_array()
        .and_then(|versions| {
            versions.iter().find_map(|version| {
                (version["id"].as_str() == Some(minecraft_version))
                    .then(|| version["url"].as_str())
                    .flatten()
            })
        })
        .ok_or_else(|| format!("Minecraft version not found: {minecraft_version}"))?;

    download_text_to(url, &path).await
}

async fn ensure_client_jar(
    downloader: &Downloader,
    game_dir: &Path,
    minecraft_version: &str,
    version_json: &Value,
) -> Result<(), String> {
    let client = &version_json["downloads"]["client"];
    let url = client["url"]
        .as_str()
        .ok_or_else(|| "Missing Minecraft client download URL".to_string())?;
    let size = client["size"].as_u64().unwrap_or(0);
    let artifact = Artifact {
        path: format!("{minecraft_version}.jar"),
        url: url.to_string(),
        size,
        sha1: client["sha1"].as_str().map(String::from),
    };

    download_artifacts(
        downloader,
        &game_dir.join("versions").join(minecraft_version),
        "minecraft client",
        &[artifact],
    )
    .await
}

async fn ensure_assets(
    downloader: &Downloader,
    game_dir: &Path,
    version_json: &Value,
) -> Result<(), String> {
    let asset_index = &version_json["assetIndex"];
    let asset_id = asset_index["id"]
        .as_str()
        .ok_or_else(|| "Missing Minecraft asset index id".to_string())?;
    let asset_url = asset_index["url"]
        .as_str()
        .ok_or_else(|| "Missing Minecraft asset index URL".to_string())?;

    let index_path = game_dir
        .join("assets")
        .join("indexes")
        .join(format!("{asset_id}.json"));
    if !index_path.exists() {
        download_text_to(asset_url, &index_path).await?;
    }

    let index_json = read_json_file(&index_path)?;
    let artifacts = collect_asset_artifacts(&index_json);
    download_artifacts(
        downloader,
        &game_dir.join("assets").join("objects"),
        "assets",
        &artifacts,
    )
    .await
}

async fn fetch_json(url: &str) -> Result<Value, String> {
    let bytes = fetch_bytes(url).await?;
    serde_json::from_slice(&bytes).map_err(|e| format!("Failed to parse JSON from {url}: {e}"))
}

async fn download_text_to(url: &str, path: &Path) -> Result<(), String> {
    let bytes = fetch_bytes(url).await?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }

    let partial = crate::download::partial_path(path);
    std::fs::write(&partial, bytes)
        .map_err(|e| format!("Failed to write {}: {e}", partial.display()))?;
    crate::download::activate_partial(&partial, path)
        .map_err(|e| format!("Failed to activate {}: {e}", path.display()))
}

/// Re-download the version json and asset index when their bytes no longer
/// match the manifest pins, so per-file hash checks trust fresh metadata.
/// A corrupt manifest file heals here instead of aborting the verify.
pub async fn heal_runtime_manifests(
    game_dir: &Path,
    minecraft_version: &str,
) -> Result<(), String> {
    let manifest = fetch_json(&version_manifest_url()).await?;
    let entry = manifest["versions"]
        .as_array()
        .and_then(|versions| {
            versions
                .iter()
                .find(|version| version["id"].as_str() == Some(minecraft_version))
        })
        .ok_or_else(|| format!("Minecraft version not found: {minecraft_version}"))?;
    let url = entry["url"]
        .as_str()
        .ok_or_else(|| format!("Minecraft version has no URL: {minecraft_version}"))?;
    let path = version_json_path(game_dir, minecraft_version);
    let fresh = entry["sha1"]
        .as_str()
        .is_none_or(|sha1| file_matches_sha1(&path, sha1));
    if !fresh {
        download_text_to(url, &path).await?;
    }
    let vanilla = read_json_file(&path)?;
    let asset_index = &vanilla["assetIndex"];
    if let (Some(id), Some(url), Some(sha1)) = (
        asset_index["id"].as_str(),
        asset_index["url"].as_str(),
        asset_index["sha1"].as_str(),
    ) {
        let index_path = game_dir
            .join("assets")
            .join("indexes")
            .join(format!("{id}.json"));
        if !file_matches_sha1(&index_path, sha1) {
            download_text_to(url, &index_path).await?;
        }
    }
    Ok(())
}

/// Blocking hash scan plus deletion, designed for `spawn_blocking`: removes
/// every same-size-corrupt runtime file and returns what was removed.
pub fn heal_corrupt_runtime_files(
    game_dir: &Path,
    minecraft_version: &str,
    loader_version_id: Option<&str>,
) -> Result<Vec<PathBuf>, String> {
    let corrupt = find_corrupt_runtime_files(game_dir, minecraft_version, loader_version_id)?;
    for path in &corrupt {
        let _ = std::fs::remove_file(path);
    }
    Ok(corrupt)
}
/// Version metadata, loader profiles and the asset index are each fetched with
/// a single attempt elsewhere; bulk file downloads retry 3x. One lost packet
/// must not fail the whole update, so metadata gets the same 3 attempts.
async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    let mut last_error = String::new();
    for attempt in 1..=3 {
        match fetch_bytes_once(url).await {
            Ok(bytes) => return Ok(bytes),
            Err(error) => {
                last_error = error;
                if attempt < 3 {
                    tokio::time::sleep(std::time::Duration::from_millis(250 * attempt)).await;
                }
            }
        }
    }
    Err(format!("{last_error} (after 3 attempts)"))
}

async fn fetch_bytes_once(url: &str) -> Result<Vec<u8>, String> {
    let bytes = metadata_client()?
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch {url}: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Failed to fetch {url}: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("Failed to read {url}: {e}"))?;
    Ok(bytes.to_vec())
}

fn metadata_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))
}

async fn download_artifacts(
    downloader: &Downloader,
    root: &Path,
    label: &str,
    artifacts: &[Artifact],
) -> Result<(), String> {
    let missing: Vec<(usize, Artifact)> = artifacts
        .iter()
        .enumerate()
        .filter(|(_, a)| artifact_needs_download(root, a))
        .map(|(i, a)| (i, a.clone()))
        .collect();

    if missing.is_empty() {
        return Ok(());
    }

    let total_bytes: u64 = missing.iter().map(|(_, a)| a.size).sum();
    let file_count = missing.len();
    let root = root.to_path_buf();
    let label = label.to_string();

    // Mostly tiny asset objects from Mojang's CDN: throughput is latency-bound, so fan out wide.
    const PARALLEL: usize = 24;

    let results: Vec<Result<(), String>> = futures_util::stream::iter(missing)
        .map(|(index, artifact)| {
            let root = root.clone();
            let label = label.clone();
            async move {
                let mut downloaded: u64 = 0;
                let dest = root.join(&artifact.path);
                let partial = crate::download::partial_path(&dest);
                let file_label = format!("{}/{}", label, artifact.path);
                downloader
                    .download_one(
                        &artifact.url,
                        &partial,
                        &file_label,
                        artifact.size,
                        index + 1,
                        file_count,
                        &mut downloaded,
                        total_bytes,
                    )
                    .await
                    .map_err(|e| format!("Failed to download {}: {}", file_label, e))?;

                if artifact.size > 0 {
                    let actual_size = partial
                        .metadata()
                        .map_err(|e| format!("Failed to stat {}: {}", partial.display(), e))?
                        .len();
                    if actual_size != artifact.size {
                        let _ = std::fs::remove_file(&partial);
                        return Err(format!(
                            "Size mismatch for {}: expected {}, got {}",
                            file_label, artifact.size, actual_size
                        ));
                    }
                }
                verify_downloaded_hash(&partial, &artifact, &file_label)?;
                crate::download::activate_partial(&partial, &dest)
                    .map_err(|e| format!("Failed to activate {}: {}", dest.display(), e))?;
                Ok(())
            }
        })
        .buffer_unordered(PARALLEL)
        .collect()
        .await;

    results.into_iter().collect()
}

fn artifact_needs_download(root: &Path, artifact: &Artifact) -> bool {
    let path = root.join(&artifact.path);
    match path.metadata() {
        Ok(metadata) if artifact.size == 0 => !metadata.is_file(),
        Ok(metadata) => !metadata.is_file() || metadata.len() != artifact.size,
        Err(_) => true,
    }
}

/// Size already matched: reject same-size corruption against the metadata hash.
/// Artifacts without a hash (Fabric maven coordinates) keep the size check.
fn verify_downloaded_hash(partial: &Path, artifact: &Artifact, label: &str) -> Result<(), String> {
    let Some(expected) = artifact.sha1.as_deref() else {
        return Ok(());
    };
    match crate::download::Downloader::sha1_file(partial) {
        Ok(actual) if actual.eq_ignore_ascii_case(expected) => Ok(()),
        _ => {
            let _ = std::fs::remove_file(partial);
            Err(format!("SHA-1 mismatch for {label}: content changed"))
        }
    }
}

fn file_matches_sha1(path: &Path, sha1: &str) -> bool {
    crate::download::Downloader::sha1_file(path)
        .is_ok_and(|actual| actual.eq_ignore_ascii_case(sha1))
}

/// Every runtime file the local metadata pins with a hash, reported when it
/// exists but its bytes no longer match (size-only checks call it healthy).
/// Missing files are not listed: the downloader already fetches those.
pub fn find_corrupt_runtime_files(
    game_dir: &Path,
    minecraft_version: &str,
    loader_version_id: Option<&str>,
) -> Result<Vec<PathBuf>, String> {
    let mut corrupt = Vec::new();
    let vanilla_json = read_json_file(&version_json_path(game_dir, minecraft_version))?;

    let client = &vanilla_json["downloads"]["client"];
    if let Some(expected) = client["sha1"].as_str() {
        let jar = version_jar_path(game_dir, minecraft_version);
        if jar.is_file() && !file_matches_sha1(&jar, expected) {
            corrupt.push(jar);
        }
    }

    let mut jsons = vec![vanilla_json.clone()];
    if let Some(loader_version_id) = loader_version_id {
        jsons.push(read_json_file(&version_json_path(
            game_dir,
            loader_version_id,
        ))?);
    }
    let libs_root = game_dir.join("libraries");
    for json in &jsons {
        let (libraries, natives) = collect_library_artifacts(json);
        for artifact in libraries.iter().chain(natives.iter()) {
            let Some(expected) = artifact.sha1.as_deref() else {
                continue;
            };
            let path = libs_root.join(&artifact.path);
            if path.is_file() && !file_matches_sha1(&path, expected) {
                corrupt.push(path);
            }
        }
    }

    if let Some(asset_id) = vanilla_json["assetIndex"]["id"].as_str() {
        let index_path = game_dir
            .join("assets")
            .join("indexes")
            .join(format!("{asset_id}.json"));
        if index_path.is_file() {
            let index_json = read_json_file(&index_path)?;
            let objects_root = game_dir.join("assets").join("objects");
            for artifact in collect_asset_artifacts(&index_json) {
                let Some(expected) = artifact.sha1.as_deref() else {
                    continue;
                };
                let path = objects_root.join(&artifact.path);
                if path.is_file() && !file_matches_sha1(&path, expected) {
                    corrupt.push(path);
                }
            }
        }
    }

    Ok(corrupt)
}

fn collect_library_artifacts(version_json: &Value) -> (Vec<Artifact>, Vec<Artifact>) {
    let mut libraries = Vec::new();
    let mut natives = Vec::new();

    let Some(entries) = version_json["libraries"].as_array() else {
        return (libraries, natives);
    };

    for library in entries {
        if !rules_allow(library) {
            continue;
        }

        if let Some(artifact) = artifact_from_library(library) {
            libraries.push(artifact);
        }

        if let Some(classifier) = native_classifier(library) {
            if let Some(artifact) =
                artifact_from_value(&library["downloads"]["classifiers"][classifier])
            {
                natives.push(artifact);
            }
        }
    }

    (libraries, natives)
}

fn collect_asset_artifacts(index_json: &Value) -> Vec<Artifact> {
    let Some(objects) = index_json["objects"].as_object() else {
        return Vec::new();
    };

    // Assets with identical content share one object; parallel downloads would race on its `.part`.
    // The object key IS the SHA-1, so every asset carries its trusted checksum.
    let artifacts = objects
        .iter()
        .filter(|(name, _)| asset_wanted(name))
        .filter_map(|(_, object)| {
            let hash = object["hash"].as_str()?;
            let prefix = hash.get(0..2)?;
            Some(Artifact {
                path: format!("{prefix}/{hash}"),
                url: format!("{}/{prefix}/{hash}", asset_base_url()),
                size: object["size"].as_u64().unwrap_or(0),
                sha1: Some(hash.to_string()),
            })
        })
        .collect();
    dedupe_artifacts(artifacts)
}

/// The index lists ~120 translations (~90 MB); the game only ever loads the selected one.
fn asset_wanted(name: &str) -> bool {
    match name.strip_prefix("minecraft/lang/") {
        Some(lang) => matches!(lang, "en_us.json" | "ru_ru.json" | "uk_ua.json"),
        None => true,
    }
}

fn artifact_from_library(library: &Value) -> Option<Artifact> {
    if let Some(artifact) = artifact_from_value(&library["downloads"]["artifact"]) {
        return Some(artifact);
    }
    // Fabric lists libraries as maven coordinates plus a repository base, with no `downloads` block.
    // The live profile ships per-library hashes (sha1/md5/sha256/sha512); only
    // entries without one (e.g. intermediary) fall back to size/existence.
    let path = maven_path(library["name"].as_str()?)?;
    let base = library["url"].as_str()?.trim_end_matches('/');
    Some(Artifact {
        url: format!("{base}/{path}"),
        path,
        size: library["size"].as_u64().unwrap_or(0),
        sha1: library["sha1"].as_str().map(String::from),
    })
}

/// `group:artifact:version[:classifier]` -> `group/path/artifact/version/artifact-version[-classifier].jar`
fn maven_path(name: &str) -> Option<String> {
    let mut parts = name.split(':');
    let group = parts.next()?.replace('.', "/");
    let artifact = parts.next()?;
    let version = parts.next()?;
    let classifier = parts.next().map_or(String::new(), |c| format!("-{c}"));
    let path = format!("{group}/{artifact}/{version}/{artifact}-{version}{classifier}.jar");
    // The version json is untrusted input that decides where we write files.
    (!path
        .split('/')
        .any(|part| part.is_empty() || part == ".." || part == "."))
    .then_some(path)
}

fn artifact_from_value(value: &Value) -> Option<Artifact> {
    let path = value["path"].as_str()?;
    let url = value["url"].as_str()?;
    Some(Artifact {
        path: path.to_string(),
        url: url.to_string(),
        size: value["size"].as_u64().unwrap_or(0),
        sha1: value["sha1"].as_str().map(String::from),
    })
}

fn dedupe_artifacts(artifacts: Vec<Artifact>) -> Vec<Artifact> {
    let mut seen = HashSet::new();
    artifacts
        .into_iter()
        .filter(|artifact| seen.insert(artifact.path.clone()))
        .collect()
}

fn artifacts_exist(game_dir: &Path, jsons: &[&Value]) -> bool {
    for json in jsons {
        let (libraries, native_libraries) = collect_library_artifacts(json);
        for artifact in libraries.iter().chain(native_libraries.iter()) {
            if artifact_needs_download(&game_dir.join("libraries"), artifact) {
                return false;
            }
        }
    }
    true
}

fn asset_objects_exist(game_dir: &Path, version_json: &Value) -> bool {
    let Some(asset_id) = version_json["assetIndex"]["id"].as_str() else {
        return false;
    };
    let index_path = game_dir
        .join("assets")
        .join("indexes")
        .join(format!("{asset_id}.json"));
    let Ok(index_json) = read_json_file(&index_path) else {
        return false;
    };

    collect_asset_artifacts(&index_json).iter().all(|artifact| {
        !artifact_needs_download(&game_dir.join("assets").join("objects"), artifact)
    })
}

fn extract_native_libraries(
    game_dir: &Path,
    minecraft_version: &str,
    native_libraries: &[Artifact],
) -> Result<(), String> {
    let dest_dir = natives_dir(game_dir, minecraft_version);
    let tracked = crate::installed_files::ExternalInstall::new(
        game_dir,
        &format!("natives/{minecraft_version}"),
    )?;
    if dest_dir.exists() {
        std::fs::remove_dir_all(&dest_dir)
            .map_err(|e| format!("Failed to clear natives dir: {e}"))?;
    }
    std::fs::create_dir_all(&dest_dir).map_err(|e| format!("Failed to create natives dir: {e}"))?;

    for artifact in native_libraries {
        let path = game_dir.join("libraries").join(&artifact.path);
        let file = std::fs::File::open(&path)
            .map_err(|e| format!("Failed to open native library {}: {e}", path.display()))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| format!("Failed to read native library {}: {e}", path.display()))?;

        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| format!("Native library entry {i} error: {e}"))?;
            // Entries are always written as fresh regular files without their stored mode, so
            // links are never created and SUID/SGID bits are never applied.
            if entry.is_dir() || entry.is_symlink() {
                continue;
            }

            let name = entry
                .name()
                .map_err(|e| format!("Native library entry {i} name error: {e}"))?
                .replace('\\', "/");
            if name.starts_with("META-INF/")
                || name.starts_with('/')
                || name.contains("..")
                || name.contains(':')
            {
                continue;
            }

            let dest = dest_dir.join(name);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
            }
            let mut out = std::fs::File::create(&dest)
                .map_err(|e| format!("Failed to create native {}: {e}", dest.display()))?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|e| format!("Failed to extract native {}: {e}", dest.display()))?;
        }
    }

    tracked.finish_tree(&dest_dir)
}

fn build_classpath(
    game_dir: &Path,
    minecraft_version: &str,
    vanilla_json: &Value,
    loader_json: Option<&Value>,
) -> String {
    let mut artifacts = collect_library_artifacts(vanilla_json).0;
    if let Some(loader_json) = loader_json {
        artifacts.extend(collect_library_artifacts(loader_json).0);
    }
    artifacts = dedupe_artifacts(artifacts);

    let mut paths: Vec<String> = artifacts
        .into_iter()
        .map(|artifact| {
            game_dir
                .join("libraries")
                .join(artifact.path)
                .to_string_lossy()
                .to_string()
        })
        .collect();
    paths.push(
        version_jar_path(game_dir, minecraft_version)
            .to_string_lossy()
            .to_string(),
    );

    paths.join(classpath_separator())
}

fn collect_arguments(version_json: &Value, kind: &str) -> Vec<String> {
    if let Some(arguments) = version_json["arguments"][kind].as_array() {
        let mut out = Vec::new();
        for argument in arguments {
            push_argument_value(argument, &mut out);
        }
        return out;
    }

    if kind == "game" {
        if let Some(legacy) = version_json["minecraftArguments"].as_str() {
            return legacy.split_whitespace().map(String::from).collect();
        }
    }

    Vec::new()
}

fn push_argument_value(argument: &Value, out: &mut Vec<String>) {
    if let Some(value) = argument.as_str() {
        out.push(value.to_string());
        return;
    }

    if !argument.is_object() || !rules_allow(argument) {
        return;
    }

    let value = &argument["value"];
    if let Some(value) = value.as_str() {
        out.push(value.to_string());
    } else if let Some(values) = value.as_array() {
        out.extend(values.iter().filter_map(|v| v.as_str().map(String::from)));
    }
}

fn launch_vars(
    game_dir: &Path,
    minecraft_version: &str,
    version_name: &str,
    asset_index: &str,
    classpath: &str,
    identity: &crate::commands::GameIdentity,
) -> HashMap<String, String> {
    let game_dir = game_dir.to_string_lossy().to_string();
    let assets_dir = PathBuf::from(&game_dir)
        .join("assets")
        .to_string_lossy()
        .to_string();
    let libraries_dir = PathBuf::from(&game_dir)
        .join("libraries")
        .to_string_lossy()
        .to_string();
    let natives_dir = natives_dir(Path::new(&game_dir), minecraft_version)
        .to_string_lossy()
        .to_string();
    HashMap::from([
        ("auth_player_name".to_string(), identity.username.clone()),
        ("auth_uuid".to_string(), identity.uuid.replace('-', "")),
        (
            "auth_access_token".to_string(),
            identity.access_token.clone(),
        ),
        ("clientid".to_string(), "0".to_string()),
        ("auth_xuid".to_string(), "0".to_string()),
        ("user_type".to_string(), "msa".to_string()),
        ("user_properties".to_string(), "{}".to_string()),
        ("version_name".to_string(), version_name.to_string()),
        ("version_type".to_string(), "release".to_string()),
        ("game_directory".to_string(), game_dir),
        ("assets_root".to_string(), assets_dir),
        ("assets_index_name".to_string(), asset_index.to_string()),
        ("library_directory".to_string(), libraries_dir),
        ("natives_directory".to_string(), natives_dir),
        ("classpath".to_string(), classpath.to_string()),
        (
            "classpath_separator".to_string(),
            classpath_separator().to_string(),
        ),
        ("launcher_name".to_string(), launcher_name().to_string()),
        (
            "launcher_version".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
        ),
        ("quickPlayPath".to_string(), "".to_string()),
    ])
}

fn expand_placeholders(input: &str, vars: &HashMap<String, String>) -> Result<String, String> {
    let mut expanded = input.to_string();
    for (key, value) in vars {
        expanded = expanded.replace(&format!("${{{key}}}"), value);
    }

    if expanded.contains("${") {
        return Err(format!("Unresolved launch placeholder: {expanded}"));
    }

    Ok(expanded)
}

fn rules_allow(value: &Value) -> bool {
    let Some(rules) = value["rules"].as_array() else {
        return true;
    };

    let mut allowed = false;
    for rule in rules {
        if rule_matches(rule) {
            allowed = rule["action"].as_str() == Some("allow");
        }
    }
    allowed
}

fn rule_matches(rule: &Value) -> bool {
    if let Some(os) = rule["os"].as_object() {
        if let Some(name) = os.get("name").and_then(Value::as_str) {
            if name != current_os_name() {
                return false;
            }
        }

        if let Some(arch) = os.get("arch").and_then(Value::as_str) {
            if arch == "x86" && std::env::consts::ARCH != "x86" {
                return false;
            }
        }
    }

    if rule["features"].is_object() {
        return false;
    }

    true
}

fn native_classifier(library: &Value) -> Option<String> {
    let natives = library["natives"].as_object()?;
    let classifier = natives.get(current_os_name())?.as_str()?;
    Some(classifier.replace("${arch}", native_arch_suffix()))
}

fn read_json_file(path: &Path) -> Result<Value, String> {
    let json = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    serde_json::from_str(&json).map_err(|e| format!("Failed to parse {}: {e}", path.display()))
}

fn version_json_path(game_dir: &Path, version: &str) -> PathBuf {
    game_dir
        .join("versions")
        .join(version)
        .join(format!("{version}.json"))
}

fn version_jar_path(game_dir: &Path, version: &str) -> PathBuf {
    game_dir
        .join("versions")
        .join(version)
        .join(format!("{version}.jar"))
}

fn natives_dir(game_dir: &Path, minecraft_version: &str) -> PathBuf {
    game_dir.join("natives").join(minecraft_version)
}

fn current_os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

fn native_arch_suffix() -> &'static str {
    if cfg!(target_pointer_width = "64") {
        "64"
    } else {
        "32"
    }
}

fn classpath_separator() -> &'static str {
    if cfg!(windows) {
        ";"
    } else {
        ":"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runtime_plan_counts_missing_client_libraries_and_assets_once() {
        let root =
            std::env::temp_dir().join(format!("blockfield-runtime-plan-{}", std::process::id()));
        let version = "1.20.1";
        let version_path = version_json_path(&root, version);
        let index_path = root.join("assets/indexes/test.json");
        std::fs::create_dir_all(version_path.parent().unwrap()).unwrap();
        std::fs::create_dir_all(index_path.parent().unwrap()).unwrap();
        std::fs::write(
            &version_path,
            serde_json::to_vec(&serde_json::json!({
                "downloads": { "client": { "url": "https://example.invalid/client.jar", "size": 10 } },
                "libraries": [{
                    "downloads": { "artifact": {
                        "path": "com/example/lib.jar",
                        "url": "https://example.invalid/lib.jar",
                        "size": 20
                    }}
                }],
                "assetIndex": { "id": "test", "url": "https://example.invalid/assets.json" }
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            &index_path,
            serde_json::to_vec(&serde_json::json!({
                "objects": {
                    "test": { "hash": "aabb", "size": 30 },
                    "test-copy": { "hash": "aabb", "size": 30 },
                    "minecraft/lang/ru_ru.json": { "hash": "ccdd", "size": 5 },
                    "minecraft/lang/de_de.json": { "hash": "eeff", "size": 500 }
                }
            }))
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            prepare_download_plan(&root, version, None).await.unwrap(),
            65
        );

        let files = [
            (root.join("versions/1.20.1/1.20.1.jar"), 10),
            (root.join("libraries/com/example/lib.jar"), 20),
            (root.join("assets/objects/aa/aabb"), 30),
            (root.join("assets/objects/cc/ccdd"), 5),
        ];
        for (path, size) in &files {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, vec![0; *size]).unwrap();
        }
        assert_eq!(
            prepare_download_plan(&root, version, None).await.unwrap(),
            0
        );

        std::fs::write(&files[2].0, b"partial").unwrap();
        assert_eq!(
            prepare_download_plan(&root, version, None).await.unwrap(),
            30
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn raknet_quick_play_address_is_a_single_unchanged_argument() {
        let root =
            std::env::temp_dir().join(format!("blockfield_raknet_args_{}", std::process::id()));
        let version = "1.21.1";
        let dir = root.join("versions").join(version);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{version}.json")),
            r#"{"mainClass":"net.minecraft.client.main.Main","libraries":[],"arguments":{"game":[],"jvm":[]}}"#).unwrap();
        let identity = crate::commands::GameIdentity {
            username: "TransportTest".into(),
            uuid: "00000000000000000000000000000000".into(),
            access_token: "test".into(),
        };
        let server = "raknet;play.blockfield.pro:25566";
        let args = build_launch_args(&root, 1024, version, None, &identity, Some(server)).unwrap();
        assert_eq!(&args[args.len() - 2..], &["--quickPlayMultiplayer", server]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn profiles_use_separate_game_arguments_and_quick_play_targets() {
        let scratch = tempfile::tempdir().unwrap();
        let mut config = crate::config::LauncherConfig {
            game_dir: scratch.path().join("game").to_string_lossy().into_owned(),
            workshop_game_dir: scratch
                .path()
                .join("workshop")
                .to_string_lossy()
                .into_owned(),
            workshop_server: "raknet;workshop.example:25566".into(),
            ..crate::config::LauncherConfig::default()
        };
        let identity = crate::commands::GameIdentity {
            username: "ProfileTest".into(),
            uuid: "00000000000000000000000000000000".into(),
            access_token: "fake-test-session".into(),
        };
        for (profile, expected_target) in [
            (
                crate::config::ClientProfile::Game,
                "raknet;game.example:25566",
            ),
            (
                crate::config::ClientProfile::Workshop,
                "raknet;workshop.example:25566",
            ),
        ] {
            config.active_profile = profile;
            let active = config.active();
            let root = Path::new(&active.game_dir);
            let version = "1.21.1";
            let directory = root.join("versions").join(version);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(directory.join(format!("{version}.json")), r#"{"mainClass":"net.minecraft.client.main.Main","libraries":[],"arguments":{"game":["--gameDir","${game_directory}"],"jvm":[]}}"#).unwrap();
            let target = active.target("raknet;game.example:25566").unwrap();
            let args =
                build_launch_args(root, 4096, version, None, &identity, target.as_deref()).unwrap();
            let game_directory_arg = args.iter().position(|arg| arg == "--gameDir").unwrap();
            assert_eq!(args[game_directory_arg + 1], active.game_dir);
            assert_eq!(
                &args[args.len() - 2..],
                &["--quickPlayMultiplayer", expected_target]
            );
        }
    }

    #[test]
    fn fabric_maven_libraries_resolve_to_repository_paths() {
        let json = serde_json::json!({"libraries": [
            {"name": "org.ow2.asm:asm:9.10.1", "url": "https://maven.fabricmc.net/", "size": 126151},
            {"name": "net.fabricmc:fabric-loader:0.19.3", "url": "https://maven.fabricmc.net/"},
            {"name": "no-coordinates", "url": "https://maven.fabricmc.net/"},
            {"name": "a:b:../../evil", "url": "https://maven.fabricmc.net/"}
        ]});

        let (libraries, _) = collect_library_artifacts(&json);
        let paths: Vec<_> = libraries.iter().map(|a| a.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "org/ow2/asm/asm/9.10.1/asm-9.10.1.jar",
                "net/fabricmc/fabric-loader/0.19.3/fabric-loader-0.19.3.jar"
            ]
        );
        assert_eq!(
            libraries[0].url,
            "https://maven.fabricmc.net/org/ow2/asm/asm/9.10.1/asm-9.10.1.jar"
        );
        assert_eq!(libraries[0].size, 126151);
        // No size in the profile json: artifact_needs_download falls back to an existence check.
        assert_eq!(libraries[1].size, 0);
        assert_eq!(
            loader_version_id("1.21.1", "0.19.3"),
            "fabric-loader-0.19.3-1.21.1"
        );
    }

    #[test]
    fn has_launch_dependencies_passes_when_no_native_libraries() {
        let root = std::env::temp_dir().join("blockfield_has_launch_deps_test");
        let _ = std::fs::remove_dir_all(&root);
        let version = "1.21.1";
        let v_dir = root.join("versions").join(version);
        std::fs::create_dir_all(&v_dir).unwrap();
        std::fs::write(
            v_dir.join(format!("{version}.json")),
            serde_json::to_vec(&serde_json::json!({
                "libraries": [],
                "assetIndex": { "id": "test" }
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(v_dir.join(format!("{version}.jar")), b"test").unwrap();
        let idx_dir = root.join("assets").join("indexes");
        std::fs::create_dir_all(&idx_dir).unwrap();
        std::fs::write(
            idx_dir.join("test.json"),
            serde_json::to_vec(&serde_json::json!({ "objects": {} })).unwrap(),
        )
        .unwrap();

        assert!(has_launch_dependencies(&root, version, None));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn fabric_library_hashes_are_used_when_present() {
        use sha1::Digest as _;
        let hex = |bytes: &[u8]| crate::download::hex_digest(&sha1::Sha1::digest(bytes));
        let json = serde_json::json!({"libraries": [
            {"name": "org.ow2.asm:asm:9.10.1", "url": "https://maven.fabricmc.net/",
             "size": 7, "sha1": hex(b"correct")},
            {"name": "net.fabricmc:intermediary:1.21.1", "url": "https://maven.fabricmc.net/"}
        ]});

        let (libraries, _) = collect_library_artifacts(&json);
        assert_eq!(libraries.len(), 2);
        assert_eq!(libraries[0].sha1.as_deref(), Some(hex(b"correct").as_str()));
        // The intermediary entry ships no hash: size/existence is all there is.
        assert_eq!(libraries[1].sha1, None);

        // Executed loader code with a same-size corruption must not verify.
        let dir = tempfile::tempdir().unwrap();
        let partial = dir.path().join("asm-9.10.1.jar.part");
        std::fs::write(&partial, b"corrupt").unwrap();
        assert!(verify_downloaded_hash(&partial, &libraries[0], "asm").is_err());
    }

    #[test]
    fn heal_corrupt_runtime_files_removes_mismatches() {
        use sha1::Digest as _;
        let hex = |bytes: &[u8]| crate::download::hex_digest(&sha1::Sha1::digest(bytes));
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let version = "1.20.1";
        let version_dir = root.join("versions").join(version);
        std::fs::create_dir_all(&version_dir).unwrap();
        std::fs::write(
            version_dir.join(format!("{version}.json")),
            serde_json::to_vec(&serde_json::json!({
                "downloads": { "client": {
                    "url": "https://example.invalid/client.jar",
                    "size": 7,
                    "sha1": hex(b"correct")
                }},
                "libraries": [],
                "assetIndex": { "id": "test", "url": "https://example.invalid/assets.json" }
            }))
            .unwrap(),
        )
        .unwrap();
        let jar = version_dir.join(format!("{version}.jar"));
        std::fs::write(&jar, b"corrupt").unwrap();
        let healed = heal_corrupt_runtime_files(root, version, None).unwrap();
        assert_eq!(healed, vec![jar.clone()]);
        assert!(!jar.exists());
        assert!(heal_corrupt_runtime_files(root, version, None)
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn corrupt_runtime_manifests_heal_by_redownload() {
        use sha1::Digest as _;
        let hex = |bytes: &[u8]| crate::download::hex_digest(&sha1::Sha1::digest(bytes));
        // Bind first so the version json can point at the real index URL.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let index_json = br#"{"objects":{}}"#.to_vec();
        let version_json = serde_json::to_vec(&serde_json::json!({
            "downloads": { "client": {
                "url": "https://example.invalid/client.jar", "size": 1 }},
            "libraries": [],
            "assetIndex": {
                "id": "test",
                "url": format!("http://{address}/index.json"),
                "sha1": hex(&index_json)
            }
        }))
        .unwrap();
        let manifest = format!(
            r#"{{"versions":[{{"id":"1.20.1","url":"http://{address}/v.json","sha1":"{}"}}]}}"#,
            hex(&version_json)
        )
        .into_bytes();
        let bodies = vec![manifest, version_json.clone(), index_json.clone()];
        std::thread::spawn(move || {
            for (body, mut stream) in bodies
                .into_iter()
                .zip(listener.incoming().take(3).flatten())
            {
                use std::io::{Read, Write};
                let mut buffer = [0u8; 4096];
                let _ = stream.read(&mut buffer);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        unsafe {
            std::env::set_var(
                "BLOCKFIELD_MOJANG_MANIFEST_URL",
                format!("http://{address}/manifest.json"),
            );
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let version_dir = root.join("versions").join("1.20.1");
        std::fs::create_dir_all(&version_dir).unwrap();
        std::fs::write(version_dir.join("1.20.1.json"), b"truncated{").unwrap();
        let index_path = root.join("assets").join("indexes").join("test.json");
        std::fs::create_dir_all(index_path.parent().unwrap()).unwrap();
        std::fs::write(&index_path, b"stale{}").unwrap();

        heal_runtime_manifests(root, "1.20.1").await.unwrap();
        unsafe {
            std::env::remove_var("BLOCKFIELD_MOJANG_MANIFEST_URL");
        }
        assert_eq!(
            std::fs::read(version_dir.join("1.20.1.json")).unwrap(),
            version_json
        );
        assert_eq!(std::fs::read(&index_path).unwrap(), index_json);
    }

    #[test]
    fn runtime_artifacts_keep_trusted_hashes() {
        let value = serde_json::json!({
            "path": "com/example/lib.jar",
            "url": "https://example.invalid/lib.jar",
            "size": 20,
            "sha1": "abc123"
        });
        let artifact = artifact_from_value(&value).unwrap();
        assert_eq!(artifact.sha1.as_deref(), Some("abc123"));

        let index = serde_json::json!({ "objects": {
            "file": { "hash": "aabbcc", "size": 3 }
        }});
        let assets = collect_asset_artifacts(&index);
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].sha1.as_deref(), Some("aabbcc"));
    }

    #[test]
    fn same_size_corrupt_runtime_file_is_detected() {
        use sha1::Digest as _;
        let hex = |bytes: &[u8]| crate::download::hex_digest(&sha1::Sha1::digest(bytes));
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        let version = "1.20.1";
        let version_dir = root.join("versions").join(version);
        std::fs::create_dir_all(&version_dir).unwrap();
        std::fs::write(
            version_dir.join(format!("{version}.json")),
            serde_json::to_vec(&serde_json::json!({
                "downloads": { "client": {
                    "url": "https://example.invalid/client.jar",
                    "size": 7,
                    "sha1": hex(b"correct")
                }},
                "libraries": [{
                    "downloads": { "artifact": {
                        "path": "com/example/lib.jar",
                        "url": "https://example.invalid/lib.jar",
                        "size": 7,
                        "sha1": hex(b"correct")
                    }}
                }],
                "assetIndex": { "id": "test", "url": "https://example.invalid/assets.json" }
            }))
            .unwrap(),
        )
        .unwrap();
        let index_dir = root.join("assets").join("indexes");
        std::fs::create_dir_all(&index_dir).unwrap();
        std::fs::write(
            index_dir.join("test.json"),
            serde_json::to_vec(&serde_json::json!({ "objects": {
                "file": { "hash": hex(b"correct"), "size": 7 }
            }}))
            .unwrap(),
        )
        .unwrap();

        // Same byte sizes, wrong bytes: size-only checks call this healthy.
        let jar = version_dir.join(format!("{version}.jar"));
        let lib = root.join("libraries/com/example/lib.jar");
        let asset = root.join("assets/objects").join(format!(
            "{}/{}",
            &hex(b"correct")[..2],
            hex(b"correct")
        ));
        for path in [&jar, &lib, &asset] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"corrupt").unwrap();
        }
        let corrupt = find_corrupt_runtime_files(root, version, None).unwrap();
        assert_eq!(corrupt.len(), 3);

        for path in [&jar, &lib, &asset] {
            std::fs::write(path, b"correct").unwrap();
        }
        assert!(find_corrupt_runtime_files(root, version, None)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn downloaded_hash_check_rejects_same_size_mismatch() {
        use sha1::Digest as _;
        let dir = tempfile::tempdir().unwrap();
        let partial = dir.path().join("lib.jar.part");
        std::fs::write(&partial, b"corrupt").unwrap();
        let artifact = Artifact {
            path: "lib.jar".into(),
            url: "https://example.invalid/lib.jar".into(),
            size: 7,
            sha1: Some(crate::download::hex_digest(&sha1::Sha1::digest(b"correct"))),
        };
        assert!(verify_downloaded_hash(&partial, &artifact, "lib").is_err());
        assert!(!partial.exists());
        std::fs::write(&partial, b"correct").unwrap();
        assert!(verify_downloaded_hash(&partial, &artifact, "lib").is_ok());
    }

    #[test]
    fn loader_profile_valid_rejects_truncated_and_wrong_shape() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profile.json");
        assert!(!loader_profile_valid(&path));
        std::fs::write(&path, r#"{"id": "fabric-loader-0.19.3-1.21.1", "trunca"#).unwrap();
        assert!(!loader_profile_valid(&path));
        std::fs::write(&path, r#"{"id": "fabric-loader-0.19.3-1.21.1"}"#).unwrap();
        assert!(!loader_profile_valid(&path));
        std::fs::write(
            &path,
            r#"{"id": "fabric-loader-0.19.3-1.21.1", "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient"}"#,
        )
        .unwrap();
        assert!(loader_profile_valid(&path));
    }

    #[tokio::test]
    async fn corrupt_loader_profile_is_refreshed_not_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Closed port: the refresh attempt must fail loudly instead of Ok.
        unsafe {
            std::env::set_var("BLOCKFIELD_FABRIC_META_URL", "http://127.0.0.1:9");
        }
        let id = loader_version_id("1.21.1", "0.19.3");
        let path = version_json_path(root, &id);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"id": "truncated"#).unwrap();
        let result = ensure_loader_profile(root, "1.21.1", "0.19.3").await;
        unsafe {
            std::env::remove_var("BLOCKFIELD_FABRIC_META_URL");
        }
        assert!(result.is_err());
        // A valid profile still returns early without any network.
        std::fs::write(
            &path,
            r#"{"id": "fabric-loader-0.19.3-1.21.1", "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient"}"#,
        )
        .unwrap();
        assert!(ensure_loader_profile(root, "1.21.1", "0.19.3")
            .await
            .is_ok());
    }

    fn serve_flaky_then_ok() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for (attempt, mut stream) in listener.incoming().take(2).flatten().enumerate() {
                use std::io::{Read, Write};
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer);
                let (status, body) = if attempt == 0 {
                    ("500 Internal Server Error", "boom")
                } else {
                    ("200 OK", r#"{"ok":true}"#)
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("http://{address}/version.json")
    }

    #[tokio::test]
    async fn metadata_fetch_retries_transient_failure() {
        let url = serve_flaky_then_ok();
        let json = fetch_json(&url).await.unwrap();
        assert_eq!(json["ok"], true);
    }

    #[test]
    fn native_extraction_writes_only_plain_files_inside_the_natives_dir() {
        use std::io::Write;
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path().join("game");
        let jar = root.join("libraries/natives.jar");
        std::fs::create_dir_all(jar.parent().unwrap()).unwrap();
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&jar).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        writer
            .add_symlink("link", "../../../evil", options)
            .unwrap();
        for name in ["../evil", "C:/evil", "META-INF/evil"] {
            writer.start_file(name, options).unwrap();
            writer.write_all(b"bad").unwrap();
        }
        writer
            .start_file("lib.so", options.unix_permissions(0o6755))
            .unwrap();
        writer.write_all(b"native").unwrap();
        // A later entry through the skipped link name must not escape either.
        writer.start_file("link/escaped", options).unwrap();
        writer.write_all(b"bad").unwrap();
        writer.finish().unwrap();

        let artifact = Artifact {
            path: "natives.jar".into(),
            url: String::new(),
            size: 0,
            sha1: None,
        };
        extract_native_libraries(&root, "1.21.1", &[artifact]).unwrap();

        let natives = natives_dir(&root, "1.21.1");
        let lib = natives.join("lib.so");
        assert_eq!(std::fs::read(&lib).unwrap(), b"native");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&lib).unwrap().permissions().mode() & 0o6000,
                0
            );
        }
        assert!(std::fs::symlink_metadata(natives.join("link"))
            .map(|meta| meta.is_dir())
            .unwrap_or(true));
        assert!(!natives.join("META-INF").exists());
        assert!(!scratch.path().join("evil").exists());
        assert!(!root.join("evil").exists());
        assert!(!root.join("natives/evil").exists());
    }
}
