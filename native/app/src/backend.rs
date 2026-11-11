//! Fixture selection is explicit; a plain launch reads the shared launcher core.
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use launcher_app::{
    LauncherAppState, LauncherEvent,
    ops::{pack_check, settings},
    status_ops,
};
use launcher_net::{
    account, minecraft,
    status::{GameRoom, ServerStatus},
};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use tokio::runtime::Handle;

use crate::{AppWindow, BattlePoint, MainData, Room, bridge, runtime};

/// Starts the never-ending background loops (10s server poll, presence)
/// exactly once per process: every enter_main re-runs only the one-shot
/// checks. Tauri starts both loops once at setup the same way.
fn claim_background_loops() -> bool {
    static STARTED: AtomicBool = AtomicBool::new(false);
    !STARTED.swap(true, Ordering::SeqCst)
}

pub enum Backend {
    Fixtures,
    Core(Handle),
}

pub fn fixtures_requested(args: &[String]) -> bool {
    args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "--state"
                | "--scenario"
                | "--e2e"
                | "--capture-dir"
                | "--e2e-golden"
                | "--e2e-update-goldens"
        )
    }) || std::env::var_os("BLOCKFIELD_CAPTURE_STATE").is_some()
}

impl Backend {
    pub fn start(self, app: &AppWindow) {
        let Self::Core(handle) = self else {
            crate::main_ops::attach_fixtures(app);
            crate::auth::attach_fixtures(app);
            crate::update::attach_fixtures(app);
            crate::stats::wire_fixtures(app);
            crate::settings_fixtures::defaults(app);
            crate::settings_tabs::wire(app, None);
            return;
        };
        app.global::<crate::SettingsForm>().set_loading(true);
        app.set_settings_tab(0);
        // The auth gate owns the window until sign-in: logged-out users must
        // never see the main Shell, even though settings init runs up front.
        app.set_show_main(false);
        app.set_show_login(false);
        app.set_show_firstrun(false);
        app.set_dev_build(launcher_core::dev::is_dev_build());
        apply_dev_status(app);
        app.set_update_label("".into());
        app.set_main_data(MainData {
            checking: true,
            progress: -1,
            phase: "idle".into(),
            players: "—".into(),
            ping: "—".into(),
            installed: "...".into(),
            latest: "...".into(),
            size: "—".into(),
            ..Default::default()
        });
        let events = bridge::sink(app);
        let weak = app.as_weak();
        let route_handle = handle.clone();
        handle.spawn(async move {
            let initialized = tokio::task::spawn_blocking(move || {
                let state = runtime::build_state(events)?;
                // SettingsResponse keeps its fields private; serialize the redacted op response.
                let settings = settings::load_settings(&state).map_err(std::io::Error::other)?;
                let settings = serde_json::to_value(settings)?;
                Ok::<_, std::io::Error>((state, settings))
            })
            .await
            .map_err(std::io::Error::other)
            .and_then(|result| result);
            let (state, settings) = match initialized {
                Ok(initialized) => initialized,
                Err(error) => {
                    let message = format!("Cannot initialize launcher: {error}");
                    eprintln!("{message}");
                    let _ = weak.upgrade_in_event_loop(move |app| {
                        apply_init_error(&app, message);
                    });
                    return;
                }
            };
            let status = account_status(&state).await;
            let stored_username = settings["username"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let _ = weak.upgrade_in_event_loop(move |app| {
                let handle = route_handle.clone();
                crate::auth::attach(&app, state.clone(), handle.clone());
                match status {
                    Ok(status) => crate::auth::enter_startup(&app, state, handle, status),
                    Err(error) => {
                        app.set_login_mode("login".into());
                        app.set_login_set_password(false);
                        app.set_login_username(stored_username.into());
                        app.set_login_error(error.into());
                        app.set_show_main(false);
                        app.set_show_firstrun(false);
                        app.set_show_login(true);
                    }
                }
            });
        });
    }
}

/// Show a fatal init failure on the main screen. Startup hides every
/// screen until auth resolves, so without this the error would sit in
/// empty chrome nobody can see.
pub fn apply_init_error(app: &AppWindow, message: String) {
    // The settings overlay would otherwise hide this error and block input.
    app.global::<crate::SettingsForm>().set_loading(false);
    app.set_show_main(true);
    let mut data = app.get_main_data();
    data.checking = false;
    data.check_error = message.clone().into();
    app.set_main_data(data);
    // TOP has no live data without init: show the failure there too
    // instead of leaving the board on its loading spinner.
    app.set_stats_board_state(1);
    app.set_stats_board_error(message.clone().into());
    app.set_stats_self_state(2);
    app.set_stats_self_error(message.into());
}

async fn account_status(state: &LauncherAppState) -> Result<account::AccountStatus, String> {
    let skins = state
        .pack
        .read()
        .await
        .as_ref()
        .and_then(|pack| pack.info.skins.clone());
    account::account_status(&state.config, &state.app_data_dir, skins).await
}

/// Enter the main Shell: wire MainScreen ops, load settings, then run the
/// pack check, server poll and presence loop (moved here so the auth screens
/// own the window until sign-in completes).
pub fn enter_main(app: &AppWindow, state: Arc<LauncherAppState>, handle: Handle) {
    app.set_show_login(false);
    app.set_show_firstrun(false);
    app.set_show_main(true);
    app.set_dev_build(launcher_core::dev::is_dev_build());
    apply_dev_status(app);
    app.set_update_label("".into());
    app.set_main_data(MainData {
        checking: true,
        progress: -1,
        phase: "idle".into(),
        players: "—".into(),
        ping: "—".into(),
        installed: "...".into(),
        latest: "...".into(),
        size: "—".into(),
        ..Default::default()
    });
    crate::main_ops::attach(app, state.clone(), handle.clone());
    crate::update::attach_core(app, state.clone(), handle.clone());
    crate::stats::wire_live(app, handle.clone(), state.clone());
    let settings_handle = handle.clone();
    let weak = app.as_weak();
    handle.spawn(async move {
        let settings = tokio::task::spawn_blocking({
            let state = state.clone();
            move || -> std::io::Result<serde_json::Value> {
                let settings = settings::load_settings(&state).map_err(std::io::Error::other)?;
                serde_json::to_value(settings).map_err(std::io::Error::other)
            }
        })
        .await
        .map_err(std::io::Error::other)
        .and_then(|result| result);
        let settings = match settings {
            Ok(settings) => settings,
            Err(error) => {
                let message = format!("Cannot load settings: {error}");
                eprintln!("{message}");
                let _ = weak.upgrade_in_event_loop(move |app| {
                    let mut data = app.get_main_data();
                    data.checking = false;
                    data.check_error = message.into();
                    app.set_main_data(data);
                });
                return;
            }
        };
        (state.events)(LauncherEvent::GameStatus(status_ops::game_status(&state)));
        let settings_state = state.clone();
        let _ = weak.upgrade_in_event_loop(move |app| {
            crate::settings_tabs::apply_response(&app, &settings);
            crate::settings_tabs::wire(&app, Some((settings_state, settings_handle)));
            app.set_username(settings["username"].as_str().unwrap_or_default().into());
            app.set_workshop(settings["activeProfile"] == "workshop");
        });
        let pack_state = state.clone();
        let pack_weak = weak.clone();
        tokio::spawn(async move {
            let result =
                pack_check::check_modpack_version(&pack_state, |dir, meta| match &meta.loader {
                    Some(loader) => {
                        let id = minecraft::loader_version_id(&meta.minecraft, loader);
                        minecraft::loader_profile_exists(dir, &id)
                            && minecraft::has_launch_dependencies(dir, &meta.minecraft, Some(&id))
                    }
                    None => minecraft::has_launch_dependencies(dir, &meta.minecraft, None),
                })
                .await;
            if let Err(error) = &result {
                eprintln!("check_modpack_version: {error}");
            }
            let _ = pack_weak.upgrade_in_event_loop(move |app| {
                let mut data = app.get_main_data();
                data.checking = false;
                match result {
                    Ok(version) => {
                        data.checked = true;
                        data.needs_update = version.needs_update;
                        data.needs_setup =
                            version.needs_update || !version.java_ok || !version.loader_ok;
                        data.installed = version.installed_version.into();
                        data.latest = version.remote_version.into();
                        data.size = if version.total_size == 0 {
                            "—".into()
                        } else {
                            format!("{:.1} GB", version.total_size as f64 / 1e9).into()
                        };
                        data.check_error = "".into();
                    }
                    Err(error) => data.check_error = error.into(),
                }
                app.set_main_data(data);
            });
        });
        // Never-ending loops start once per process; re-logins only re-run
        // the one-shot pack check above, never piling up duplicate loops.
        if !claim_background_loops() {
            return;
        }
        let server_state = state.clone();
        tokio::spawn(async move {
            let mut timer = tokio::time::interval(Duration::from_secs(10));
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                timer.tick().await;
                let result = match pack_check::pack_base_url() {
                    Ok(base) => status_ops::server_status(&server_state, &base).await,
                    Err(error) => Err(error),
                };
                if let Err(error) = &result {
                    eprintln!("server_status: {error}");
                }
                if weak
                    .upgrade_in_event_loop(move |app| {
                        apply_server(&app, result);
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        presence(state).await;
    });
}

async fn presence(state: Arc<LauncherAppState>) {
    // Same store the join flow reads: a Discord invite must auto-join through
    // the exact target stored here (Tauri keeps one PendingRoom per app).
    let pending = crate::main_ops::shared_pending();
    loop {
        if state.config.read().await.discord_presence
            && let Err(error) = launcher_app::presence::session(&state, &pending).await
        {
            eprintln!("Discord presence disconnected: {error}");
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Dev panel data (Tauri `dev_status` parity): `None` in production builds,
// so the panel stays invisible exactly like the Tauri DevPanel.
pub fn apply_dev_status(app: &AppWindow) {
    let status = launcher_app::status_ops::dev_status(
        &launcher_app::ops::pack_check::pack_base_url().unwrap_or_default(),
    );
    app.set_dev_data(crate::DevData {
        server: status.as_ref().map_or_else(slint::SharedString::new, |s| {
            if s.server.is_empty() {
                "сервер пака".into()
            } else {
                s.server.as_str().into()
            }
        }),
        freeze: status.as_ref().map_or_else(slint::SharedString::new, |s| {
            if s.freeze_pack {
                "пак заморожен".into()
            } else {
                "выкл".into()
            }
        }),
        jvm_args: status.as_ref().map_or_else(slint::SharedString::new, |s| {
            if s.jvm_args.is_empty() {
                "—".into()
            } else {
                s.jvm_args.as_str().into()
            }
        }),
        pack_url: status
            .as_ref()
            .map_or_else(slint::SharedString::new, |s| s.pack_url.as_str().into()),
    });
}

/// Room subtitle parity with Tauri `RoomList.tsx`: optional format label,
/// mode, map and RU phase name, skipping whatever is absent.
fn room_description(room: &GameRoom) -> String {
    let format = match room.format.as_deref() {
        Some("casual") => Some("Casual"),
        Some("ranked") => Some("Ranked"),
        _ => None,
    };
    let phase = match room.phase.as_str() {
        "IDLE" => "Ожидание игроков",
        "VOTING" => "Голосование",
        "PREPARING" => "Подготовка",
        "GAME" => "Бой",
        "MATCH_END" => "Матч завершён",
        other => other,
    };
    let mut parts = Vec::with_capacity(4);
    if let Some(format) = format {
        parts.push(format);
    }
    if !room.mode.is_empty() {
        parts.push(room.mode.as_str());
    }
    if !room.map.is_empty() {
        parts.push(room.map.as_str());
    }
    if !phase.is_empty() {
        parts.push(phase);
    }
    parts.join(" · ")
}

fn apply_server(app: &AppWindow, status: Result<ServerStatus, String>) {
    let selected = app
        .get_rooms()
        .row_data(app.get_room_index() as usize)
        .map(|room| room.id);
    let mut data = app.get_main_data();
    let Ok(status) = status else {
        data.online = false;
        data.rooms_available = false;
        data.updating = false;
        data.players = "—".into();
        data.ping = "—".into();
        data.region = "—".into();
        data.location = "".into();
        data.maintenance = "".into();
        app.set_room_index(0);
        app.set_rooms(ModelRc::default());
        app.set_room_names(ModelRc::default());
        app.set_main_data(data);
        return;
    };
    data.online = status.online;
    data.rooms_available = status.rooms.is_some();
    data.updating = status.online && status.game_available == Some(false);
    data.players = status
        .players_online
        .map_or("—".into(), |v| v.to_string())
        .into();
    data.ping = status
        .server_latency_ms
        .map_or("—".into(), |v| v.to_string())
        .into();
    data.region = status.region_code.into();
    data.location = status.location_name.into();
    // status host/port resolve from the launcher config target (workshop
    // override or pack server), so the panel shows the real address.
    data.server_address = format!("{}:{}", status.host, status.port).into();
    data.maintenance = if status.online {
        status.maintenance.map_or(String::new(), |m| {
            if m.message.is_empty() {
                "Идут технические работы".into()
            } else {
                m.message
            }
        })
    } else {
        String::new()
    }
    .into();
    let rooms: Vec<_> = status
        .rooms
        .unwrap_or_default()
        .into_iter()
        .map(|room| {
            let battle = room.battle.as_ref();
            let description = room_description(&room);
            Room {
                id: room.id.into(),
                name: room.name.into(),
                description: description.into(),
                players: room.players.min(i32::MAX as u32) as i32,
                ready: room
                    .ready
                    .map_or(String::new(), |r| {
                        format!("Готовы: {}/{}", r.ready, r.required)
                    })
                    .into(),
                join: match room.admission.as_deref() {
                    Some("waiting") => "Встать в очередь",
                    Some("closed") => "Мест нет",
                    _ => "Войти",
                }
                .into(),
                joinable: room.joinable && room.admission.as_deref() != Some("closed"),
                has_battle: battle.is_some(),
                battle_red: battle.map_or(0, |b| b.red.min(i32::MAX as u32) as i32),
                battle_blue: battle.map_or(0, |b| b.blue.min(i32::MAX as u32) as i32),
                points: slint::ModelRc::new(slint::VecModel::from(battle.map_or(vec![], |b| {
                    b.points
                        .iter()
                        .map(|point| {
                            let team = if point.owner == "NEUTRAL" {
                                point.claiming.clone()
                            } else {
                                point.owner.clone()
                            };
                            let owner = match team.as_str() {
                                "RED" => "Красные",
                                "BLUE" => "Синие",
                                _ => "Нейтральная",
                            };
                            let action = if point.capturing != "NONE" && point.capturing != team {
                                match point.capturing.as_str() {
                                    "RED" => " · Красные нейтрализуют",
                                    "BLUE" => " · Синие нейтрализуют",
                                    _ => "",
                                }
                            } else {
                                ""
                            };
                            BattlePoint {
                                label: format!("{} · {}", point.name, owner).into(),
                                sub: format!("{}%{}", point.progress, action).into(),
                                progress: i32::from(point.progress.min(100)),
                                tone: match team.as_str() {
                                    "RED" => 1,
                                    "BLUE" => 2,
                                    _ => 0,
                                },
                            }
                        })
                        .collect::<Vec<_>>()
                }))),
            }
        })
        .collect();
    app.set_room_index(
        rooms
            .iter()
            .position(|room| Some(&room.id) == selected.as_ref())
            .unwrap_or(0) as i32,
    );
    app.set_room_names(ModelRc::new(VecModel::from(
        rooms.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
    )));
    app.set_rooms(ModelRc::new(VecModel::from(rooms)));
    app.set_server_label(format!("IP · {}:{}", status.host, status.port).into());
    app.set_main_data(data);
}

#[cfg(test)]
mod tests {
    use super::room_description;
    use launcher_net::status::GameRoom;

    fn subtitle_room(format: Option<&str>, mode: &str, map: &str, phase: &str) -> GameRoom {
        GameRoom {
            id: "room".into(),
            name: "Room".into(),
            mode: mode.into(),
            map: map.into(),
            phase: phase.into(),
            players: 3,
            joinable: true,
            format: format.map(str::to_owned),
            admission: None,
            ready: None,
            battle: None,
        }
    }

    #[test]
    fn room_subtitle_matches_tauri_format_and_ru_phase() {
        // Tauri RoomList.tsx: [format, mode, map, phase].filter(Boolean).join(' · ').
        assert_eq!(
            room_description(&subtitle_room(None, "room", "Map", "IDLE")),
            "room · Map · Ожидание игроков"
        );
        assert_eq!(
            room_description(&subtitle_room(Some("casual"), "room", "Map", "GAME")),
            "Casual · room · Map · Бой"
        );
        assert_eq!(
            room_description(&subtitle_room(
                Some("ranked"),
                "campaign",
                "Crossroads",
                "MATCH_END"
            )),
            "Ranked · campaign · Crossroads · Матч завершён"
        );
        // Unknown format is dropped like Tauri's undefined lookup; unknown
        // phase and empty mode/map fall back to the raw values.
        assert_eq!(
            room_description(&subtitle_room(Some("custom"), "room", "Map", "VOTING")),
            "room · Map · Голосование"
        );
        assert_eq!(
            room_description(&subtitle_room(Some("casual"), "room", "Map", "CUSTOM")),
            "Casual · room · Map · CUSTOM"
        );
        assert_eq!(
            room_description(&subtitle_room(Some("ranked"), "", "", "PREPARING")),
            "Ranked · Подготовка"
        );
    }

    #[test]
    fn background_loops_start_exactly_once() {
        assert!(super::claim_background_loops());
        assert!(!super::claim_background_loops());
    }

    #[test]
    fn server_refresh_preserves_game_state_and_clears_stale_rooms() {
        use launcher_net::status::ServerStatus;
        use slint::Model;
        i_slint_backend_testing::init_no_event_loop();
        let app = crate::AppWindow::new().unwrap();
        app.set_main_data(crate::MainData {
            phase: "running".into(),
            ..Default::default()
        });
        let mut status = ServerStatus {
            online: true,
            rooms: Some(vec![GameRoom {
                id: "room".into(),
                name: "Room".into(),
                mode: "room".into(),
                map: "Map".into(),
                phase: "IDLE".into(),
                players: 3,
                joinable: true,
                format: None,
                admission: Some("closed".into()),
                ready: None,
                battle: None,
            }]),
            game_available: Some(false),
            maintenance: None,
            players_online: Some(3),
            players_max: Some(100),
            minecraft_version: None,
            motd: None,
            host: "localhost".into(),
            port: 25565,
            region_code: "EU".into(),
            location_name: "Test".into(),
            server_latency_ms: Some(7),
            checked_at: 0,
        };
        super::apply_server(&app, Ok(status.clone()));
        assert_eq!(app.get_main_data().phase, "running");
        assert!(app.get_main_data().updating);
        assert_eq!(app.get_main_data().players, "3");
        assert_eq!(app.get_main_data().ping, "7");
        assert_eq!(app.get_main_data().server_address, "localhost:25565");
        assert!(!app.get_rooms().row_data(0).unwrap().joinable);
        assert_eq!(
            app.get_rooms().row_data(0).unwrap().description,
            "room · Map · Ожидание игроков"
        );
        let mut second = status.rooms.as_ref().unwrap()[0].clone();
        second.id = "second".into();
        status.rooms.as_mut().unwrap().push(second);
        super::apply_server(&app, Ok(status.clone()));
        app.set_room_index(1);
        status.rooms.as_mut().unwrap().reverse();
        super::apply_server(&app, Ok(status.clone()));
        assert_eq!(
            app.get_room_index(),
            0,
            "refresh preserves the selected room ID"
        );
        status.rooms.as_mut().unwrap().remove(0);
        super::apply_server(&app, Ok(status.clone()));
        assert_eq!(
            app.get_room_index(),
            0,
            "removed selection falls back to the first room"
        );

        status.online = false;
        status.rooms = None;
        status.players_online = None;
        status.server_latency_ms = None;
        super::apply_server(&app, Ok(status));
        assert!(!app.get_main_data().online);
        assert!(!app.get_main_data().rooms_available);
        assert!(!app.get_main_data().updating);
        assert_eq!(app.get_rooms().row_count(), 0);
        assert_eq!(app.get_main_data().phase, "running");
        super::apply_server(&app, Err("offline".into()));
        assert_eq!(app.get_main_data().players, "—");
        assert_eq!(app.get_main_data().ping, "—");
        assert!(!app.get_main_data().online);
        // The panel keeps the last known address while unreachable.
        assert_eq!(app.get_main_data().server_address, "localhost:25565");
    }

    #[test]
    fn explicit_capture_and_test_flags_use_fixtures() {
        for flag in [
            "--state",
            "--scenario",
            "--capture-dir",
            "--e2e",
            "--e2e-golden",
            "--e2e-update-goldens",
        ] {
            assert!(super::fixtures_requested(&[flag.into(), "value".into()]));
        }
        if std::env::var_os("BLOCKFIELD_CAPTURE_STATE").is_none() {
            assert!(!super::fixtures_requested(&[]));
            assert!(!super::fixtures_requested(&[
                "--idle-seconds".into(),
                "1".into()
            ]));
        }
    }
}
