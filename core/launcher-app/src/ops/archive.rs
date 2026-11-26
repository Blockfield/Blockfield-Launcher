//! Java runtime archive handling, moved unchanged from
//! `src-tauri/src/commands.rs` (phase 2, no behaviour change).
//!
//! Pure filesystem logic plus the `tar`/zip extraction; the Tauri adapter and
//! the later native app share it.

use super::java::find_bin_java;
use std::path::{Path, PathBuf};

pub(crate) fn safe_relative_path(value: &str) -> Result<PathBuf, String> {
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

pub(crate) fn install_java_archive(
    archive_path: &Path,
    game_dir: &Path,
    version: &str,
) -> Result<PathBuf, String> {
    let tracked = launcher_net::installed_files::ExternalInstall::new(game_dir, "java")?;
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
    tracked.finish_tree(&java_dir)?;
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

/// Extract a Java runtime archive: ZIP (Windows builds) or tar.gz (Linux/macOS Temurin).
fn extract_archive(archive_path: &Path, dest_dir: &Path) -> Result<(), String> {
    if archive_path.to_string_lossy().ends_with(".tar.gz") {
        return extract_tar_gz_with_limits(archive_path, dest_dir, &archive_limits());
    }
    extract_archive_with_limits(archive_path, dest_dir, &archive_limits())
}

/// In-process tar.gz extraction applying the same entry guards as the zip path.
/// Replaces the former system `tar -xzf` call, which extracted absolute paths,
/// `..` and symlinks without checks.
fn extract_tar_gz_with_limits(
    archive_path: &Path,
    dest_dir: &Path,
    limits: &blockfield_shared::ArchiveLimits,
) -> Result<(), String> {
    use flate2::read::GzDecoder;
    use std::io::Read;

    let compressed_bytes = std::fs::metadata(archive_path)
        .map_err(|e| format!("Failed to inspect archive: {e}"))?
        .len();
    if compressed_bytes == 0 || compressed_bytes > limits.max_compressed_bytes {
        return Err("Java archive is empty or exceeds the compressed-size limit".to_string());
    }

    // First pass: validate every entry before writing anything.
    let mut count = 0usize;
    let mut declared_total = 0u64;
    {
        let file = std::fs::File::open(archive_path)
            .map_err(|e| format!("Failed to open archive: {e}"))?;
        let mut archive = tar::Archive::new(GzDecoder::new(file));
        let entries = archive
            .entries()
            .map_err(|e| format!("Failed to read archive: {e}"))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("Archive entry error: {e}"))?;
            let header = entry.header().clone();
            let entry_type = header.entry_type();
            let name = entry
                .path()
                .map_err(|e| format!("Archive entry name error: {e}"))?
                .to_string_lossy()
                .into_owned();
            if name.split('/').count() > limits.max_path_depth {
                return Err(format!("Archive path is too deep: {name}"));
            }
            if !(entry_type.is_file() || entry_type.is_dir()) {
                // Symlinks, hardlinks, devices and fifos never extract.
                if entry_type.is_symlink() || entry_type == tar::EntryType::Link {
                    return Err(format!("Archive symlink rejected: {name}"));
                }
                return Err(format!("Unsupported archive entry: {name}"));
            }
            let size = header
                .size()
                .map_err(|e| format!("Archive entry {name} error: {e}"))?;
            if size > limits.max_entry_bytes {
                return Err(format!("Archive entry is too large: {name}"));
            }
            declared_total = declared_total
                .checked_add(size)
                .filter(|total| *total <= limits.max_uncompressed_bytes)
                .ok_or_else(|| "Java archive exceeds the uncompressed-size limit".to_string())?;
            // tar.gz compresses the whole stream, so the ratio guard is global.
            if declared_total > compressed_bytes.saturating_mul(limits.max_compression_ratio) {
                return Err(format!("Suspicious compression ratio: {name}"));
            }
            count += 1;
            if count > limits.max_entries {
                return Err("Java archive contains too many entries".to_string());
            }
        }
    }
    let required_space = declared_total.saturating_add(limits.min_free_space_bytes);
    if fs2::available_space(dest_dir).map_err(|e| format!("Failed to inspect free space: {e}"))?
        < required_space
    {
        return Err("Not enough free space to extract the Java archive safely".to_string());
    }

    // Second pass: extract, re-checking paths and links.
    let mut actual_total = 0u64;
    {
        let file = std::fs::File::open(archive_path)
            .map_err(|e| format!("Failed to open archive: {e}"))?;
        let mut archive = tar::Archive::new(GzDecoder::new(file));
        let entries = archive
            .entries()
            .map_err(|e| format!("Failed to read archive: {e}"))?;
        for entry in entries {
            let mut entry = entry.map_err(|e| format!("Archive entry error: {e}"))?;
            let entry_type = entry.header().entry_type();
            let name = entry
                .path()
                .map_err(|e| format!("Archive entry name error: {e}"))?
                .to_string_lossy()
                .into_owned();
            if !(entry_type.is_file() || entry_type.is_dir()) {
                if entry_type.is_symlink() || entry_type == tar::EntryType::Link {
                    return Err(format!("Archive symlink rejected: {name}"));
                }
                return Err(format!("Unsupported archive entry: {name}"));
            }
            if entry_type.is_dir() {
                continue;
            }
            safe_relative_path(&name)
                .map_err(|_| format!("Unsafe archive entry rejected: {name}"))?;

            // Strip top-level directory (e.g. "jdk-21.0.5+11/" → "")
            let relative = if let Some(pos) = name.find('/') {
                &name[pos + 1..]
            } else {
                name.as_str()
            };
            if relative.is_empty() {
                continue;
            }
            let relative = safe_relative_path(relative)
                .map_err(|_| format!("Unsafe archive entry rejected: {name}"))?;
            let dest = dest_dir.join(&relative);
            if !dest.starts_with(dest_dir) {
                return Err(format!("Unsafe archive entry rejected: {name}"));
            }
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create dir {}: {e}", parent.display()))?;
            }

            let expected = entry
                .header()
                .size()
                .map_err(|e| format!("Archive entry {name} error: {e}"))?;
            // Only Unix applies permission bits; Windows has no mode to set.
            #[cfg(unix)]
            let mode = entry.header().mode().unwrap_or(0o644) & 0o777;
            let mut out = std::fs::File::create(&dest)
                .map_err(|e| format!("Failed to create {}: {e}", dest.display()))?;
            let mut take = (&mut entry as &mut dyn Read).take(expected.saturating_add(1));
            let copied = std::io::copy(&mut take, &mut out)
                .map_err(|e| format!("Failed to extract {name}: {e}"))?;
            actual_total = actual_total
                .checked_add(copied)
                .filter(|total| *total <= declared_total)
                .ok_or_else(|| "Java archive produced more data than declared".to_string())?;
            if copied != expected {
                return Err(format!("Archive entry size mismatch: {name}"));
            }
            // Keep executable bits from the runtime, never SUID/SGID/sticky.
            #[cfg(unix)]
            if mode != 0 {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(mode))
                    .map_err(|e| format!("Failed to set mode {}: {e}", dest.display()))?;
            }
        }
    }

    log::info!("Extracted {count} entries to {}", dest_dir.display());
    Ok(())
}

fn extract_archive_with_limits(
    archive_path: &Path,
    dest_dir: &Path,
    limits: &blockfield_shared::ArchiveLimits,
) -> Result<(), String> {
    let compressed_bytes = std::fs::metadata(archive_path)
        .map_err(|e| format!("Failed to inspect archive: {e}"))?
        .len();
    if compressed_bytes == 0 || compressed_bytes > limits.max_compressed_bytes {
        return Err("Java archive is empty or exceeds the compressed-size limit".to_string());
    }
    let file =
        std::fs::File::open(archive_path).map_err(|e| format!("Failed to open archive: {e}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Failed to read archive: {e}"))?;

    if archive.len() > limits.max_entries {
        return Err("Java archive contains too many entries".to_string());
    }
    let mut declared_total = 0u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| format!("Archive entry {index} error: {e}"))?;
        let name = entry
            .name()
            .map_err(|e| format!("Archive entry {index} name error: {e}"))?;
        if entry.encrypted()
            || !matches!(
                entry.compression(),
                zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
            )
        {
            return Err(format!("Unsupported archive entry: {name}"));
        }
        if name.split('/').count() > limits.max_path_depth {
            return Err(format!("Archive path is too deep: {name}"));
        }
        if entry.size() > limits.max_entry_bytes {
            return Err(format!("Archive entry is too large: {name}"));
        }
        if entry.size() > 0
            && (entry.compressed_size() == 0
                || entry.size()
                    > entry
                        .compressed_size()
                        .saturating_mul(limits.max_compression_ratio))
        {
            return Err(format!("Suspicious compression ratio: {name}"));
        }
        declared_total = declared_total
            .checked_add(entry.size())
            .filter(|total| *total <= limits.max_uncompressed_bytes)
            .ok_or_else(|| "Java archive exceeds the uncompressed-size limit".to_string())?;
    }
    let required_space = declared_total.saturating_add(limits.min_free_space_bytes);
    if fs2::available_space(dest_dir).map_err(|e| format!("Failed to inspect free space: {e}"))?
        < required_space
    {
        return Err("Not enough free space to extract the Java archive safely".to_string());
    }

    let mut actual_total = 0u64;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Archive entry {i} error: {e}"))?;

        let name = entry
            .name()
            .map_err(|e| format!("Archive entry {i} name error: {e}"))?
            .into_owned();
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
        let expected = entry.size();
        let copied = std::io::copy(&mut entry, &mut out)
            .map_err(|e| format!("Failed to extract {}: {e}", name))?;
        actual_total = actual_total
            .checked_add(copied)
            .filter(|total| *total <= declared_total)
            .ok_or_else(|| "Java archive produced more data than declared".to_string())?;
        if copied != expected {
            return Err(format!("Archive entry size mismatch: {name}"));
        }
    }

    log::info!(
        "Extracted {} entries to {}",
        archive.len(),
        dest_dir.display()
    );
    Ok(())
}

fn archive_limits() -> blockfield_shared::ArchiveLimits {
    fn limit(name: &str, compiled: Option<&str>, default: u64) -> u64 {
        std::env::var(name)
            .ok()
            .or_else(|| compiled.map(str::to_string))
            .and_then(|value| value.parse().ok())
            .unwrap_or(default)
    }

    let mut limits = blockfield_shared::ArchiveLimits::default();
    limits.max_compressed_bytes = limit(
        "BLOCKFIELD_MAX_ARCHIVE_BYTES",
        option_env!("BLOCKFIELD_MAX_ARCHIVE_BYTES"),
        limits.max_compressed_bytes,
    );
    limits.max_uncompressed_bytes = limit(
        "BLOCKFIELD_MAX_EXTRACTED_BYTES",
        option_env!("BLOCKFIELD_MAX_EXTRACTED_BYTES"),
        limits.max_uncompressed_bytes,
    );
    limits.max_entry_bytes = limit(
        "BLOCKFIELD_MAX_ARCHIVE_ENTRY_BYTES",
        option_env!("BLOCKFIELD_MAX_ARCHIVE_ENTRY_BYTES"),
        limits.max_entry_bytes,
    );
    limits.max_entries = limit(
        "BLOCKFIELD_MAX_ARCHIVE_ENTRIES",
        option_env!("BLOCKFIELD_MAX_ARCHIVE_ENTRIES"),
        limits.max_entries as u64,
    ) as usize;
    limits.max_compression_ratio = limit(
        "BLOCKFIELD_MAX_ARCHIVE_RATIO",
        option_env!("BLOCKFIELD_MAX_ARCHIVE_RATIO"),
        limits.max_compression_ratio,
    );
    limits.max_path_depth = limit(
        "BLOCKFIELD_MAX_ARCHIVE_DEPTH",
        option_env!("BLOCKFIELD_MAX_ARCHIVE_DEPTH"),
        limits.max_path_depth as u64,
    ) as usize;
    limits.min_free_space_bytes = limit(
        "BLOCKFIELD_MIN_FREE_SPACE_BYTES",
        option_env!("BLOCKFIELD_MIN_FREE_SPACE_BYTES"),
        limits.min_free_space_bytes,
    );
    limits
}

#[cfg(test)]
mod tests {
    use super::super::java::java_exe_name;
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

    fn write_deflated_zip(path: &Path, name: &str, contents: &[u8]) {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(contents).unwrap();
        writer.finish().unwrap();
    }

    #[test]
    fn java_install_is_staged_and_rejects_unsafe_archives() {
        let root =
            std::env::temp_dir().join(format!("blockfield-java-install-{}", std::process::id()));
        let old_java = root.join("java").join("bin").join(java_exe_name());
        std::fs::create_dir_all(old_java.parent().unwrap()).unwrap();
        std::fs::write(&old_java, b"old").unwrap();

        let malicious = root.join("malicious.zip");
        for entry in [
            "jdk/../../evil",
            "../evil",
            "jdk/../evil",
            "C:/evil",
            "jdk/evil:stream",
            "jdk\\evil",
        ] {
            write_zip(&malicious, &[(entry, b"bad")]);
            assert!(
                install_java_archive(&malicious, &root, "2")
                    .unwrap_err()
                    .contains("Unsafe archive entry"),
                "{entry}"
            );
            assert_eq!(std::fs::read(&old_java).unwrap(), b"old");
            assert!(!root.join("evil").exists());
        }

        let mut links = zip::ZipWriter::new(std::fs::File::create(&malicious).unwrap());
        links
            .add_symlink(
                "jdk/link",
                "../../evil",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        links.finish().unwrap();
        assert!(install_java_archive(&malicious, &root, "2")
            .unwrap_err()
            .contains("symlink"));
        assert_eq!(std::fs::read(&old_java).unwrap(), b"old");

        let missing_java = root.join("missing-java.zip");
        write_zip(&missing_java, &[("jdk/readme.txt", b"no java")]);
        assert!(install_java_archive(&missing_java, &root, "2").is_err());
        assert_eq!(std::fs::read(&old_java).unwrap(), b"old");

        let valid = root.join("valid.zip");
        let java_entry = format!("jdk/bin/{}", java_exe_name());
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&valid).unwrap());
        writer
            .start_file(
                java_entry.as_str(),
                zip::write::SimpleFileOptions::default().unix_permissions(0o6755),
            )
            .unwrap();
        writer.write_all(b"new").unwrap();
        writer.finish().unwrap();
        let installed = install_java_archive(&valid, &root, "2").unwrap();
        assert_eq!(std::fs::read(&installed).unwrap(), b"new");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&installed).unwrap().permissions().mode();
            assert_eq!(mode & 0o6000, 0, "SUID/SGID must not be applied");
        }
        assert_eq!(
            std::fs::read_to_string(root.join("java/.version")).unwrap(),
            "2"
        );
        assert!(!root.join(".java-backup").exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn java_archive_limits_reject_resource_exhaustion() {
        let root = std::env::temp_dir().join(format!(
            "blockfield-java-limits-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let dest = root.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        let archive = root.join("test.zip");
        write_zip(&archive, &[("jdk/a", b"aa"), ("jdk/b", b"b")]);

        let archive_size = std::fs::metadata(&archive).unwrap().len();
        let mut limits = blockfield_shared::ArchiveLimits {
            min_free_space_bytes: 0,
            max_compressed_bytes: archive_size - 1,
            ..Default::default()
        };
        assert!(extract_archive_with_limits(&archive, &dest, &limits)
            .unwrap_err()
            .contains("compressed-size"));
        limits.max_compressed_bytes = archive_size;
        limits.max_path_depth = 1;
        assert!(extract_archive_with_limits(&archive, &dest, &limits)
            .unwrap_err()
            .contains("too deep"));
        assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 0);

        let mut limits = blockfield_shared::ArchiveLimits {
            min_free_space_bytes: 0,
            max_entries: 1,
            ..Default::default()
        };
        assert!(extract_archive_with_limits(&archive, &dest, &limits).is_err());
        limits.max_entries = 2;
        limits.max_entry_bytes = 1;
        assert!(extract_archive_with_limits(&archive, &dest, &limits).is_err());
        limits.max_entry_bytes = 2;
        limits.max_uncompressed_bytes = 2;
        assert!(extract_archive_with_limits(&archive, &dest, &limits).is_err());
        limits.max_uncompressed_bytes = 3;
        limits.min_free_space_bytes = u64::MAX;
        assert!(extract_archive_with_limits(&archive, &dest, &limits).is_err());

        let bomb = root.join("bomb.zip");
        write_deflated_zip(&bomb, "jdk/bin/java.exe", &[0; 16_384]);
        limits = blockfield_shared::ArchiveLimits::default();
        limits.min_free_space_bytes = 0;
        limits.max_compression_ratio = 2;
        assert!(extract_archive_with_limits(&bomb, &dest, &limits).is_err());

        limits.max_compression_ratio = 10_000;
        assert!(extract_archive_with_limits(&bomb, &dest, &limits).is_ok());
        assert_eq!(
            std::fs::metadata(dest.join("bin/java.exe")).unwrap().len(),
            16_384
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    fn write_tar_gz(path: &Path, entries: &[(&str, u8, u32, &[u8], &str)]) {
        // Raw ustar writer so crafted paths bypass `tar::Builder` validation.
        // kind: b'0' file, b'5' dir, b'2' symlink, b'1' hardlink.
        use flate2::{write::GzEncoder, Compression};
        let file = std::fs::File::create(path).unwrap();
        let mut gz = GzEncoder::new(file, Compression::default());
        for (name, kind, mode, data, link) in entries {
            let mut header = [0u8; 512];
            let name_bytes = name.as_bytes();
            header[..name_bytes.len().min(100)]
                .copy_from_slice(&name_bytes[..name_bytes.len().min(100)]);
            let mode_str = format!("{mode:07o}\0");
            header[100..108].copy_from_slice(&mode_str.as_bytes()[..8]);
            let size = if *kind == b'5' { 0 } else { data.len() as u64 };
            let size_str = format!("{size:011o}\0");
            header[124..136].copy_from_slice(&size_str.as_bytes()[..12]);
            header[148..156].copy_from_slice(b"        ");
            header[156] = *kind;
            let link_bytes = link.as_bytes();
            header[157..157 + link_bytes.len().min(100)]
                .copy_from_slice(&link_bytes[..link_bytes.len().min(100)]);
            header[257..262].copy_from_slice(b"ustar");
            header[263..265].copy_from_slice(b"00");
            let sum: u32 = header.iter().map(|b| *b as u32).sum();
            let chk = format!("{sum:06o}\0 ");
            header[148..156].copy_from_slice(&chk.as_bytes()[..8]);
            gz.write_all(&header).unwrap();
            if *kind != b'5' && !data.is_empty() {
                gz.write_all(data).unwrap();
                let pad = (512 - (data.len() % 512)) % 512;
                gz.write_all(&vec![0u8; pad]).unwrap();
            }
        }
        gz.write_all(&[0u8; 1024]).unwrap();
        gz.finish().unwrap();
    }

    fn tar_file(name: &str, contents: &[u8]) -> (String, u8, u32, Vec<u8>, String) {
        (
            name.to_string(),
            b'0',
            0o644,
            contents.to_vec(),
            String::new(),
        )
    }

    #[test]
    fn tar_gz_install_rejects_unsafe_entries_and_links() {
        let root = std::env::temp_dir().join(format!(
            "blockfield-java-tar-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let old_java = root.join("java").join("bin").join(java_exe_name());
        std::fs::create_dir_all(old_java.parent().unwrap()).unwrap();
        std::fs::write(&old_java, b"old").unwrap();

        let malicious = root.join("malicious.tar.gz");
        for entry in [
            "jdk/../../evil",
            "../evil",
            "jdk/../evil",
            "C:/evil",
            "jdk/evil:stream",
            "jdk\\evil",
            "/absolute",
        ] {
            let e = tar_file(entry, b"bad");
            write_tar_gz(&malicious, &[(&e.0, e.1, e.2, &e.3, &e.4)]);
            assert!(
                install_java_archive(&malicious, &root, "2")
                    .unwrap_err()
                    .contains("Unsafe archive entry"),
                "{entry}"
            );
            assert_eq!(std::fs::read(&old_java).unwrap(), b"old");
            assert!(!root.join("evil").exists());
        }

        write_tar_gz(&malicious, &[("jdk/link", b'2', 0o777, b"", "../../evil")]);
        assert!(install_java_archive(&malicious, &root, "2")
            .unwrap_err()
            .contains("symlink"));
        assert_eq!(std::fs::read(&old_java).unwrap(), b"old");

        write_tar_gz(&malicious, &[("jdk/hard", b'1', 0o644, b"", "jdk/other")]);
        assert!(install_java_archive(&malicious, &root, "2")
            .unwrap_err()
            .contains("symlink"));
        assert_eq!(std::fs::read(&old_java).unwrap(), b"old");

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tar_gz_limits_reject_resource_exhaustion() {
        let root = std::env::temp_dir().join(format!(
            "blockfield-java-tar-limits-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let dest = root.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        let archive = root.join("test.tar.gz");
        let a = tar_file("jdk/a", b"aa");
        let b = tar_file("jdk/b", b"b");
        write_tar_gz(
            &archive,
            &[(&a.0, a.1, a.2, &a.3, &a.4), (&b.0, b.1, b.2, &b.3, &b.4)],
        );

        let archive_size = std::fs::metadata(&archive).unwrap().len();
        let mut limits = blockfield_shared::ArchiveLimits {
            min_free_space_bytes: 0,
            max_compressed_bytes: archive_size - 1,
            ..Default::default()
        };
        assert!(extract_tar_gz_with_limits(&archive, &dest, &limits)
            .unwrap_err()
            .contains("compressed-size"));
        limits.max_compressed_bytes = archive_size;
        limits.max_path_depth = 1;
        assert!(extract_tar_gz_with_limits(&archive, &dest, &limits)
            .unwrap_err()
            .contains("too deep"));
        assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 0);

        let mut limits = blockfield_shared::ArchiveLimits {
            min_free_space_bytes: 0,
            max_entries: 1,
            ..Default::default()
        };
        assert!(extract_tar_gz_with_limits(&archive, &dest, &limits).is_err());
        limits.max_entries = 2;
        limits.max_entry_bytes = 1;
        assert!(extract_tar_gz_with_limits(&archive, &dest, &limits).is_err());
        limits.max_entry_bytes = 2;
        limits.max_uncompressed_bytes = 2;
        assert!(extract_tar_gz_with_limits(&archive, &dest, &limits).is_err());
        limits.max_uncompressed_bytes = 3;
        limits.min_free_space_bytes = u64::MAX;
        assert!(extract_tar_gz_with_limits(&archive, &dest, &limits).is_err());

        let bomb = root.join("bomb.tar.gz");
        let zeros = vec![0u8; 16_384];
        let e = tar_file("jdk/bin/java", &zeros);
        write_tar_gz(&bomb, &[(&e.0, e.1, e.2, &e.3, &e.4)]);
        limits = blockfield_shared::ArchiveLimits::default();
        limits.min_free_space_bytes = 0;
        limits.max_compression_ratio = 2;
        assert!(extract_tar_gz_with_limits(&bomb, &dest, &limits).is_err());

        limits.max_compression_ratio = 10_000;
        assert!(extract_tar_gz_with_limits(&bomb, &dest, &limits).is_ok());
        assert_eq!(
            std::fs::metadata(dest.join("bin/java")).unwrap().len(),
            16_384
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tar_gz_installs_valid_runtime_and_keeps_exec_bits() {
        let root = std::env::temp_dir().join(format!(
            "blockfield-java-tar-valid-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let valid = root.join("valid.tar.gz");
        let java_entry = format!("jdk-21.0.5+11/bin/{}", java_exe_name());
        write_tar_gz(&valid, &[(&java_entry, b'0', 0o6755, b"new", "")]);
        let installed = install_java_archive(&valid, &root, "2").unwrap();
        assert_eq!(std::fs::read(&installed).unwrap(), b"new");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&installed).unwrap().permissions().mode();
            assert_eq!(mode & 0o6000, 0, "SUID/SGID must not be applied");
            assert_ne!(mode & 0o100, 0, "owner executable bit must survive");
        }
        assert_eq!(
            std::fs::read_to_string(root.join("java/.version")).unwrap(),
            "2"
        );
        assert!(!root.join(".java-backup").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_unsafe_relative_paths() {
        for value in ["", "/abs", "a\\b", "../x", "a/../b", "c:/x", "./a"] {
            assert!(safe_relative_path(value).is_err(), "{value}");
        }
        assert_eq!(
            safe_relative_path("mods/a.jar").unwrap(),
            PathBuf::from("mods/a.jar")
        );
    }
}
