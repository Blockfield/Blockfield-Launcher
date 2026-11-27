use crate::config::LauncherConfig;
use std::io::Write;
use std::process::Stdio;
pub fn run_hook(
    script: &str,
    config: &LauncherConfig,
    java: &str,
    exit_code: Option<i32>,
) -> Result<(), String> {
    if script.trim().is_empty() {
        return Ok(());
    }
    let log_dir = std::path::Path::new(&config.game_dir).join("logs");
    std::fs::create_dir_all(&log_dir).map_err(|e| e.to_string())?;
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("launcher-hooks.log"))
        .map_err(|e| e.to_string())?;
    writeln!(log, "\n--- Launcher command ---").map_err(|e| e.to_string())?;
    #[cfg(windows)]
    let mut command = {
        let mut command = crate::host_env::command("cmd.exe");
        command.args(["/D", "/S", "/C", script]);
        command
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut command = crate::host_env::command("/bin/sh");
        command.args(["-c", script]);
        command
    };
    let status = command
        .current_dir(&config.game_dir)
        .env("INST_DIR", &config.game_dir)
        .env("INST_MC_DIR", &config.game_dir)
        .env("INST_JAVA", java)
        .env(
            "INST_EXIT_CODE",
            exit_code.map(|code| code.to_string()).unwrap_or_default(),
        )
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log)
        .status()
        .map_err(|e| format!("Cannot run command: {e}"))?;
    if !status.success() {
        return Err(format!(
            "Command exited with {status}; see logs/launcher-hooks.log"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_hooks_do_not_touch_the_filesystem() {
        assert!(run_hook(" \n", &LauncherConfig::default(), "java", None).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn hooks_use_game_directory_environment_and_report_failure() {
        let dir = std::env::temp_dir().join(format!("blockfield-hooks-{}", std::process::id()));
        let config = LauncherConfig {
            game_dir: dir.to_string_lossy().into_owned(),
            ..LauncherConfig::default()
        };
        run_hook(
            "printf '%s' \"$INST_JAVA:$INST_EXIT_CODE\" > hook-result",
            &config,
            "/java with spaces/bin/java",
            Some(7),
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("hook-result")).unwrap(),
            "/java with spaces/bin/java:7"
        );
        assert!(run_hook("exit 3", &config, "java", None).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
