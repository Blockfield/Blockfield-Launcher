use std::ffi::OsStr;
use std::process::Command;

/// Spawns host programs with the AppImage runtime environment stripped out.
///
/// AppRun points LD_LIBRARY_PATH, PYTHONHOME, PERLLIB and the GTK/GIO module paths at the
/// bundled copies inside the mounted AppImage. Anything we spawn inherits them and breaks:
/// `python3` dies with "No module named 'encodings'", `gio`/`xdg-open` hit undefined symbols,
/// `flatpak` fails on the older bundled OpenSSL. Outside an AppImage this is a plain `Command`.
///
/// After a self-update relaunch the new AppRun appends its own mount to the inherited
/// variables, so entries from the previous `/tmp/.mount_*` survive while APPDIR points at the
/// new one. Every `.mount_` entry is dropped, not just the current APPDIR.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    let appdir = std::env::var("APPDIR").ok();
    let vars = std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)));
    for (key, value) in overrides(appdir.as_deref(), vars) {
        match value {
            Some(value) => command.env(key, value),
            None => command.env_remove(key),
        };
    }
    command
}

fn is_appimage_entry(entry: &str, appdir: Option<&str>) -> bool {
    entry.contains("/.mount_") || appdir.is_some_and(|dir| !dir.is_empty() && entry.contains(dir))
}

/// Environment edits that remove every AppImage entry: `None` means unset the variable.
fn overrides(
    appdir: Option<&str>,
    vars: impl Iterator<Item = (String, String)>,
) -> Vec<(String, Option<String>)> {
    vars.filter(|(_, value)| is_appimage_entry(value, appdir))
        .map(|(key, value)| {
            let kept = value
                .split(':')
                .filter(|entry| !is_appimage_entry(entry, appdir))
                .collect::<Vec<_>>()
                .join(":");
            (key, (!kept.is_empty()).then_some(kept))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appimage_entries_are_dropped_and_host_entries_survive() {
        let vars = [
            ("PYTHONHOME", "/tmp/.mount_bf/usr/"),
            ("LD_LIBRARY_PATH", "/tmp/.mount_bf/usr/lib:/usr/lib64"),
            ("APPDIR", "/tmp/.mount_bf"),
            ("PATH", "/usr/bin:/usr/local/bin"),
        ]
        .map(|(key, value)| (key.to_string(), value.to_string()));

        assert_eq!(
            overrides(Some("/tmp/.mount_bf"), vars.into_iter()),
            vec![
                ("PYTHONHOME".into(), None),
                ("LD_LIBRARY_PATH".into(), Some("/usr/lib64".into())),
                ("APPDIR".into(), None),
            ]
        );
    }

    #[test]
    fn stale_mounts_from_relaunch_are_purged_even_without_appdir() {
        let vars = [
            (
                "LD_LIBRARY_PATH",
                "/tmp/.mount_new/usr/lib:/usr/lib64:/tmp/.mount_old/usr/lib",
            ),
            ("PYTHONPATH", "/tmp/.mount_old/usr/share/pyshared/"),
        ]
        .map(|(key, value)| (key.to_string(), value.to_string()));

        assert_eq!(
            overrides(None, vars.into_iter()),
            vec![
                ("LD_LIBRARY_PATH".into(), Some("/usr/lib64".into())),
                ("PYTHONPATH".into(), None),
            ]
        );
    }

    #[test]
    fn extracted_appdir_outside_tmp_is_still_stripped() {
        let vars = [("PERLLIB", "/opt/squashfs-root/usr/lib/perl5:/usr/lib/perl5")]
            .map(|(key, value)| (key.to_string(), value.to_string()));

        assert_eq!(
            overrides(Some("/opt/squashfs-root"), vars.into_iter()),
            vec![("PERLLIB".into(), Some("/usr/lib/perl5".into()))]
        );
    }
}
