//! Fixture selection is explicit; a plain launch reads the shared launcher core.
use std::{sync::Arc, time::Duration};

use launcher_app::{
    LauncherAppState, LauncherEvent,
    ops::{pack_check, settings},
    status_ops,
};
use launcher_net::{minecraft, status::ServerStatus};
use slint::{ComponentHandle, ModelRc, VecModel};
use tokio::runtime::Handle;

use crate::{AppWindow, MainData, Room, bridge, runtime};

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
            crate::settings_fixtures::defaults(app);
            crate::settings_tabs::wire(app, None);
            return;
        };
        app.global::<crate::SettingsForm>().set_loading(true);
        app.set_settings_tab(0);
        app.set_show_main(true);
        app.set_dev_build(launcher_core::dev::is_dev_build());
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
            let settings_handle = Handle::current();
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
                    pack_check::check_modpack_version(&pack_state, |dir, meta| {
                        match &meta.loader {
                            Some(loader) => {
                                let id = minecraft::loader_version_id(&meta.minecraft, loader);
                                minecraft::loader_profile_exists(dir, &id)
                                    && minecraft::has_launch_dependencies(
                                        dir,
                                        &meta.minecraft,
                                        Some(&id),
                                    )
                            }
                            None => minecraft::has_launch_dependencies(dir, &meta.minecraft, None),
                        }
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
}

async fn presence(state: Arc<LauncherAppState>) {
    let pending = launcher_app::rooms::PendingRoom::default();
    loop {
        if state.config.read().await.discord_presence
            && let Err(error) = launcher_app::presence::session(&state, &pending).await
        {
            eprintln!("Discord presence disconnected: {error}");
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

fn apply_server(app: &AppWindow, status: Result<ServerStatus, String>) {
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
        .map(|room| Room {
            id: room.id.into(),
            name: room.name.into(),
            description: format!("{} · {} · {}", room.mode, room.map, room.phase).into(),
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
        })
        .collect();
    app.set_room_names(ModelRc::new(VecModel::from(
        rooms.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
    )));
    app.set_rooms(ModelRc::new(VecModel::from(rooms)));
    app.set_server_label(format!("IP · {}:{}", status.host, status.port).into());
    app.set_main_data(data);
}

#[cfg(test)]
mod tests {
    #[test]
    fn server_refresh_preserves_game_state_and_clears_stale_rooms() {
        use launcher_net::status::{GameRoom, ServerStatus};
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
        assert!(!app.get_rooms().row_data(0).unwrap().joinable);
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
