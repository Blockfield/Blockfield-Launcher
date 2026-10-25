// Download engine moved to launcher-net (phase 2, no behaviour change);
// re-export keeps every `crate::download::…` call site unchanged. The two
// items below stay shell-side: they depend on installed-files tracking state,
// which moves to core in a later unit.
pub use launcher_net::download::*;

use std::path::Path;

/// Remove stale `.part` files (delegates to the ownership tracker).
pub fn cleanup_partials(game_dir: &Path) {
    crate::installed_files::cleanup_partials(game_dir);
}

pub fn activate_partial(partial: &Path, destination: &Path) -> Result<(), std::io::Error> {
    let owned = crate::installed_files::Write::new(destination);
    if !destination.exists() {
        std::fs::rename(partial, destination)?;
        owned.finish();
        return Ok(());
    }
    let backup = partial_path(destination).with_extension("backup");
    let _ = std::fs::remove_file(&backup);
    std::fs::rename(destination, &backup)?;
    if let Err(error) = std::fs::rename(partial, destination) {
        let _ = std::fs::rename(&backup, destination);
        return Err(error);
    }
    let _ = std::fs::remove_file(backup);
    owned.finish();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_streams_files_and_failed_activation_restores_the_previous_file() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("artifact.jar");
        std::fs::write(&destination, b"abc").unwrap();
        assert_eq!(
            Downloader::sha256_file(&destination).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let large = temp.path().join("large.jar");
        std::fs::write(&large, vec![b'a'; 1_000_000]).unwrap();
        assert_eq!(
            Downloader::sha256_file(&large).unwrap(),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        let partial = partial_path(&destination);
        assert!(activate_partial(&partial, &destination).is_err());
        assert_eq!(std::fs::read(&destination).unwrap(), b"abc");
        std::fs::write(&partial, b"new").unwrap();
        activate_partial(&partial, &destination).unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), b"new");
        assert!(!partial.exists());
    }

    #[test]
    fn cleanup_preserves_packwiz_resume_files() {
        let game = tempfile::tempdir().unwrap();
        let cache = game.path().join(".packwiz-downloads");
        std::fs::create_dir(&cache).unwrap();
        let resumable = cache.join("mod.partial");
        std::fs::write(&resumable, b"downloaded prefix").unwrap();
        let obsolete = game.path().join("runtime.part");
        std::fs::write(&obsolete, b"old runtime download").unwrap();
        cleanup_partials(game.path());
        assert_eq!(std::fs::read(resumable).unwrap(), b"downloaded prefix");
        assert_eq!(std::fs::read(obsolete).unwrap(), b"old runtime download");
    }
}
