use std::ffi::OsStr;
use std::process::Command;

/// Spawns host programs with the AppImage runtime environment stripped out.
///
/// AppRun points LD_LIBRARY_PATH, PYTHONHOME, PERLLIB and the GTK/GIO module paths at the
/// bundled copies inside the mounted AppImage. Anything we spawn inherits them and breaks:
/// `python3` dies with "No module named 'encodings'", `gio`/`xdg-open` hit undefined symbols,
/// `flatpak` fails on the older bundled OpenSSL. Outside an AppImage this is a plain `Command`.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    let Some(appdir) = std::env::var_os("APPDIR").and_then(|dir| dir.into_string().ok()) else {
        return command;
    };
    let vars = std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)));
    for (key, value) in overrides(&appdir, vars) {
        match value {
            Some(value) => command.env(key, value),
            None => command.env_remove(key),
        };
    }
    command
}

/// Environment edits that remove every `appdir` entry: `None` means unset the variable.
fn overrides(
    appdir: &str,
    vars: impl Iterator<Item = (String, String)>,
) -> Vec<(String, Option<String>)> {
    vars.filter(|(_, value)| value.contains(appdir))
        .map(|(key, value)| {
            let kept = value
                .split(':')
                .filter(|entry| !entry.contains(appdir))
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
            overrides("/tmp/.mount_bf", vars.into_iter()),
            vec![
                ("PYTHONHOME".into(), None),
                ("LD_LIBRARY_PATH".into(), Some("/usr/lib64".into())),
                ("APPDIR".into(), None),
            ]
        );
    }
}
