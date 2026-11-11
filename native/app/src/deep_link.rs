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

#[cfg(target_os = "linux")]
fn desktop_file_name() -> &'static str {
    if launcher_core::dev::is_dev_build() {
        "blockfield-launcher-dev.desktop"
    } else {
        "blockfield-launcher.desktop"
    }
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
    // String-value layer: newline/tab/CR would otherwise break the key=value line.
    let mut value = String::with_capacity(quoted.len());
    for ch in quoted.chars() {
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

/// Linux desktop entry: `Exec` is the current binary, `%U` receives the URL.
pub fn desktop_entry(exec: &str) -> String {
    let quoted = quote_exec_arg(exec);
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
         Terminal=false\n\
         Categories=Game;\n\
         MimeType={mime};\n"
    )
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

#[cfg(target_os = "linux")]
fn register_linux() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let exe = exe.to_string_lossy();
    let Some(data_dir) = dirs::data_dir() else {
        return Ok(());
    };
    let dir = data_dir.join("applications");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(desktop_file_name()), desktop_entry(&exe))?;
    for scheme in schemes() {
        let mime = format!("x-scheme-handler/{scheme}");
        if let Err(error) = std::process::Command::new("xdg-mime")
            .arg("default")
            .arg(desktop_file_name())
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
    fn schemes_match_the_shipping_tauri_conf() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../../../src-tauri/tauri.conf.json")).unwrap();
        let tauri = conf["plugins"]["deep-link"]["desktop"]["schemes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        // Default build: the env override for the Discord app id is unset.
        assert_eq!(tauri, schemes());
        assert_eq!(
            tauri,
            vec![
                "blockfield".to_string(),
                format!("discord-{}", launcher_app::presence::APPLICATION_ID),
            ]
        );
    }

    #[test]
    fn desktop_entry_advertises_both_handlers() {
        let entry = desktop_entry("/opt/blockfield/blockfield-launcher");
        assert!(entry.contains("Exec=\"/opt/blockfield/blockfield-launcher\" %U"));
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
