use crate::download::Downloader;
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;
use sha2::Digest;
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

fn launcher_name() -> String {
    std::env::var("BLOCKFIELD_LAUNCHER_NAME")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| option_env!("BLOCKFIELD_LAUNCHER_NAME").map(String::from))
        .unwrap_or_else(|| "BlockfieldLauncher".to_string())
}

fn offline_username() -> String {
    std::env::var("BLOCKFIELD_OFFLINE_USERNAME")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| option_env!("BLOCKFIELD_OFFLINE_USERNAME").map(String::from))
        .unwrap_or_else(|| "Blockfield".to_string())
}

#[derive(Debug, Clone)]
struct Artifact {
    path: String,
    url: String,
    size: u64,
}

pub fn forge_version_id(forge_version: &str) -> String {
    forge_version.replacen('-', "-forge-", 1)
}

pub fn find_forge_version_id(game_dir: &Path) -> Option<String> {
    let versions_dir = game_dir.join("versions");
    let entries = std::fs::read_dir(versions_dir).ok()?;

    entries.filter_map(Result::ok).find_map(|entry| {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.contains("-forge-") {
            return None;
        }

        let json = entry.path().join(format!("{name}.json"));
        json.exists().then_some(name)
    })
}

pub fn has_launch_dependencies(
    game_dir: &Path,
    minecraft_version: &str,
    forge_version_id: Option<&str>,
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

    if let Some(forge_version_id) = forge_version_id {
        let forge_json_path = version_json_path(game_dir, forge_version_id);
        let forge_json = match read_json_file(&forge_json_path) {
            Ok(json) => json,
            Err(_) => return false,
        };

        if !artifacts_exist(game_dir, &[&forge_json]) {
            return false;
        }

        if !forge_processed_jars_exist(game_dir, forge_version_id) {
            return false;
        }
    }

    natives_dir(game_dir, minecraft_version).exists()
}

pub async fn ensure_launch_dependencies(
    downloader: &Downloader,
    app_handle: &AppHandle,
    game_dir: &Path,
    minecraft_version: &str,
    forge_version_id: Option<&str>,
) -> Result<(), String> {
    std::fs::create_dir_all(game_dir).map_err(|e| format!("Failed to create game dir: {e}"))?;

    emit(app_handle, "runtime", "Fetching version manifest", false);
    ensure_version_json(game_dir, minecraft_version).await?;
    let vanilla_json = read_json_file(&version_json_path(game_dir, minecraft_version))?;

    emit(app_handle, "runtime", "Downloading Minecraft client", true);
    ensure_client_jar(downloader, game_dir, minecraft_version, &vanilla_json).await?;

    let mut jsons = vec![vanilla_json.clone()];
    if let Some(forge_version_id) = forge_version_id {
        let forge_json_path = version_json_path(game_dir, forge_version_id);
        if !forge_json_path.exists() {
            return Err(format!(
                "Forge version JSON missing: {}",
                forge_json_path.display()
            ));
        }
        ensure_forge_processed_jars(game_dir, forge_version_id)?;
        jsons.push(read_json_file(&forge_json_path)?);
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
    forge_version_id: Option<&str>,
) -> Result<Vec<String>, String> {
    let vanilla_json = read_json_file(&version_json_path(game_dir, minecraft_version))?;
    let forge_json = forge_version_id
        .map(|id| read_json_file(&version_json_path(game_dir, id)))
        .transpose()?;

    let version_name = forge_version_id.unwrap_or(minecraft_version);
    let main_class = forge_json
        .as_ref()
        .and_then(|json| json["mainClass"].as_str())
        .or_else(|| vanilla_json["mainClass"].as_str())
        .ok_or_else(|| "Missing Minecraft mainClass".to_string())?;

    let classpath = build_classpath(
        game_dir,
        minecraft_version,
        &vanilla_json,
        forge_json.as_ref(),
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
    );

    let mut args = vec![format!("-Xmx{ram_mb}M"), format!("-Xms{ram_mb}M")];

    let mut jvm_args = collect_arguments(&vanilla_json, "jvm");
    if let Some(ref forge_json) = forge_json {
        jvm_args.extend(collect_arguments(forge_json, "jvm"));
    }

    for arg in jvm_args {
        args.push(expand_placeholders(&arg, &vars)?);
    }

    args.push(main_class.to_string());

    let mut game_args = collect_arguments(&vanilla_json, "game");
    if let Some(ref forge_json) = forge_json {
        game_args.extend(collect_arguments(forge_json, "game"));
    }

    for arg in game_args {
        args.push(expand_placeholders(&arg, &vars)?);
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
    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("Failed to fetch {url}: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Failed to fetch {url}: {e}"))?;
    response
        .json::<Value>()
        .await
        .map_err(|e| format!("Failed to parse JSON from {url}: {e}"))
}

async fn download_text_to(url: &str, path: &Path) -> Result<(), String> {
    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("Failed to download {url}: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Failed to download {url}: {e}"))?;
    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read {url}: {e}"))?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }

    std::fs::write(path, text).map_err(|e| format!("Failed to write {}: {e}", path.display()))
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
    downloader.add_to_grand_total(total_bytes);
    let file_count = missing.len();
    let root = root.to_path_buf();
    let label = label.to_string();

    // ponytail: same parallel pattern as download_jobs in download.rs.
    // 6 concurrent downloads matches the modpack download fan-out.
    const PARALLEL: usize = 6;

    let results: Vec<Result<(), String>> = futures_util::stream::iter(missing)
        .map(|(index, artifact)| {
            let root = root.clone();
            let label = label.clone();
            async move {
                let mut downloaded: u64 = 0;
                let dest = root.join(&artifact.path);
                let file_label = format!("{}/{}", label, artifact.path);
                downloader
                    .download_one(
                        &artifact.url,
                        &dest,
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
                    let actual_size = dest
                        .metadata()
                        .map_err(|e| format!("Failed to stat {}: {}", dest.display(), e))?
                        .len();
                    if actual_size != artifact.size {
                        let _ = std::fs::remove_file(&dest);
                        return Err(format!(
                            "Size mismatch for {}: expected {}, got {}",
                            file_label, artifact.size, actual_size
                        ));
                    }
                }
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

        if let Some(artifact) = artifact_from_value(&library["downloads"]["artifact"]) {
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

    objects
        .values()
        .filter_map(|object| {
            let hash = object["hash"].as_str()?;
            let prefix = hash.get(0..2)?;
            Some(Artifact {
                path: format!("{prefix}/{hash}"),
                url: format!("{}/{prefix}/{hash}", asset_base_url()),
                size: object["size"].as_u64().unwrap_or(0),
            })
        })
        .collect()
}

fn artifact_from_value(value: &Value) -> Option<Artifact> {
    let path = value["path"].as_str()?;
    let url = value["url"].as_str()?;
    Some(Artifact {
        path: path.to_string(),
        url: url.to_string(),
        size: value["size"].as_u64().unwrap_or(0),
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
            if entry.is_dir() {
                continue;
            }

            let name = entry.name().replace('\\', "/");
            if name.starts_with("META-INF/") || name.starts_with('/') || name.contains("..") {
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

    Ok(())
}

fn build_classpath(
    game_dir: &Path,
    minecraft_version: &str,
    vanilla_json: &Value,
    forge_json: Option<&Value>,
) -> String {
    let mut artifacts = collect_library_artifacts(vanilla_json).0;
    if let Some(forge_json) = forge_json {
        artifacts.extend(collect_library_artifacts(forge_json).0);
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
    if let Some(forge_json) = forge_json {
        let forge_version_id = forge_json["id"].as_str().unwrap_or_default();
        paths.extend(
            forge_processed_jars(game_dir, forge_version_id)
                .into_iter()
                .filter(|path| path.exists())
                .map(|path| path.to_string_lossy().to_string()),
        );
    } else {
        paths.push(
            version_jar_path(game_dir, minecraft_version)
                .to_string_lossy()
                .to_string(),
        );
    }

    paths.join(classpath_separator())
}

fn ensure_forge_processed_jars(game_dir: &Path, forge_version_id: &str) -> Result<(), String> {
    if forge_processed_jars_exist(game_dir, forge_version_id) {
        return Ok(());
    }

    let expected = forge_processed_jars(game_dir, forge_version_id)
        .into_iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "Forge processed client jars are missing. Reinstall Forge; expected: {expected}"
    ))
}

fn forge_processed_jars_exist(game_dir: &Path, forge_version_id: &str) -> bool {
    forge_processed_jars(game_dir, forge_version_id)
        .iter()
        .all(|path| path.exists())
}

fn forge_processed_jars(game_dir: &Path, forge_version_id: &str) -> Vec<PathBuf> {
    let forge_version = forge_version_id.replace("-forge-", "-");
    let forge_dir = game_dir
        .join("libraries")
        .join("net")
        .join("minecraftforge")
        .join("forge")
        .join(&forge_version);
    vec![
        forge_dir.join(format!("forge-{forge_version}-client.jar")),
        forge_dir.join(format!("forge-{forge_version}-universal.jar")),
    ]
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
    let username = std::env::var("BLOCKFIELD_PLAYER_NAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| option_env!("BLOCKFIELD_PLAYER_NAME").map(String::from))
        .unwrap_or_else(offline_username);

    HashMap::from([
        ("auth_player_name".to_string(), username.clone()),
        ("auth_uuid".to_string(), offline_uuid(&username)),
        ("auth_access_token".to_string(), "0".to_string()),
        ("clientid".to_string(), "0".to_string()),
        ("auth_xuid".to_string(), "0".to_string()),
        ("user_type".to_string(), "legacy".to_string()),
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

fn offline_uuid(username: &str) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(format!("OfflinePlayer:{username}").as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    hash.chars().take(32).collect()
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
