//! OS registration for invite links (Tauri deep-link parity).
//!
//! Tauri side is `src-tauri/tauri.conf.json` plus `register_all` on Linux and
//! NSIS `deep_link_protocols` on Windows: the OS hands invite URLs to the
//! launcher, which routes them through the single-instance channel. This
//! module owns the native side: the Linux desktop entry, the Windows class
//! entries, and the runtime `register()`.

/// `blockfield` plus the Discord invite scheme for this build.
pub fn discord_scheme() -> String {
    format!("discord-{}", launcher_app::presence::application_id())
}

/// Schemes the OS must hand to the launcher, in Tauri conf order.
pub fn schemes() -> Vec<String> {
    vec!["blockfield".to_string(), discord_scheme()]
}

#[cfg(any(target_os = "linux", test))]
fn desktop_file_name() -> &'static str {
    if launcher_core::dev::is_dev_build() {
        "blockfield-launcher-dev.desktop"
    } else {
        "blockfield-launcher.desktop"
    }
}

/// Escape string-value control characters (`\\`, `\n`, `\t`, `\r`) per the
/// desktop-entry spec so a value never breaks the `Key=Value` line.
fn escape_desktop_value(raw: &str) -> String {
    let mut value = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => value.push_str("\\\\"),
            '\n' => value.push_str("\\n"),
            '\t' => value.push_str("\\t"),
            '\r' => value.push_str("\\r"),
            _ => value.push(ch),
        }
    }
    value
}

/// Quote one `Exec` argument per the desktop-entry spec, in the order the spec
/// decodes: first the `Exec` parsing layer (enclose in double quotes, escape
/// `"`, '`', `$` and `\` with a backslash, `%` becomes `%%`), then the
/// string-value layer, where every backslash from the first layer is doubled
/// (value escapes are applied first on decode). Field codes (`%U` etc.) are
/// appended by the caller, never quoted here.
fn quote_exec_arg(arg: &str) -> String {
    let mut quoted = String::with_capacity(arg.len() + 2);
    quoted.push('"');
    for ch in arg.chars() {
        match ch {
            '"' | '`' | '$' | '\\' => {
                quoted.push('\\');
                quoted.push(ch);
            }
            '%' => quoted.push_str("%%"),
            _ => quoted.push(ch),
        }
    }
    quoted.push('"');
    escape_desktop_value(&quoted)
}

/// Prefer a sibling `icon.png` (user install via `install-linux.sh`),
/// otherwise fall back to the themed `blockfield-launcher` icon installed by
/// the `.deb` package.
fn desktop_icon(exec: &str) -> String {
    if let Some(icon) = std::path::Path::new(exec)
        .parent()
        .map(|dir| dir.join("icon.png"))
        .filter(|path| path.is_file())
    {
        return escape_desktop_value(&icon.to_string_lossy());
    }
    "blockfield-launcher".to_string()
}

/// Linux desktop entry: `Exec` is the current binary, `%U` receives the URL.
pub fn desktop_entry(exec: &str) -> String {
    let quoted = quote_exec_arg(exec);
    let icon = desktop_icon(exec);
    let mime = schemes()
        .iter()
        .map(|scheme| format!("x-scheme-handler/{scheme}"))
        .collect::<Vec<_>>()
        .join(";");
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Blockfield Launcher\n\
         Comment=Blockfield game launcher\n\
         Exec={quoted} %U\n\
         Icon={icon}\n\
         Terminal=false\n\
         Categories=Game;\n\
         StartupWMClass=blockfield-native\n\
         MimeType={mime};\n"
    )
}

#[cfg(any(target_os = "linux", test))]
fn packaged_desktop_matches(path: &std::path::Path, exec: &str) -> bool {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return false;
    };
    let quoted_exec = format!("Exec={} %U", quote_exec_arg(exec));
    let plain_exec = format!("Exec={exec} %U");
    let has_exec = contents
        .lines()
        .any(|line| line == quoted_exec || line == plain_exec);
    has_exec
        && schemes()
            .iter()
            .all(|scheme| contents.contains(&format!("x-scheme-handler/{scheme}")))
}

/// `reg add` argv per scheme (HKCU, no elevation). Kept as data so tests pin
/// the exact keys without touching the registry.
pub fn reg_commands(exe: &str) -> Vec<Vec<String>> {
    let mut commands = Vec::new();
    for scheme in schemes() {
        let key = format!(r"HKCU\Software\Classes\{scheme}");
        commands.push(vec![
            "add".into(),
            key.clone(),
            "/ve".into(),
            "/d".into(),
            format!("URL:{scheme} protocol"),
            "/f".into(),
        ]);
        commands.push(vec![
            "add".into(),
            key.clone(),
            "/v".into(),
            "URL Protocol".into(),
            "/d".into(),
            String::new(),
            "/f".into(),
        ]);
        commands.push(vec![
            "add".into(),
            format!(r"{key}\DefaultIcon"),
            "/ve".into(),
            "/d".into(),
            format!("{exe},0"),
            "/f".into(),
        ]);
        commands.push(vec![
            "add".into(),
            format!(r"{key}\shell\open\command"),
            "/ve".into(),
            "/d".into(),
            format!("\"{exe}\" \"%1\""),
            "/f".into(),
        ]);
    }
    commands
}

/// Best-effort registration at startup: never fail the launch, report to
/// stderr like the Tauri `register_all` warning path.
pub fn register() -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    return register_linux();
    #[cfg(target_os = "windows")]
    return register_windows();
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    Ok(())
}

#[cfg(any(target_os = "linux", test))]
fn resolve_linux_exec(
    current_exe: &std::path::Path,
    appimage: Option<&std::ffi::OsStr>,
) -> std::path::PathBuf {
    if let Some(path) = appimage
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_file())
    {
        return path;
    }
    current_exe.to_path_buf()
}

#[cfg(target_os = "linux")]
fn register_linux() -> std::io::Result<()> {
    let current = std::env::current_exe()?;
    let appimage = std::env::var_os("APPIMAGE");
    let exe = resolve_linux_exec(&current, appimage.as_deref());
    let exe = exe.to_string_lossy();
    let Some(data_dir) = dirs::data_dir() else {
        return Ok(());
    };
    let name = desktop_file_name();
    let dir = data_dir.join("applications");
    let user_desktop = dir.join(name);
    let system_desktop = std::path::Path::new("/usr/share/applications").join(name);
    // Avoid shadowing an already matching packaged .deb desktop entry when no
    // user override exists; otherwise write/refresh the user entry.
    if user_desktop.exists() || !packaged_desktop_matches(&system_desktop, &exe) {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(&user_desktop, desktop_entry(&exe))?;
    }
    for scheme in schemes() {
        let mime = format!("x-scheme-handler/{scheme}");
        if let Err(error) = std::process::Command::new("xdg-mime")
            .arg("default")
            .arg(name)
            .arg(&mime)
            .status()
        {
            eprintln!("Cannot register {mime} handler: {error}");
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn register_windows() -> std::io::Result<()> {
    let exe = std::env::current_exe()?
        .to_string_lossy()
        .replace('/', "\\");
    for args in reg_commands(&exe) {
        if let Err(error) = std::process::Command::new("reg").args(&args).status() {
            eprintln!("Cannot register deep-link handler ({}): {error}", args[1]);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemes_include_blockfield_and_discord_app_id() {
        assert_eq!(
            schemes(),
            vec![
                "blockfield".to_string(),
                format!("discord-{}", launcher_app::presence::APPLICATION_ID),
            ]
        );
    }

    #[test]
    fn linux_exec_prefers_appimage_path_over_ephemeral_mount() {
        let dir = tempfile::tempdir().unwrap();
        let appimage = dir.path().join("blockfield-launcher.AppImage");
        std::fs::write(&appimage, b"appimage").unwrap();
        let mounted = std::path::Path::new("/tmp/.mount_bf/usr/bin/blockfield-launcher");
        assert_eq!(
            resolve_linux_exec(mounted, Some(appimage.as_os_str())),
            appimage
        );
        assert_eq!(resolve_linux_exec(mounted, None), mounted.to_path_buf());
    }

    #[test]
    fn desktop_entry_advertises_both_handlers() {
        assert_eq!(desktop_file_name(), "blockfield-launcher.desktop");
        let entry = desktop_entry("/opt/blockfield/blockfield-launcher");
        assert!(entry.contains("Exec=\"/opt/blockfield/blockfield-launcher\" %U\n"));
        assert!(entry.contains("Icon=blockfield-launcher\n"));
        assert!(entry.contains("StartupWMClass=blockfield-native\n"));
        for scheme in schemes() {
            assert!(entry.contains(&format!("x-scheme-handler/{scheme}")));
        }
        let asset = include_str!("../assets/blockfield-launcher.desktop");
        for scheme in schemes() {
            assert!(
                asset.contains(&format!("x-scheme-handler/{scheme}")),
                "asset desktop file must register {scheme}"
            );
        }
        assert!(asset.contains("%U"));
    }

    #[test]
    fn desktop_entry_uses_sibling_icon_and_matches_packaged_entry() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("blockfield-launcher");
        let icon = dir.path().join("icon.png");
        std::fs::write(&exe, b"elf").unwrap();
        std::fs::write(&icon, b"png").unwrap();

        let entry = desktop_entry(&exe.to_string_lossy());
        assert!(entry.contains(&format!(
            "Icon={}\n",
            escape_desktop_value(&icon.to_string_lossy())
        )));
        assert!(entry.contains("StartupWMClass=blockfield-native\n"));

        let packaged = dir.path().join("blockfield-launcher.desktop");
        let mime = schemes()
            .iter()
            .map(|scheme| format!("x-scheme-handler/{scheme}"))
            .collect::<Vec<_>>()
            .join(";");
        std::fs::write(
            &packaged,
            format!(
                "[Desktop Entry]\nExec=/usr/bin/blockfield-launcher %U\nIcon=blockfield-launcher\nMimeType={mime};\n"
            ),
        )
        .unwrap();
        assert!(packaged_desktop_matches(
            &packaged,
            "/usr/bin/blockfield-launcher"
        ));
        assert!(!packaged_desktop_matches(
            &packaged,
            "/opt/other/blockfield-launcher"
        ));
    }

    #[test]
    fn desktop_entry_quotes_exec_per_spec() {
        let line = |exec: &str| {
            desktop_entry(exec)
                .lines()
                .find_map(|line| line.strip_prefix("Exec="))
                .unwrap()
                .to_string()
        };
        // Both escaping layers: the Exec quoting backslash is itself doubled
        // for the string-value layer, so `"` encodes as `\\"` in the file.
        assert_eq!(
            line("/opt/100% ready/la\"uncher"),
            "\"/opt/100%% ready/la\\\\\"uncher\" %U"
        );
        assert_eq!(
            line("/opt/dir with spaces/launcher"),
            "\"/opt/dir with spaces/launcher\" %U"
        );
        assert_eq!(
            line("/opt/a\"b\\c`d$e"),
            "\"/opt/a\\\\\"b\\\\\\\\c\\\\`d\\\\$e\" %U"
        );
        // A plain path is quoted as well, and %U keeps its field-code meaning.
        assert_eq!(
            line("/opt/blockfield/launcher"),
            "\"/opt/blockfield/launcher\" %U"
        );
    }

    /// Tiny spec-following inverse of `quote_exec_arg`, test-only: strips the
    /// caller's ` %U`, undoes the string-value layer (`\\`, `\n`, `\t`, `\r`, `\s`, applied
    /// first on decode), then undoes the Exec layer (quotes, `\`-escapes,
    /// `%%` -> `%`).
    fn unescape_exec_value(exec_value: &str) -> String {
        let quoted = exec_value.strip_suffix(" %U").unwrap_or(exec_value);
        let mut layer1 = String::with_capacity(quoted.len());
        let mut chars = quoted.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                match chars.next() {
                    Some('n') => layer1.push('\n'),
                    Some('t') => layer1.push('\t'),
                    Some('r') => layer1.push('\r'),
                    Some('s') => layer1.push(' '),
                    Some(next) => layer1.push(next),
                    None => {}
                }
            } else {
                layer1.push(ch);
            }
        }
        let inner = layer1.strip_prefix('"').unwrap_or(&layer1);
        let inner = inner.strip_suffix('"').unwrap_or(inner);
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if ch == '%' {
                match chars.next() {
                    Some('%') | None => out.push('%'),
                    Some(other) => {
                        out.push('%');
                        out.push(other);
                    }
                }
            } else {
                out.push(ch);
            }
        }
        out
    }

    #[test]
    fn desktop_entry_exec_round_trips_reserved_chars() {
        for path in [
            "/opt/blockfield/launcher",
            "/opt/dir with spaces/launcher",
            "/opt/100% ready/la\"uncher",
            "/opt/a\"b\\c`d$e",
            "/tmp/`back`-$quote\"-\\-%",
            "/home/u/new\nline/\ttab\r/launcher",
        ] {
            let entry = desktop_entry(path);
            assert!(
                entry.lines().filter(|l| l.starts_with("Exec=")).count() == 1
                    && !entry.contains("\nline"),
                "control chars must not split the entry: {path:?}"
            );
            let exec_value = entry
                .lines()
                .find_map(|line| line.strip_prefix("Exec=").map(str::to_string))
                .unwrap();
            assert_eq!(
                unescape_exec_value(&exec_value),
                path,
                "round-trip {path:?}"
            );
        }
    }

    #[test]
    fn windows_commands_register_both_schemes() {
        let commands = reg_commands(r"C:\Blockfield\blockfield-launcher.exe");
        // Four `reg add` calls per scheme (protocol, flag, icon, command).
        assert_eq!(commands.len(), 4 * schemes().len());
        let flat = commands
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");
        for scheme in schemes() {
            assert!(flat.contains(&format!(r"Software\Classes\{scheme}")));
            assert!(flat.contains("URL Protocol"));
        }
        assert!(flat.contains(r#""C:\Blockfield\blockfield-launcher.exe" "%1""#));
    }
}
