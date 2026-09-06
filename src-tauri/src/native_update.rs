use flate2::read::GzDecoder;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

const MARKER: &str = ".blockfield-native-install";
const MAX_PACKAGE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_BINARY_BYTES: u64 = 128 * 1024 * 1024;
const MAX_UNPACKED_BYTES: u64 = 512 * 1024 * 1024;

pub fn enabled(executable: &Path) -> bool {
    executable
        .parent()
        .is_some_and(|dir| dir.join(MARKER).is_file())
}

pub fn install_deb(package: &[u8], executable: &Path) -> Result<(), String> {
    let binary = extract_binary(package)?;
    replace_binary(&binary, executable).map_err(|e| e.to_string())
}

fn extract_binary(package: &[u8]) -> Result<Vec<u8>, String> {
    if package.len() as u64 > MAX_PACKAGE_BYTES || !package.starts_with(b"!<arch>\n") {
        return Err("Invalid or oversized Debian update package".into());
    }
    let mut archive = ar::Archive::new(Cursor::new(package));
    let mut binary = None;
    while let Some(entry) = archive.next_entry() {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.header().identifier() != b"data.tar.gz" {
            continue;
        }
        if binary.is_some() {
            return Err("Duplicate update payload".into());
        }
        let decoder = GzDecoder::new(entry).take(MAX_UNPACKED_BYTES);
        let mut data = tar::Archive::new(decoder);
        for entry in data.entries().map_err(|e| e.to_string())? {
            let mut entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path().map_err(|e| e.to_string())?;
            let path = path.strip_prefix(".").unwrap_or(&path);
            if path != Path::new("usr/bin/blockfield-launcher") {
                continue;
            }
            if binary.is_some()
                || !entry.header().entry_type().is_file()
                || entry.size() > MAX_BINARY_BYTES
            {
                return Err("Invalid launcher executable in update".into());
            }
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            validate_binary(&bytes)?;
            binary = Some(bytes);
        }
    }
    binary
        .ok_or_else(|| "Update does not contain usr/bin/blockfield-launcher in data.tar.gz".into())
}

fn validate_binary(bytes: &[u8]) -> Result<(), String> {
    let machine = match std::env::consts::ARCH {
        "x86_64" => 62u16,
        "aarch64" => 183u16,
        _ => return Err("Native updates are not supported on this CPU architecture".into()),
    };
    if bytes.len() < 64
        || &bytes[..4] != b"\x7fELF"
        || bytes[4] != 2
        || bytes[5] != 1
        || u16::from_le_bytes([bytes[18], bytes[19]]) != machine
        || &bytes[8..10] == b"AI"
    {
        return Err(
            "Update must contain a native ELF executable for this CPU, not an AppImage".into(),
        );
    }
    Ok(())
}

fn replace_binary(binary: &[u8], executable: &Path) -> std::io::Result<()> {
    validate_binary(binary).map_err(std::io::Error::other)?;
    let directory = executable
        .parent()
        .ok_or_else(|| std::io::Error::other("No install directory"))?;
    let mut staged = tempfile::NamedTempFile::new_in(directory)?;
    staged.write_all(binary)?;
    staged
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o755))?;
    staged.as_file().sync_all()?;

    let check = std::process::Command::new("ldd")
        .arg(staged.path())
        .env("LC_ALL", "C")
        .output()?;
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
    if !check.status.success() || diagnostics.contains("not found") {
        return Err(std::io::Error::other(format!(
            "Update requires unavailable system libraries: {diagnostics}"
        )));
    }

    // Both renames stay on the installation filesystem. Failure before replacement leaves the
    // running executable intact; the previous version remains available for manual rollback.
    let backup = tempfile::NamedTempFile::new_in(directory)?;
    fs::copy(executable, backup.path())?;
    backup.as_file().sync_all()?;
    backup.persist(directory.join("blockfield-launcher.previous"))?;
    staged.persist(executable)?;
    fs::File::open(directory)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::GzEncoder, Compression};

    fn package(entries: &[(&str, tar::EntryType, &[u8])]) -> Vec<u8> {
        let mut tar = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
        for (path, kind, bytes) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_entry_type(*kind);
            header.set_cksum();
            tar.append_data(&mut header, path, *bytes).unwrap();
        }
        let data = tar.into_inner().unwrap().finish().unwrap();
        let mut ar = ar::Builder::new(Vec::new());
        ar.append(
            &ar::Header::new(b"data.tar.gz".to_vec(), data.len() as u64),
            Cursor::new(data),
        )
        .unwrap();
        ar.into_inner().unwrap()
    }

    #[test]
    fn extracts_only_the_native_executable() {
        let binary = fs::read("/bin/true").unwrap();
        let deb = package(&[
            ("usr/share/ignored", tar::EntryType::Regular, b"ignore"),
            (
                "usr/bin/blockfield-launcher",
                tar::EntryType::Regular,
                &binary,
            ),
        ]);
        assert_eq!(extract_binary(&deb).unwrap(), binary);
        assert!(extract_binary(b"not a deb").is_err());
        let deb = package(&[("usr/bin/blockfield-launcher", tar::EntryType::Symlink, b"")]);
        assert!(extract_binary(&deb).is_err());
        let mut appimage = binary.clone();
        appimage[8..11].copy_from_slice(b"AI\x02");
        assert!(validate_binary(&appimage).is_err());
        let deb = package(&[
            (
                "usr/bin/blockfield-launcher",
                tar::EntryType::Regular,
                &binary,
            ),
            (
                "usr/bin/blockfield-launcher",
                tar::EntryType::Regular,
                &binary,
            ),
        ]);
        assert!(extract_binary(&deb).is_err());
    }

    #[test]
    fn replacement_keeps_a_working_backup_and_rejects_bad_payloads() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("blockfield-launcher");
        let old = fs::read("/bin/false").unwrap();
        fs::write(&executable, &old).unwrap();
        assert!(!enabled(&executable));
        fs::write(directory.path().join(MARKER), b"1\n").unwrap();
        assert!(enabled(&executable));
        assert!(replace_binary(b"broken", &executable).is_err());
        assert_eq!(fs::read(&executable).unwrap(), old);
        replace_binary(&fs::read("/bin/true").unwrap(), &executable).unwrap();
        assert_eq!(
            fs::read(directory.path().join("blockfield-launcher.previous")).unwrap(),
            old
        );
        assert!(std::process::Command::new(executable)
            .status()
            .unwrap()
            .success());
    }

    #[tokio::test]
    #[ignore = "downloads and verifies the public signed Linux release"]
    async fn published_deb_has_a_valid_signature_and_native_payload() {
        use tauri_plugin_updater::UpdaterExt;
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context
            .config_mut()
            .plugins
            .0
            .insert("updater".into(), config["plugins"]["updater"].clone());
        let app = tauri::test::mock_builder().build(context).unwrap();
        app.handle()
            .plugin(
                tauri_plugin_updater::Builder::new()
                    .pubkey(config["plugins"]["updater"]["pubkey"].as_str().unwrap())
                    .build(),
            )
            .unwrap();
        let update = app
            .updater_builder()
            .endpoints(vec![config["plugins"]["updater"]["endpoints"][0]
                .as_str()
                .unwrap()
                .parse()
                .unwrap()])
            .unwrap()
            .target(format!("linux-{}-deb", std::env::consts::ARCH))
            .version_comparator(|_, _| true)
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap()
            .check()
            .await
            .unwrap()
            .unwrap();
        let package = update.download(|_, _| {}, || {}).await.unwrap();
        let binary = extract_binary(&package).unwrap();
        assert!(binary.len() > 1_000_000);
        let mut invalid = update.clone();
        invalid.signature = "invalid".into();
        assert!(invalid.download(|_, _| {}, || {}).await.is_err());
    }
}
