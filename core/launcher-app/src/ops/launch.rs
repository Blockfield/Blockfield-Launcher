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
//! checks -> dependency ensure (incl. self-healing pack repair refusal and
//! loader-profile refresh) -> seed options -> args -> pre-launch hook
//! (warn-only) -> log files -> pending room read+filter -> bridge dir ->
//! room-request write/remove -> game-presence remove -> spawn with env ->
//! running.started -> pending clear-if-unchanged -> hide -> status -> exit
//! monitor thread (finishing, restore, post-exit hook, statuses).

use super::install::pack_phase_complete;
use super::java::{resolved_java_path, system_java_available};
use super::pack_check::{installed_pack, pack_base_url_for};
use crate::rooms::PendingRoom;
use crate::{EventSink, GameIdentity, LauncherAppState, LauncherEvent, PackState};
use launcher_core::config::{ClientProfile, LauncherConfig};
use launcher_core::game::GameSession;
use launcher_net::pack::PackMeta;
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

/// Launch-time staleness verdict: Some PACK_STALE string when the installed pack
/// is positively older than the published manifest. Any client change (a new
/// mod JAR carrying a protocol bump included) changes the pack index hash,
/// so the hash comparison covers protocol updates without parsing them; the
/// server gate stays the protocol authority and only kicks as a last resort.
/// The UI routes this to the update screen, which installs the pack itself
/// before joining. Ports stale_pack_message from commands.rs (PR 73).
pub fn stale_pack_message(installed_hash: &str, meta: &PackMeta) -> Option<String> {
    (installed_hash != meta.index_hash).then(|| {
        format!(
            "PACK_STALE: installed pack is outdated (remote {}). Open Updates to install it, then launch again.",
            meta.version
        )
    })
}

async fn fresh_pack_state(profile: ClientProfile) -> Result<PackState, String> {
    let base = pack_base_url_for(profile)?;
    let info = launcher_net::pack::fetch_launcher_info(&base).await?;
    let meta = launcher_net::pack::fetch_pack_meta(&info.pack).await?;
    Ok(PackState {
        profile,
        info,
        meta,
    })
}

/// Offline fallback after a failed manifest fetch: a cached pack checked for
/// the other profile must not verify the current game dir (a valid offline
/// install would be refused with PACK_REPAIR); use the uncached path instead.
fn offline_pack_for_launch(cached: Option<PackState>, active: ClientProfile) -> Option<PackState> {
    cached.filter(|pack| pack.profile == active)
}

/// Blocking pack hash check, designed for `spawn_blocking`: a corrupt pack
/// must not survive a launch, so the check runs off the async worker.
async fn pack_phase_done(app_data: PathBuf, game_dir: PathBuf, index_hash: String) -> bool {
    tokio::task::spawn_blocking(move || pack_phase_complete(&app_data, &game_dir, &index_hash))
        .await
        .unwrap_or(false)
}

async fn pack_files_valid_blocking(app_data: PathBuf, game_dir: PathBuf) -> bool {
    tokio::task::spawn_blocking(move || {
        launcher_net::installed_files::pack_files_valid(&game_dir, &app_data)
    })
    .await
    .unwrap_or(false)
}

/// A same-size-corrupted pack must not survive a launch: refuse with a stable
/// prefix so the UI routes into the sync (which heals) instead of showing a
/// dead-end error.
async fn refuse_unverifiable_pack(
    app_data: &Path,
    game_dir: &Path,
    pack: &Option<PackState>,
) -> Result<(), String> {
    let verifiable = match pack {
        Some(pack) => {
            pack_phase_done(
                app_data.to_path_buf(),
                game_dir.to_path_buf(),
                pack.meta.index_hash.clone(),
            )
            .await
        }
        None => pack_files_valid_blocking(app_data.to_path_buf(), game_dir.to_path_buf()).await,
    };
    verifiable.then_some(()).ok_or_else(|| {
        "PACK_REPAIR: pack files changed or are missing. Open Updates to repair them, then launch again."
            .to_string()
    })
}

/// Play pressed while the pack was stale routes to Updates; the game must start
/// on its own once the sync finishes. A manual visit to Updates never arms the
/// intent, so plain updates stay on the done screen.
/// Ports shouldAutoLaunchAfterSync from auto-launch.ts (PR 73).
pub fn should_auto_launch_after_sync(
    phase: &str,
    play_triggered: bool,
    launching: bool,
    game_busy: bool,
) -> bool {
    if !play_triggered || launching || game_busy {
        return false;
    }
    phase == "complete" || phase == "uptodate"
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
    // A cached (or missing) version check may predate a release: launching the
    // stale pack would only die at the server gate with a manual close-the-game
    // and-update step. Re-check the published manifest here so the UI routes to
    // the update screen instead. An unreachable manifest is fail-open (offline
    // launch keeps working); a positive mismatch fails closed.
    let pack = if !launcher_core::dev::freeze_pack() {
        match fresh_pack_state(config.active_profile).await {
            Ok(fresh) => {
                let installed_hash = installed_pack(&PathBuf::from(&config.game_dir)).0;
                if let Some(stale) = stale_pack_message(&installed_hash, &fresh.meta) {
                    return Err(stale);
                }
                *state.pack.write().await = Some(fresh.clone());
                Some(fresh)
            }
            Err(error) => {
                log::warn!("launch: pack manifest unreachable, using cached state: {error}");
                offline_pack_for_launch(pack, config.active_profile)
            }
        }
    } else {
        pack
    };
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
    // A same-size-corrupted pack must not survive a launch: refuse with a
    // stable prefix so the UI routes into the sync (which heals) instead of
    // showing a dead-end error. Skipped for frozen dev packs.
    if !launcher_core::dev::freeze_pack() {
        refuse_unverifiable_pack(&state.app_data_dir, &game_dir_path, &pack).await?;
    }

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
            // Offline after a profile switch: the manifest is unreachable and
            // the cached pack (if any) was checked for the other profile, so
            // it cannot verify the install — but its launcher info still
            // carries the Game host needed to validate the configured
            // Workshop address. Reuse it with no network request (state.pack
            // still holds the pre-drop cache; launch_game only overwrites it
            // on a fresh fetch). Empty when never checked: target() then uses
            // the configured Workshop address without the host check.
            state
                .pack
                .read()
                .await
                .as_ref()
                .map(|cached| cached.info.server.clone())
                .unwrap_or_default()
        }
        None => String::new(),
    };
    let quick_play = config.target(&game_server)?;
    // Dev build: another server, or none at all so the game opens on the title screen.
    let quick_play = launcher_core::dev::server_override().unwrap_or(quick_play);
    // TCP fallback (proxy#74 U3): the proxy routes TCP 25565 logins to the Game.
    // A remembered RakNet failure (<24 h) skips the probe; otherwise one
    // unconnected ping decides whether the RakNet path is usable from here.
    let mut quick_play = quick_play;
    let mut via_tcp_fallback = false;
    if let Some(target) = quick_play.clone() {
        // TCP fallback is the proxy's own route only (the pack's game host):
        // a custom/dev RakNet target must never remap, its TCP port may serve
        // another server.
        if launcher_net::tcp_fallback::is_proxy_target(&target, &game_server) {
            let fallback_state =
                launcher_net::tcp_fallback::FallbackState::load(&state.app_data_dir);
            let now = launcher_net::tcp_fallback::now_secs();
            let resolved =
                launcher_net::tcp_fallback::resolve_target(&target, &fallback_state, now);
            if resolved != target {
                quick_play = Some(resolved);
                via_tcp_fallback = true;
            } else if let Some((host, port)) = launcher_net::tcp_fallback::raknet_endpoint(&target)
            {
                if !launcher_net::tcp_fallback::raknet_reachable(&host, port) {
                    log::warn!("RakNet path to {host}:{port} unreachable, using TCP fallback");
                    let mut updated = fallback_state;
                    updated.record(&host, now);
                    if let Err(error) = updated.save(&state.app_data_dir, now) {
                        log::warn!("Cannot persist TCP fallback state: {error}");
                    }
                    quick_play = launcher_net::tcp_fallback::tcp_target_for(&target).or(quick_play);
                    via_tcp_fallback = true;
                }
            }
        }
    }

    log::info!(
        "Launching game: java={java}, ram={ram_mb}MB, dir={game_dir}, user={}",
        identity.username
    );

    {
        state.downloader.reset_cancel();
        // Floor so the frontend always has a non-zero denominator for progress.
        state.downloader.set_grand_total(16 * 1024 * 1024);
        emit_status(&state.events, "launch", "Проверка компонентов игры", false);
        // A profile truncated by a killed update must heal here, not die in
        // build_launch_args with a parse error (#68).
        if let Some((minecraft, loader)) = pack.as_ref().and_then(|pack| {
            pack.meta
                .loader
                .as_deref()
                .map(|loader| (pack.meta.minecraft.as_str(), loader))
        }) {
            launcher_net::minecraft::ensure_loader_profile(&game_dir_path, minecraft, loader)
                .await?;
        }
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
        let result = tokio::task::spawn_blocking(move || {
            launcher_net::launch_hooks::run_hook(
                &hook_config.pre_launch_command,
                &hook_config,
                &hook_java,
                None,
            )
        })
        .await
        .map_err(|e| format!("Pre-launch task failed: {e}"))
        .and_then(|result| result);
        // A user hook (e.g. display switching) must not block the game itself.
        if let Err(error) = result {
            log::warn!("Pre-launch command failed: {error}");
            emit_status(
                &state.events,
                "hook-error",
                format!("Команда перед запуском: {error}"),
                false,
            );
        }
    }
    emit_status(
        &state.events,
        "launch",
        if via_tcp_fallback {
            "Запуск игры: вход по TCP"
        } else {
            "Запуск игры"
        },
        false,
    );
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
                        emit_status(
                            &events,
                            "hook-error",
                            format!("Команда после выхода: {error}"),
                            false,
                        );
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

    /// The single-installation tracking guard is process-wide: tests driving
    /// launch_with_identity serialize on it so parallel runs cannot fail with
    /// "Another installation is being tracked".
    static TRACKING_GUARD: StdMutex<()> = StdMutex::new(());

    fn pack_for(profile: ClientProfile) -> PackState {
        PackState {
            profile,
            info: launcher_net::pack::LauncherInfo {
                pack: String::new(),
                server: String::new(),
                installer: launcher_net::pack::Artifact {
                    url: String::new(),
                    sha256: String::new(),
                },
                bootstrap: launcher_net::pack::Artifact {
                    url: String::new(),
                    sha256: String::new(),
                },
                java: Default::default(),
                skins: None,
            },
            meta: PackMeta {
                version: "1.0".into(),
                index_hash: "hash".into(),
                minecraft: "1.21.1".into(),
                loader: None,
            },
        }
    }

    #[test]
    fn offline_launch_drops_a_cached_pack_of_the_other_profile() {
        assert!(
            offline_pack_for_launch(Some(pack_for(ClientProfile::Game)), ClientProfile::Game)
                .is_some()
        );
        assert!(offline_pack_for_launch(
            Some(pack_for(ClientProfile::Workshop)),
            ClientProfile::Workshop
        )
        .is_some());
        // Switched profile after the check + lost manifest: the Game pack's
        // index hash must not verify the Workshop dir (PACK_REPAIR). Uncached.
        assert!(offline_pack_for_launch(
            Some(pack_for(ClientProfile::Game)),
            ClientProfile::Workshop
        )
        .is_none());
        assert!(offline_pack_for_launch(
            Some(pack_for(ClientProfile::Workshop)),
            ClientProfile::Game
        )
        .is_none());
        assert!(offline_pack_for_launch(None, ClientProfile::Game).is_none());
    }

    /// Offline Workshop launch after a profile switch: the manifest is
    /// unreachable and the only cached pack was checked for Game, so the
    /// launch drops it (pack=None). The Workshop target must still resolve
    /// locally with no network request: the cached Game launcher info
    /// supplies the Game host for validation, and the game spawns with
    /// BLOCKFIELD_SERVER set to the configured Workshop address.
    #[test]
    fn offline_workshop_launch_uses_cached_game_server_without_network() {
        if cfg!(windows) {
            eprintln!("skipped: the fake java is a unix shell script");
            return;
        }
        let _tracking_guard = TRACKING_GUARD.lock().unwrap();
        let unique = format!(
            "blockfield-workshop-offline-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let app_data = root.join("app-data");
        let game_dir = root.join("game");
        let workshop_dir = root.join("workshop-game");
        std::fs::create_dir_all(&workshop_dir).unwrap();
        std::fs::create_dir_all(&game_dir).unwrap();
        let server_file = root.join("blockfield-server.txt");
        let release = root.join("release");
        let order_log = root.join("order.log");

        // Same offline fixtures as the launch-order test, in the Workshop dir:
        // every runtime file resolve hits local disk, so the dependency pass
        // runs with no network.
        std::env::set_var("BLOCKFIELD_MINECRAFT_VERSION", "9.9.9-test");
        let version_json = serde_json::json!({
            "mainClass": "net.minecraft.client.main.Main",
            "assetIndex": {"id": "test-index", "url": "https://example.com/index.json"},
            "assets": "test-index",
            "downloads": {"client": {"url": "https://example.com/client.jar", "size": 3}},
            "libraries": [],
            "arguments": {"game": ["--username", "${auth_player_name}"]},
        });
        let version_dir = workshop_dir.join("versions").join("9.9.9-test");
        std::fs::create_dir_all(&version_dir).unwrap();
        std::fs::write(
            version_dir.join("9.9.9-test.json"),
            serde_json::to_string(&version_json).unwrap(),
        )
        .unwrap();
        std::fs::write(version_dir.join("9.9.9-test.jar"), b"mc!").unwrap();
        let index_dir = workshop_dir.join("assets").join("indexes");
        std::fs::create_dir_all(&index_dir).unwrap();
        std::fs::write(index_dir.join("test-index.json"), r#"{"objects": {}}"#).unwrap();

        // Uncached repair gate needs a verifiable install: one tracked mod
        // with a correct hash.
        let mod_jar = workshop_dir.join("mods").join("offline-mod.jar");
        std::fs::create_dir_all(mod_jar.parent().unwrap()).unwrap();
        std::fs::write(&mod_jar, b"offline-bytes").unwrap();
        std::fs::write(
            workshop_dir.join("packwiz.json"),
            serde_json::to_string(&serde_json::json!({ "cachedFiles": {
                "offline-mod": {
                    "cachedLocation": "mods/offline-mod.jar",
                    "hash": {
                        "type": "sha256",
                        "value": launcher_net::download::Downloader::sha256_file(&mod_jar)
                            .unwrap()
                    }
                }
            }}))
            .unwrap(),
        )
        .unwrap();

        // Fake java: answers -version (validated_active runs it), otherwise
        // records the Workshop target it was given, then blocks until the
        // test releases it so exit/restore ordering stays deterministic.
        let java = root.join("fake-java.sh");
        std::fs::write(
            &java,
            format!(
                "#!/bin/sh\n\
                 if [ \"$1\" = \"-version\" ]; then\n\
                   echo 'openjdk version \"21.0.5\"' >&2\n\
                   exit 0\n\
                 fi\n\
                 echo \"$BLOCKFIELD_SERVER\" > '{}'\n\
                 echo spawn >> '{}'\n\
                 i=0\n\
                 while [ ! -f '{}' ] && [ $i -lt 600 ]; do\n\
                   sleep 0.05\n\
                   i=$((i+1))\n\
                 done\n\
                 exit 0\n",
                server_file.display(),
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
        // The built-in Workshop host: the only address trusted without a
        // known Game host (target() refuses a custom address offline).
        let workshop_server = "raknet;workshop.blockfield.pro:25567";
        let config = LauncherConfig {
            game_dir: game_dir.to_string_lossy().to_string(),
            workshop_game_dir: workshop_dir.to_string_lossy().to_string(),
            workshop_java_path: java.to_string_lossy().to_string(),
            workshop_server: workshop_server.to_string(),
            active_profile: ClientProfile::Workshop,
            ..LauncherConfig::default()
        };
        let config = config.validated_active().unwrap();
        let mut state = test_state(app_data.clone(), config.clone());
        state.events = sink;
        // The pre-switch check cached the Game pack; launch_game drops it for
        // verification (pack=None) but state still holds its launcher info.
        *state.pack.blocking_write() = Some(PackState {
            profile: ClientProfile::Game,
            info: launcher_net::pack::LauncherInfo {
                pack: "https://example.com/pack.toml".to_string(),
                server: "raknet;game.example:25566".to_string(),
                installer: launcher_net::pack::Artifact {
                    url: String::new(),
                    sha256: String::new(),
                },
                bootstrap: launcher_net::pack::Artifact {
                    url: String::new(),
                    sha256: String::new(),
                },
                java: Default::default(),
                skins: None,
            },
            meta: PackMeta {
                version: "1.0".into(),
                index_hash: "hash".into(),
                minecraft: "1.21.1".into(),
                loader: None,
            },
        });
        let pending = PendingRoom::default();
        let identity = GameIdentity {
            username: "WorkshopOfflineTest".to_string(),
            uuid: "00000000-0000-0000-0000-000000000000".to_string(),
            access_token: "test-token".to_string(),
        };
        let window = LaunchWindow {
            hide: Box::new(|| {}),
            restore: Box::new({
                let order_log = order_log.clone();
                move || {
                    use std::io::Write;
                    std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&order_log)
                        .unwrap()
                        .write_all(b"restore\n")
                        .unwrap();
                }
            }),
        };

        // pack=None is what launch_game passes after offline_pack_for_launch
        // drops the other profile's cached pack. Before the fix this fetches
        // the Workshop launcher.json over the network and fails offline;
        // after it the cached Game host validates the Workshop target locally.
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

        // The child saw the configured Workshop address, with no fetch: the
        // pack URL is unset here, so any launcher.json request would have
        // failed before spawning.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            if let Ok(target) = std::fs::read_to_string(&server_file) {
                assert_eq!(target.trim(), workshop_server);
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "fake java never started"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

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
                "exit thread did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        std::env::remove_var("BLOCKFIELD_MINECRAFT_VERSION");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Offline Workshop launch with a custom address and no cached Game host
    /// (never checked online): the distinct-host check cannot run, so the
    /// launch refuses instead of risking the Game server. Only the built-in
    /// Workshop host is trusted without a known Game host.
    #[test]
    fn offline_workshop_launch_with_custom_address_and_no_cached_host_refuses() {
        if cfg!(windows) {
            eprintln!("skipped: the fake java is a unix shell script");
            return;
        }
        let _tracking_guard = TRACKING_GUARD.lock().unwrap();
        let unique = format!(
            "blockfield-workshop-refuse-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let app_data = root.join("app-data");
        let workshop_dir = root.join("workshop-game");
        std::fs::create_dir_all(&workshop_dir).unwrap();

        // Verifiable install so the repair gate (which runs before target
        // resolution) lets the launch reach the server-target check.
        let mod_jar = workshop_dir.join("mods").join("offline-mod.jar");
        std::fs::create_dir_all(mod_jar.parent().unwrap()).unwrap();
        std::fs::write(&mod_jar, b"offline-bytes").unwrap();
        std::fs::write(
            workshop_dir.join("packwiz.json"),
            serde_json::to_string(&serde_json::json!({ "cachedFiles": {
                "offline-mod": {
                    "cachedLocation": "mods/offline-mod.jar",
                    "hash": {
                        "type": "sha256",
                        "value": launcher_net::download::Downloader::sha256_file(&mod_jar)
                            .unwrap()
                    }
                }
            }}))
            .unwrap(),
        )
        .unwrap();

        // Any existing executable: resolved_java_path only checks existence
        // here (no -version probe outside validated_active), and the launch
        // must refuse before spawning it.
        let java = std::env::current_exe().unwrap();
        let config = LauncherConfig {
            game_dir: workshop_dir.to_string_lossy().to_string(),
            workshop_game_dir: workshop_dir.to_string_lossy().to_string(),
            java_path: java.to_string_lossy().to_string(),
            workshop_server: "raknet;play.example:25567".to_string(),
            active_profile: ClientProfile::Workshop,
            ..LauncherConfig::default()
        };
        let state = test_state(app_data, config.clone());
        let pending = PendingRoom::default();
        let identity = GameIdentity {
            username: "WorkshopRefuseTest".to_string(),
            uuid: "00000000-0000-0000-0000-000000000000".to_string(),
            access_token: "test-token".to_string(),
        };
        let window = LaunchWindow {
            hide: Box::new(|| {}),
            restore: Box::new(|| {}),
        };

        // pack=None and no cached pack: game_server is empty (never checked),
        // so a custom Workshop address must refuse without any network.
        let error = tokio::runtime::Builder::new_current_thread()
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
            });
        let error = error.unwrap_err();
        assert!(
            error.contains("Нет связи"),
            "custom offline address must refuse, got: {error}"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_pack_message_fires_only_on_index_mismatch() {
        let meta = PackMeta {
            version: "2.80.0".into(),
            index_hash: "new-hash".into(),
            minecraft: "1.21.1".into(),
            loader: None,
        };
        assert!(stale_pack_message("new-hash", &meta).is_none());
        let stale = stale_pack_message("old-hash", &meta).expect("stale pack must refuse launch");
        assert!(stale.starts_with("PACK_STALE"), "{stale}");
        assert!(stale.contains("2.80.0"), "{stale}");
    }

    #[test]
    fn auto_launch_continues_only_play_triggered_syncs() {
        assert!(should_auto_launch_after_sync(
            "complete", true, false, false
        ));
        assert!(should_auto_launch_after_sync(
            "uptodate", true, false, false
        ));
        assert!(!should_auto_launch_after_sync(
            "complete", false, false, false
        ));
        for phase in ["checking", "downloading", "verifying", "launching", "error"] {
            assert!(
                !should_auto_launch_after_sync(phase, true, false, false),
                "{phase}"
            );
        }
        assert!(!should_auto_launch_after_sync(
            "complete", true, true, false
        ));
        assert!(!should_auto_launch_after_sync(
            "complete", true, false, true
        ));
    }

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

    /// The repair gate fires before any spawn: an empty game dir (nothing
    /// verifiable) refuses with PACK_REPAIR so the UI routes into the sync.
    /// Drives the gate helper directly: no operation lock, no tracking, so it
    /// cannot race the other launch tests on the single-installation guard.
    #[test]
    fn launch_refuses_unverifiable_pack_with_repair_routing() {
        let root = std::env::temp_dir().join(format!(
            "blockfield-launch-repair-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let app_data = root.join("app-data");
        let game_dir = root.join("game");
        std::fs::create_dir_all(&game_dir).unwrap();

        let error = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(refuse_unverifiable_pack(&app_data, &game_dir, &None));
        assert_eq!(
            error.unwrap_err(),
            "PACK_REPAIR: pack files changed or are missing. Open Updates to repair them, then launch again."
        );
        std::fs::remove_dir_all(root).unwrap();
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
        let _tracking_guard = TRACKING_GUARD.lock().unwrap();
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

        // The launch-time repair gate needs a verifiable pack: one tracked
        // mod with a correct hash (same shape as install.rs's pack_phase test).
        let mod_jar = game_dir.join("mods").join("order-mod.jar");
        std::fs::create_dir_all(mod_jar.parent().unwrap()).unwrap();
        std::fs::write(&mod_jar, b"order-bytes").unwrap();
        std::fs::write(
            game_dir.join("packwiz.json"),
            serde_json::to_string(&serde_json::json!({ "cachedFiles": {
                "order-mod": {
                    "cachedLocation": "mods/order-mod.jar",
                    "hash": {
                        "type": "sha256",
                        "value": launcher_net::download::Downloader::sha256_file(&mod_jar)
                            .unwrap()
                    }
                }
            }}))
            .unwrap(),
        )
        .unwrap();

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
