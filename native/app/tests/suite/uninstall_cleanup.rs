//! Tests for the shared `--uninstall-cleanup` flow in the native launcher.

use std::fs;
use std::path::Path;
use std::sync::Mutex;

use launcher_net::installed_files;

// `installed_files::begin` enforces one active tracking session per process.
static TRACKING_SERIAL: Mutex<()> = Mutex::new(());

fn seed_install(data_dir: &Path, game_dir: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let _guard = TRACKING_SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let app_data = data_dir.join(blockfield_native::runtime::identifier(
        launcher_core::dev::is_dev_build(),
    ));
    fs::create_dir_all(&app_data).unwrap();
    fs::create_dir_all(game_dir).unwrap();

    fs::write(
        launcher_core::config::config_path(&app_data),
        b"{\"ramMb\":4096}",
    )
    .unwrap();

    let owned = game_dir.join("owned-mod.jar");
    let modified = game_dir.join("player-edited.jar");
    let manual = game_dir.join("manual-world.dat");
    fs::write(&manual, b"player world").unwrap();

    {
        let _session = installed_files::begin(&app_data, game_dir).unwrap();
        installed_files::write(&owned, b"auto downloaded").unwrap();
        installed_files::write(&modified, b"initial download").unwrap();
    }
    fs::write(&modified, b"edited by player").unwrap();
    (owned, modified)
}

#[test]
fn uninstall_cleanup_uses_shared_ownership_verification() {
    if launcher_core::dev::is_dev_build() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let game_dir = tmp.path().join("game");
    let (owned, modified) = seed_install(&data_dir, &game_dir);
    let manual = game_dir.join("manual-world.dat");
    let app_data = data_dir.join("com.blockfield.launcher");

    installed_files::uninstall_from(&data_dir).unwrap();

    assert!(
        !owned.exists(),
        "unchanged tracked download must be removed"
    );
    assert!(modified.is_file(), "player-modified file must be preserved");
    assert!(manual.is_file(), "untracked player file must be preserved");
    assert!(!app_data.join("launcher-config.json").exists());
    assert!(!app_data.join("installed-files.json").exists());
    assert!(!app_data.exists(), "empty app-data directory is removed");
}

#[cfg(target_os = "linux")]
#[test]
fn cli_uninstall_cleanup_runs_headless_before_slint_init() {
    use std::os::unix::fs::symlink;
    use std::process::Command;

    if launcher_core::dev::is_dev_build() {
        return;
    }
    let bin = env!("CARGO_BIN_EXE_blockfield-native");
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("xdg-data");
    let game_dir = tmp.path().join("game");
    let (owned, modified) = seed_install(&data_dir, &game_dir);
    let manual = game_dir.join("manual-world.dat");

    // No DISPLAY/WAYLAND_DISPLAY: must exit cleanly before any Slint or GPU probe.
    let status = Command::new(bin)
        .arg("--uninstall-cleanup")
        .env("XDG_DATA_HOME", &data_dir)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .status()
        .expect("spawn blockfield-native --uninstall-cleanup");
    assert!(status.success(), "expected exit 0, got {status}");
    assert!(!owned.exists());
    assert!(modified.is_file());
    assert!(manual.is_file());
    assert!(!data_dir.join("com.blockfield.launcher").exists());

    // A symlinked app-data directory must be refused with nonzero exit code.
    let real_target = tmp.path().join("real-target");
    fs::create_dir_all(&real_target).unwrap();
    fs::create_dir_all(&data_dir).unwrap();
    symlink(&real_target, data_dir.join("com.blockfield.launcher")).unwrap();
    let refused = Command::new(bin)
        .arg("--uninstall-cleanup")
        .env("XDG_DATA_HOME", &data_dir)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .status()
        .expect("spawn refusal check");
    assert!(!refused.success(), "symlink app-data must fail cleanup");
}
