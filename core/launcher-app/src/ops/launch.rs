//! Game launch orchestration, moved unchanged from
//! `src-tauri/src/commands.rs` (phase 2, no behaviour change).
//!
//! Tauri-only inputs arrive computed: the wrapper passes `&LauncherAppState`
//! and `&PendingRoom` straight through, `launcher_version` from its own
//! `env!("CARGO_PKG_VERSION")` (a core crate would evaluate it to 0.0.0), and
//! the window as [`LaunchWindow`]. Session expiry goes through
//! [`crate::LauncherEvent::AccountExpired`] so the shell adapter maps it back
//! to today's exact `account://expired` emit. Step order matches
//! `commands.rs` exactly: gate -> begin -> config/identity/pack -> java
//! checks -> dependency ensure -> seed options -> args -> pre-launch hook ->
//! log files -> pending room read+filter -> bridge dir -> room-request
//! write/remove -> game-presence remove -> spawn with env -> running.started
//! -> pending clear-if-unchanged -> hide -> status -> exit monitor thread
//! (finishing, restore, post-exit hook, statuses).

use super::java::{resolved_java_path, system_java_available};
use super::pack_check::pack_base_url;
use crate::rooms::PendingRoom;
use crate::{EventSink, GameIdentity, LauncherAppState, LauncherEvent, PackState};
use launcher_core::config::{ClientProfile, LauncherConfig};
use launcher_core::game::GameSession;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn emit_status(
    events: &EventSink,
    phase: &'static str,
    message: impl Into<String>,
    cancelable: bool,
) {
    events(LauncherAppState::status_event(phase, message, cancelable));
}

/// Status sink for the core minecraft runtime: produces today's
/// `launcher://status` payload (`{"phase", "message", "cancelable"}`).
fn minecraft_status_sink(events: &EventSink) -> launcher_net::minecraft::StatusSink {
    let events = events.clone();
    std::sync::Arc::new(
        move |phase: &'static str, message: &str, cancelable: bool| {
            events(LauncherAppState::status_event(phase, message, cancelable));
        },
    )
}

/// The smallest window seam: `hide` runs exactly where `window.hide()` runs
/// today (only when `hide_while_playing`), `restore` runs in the exit thread
/// exactly where `restore_hidden_launcher` runs today. The shell passes a
/// no-op `hide` when there is no main window.
pub struct LaunchWindow {
    pub hide: Box<dyn FnOnce() + Send>,
    pub restore: Box<dyn FnOnce() + Send>,
}

/// Server defaults for a first launch. Minecraft fills in every other option itself and rewrites
/// the whole file on exit, so a partial file is enough. An existing file is the player's own
/// settings and is never touched — and the modpack cannot carry these, the pack host treats
/// options.txt as a private file.
pub fn seed_game_options(game_dir: &Path) -> std::io::Result<()> {
    let path = game_dir.join("options.txt");
    if path.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(game_dir)?;
    std::fs::write(
        path,
        "fullscreen:true\nrenderDistance:20\nsimulationDistance:20\n",
    )
}

pub async fn launch_game(
    state: &LauncherAppState,
    pending: &PendingRoom,
    launcher_version: &str,
    window: LaunchWindow,
) -> Result<(), String> {
    let _operation = state.begin_operation()?;
    let running = state.game.begin()?;
    let config = state.config.read().await.validated_active()?;
    let skins_base = state
        .pack
        .read()
        .await
        .as_ref()
        .and_then(|pack| pack.info.skins.clone());
    let events = state.events.clone();
    let on_expired: launcher_net::account::SessionExpiredSink =
        Arc::new(move || events(LauncherEvent::AccountExpired));
    let identity = launcher_net::account::game_identity(
        &state.config,
        &state.app_data_dir,
        skins_base,
        &on_expired,
    )
    .await?;
    let pack = state.pack.read().await.clone();
    launch_with_identity(
        state,
        pending,
        config,
        identity,
        pack,
        launcher_version,
        window,
        running,
    )
    .await
}

/// Everything after the session check, in the exact `commands.rs` order. A
/// separate fn (no behaviour change) so tests can drive the launch with a
/// fabricated identity and fixtures instead of the account network.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn launch_with_identity(
    state: &LauncherAppState,
    pending: &PendingRoom,
    config: LauncherConfig,
    identity: GameIdentity,
    pack: Option<PackState>,
    launcher_version: &str,
    window: LaunchWindow,
    running: GameSession,
) -> Result<(), String> {
    let LaunchWindow { hide, restore } = window;
    let java = resolved_java_path(&config);
    if java == "java" && !system_java_available() {
        return Err(
            "Java runtime is not installed. Open Updates to install it or select a Java executable in Settings."
                .to_string(),
        );
    }

    let ram_mb = config.ram_mb;
    let game_dir = config.game_dir.clone();
    let game_dir_path = PathBuf::from(&game_dir);
    std::fs::create_dir_all(&game_dir_path).map_err(|e| e.to_string())?;
    let _tracking = launcher_net::installed_files::begin(&state.app_data_dir, &game_dir_path)?;

    let loader_version_id =
        pack.as_ref()
            .and_then(|p| {
                p.meta.loader.as_deref().map(|loader| {
                    launcher_net::minecraft::loader_version_id(&p.meta.minecraft, loader)
                })
            })
            .or_else(|| launcher_net::minecraft::find_loader_version_id(&game_dir_path));
    let minecraft_version = pack
        .as_ref()
        .map(|p| p.meta.minecraft.clone())
        .or_else(|| {
            loader_version_id.as_ref().and_then(|id| {
                id.strip_prefix("fabric-loader-")?
                    .split_once('-')
                    .map(|(_, mc)| mc.to_string())
            })
        })
        .unwrap_or_else(|| {
            std::env::var("BLOCKFIELD_MINECRAFT_VERSION")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .or_else(|| option_env!("BLOCKFIELD_MINECRAFT_VERSION").map(String::from))
                .unwrap_or_else(|| "1.21.1".to_string())
        });
    let game_server = match &pack {
        Some(pack) => pack.info.server.clone(),
        None if config.active_profile == ClientProfile::Workshop => {
            launcher_net::pack::fetch_launcher_info(&pack_base_url()?)
                .await?
                .server
        }
        None => String::new(),
    };
    let quick_play = config.target(&game_server)?;
    // Dev build: another server, or none at all so the game opens on the title screen.
    let quick_play = launcher_core::dev::server_override().unwrap_or(quick_play);

    log::info!(
        "Launching game: java={java}, ram={ram_mb}MB, dir={game_dir}, user={}",
        identity.username
    );

    {
        state.downloader.reset_cancel();
        // Floor so the frontend always has a non-zero denominator for progress.
        state.downloader.set_grand_total(16 * 1024 * 1024);
        emit_status(&state.events, "launch", "Проверка компонентов игры", false);
        let launch_status = minecraft_status_sink(&state.events);
        launcher_net::minecraft::ensure_launch_dependencies(
            &state.downloader,
            &launch_status,
            &game_dir_path,
            &minecraft_version,
            loader_version_id.as_deref(),
        )
        .await?;
    }

    if let Err(error) = seed_game_options(&game_dir_path) {
        log::warn!("Cannot write default game options: {error}");
    }

    emit_status(&state.events, "launch", "Подготовка запуска", false);
    let args = launcher_net::minecraft::build_launch_args(
        &game_dir_path,
        ram_mb,
        &minecraft_version,
        loader_version_id.as_deref(),
        &identity,
        quick_play.as_deref(),
        // Launcher crate version, passed in from the shell (core crates are 0.0.0).
        launcher_version,
    )?;

    if !config.pre_launch_command.trim().is_empty() {
        emit_status(
            &state.events,
            "launch",
            "Выполнение команды перед запуском",
            false,
        );
        let hook_config = config.clone();
        let hook_java = java.clone();
        tokio::task::spawn_blocking(move || {
            launcher_net::launch_hooks::run_hook(
                &hook_config.pre_launch_command,
                &hook_config,
                &hook_java,
                None,
            )
        })
        .await
        .map_err(|e| format!("Pre-launch task failed: {e}"))??;
    }
    emit_status(&state.events, "launch", "Запуск игры", false);
    let log_dir = game_dir_path.join("logs");
    std::fs::create_dir_all(&log_dir).map_err(|e| format!("Cannot create log directory: {e}"))?;
    let output = std::fs::File::create(log_dir.join("launcher-game.log"))
        .map_err(|e| format!("Cannot create game log: {e}"))?;
    let pending_room = crate::rooms::pending_room(pending)
        .filter(|_| config.active_profile == ClientProfile::Game);
    let bridge_dir = config.bridge_dir(&state.app_data_dir);
    std::fs::create_dir_all(&bridge_dir).map_err(|e| e.to_string())?;
    if let Some(id) = pending_room.as_ref() {
        crate::rooms::write_request(state, id)?;
    } else {
        let _ = std::fs::remove_file(bridge_dir.join("room-request.json"));
    }
    let _ = std::fs::remove_file(bridge_dir.join("game-presence.json"));
    let mut child = launcher_core::host_env::command(&java)
        .env("BLOCKFIELD_BRIDGE_DIR", &bridge_dir)
        .env("BLOCKFIELD_SERVER", quick_play.as_deref().unwrap_or(""))
        .args(&args)
        .current_dir(&game_dir)
        .stdin(std::process::Stdio::null())
        .stdout(output.try_clone().map_err(|e| e.to_string())?)
        .stderr(output)
        .spawn()
        .map_err(|e| format!("Failed to spawn game process: {e}"))?;
    running.started();
    {
        let mut value = pending.0.lock().unwrap();
        if *value == pending_room {
            *value = None;
        }
    }
    if config.hide_while_playing {
        hide();
    }
    log::info!("Game process spawned (pid {})", child.id());
    emit_status(&state.events, "launch", "Игра запущена", false);
    let events = state.events.clone();
    std::thread::spawn(move || {
        let _running = running;
        let exit = child.wait();
        _running.finishing();
        restore();
        match exit {
            Ok(exit) => {
                let result = launcher_net::launch_hooks::run_hook(
                    &config.post_exit_command,
                    &config,
                    &java,
                    exit.code(),
                );
                let message = match result {
                    Ok(()) => format!("Game exited ({exit})"),
                    Err(error) => {
                        emit_status(&events, "hook-error", &error, false);
                        format!("Post-exit command failed: {error}")
                    }
                };
                emit_status(&events, "exited", message, false);
            }
            Err(error) => emit_status(
                &events,
                "exited",
                format!("Failed to wait for game: {error}"),
                false,
            ),
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::test_state;
    use super::*;
    use crate::{EventSink, LauncherEvent};
    use std::sync::{Arc, Mutex as StdMutex};

    #[test]
    fn game_options_are_seeded_once_and_never_overwrite_the_player() {
        let dir = std::env::temp_dir().join(format!("blockfield-options-{}", std::process::id()));
        let options = dir.join("options.txt");

        seed_game_options(&dir).unwrap();
        let seeded = std::fs::read_to_string(&options).unwrap();
        assert!(seeded.contains("fullscreen:true"));
        assert!(seeded.contains("renderDistance:20"));
        assert!(seeded.contains("simulationDistance:20"));

        std::fs::write(&options, "renderDistance:8\n").unwrap();
        seed_game_options(&dir).unwrap();
        assert_eq!(
            std::fs::read_to_string(&options).unwrap(),
            "renderDistance:8\n"
        );

        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Order proof with a fake java (a shell script that exits 0): the
    /// room-request bridge file is written before spawn, pending is cleared
    /// after spawn, hide runs once, restore runs after exit, statuses arrive
    /// in launch order. Unix only: the fake java is a shell script.
    #[test]
    fn launch_order_bridge_then_spawn_then_clear_hide_restore() {
        if cfg!(windows) {
            eprintln!("skipped: the fake java is a unix shell script");
            return;
        }
        let unique = format!(
            "blockfield-launch-order-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let app_data = root.join("app-data");
        let game_dir = root.join("game");
        std::fs::create_dir_all(&game_dir).unwrap();
        let order_log = root.join("order.log");

        // A version fixture with no libraries/assets to fetch: every runtime
        // file resolve hits local disk, so ensure_launch_dependencies and
        // build_launch_args run with no network.
        std::env::set_var("BLOCKFIELD_MINECRAFT_VERSION", "9.9.9-test");
        let version_json = serde_json::json!({
            "mainClass": "net.minecraft.client.main.Main",
            "assetIndex": {"id": "test-index", "url": "https://example.com/index.json"},
            "assets": "test-index",
            "downloads": {"client": {"url": "https://example.com/client.jar", "size": 3}},
            "libraries": [],
            "arguments": {"game": ["--username", "${auth_player_name}"]},
        });
        let version_dir = game_dir.join("versions").join("9.9.9-test");
        std::fs::create_dir_all(&version_dir).unwrap();
        std::fs::write(
            version_dir.join("9.9.9-test.json"),
            serde_json::to_string(&version_json).unwrap(),
        )
        .unwrap();
        std::fs::write(version_dir.join("9.9.9-test.jar"), b"mc!").unwrap();
        let index_dir = game_dir.join("assets").join("indexes");
        std::fs::create_dir_all(&index_dir).unwrap();
        std::fs::write(index_dir.join("test-index.json"), r#"{"objects": {}}"#).unwrap();

        // Fake java: answers -version like a real runtime (validated_active
        // runs it), otherwise records the spawn and what it could see, then
        // blocks until the test releases it so exit/restore ordering is
        // deterministic instead of a scheduling race.
        let release = root.join("release");
        let java = root.join("fake-java.sh");
        std::fs::write(
            &java,
            format!(
                "#!/bin/sh\n\
                 if [ \"$1\" = \"-version\" ]; then\n\
                   echo 'openjdk version \"21.0.5\"' >&2\n\
                   exit 0\n\
                 fi\n\
                 echo spawn >> '{}'\n\
                 if [ -f \"$BLOCKFIELD_BRIDGE_DIR/room-request.json\" ]; then\n\
                   echo spawn-room-request-present >> '{}'\n\
                 else\n\
                   echo spawn-room-request-missing >> '{}'\n\
                 fi\n\
                 i=0\n\
                 while [ ! -f '{}' ] && [ $i -lt 600 ]; do\n\
                   sleep 0.05\n\
                   i=$((i+1))\n\
                 done\n\
                 exit 0\n",
                order_log.display(),
                order_log.display(),
                order_log.display(),
                release.display(),
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&java).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&java, perms).unwrap();
        }

        let received = Arc::new(StdMutex::new(Vec::new()));
        let sink: EventSink = Arc::new({
            let received = received.clone();
            move |event| received.lock().unwrap().push(event)
        });
        let config = LauncherConfig {
            game_dir: game_dir.to_string_lossy().to_string(),
            java_path: java.to_string_lossy().to_string(),
            hide_while_playing: true,
            ..LauncherConfig::default()
        };
        let config = config.validated_active().unwrap();
        let mut state = test_state(app_data.clone(), config.clone());
        state.events = sink;
        let pending = PendingRoom::default();
        *pending.0.lock().unwrap() = Some("lobby".to_string());
        let identity = GameIdentity {
            username: "OrderTest".to_string(),
            uuid: "00000000-0000-0000-0000-000000000000".to_string(),
            access_token: "test-token".to_string(),
        };

        let append_line = {
            let order_log = order_log.clone();
            move |line: &str| {
                use std::io::Write;
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&order_log)
                    .unwrap()
                    .write_all(format!("{line}\n").as_bytes())
                    .unwrap();
            }
        };
        let window = LaunchWindow {
            hide: Box::new({
                let append_line = append_line.clone();
                move || append_line("hide")
            }),
            restore: Box::new({
                let append_line = append_line.clone();
                move || append_line("restore")
            }),
        };

        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let _operation = state.begin_operation().unwrap();
                let running = state.game.begin().unwrap();
                launch_with_identity(
                    &state,
                    &pending,
                    config,
                    identity,
                    None,
                    "9.9.9-test-launcher",
                    window,
                    running,
                )
                .await
                .unwrap();
            });

        // The child blocks on the release file, so after launch returns the
        // post-spawn steps (clear, hide, status) have all run synchronously.
        assert_eq!(
            std::fs::read_to_string(&order_log)
                .unwrap()
                .lines()
                .filter(|line| *line == "hide")
                .count(),
            1,
            "hide runs exactly once, synchronously after spawn"
        );

        // The child has started: spawn fully happened, and the bridge file
        // predates it (the child itself saw room-request.json).
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            let log = std::fs::read_to_string(&order_log).unwrap_or_default();
            if log.lines().any(|line| line == "spawn") {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "fake java never started"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let lines: Vec<String> = std::fs::read_to_string(&order_log)
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect();
        assert!(lines.contains(&"spawn-room-request-present".to_string()));
        assert!(!lines.contains(&"spawn-room-request-missing".to_string()));

        // Pending cleared after spawn: the launch cleared exactly our value.
        assert_eq!(pending.0.lock().unwrap().as_deref(), None);
        let request = std::fs::read_to_string(app_data.join("room-request.json")).unwrap();
        assert!(request.contains("\"room\":\"lobby\""), "{request}");
        assert!(!app_data.join("game-presence.json").exists());

        // Release the child; the exit thread then runs restore + hook +
        // exited status. The release is created after hide, and restore runs
        // after the child's exit, so hide < restore holds by construction.
        std::fs::write(&release, b"go").unwrap();
        loop {
            let log = std::fs::read_to_string(&order_log).unwrap_or_default();
            let exited = received.lock().unwrap().iter().any(|event| {
                matches!(event, LauncherEvent::Status(payload) if payload.phase == "exited")
            });
            if log.lines().any(|line| line == "restore") && exited {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "exit thread did not finish: order log {log:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        let lines: Vec<String> = std::fs::read_to_string(&order_log)
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect();
        let position = |line: &str| lines.iter().position(|l| l == line).unwrap();
        assert!(position("spawn") < position("restore"), "{lines:?}");
        assert!(position("hide") < position("restore"), "{lines:?}");
        assert_eq!(lines.last().map(String::as_str), Some("restore"));

        // Statuses in launch order, exited last.
        let messages: Vec<(String, String)> = received
            .lock()
            .unwrap()
            .iter()
            .filter_map(|event| match event {
                LauncherEvent::Status(payload) => {
                    Some((payload.phase.to_string(), payload.message.clone()))
                }
                _ => None,
            })
            .collect();
        let phase_position = |phase: &str, message: &str| {
            messages
                .iter()
                .position(|(p, m)| p == phase && m == message)
                .unwrap()
        };
        assert!(
            phase_position("launch", "Проверка компонентов игры")
                < phase_position("launch", "Подготовка запуска")
        );
        assert!(
            phase_position("launch", "Подготовка запуска")
                < phase_position("launch", "Запуск игры")
        );
        assert!(
            phase_position("launch", "Запуск игры") < phase_position("launch", "Игра запущена")
        );
        let exited = messages
            .iter()
            .find(|(phase, _)| phase.as_str() == "exited")
            .unwrap();
        assert!(exited.1.starts_with("Game exited ("), "{exited:?}");
        assert!(game_dir.join("logs").join("launcher-game.log").exists());

        std::env::remove_var("BLOCKFIELD_MINECRAFT_VERSION");
        std::fs::remove_dir_all(root).unwrap();
    }
}
