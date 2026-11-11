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

fn desktop_file_name() -> &'static str {
    if launcher_core::dev::is_dev_build() {
        "blockfield-launcher-dev.desktop"
    } else {
        "blockfield-launcher.desktop"
    }
}

/// Escape one `Exec` argument per the desktop-entry spec: a literal `%`
/// becomes `%%`, every other reserved character takes a backslash. Field
/// codes (`%U` etc.) are appended by the caller, never escaped here.
fn escape_exec_arg(arg: &str) -> String {
    let mut out = String::with_capacity(arg.len());
    for ch in arg.chars() {
        match ch {
            '%' => out.push_str("%%"),
            ' ' | '\t' | '\n' | '"' | '\'' | '\\' | '>' | '<' | '~' | '|' | '&' | ';' | '*'
            | '?' | '#' | '(' | ')' | '`' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Linux desktop entry: `Exec` is the current binary, `%U` receives the URL.
pub fn desktop_entry(exec: &str) -> String {
    let escaped = escape_exec_arg(exec);
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
         Exec={escaped} %U\n\
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
        assert!(entry.contains("Exec=/opt/blockfield/blockfield-launcher %U"));
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
    fn desktop_entry_escapes_reserved_exec_chars() {
        let line = |exec: &str| {
            desktop_entry(exec)
                .lines()
                .find_map(|line| line.strip_prefix("Exec="))
                .unwrap()
                .to_string()
        };
        assert_eq!(
            line("/opt/100% ready/launcher;v2"),
            "/opt/100%%\\ ready/launcher\\;v2 %U"
        );
        assert_eq!(
            line("/opt/dir with spaces/launcher"),
            "/opt/dir\\ with\\ spaces/launcher %U"
        );
        assert_eq!(line("/opt/a\"b\\c"), "/opt/a\\\"b\\\\c %U");
        // A plain path stays untouched, and %U keeps its field-code meaning.
        assert_eq!(
            line("/opt/blockfield/launcher"),
            "/opt/blockfield/launcher %U"
        );
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
