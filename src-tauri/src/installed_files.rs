use crate::download::Downloader;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

const RECORD: &str = "installed-files.json";
#[cfg(windows)]
const BUNDLE_ID: &str = "com.blockfield.launcher";

#[derive(Default, Serialize, Deserialize)]
struct Record {
    directories: BTreeMap<PathBuf, BTreeMap<String, String>>,
}

struct Tracking {
    path: PathBuf,
    root: PathBuf,
    record: Record,
}

// ponytail: one installation at a time, matching LauncherAppState.operation; split if installs become concurrent.
static ACTIVE: Mutex<Option<Tracking>> = Mutex::new(None);

pub struct Session;

pub fn begin(app_data: &Path, root: &Path) -> Result<Option<Session>, String> {
    if crate::dev::is_dev_build() {
        return Ok(None);
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    if root.parent().is_none() {
        return Err("Cannot track a filesystem root".into());
    }
    let path = app_data.join(RECORD);
    let mut record = load(&path)?;
    record.directories.entry(root.clone()).or_default();
    let mut active = ACTIVE.lock().map_err(|e| e.to_string())?;
    if active.is_some() {
        return Err("Another installation is being tracked".into());
    }
    *active = Some(Tracking { path, root, record });
    Ok(Some(Session))
}

impl Drop for Session {
    fn drop(&mut self) {
        let tracking = ACTIVE.lock().unwrap().take();
        if let Some(tracking) = tracking {
            if let Err(error) = save(&tracking.path, &tracking.record) {
                log::error!("Cannot save downloaded-file ownership: {error}");
            }
        }
    }
}

pub struct Write(Option<(PathBuf, String)>);

impl Write {
    pub fn new(path: &Path) -> Self {
        let mut active = ACTIVE.lock().unwrap();
        let Some(tracking) = active.as_mut() else {
            return Self(None);
        };
        let Some(relative) = relative_file(&tracking.root, path) else {
            return Self(None);
        };
        let files = tracking.record.directories.get_mut(&tracking.root).unwrap();
        let eligible = match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
            Ok(metadata) if metadata.is_file() && !is_link(&metadata) => {
                files.get(&relative).is_some_and(|hash| {
                    Downloader::sha256_file(path).is_ok_and(|value| value == *hash)
                })
            }
            _ => false,
        };
        if !eligible {
            files.remove(&relative);
        }
        Self(eligible.then(|| (tracking.root.clone(), relative)))
    }

    pub fn finish(self) {
        let Some((root, relative)) = self.0 else {
            return;
        };
        record_file(&root, &relative);
    }
}

pub fn write(path: &Path, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
    let owned = Write::new(path);
    fs::write(path, contents)?;
    owned.finish();
    Ok(())
}

pub struct ExternalInstall {
    root: PathBuf,
    existing: BTreeSet<String>,
    updates: BTreeMap<String, Write>,
}

impl ExternalInstall {
    pub fn new(root: &Path, scope: &str) -> Result<Self, String> {
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        let mut existing = BTreeSet::new();
        inventory(&root, &root.join(scope), &mut existing)?;
        let pack_files = pack_files(&root);
        let tracked: Vec<String> = ACTIVE
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|tracking| tracking.record.directories.get(&root))
            .map(|files| {
                files
                    .keys()
                    .filter(|path| {
                        if scope.is_empty() {
                            path.as_str() == "packwiz.json"
                                || pack_files.iter().any(|(file, _)| file == *path)
                        } else {
                            Path::new(path).starts_with(scope)
                        }
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let updates = tracked
            .iter()
            .map(|relative| (relative.clone(), Write::new(&root.join(relative))))
            .collect();
        Ok(Self {
            root,
            existing,
            updates,
        })
    }

    pub fn finish(mut self, paths: impl IntoIterator<Item = PathBuf>) {
        for path in paths {
            if let Some(relative) = relative_file(&self.root, &path) {
                if let Some(owned) = self.updates.remove(&relative) {
                    owned.finish();
                } else if !self.existing.contains(&relative) {
                    record_file(&self.root, &relative);
                }
            }
        }
    }

    pub fn finish_pack(self) {
        let manifest = self.root.join("packwiz.json");
        let paths = pack_files(&self.root)
            .into_iter()
            .filter_map(|(relative, expected)| {
                let path = self.root.join(relative);
                pack_hash_matches(&path, &expected).then_some(path)
            })
            .collect::<Vec<_>>();
        self.finish(paths.into_iter().chain([manifest]));
    }

    pub fn finish_tree(self, tree: &Path) -> Result<(), String> {
        let mut files = BTreeSet::new();
        inventory(&self.root, tree, &mut files)?;
        let paths = files
            .iter()
            .map(|file| self.root.join(file))
            .collect::<Vec<_>>();
        self.finish(paths);
        Ok(())
    }
}

fn pack_files(root: &Path) -> Vec<(String, serde_json::Value)> {
    fs::read(root.join("packwiz.json"))
        .ok()
        .and_then(|data| serde_json::from_slice::<serde_json::Value>(&data).ok())
        .and_then(|json| json["cachedFiles"].as_object().cloned())
        .map(|files| {
            files
                .values()
                .filter_map(|file| {
                    let path = file["cachedLocation"]
                        .as_str()
                        .filter(|path| valid_relative(path))?;
                    let expected = if file["linkedFileHash"].is_object() {
                        &file["linkedFileHash"]
                    } else {
                        &file["hash"]
                    };
                    Some((path.into(), expected.clone()))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn record_file(root: &Path, relative: &str) {
    let Some(path) = safe_file(root, relative) else {
        return;
    };
    let Ok(hash) = Downloader::sha256_file(&path) else {
        return;
    };
    if let Some(tracking) = ACTIVE.lock().unwrap().as_mut() {
        if tracking.root == root {
            tracking
                .record
                .directories
                .get_mut(root)
                .unwrap()
                .insert(relative.into(), hash);
        }
    }
}

pub fn cleanup_partials(root: &Path) {
    let Ok(root) = root.canonicalize() else {
        return;
    };
    let active = ACTIVE.lock().unwrap();
    let Some(files) = active
        .as_ref()
        .and_then(|tracking| tracking.record.directories.get(&root))
    else {
        return;
    };
    for (relative, expected) in files.iter().filter(|(path, _)| path.ends_with(".part")) {
        if let Some(path) = safe_file(&root, relative) {
            if Downloader::sha256_file(&path).is_ok_and(|hash| hash == *expected) {
                let _ = fs::remove_file(path);
            }
        }
    }
}

fn pack_hash_matches(path: &Path, expected: &serde_json::Value) -> bool {
    use sha2::Digest;
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    let hash = match expected["type"].as_str() {
        Some("sha256") => crate::download::hex_digest(&sha2::Sha256::digest(&bytes)),
        Some("sha512") => crate::download::hex_digest(&sha2::Sha512::digest(&bytes)),
        Some("sha1") => crate::download::hex_digest(&sha1::Sha1::digest(&bytes)),
        Some("md5") => crate::download::hex_digest(&md5::Md5::digest(&bytes)),
        _ => return false,
    };
    expected["value"]
        .as_str()
        .is_some_and(|value| hash.eq_ignore_ascii_case(value))
}

fn inventory(root: &Path, directory: &Path, files: &mut BTreeSet<String>) -> Result<(), String> {
    if !directory.exists() || fs::symlink_metadata(directory).is_ok_and(|meta| is_link(&meta)) {
        return Ok(());
    }
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
        if is_link(&metadata) {
            continue;
        }
        if metadata.is_dir() {
            inventory(root, &entry.path(), files)?;
        } else if let Some(relative) = relative_file(root, &entry.path()) {
            files.insert(relative);
        }
    }
    Ok(())
}

fn valid_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', ':'])
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

fn relative_file(root: &Path, path: &Path) -> Option<String> {
    let mut ancestor = path;
    let mut suffix = Vec::new();
    while !ancestor.exists() {
        suffix.push(ancestor.file_name()?);
        ancestor = ancestor.parent()?;
    }
    let mut resolved = ancestor.canonicalize().ok()?;
    for part in suffix.into_iter().rev() {
        resolved.push(part);
    }
    let relative = resolved
        .strip_prefix(root)
        .ok()?
        .to_str()?
        .replace('\\', "/");
    valid_relative(&relative).then_some(relative)
}

fn safe_file(root: &Path, relative: &str) -> Option<PathBuf> {
    if !valid_relative(relative) {
        return None;
    }
    let mut path = root.to_path_buf();
    for part in relative.split('/') {
        path.push(part);
        let metadata = fs::symlink_metadata(&path).ok()?;
        if is_link(&metadata) {
            return None;
        }
    }
    fs::metadata(&path).ok()?.is_file().then_some(path)
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn load(path: &Path) -> Result<Record, String> {
    match fs::read(path) {
        Ok(data) => {
            serde_json::from_slice(&data).map_err(|e| format!("Invalid ownership record: {e}"))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Record::default()),
        Err(error) => Err(error.to_string()),
    }
}

fn save(path: &Path, record: &Record) -> Result<(), String> {
    let partial = crate::download::partial_path(path);
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(
        &partial,
        serde_json::to_vec(record).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    crate::download::activate_partial(&partial, path).map_err(|e| e.to_string())
}

#[cfg(any(windows, test))]
fn clean_downloads(record: &Record) -> Result<(), String> {
    for (root, files) in &record.directories {
        if !root.is_absolute() || root.parent().is_none() || !root.exists() {
            continue;
        }
        if root
            .ancestors()
            .any(|path| fs::symlink_metadata(path).is_ok_and(|meta| is_link(&meta)))
        {
            continue;
        }
        let mut directories = BTreeSet::new();
        for (relative, expected) in files {
            let Some(path) = safe_file(root, relative) else {
                continue;
            };
            if Downloader::sha256_file(&path).is_ok_and(|hash| hash == *expected) {
                fs::remove_file(&path)
                    .map_err(|e| format!("Cannot remove {}: {e}", path.display()))?;
                for parent in path
                    .ancestors()
                    .skip(1)
                    .take_while(|parent| parent.starts_with(root))
                {
                    directories.insert(parent.to_path_buf());
                }
            }
        }
        for directory in directories.into_iter().rev() {
            // Nonempty directories contain retained files and must remain.
            let _ = fs::remove_dir(directory);
        }
    }
    Ok(())
}

#[cfg(windows)]
pub fn uninstall() -> Result<(), String> {
    let base = directories::BaseDirs::new().ok_or("Cannot resolve app data")?;
    let app_data = base.data_dir().join(BUNDLE_ID);
    if app_data
        .ancestors()
        .any(|path| fs::symlink_metadata(path).is_ok_and(|meta| is_link(&meta)))
    {
        return Err("App data contains a filesystem link; cleanup refused".into());
    }
    let record_path = app_data.join(RECORD);
    clean_downloads(&load(&record_path)?)?;
    for name in ["launcher-config.json", RECORD] {
        let path = app_data.join(name);
        if path.is_file() {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    let _ = fs::remove_dir(app_data);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_hashes_match_published_digest_vectors() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("artifact");
        fs::write(&path, b"abc").unwrap();
        for (algorithm, digest) in [
            ("md5", "900150983cd24fb0d6963f7d28e17f72"),
            ("sha1", "a9993e364706816aba3e25717850c26c9cd0d89d"),
            ("sha256", "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
            ("sha512", "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"),
        ] {
            assert!(pack_hash_matches(
                &path,
                &serde_json::json!({ "type": algorithm, "value": digest.to_uppercase() })
            ));
            assert!(!pack_hash_matches(
                &path,
                &serde_json::json!({ "type": algorithm, "value": "wrong" })
            ));
        }
        assert!(!pack_hash_matches(
            &path,
            &serde_json::json!({ "type": "unsupported", "value": "" })
        ));
        fs::remove_file(&path).unwrap();
        assert!(!pack_hash_matches(
            &path,
            &serde_json::json!({ "type": "sha256", "value": "" })
        ));
    }

    #[test]
    fn tracks_actual_writes_and_preserves_preexisting_and_modified_files() {
        if crate::dev::is_dev_build() {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Blockfield");
        let data = temp.path().join("app-data");
        fs::create_dir_all(root.join("mods")).unwrap();
        let manual = root.join("mods/manual.jar");
        fs::write(&manual, b"manual").unwrap();
        let automatic = root.join("automatic.jar");
        let modified = root.join("modified.jar");
        {
            let _session = begin(&data, &root).unwrap();
            write(&automatic, b"downloaded").unwrap();
            write(&modified, b"downloaded").unwrap();
            write(&manual, b"replaced by installer").unwrap();
        }
        fs::write(&modified, b"edited by player").unwrap();
        {
            let _session = begin(&data, &root).unwrap();
            write(&modified, b"repaired by installer").unwrap();
            let pack = ExternalInstall::new(&root, "").unwrap();
            let new_mod = root.join("mods/new.jar");
            let mismatch = root.join("mods/mismatch.jar");
            fs::write(&new_mod, b"pack download").unwrap();
            fs::write(&mismatch, b"manual copy during installation").unwrap();
            fs::write(root.join("packwiz.json"), serde_json::to_vec(&serde_json::json!({
                "cachedFiles": {
                    "mods/new.pw.toml": {
                        "cachedLocation": "mods/new.jar",
                        "linkedFileHash": { "type": "sha256", "value": Downloader::sha256_file(&new_mod).unwrap() }
                    },
                    "mods/manual.jar": {
                        "cachedLocation": "mods/manual.jar",
                        "hash": { "type": "sha256", "value": Downloader::sha256_file(&manual).unwrap() }
                    },
                    "mods/mismatch.jar": {
                        "cachedLocation": "mods/mismatch.jar",
                        "hash": { "type": "sha256", "value": "00".repeat(32) }
                    }
                }
            })).unwrap()).unwrap();
            pack.finish_pack();
            let java = ExternalInstall::new(&root, "java").unwrap();
            fs::create_dir_all(root.join("java/bin")).unwrap();
            fs::write(root.join("java/bin/java.exe"), b"runtime").unwrap();
            java.finish_tree(&root.join("java")).unwrap();
            write(&root.join("owned.part"), b"partial download").unwrap();
            fs::write(root.join("manual.part"), b"user data").unwrap();
            cleanup_partials(&root);
            assert!(!root.join("owned.part").exists());
            assert!(root.join("manual.part").is_file());
        }
        let record = load(&data.join(RECORD)).unwrap();
        clean_downloads(&record).unwrap();
        assert!(!automatic.exists());
        assert!(!root.join("mods/new.jar").exists());
        assert!(!root.join("java").exists());
        assert!(manual.is_file());
        assert!(modified.is_file());
        assert!(root.join("mods/mismatch.jar").is_file());
        assert!(root.join("manual.part").is_file());
    }

    #[test]
    fn cleanup_removes_only_unchanged_recorded_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Blockfield");
        fs::create_dir_all(root.join("mods")).unwrap();
        fs::create_dir_all(root.join("saves/world")).unwrap();
        let downloaded = root.join("mods/downloaded.jar");
        let modified = root.join("mods/modified.jar");
        let manual = root.join("mods/manual.jar");
        fs::write(&downloaded, b"automatic").unwrap();
        fs::write(&modified, b"automatic").unwrap();
        fs::write(&manual, b"manual").unwrap();
        fs::write(root.join("saves/world/level.dat"), b"world").unwrap();
        let mut files = BTreeMap::from([
            (
                "mods/downloaded.jar".into(),
                Downloader::sha256_file(&downloaded).unwrap(),
            ),
            (
                "mods/modified.jar".into(),
                Downloader::sha256_file(&modified).unwrap(),
            ),
        ]);
        let outside = temp.path().join("personal.txt");
        fs::write(&outside, b"personal").unwrap();
        files.insert(
            "../personal.txt".into(),
            Downloader::sha256_file(&outside).unwrap(),
        );
        fs::write(&modified, b"manual changes").unwrap();
        let record = Record {
            directories: BTreeMap::from([(root.clone(), files)]),
        };
        clean_downloads(&record).unwrap();
        clean_downloads(&record).unwrap();
        assert!(!downloaded.exists());
        for path in [
            modified,
            manual,
            outside,
            root.join("saves/world/level.dat"),
        ] {
            assert!(path.is_file(), "{}", path.display());
        }
    }

    #[test]
    fn rejects_unsafe_relative_paths_and_corrupt_records() {
        for path in [
            "",
            "../file",
            "/file",
            "a/../file",
            "a//file",
            "C:/file",
            "a\\file",
            "file:stream",
        ] {
            assert!(!valid_relative(path), "{path}");
        }
        assert!(valid_relative("mods/example.jar"));
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join(RECORD), b"invalid json").unwrap();
        assert!(load(&temp.path().join(RECORD)).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_never_follows_links() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Blockfield");
        let outside = temp.path().join("personal");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&outside).unwrap();
        let file = outside.join("keep.txt");
        fs::write(&file, b"keep").unwrap();
        symlink(&outside, root.join("mods")).unwrap();
        let record = Record {
            directories: BTreeMap::from([(
                root.clone(),
                BTreeMap::from([(
                    "mods/keep.txt".into(),
                    Downloader::sha256_file(&file).unwrap(),
                )]),
            )]),
        };
        clean_downloads(&record).unwrap();
        assert!(file.is_file());
        assert!(root
            .join("mods")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
    }
}
