//! Java runtime discovery helpers, moved unchanged from
//! `src-tauri/src/commands.rs` (phase 2, no behaviour change).
//!
//! Pure filesystem logic; the Tauri adapter and the later native app share it.
//! The remaining `commands.rs` code (download/verify/launch) imports what it
//! still needs from here.

use std::path::{Path, PathBuf};

pub fn java_exe_name() -> &'static str {
    if cfg!(windows) {
        "javaw.exe"
    } else {
        "java"
    }
}

pub fn bundled_java_path(game_dir: impl AsRef<Path>) -> Option<PathBuf> {
    let java_dir = game_dir.as_ref().join("java");
    let direct = java_dir.join("bin").join(java_exe_name());
    if direct.exists() {
        return Some(direct);
    }

    find_bin_java(&java_dir)
}

pub fn find_bin_java(dir: &Path) -> Option<PathBuf> {
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

pub fn is_bin_java(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(java_exe_name()))
        && path
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("bin"))
}

pub fn java_runtime_ready(game_dir: &Path, version: &str) -> bool {
    let java_ver_path = game_dir.join("java").join(".version");
    matches!(std::fs::read_to_string(&java_ver_path), Ok(v) if v.trim() == version)
        && bundled_java_path(game_dir).is_some()
}

pub fn resolved_java_path(config: &launcher_core::config::LauncherConfig) -> String {
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
pub fn try_javaw(path: &Path) -> Option<PathBuf> {
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

pub fn system_java_available() -> bool {
    launcher_core::host_env::command("java")
        .arg("-version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
