//! Phase 2 close-out: goldens for every command return type and every event
//! payload of the Tauri frontend/backend contract. Tauri sends the `Ok` value
//! directly, so each golden is the JSON of the value. Goldens compare as
//! parsed JSON (`serde_json::Value`): key order is not contracted (the
//! frontend uses `JSON.parse`), but field names, casing, null/absent shapes
//! and enum spellings must match — a rename on either side fails here before
//! it can break the frontend.

use launcher_app::{LauncherEvent, LauncherStatusPayload, VersionCheckResult};
use launcher_core::config::LauncherConfig;
use serde::Serialize;
use std::path::{Path, PathBuf};

fn goldens_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("goldens")
}

fn golden_value(name: &str) -> serde_json::Value {
    let raw = std::fs::read_to_string(goldens_dir().join(name)).unwrap();
    serde_json::from_str(raw.trim_end()).unwrap()
}

fn assert_golden(name: &str, value: &impl Serialize) {
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        golden_value(name),
        "{name}"
    );
}

fn test_state(config: LauncherConfig) -> launcher_app::LauncherAppState {
    use launcher_core::game::GameState;
    use launcher_net::download::{noop_guard_sink, Downloader};
    use std::sync::Arc;
    let downloader = Arc::new(Downloader::new(
        Arc::new(|_| {}),
        Arc::new(|_, _| {}),
        noop_guard_sink(),
    ));
    launcher_app::LauncherAppState {
        cancel_flag: downloader.cancel_flag.clone(),
        downloader,
        pack: tokio::sync::RwLock::new(None),
        config: tokio::sync::RwLock::new(config),
        app_data_dir: PathBuf::from("/tmp/blockfield-contract-test"),
        game: Arc::new(GameState::new(|_| {})),
        operation: tokio::sync::Mutex::new(()),
        events: Arc::new(|_| {}),
    }
}

fn fixed_config() -> LauncherConfig {
    LauncherConfig {
        game_dir: "/games/blockfield".into(),
        active_profile: launcher_core::config::ClientProfile::Game,
        workshop_game_dir: "/games/bf-workshop".into(),
        workshop_java_path: "/opt/java-ws/bin/java".into(),
        workshop_server: "workshop.example.com:25566".into(),
        java_path: "/opt/java/bin/java".into(),
        ram_mb: 6144,
        auto_update: false,
        lang: "ru".into(),
        username: "Steve".into(),
        player_uuid: "12345678-1234-1234-1234-123456789abc".into(),
        access_token: "tok".into(),
        client_token: "cli".into(),
        api_token: "api".into(),
        pre_launch_command: "echo pre".into(),
        post_exit_command: "echo post".into(),
        hide_while_playing: true,
        discord_presence: false,
        skin_password: "old".into(),
        skin_username: "OldName".into(),
    }
}

#[test]
fn check_modpack_version_shape() {
    use launcher_net::pack::JavaInfo;
    assert_golden(
        "version_check_result.json",
        &VersionCheckResult {
            needs_update: true,
            remote_version: "2.1.0".into(),
            installed_version: "2.0.0".into(),
            mirror: "https://cdn.blockfield.pro/pack".into(),
            file_count: 3,
            total_size: 123456789,
            java: Some(JavaInfo {
                version: "21.0.5".into(),
                url: "https://cdn.blockfield.pro/java.zip".into(),
                sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
                size: 101,
                platform: "linux-x86_64".into(),
            }),
            java_ok: true,
            loader_ok: false,
        },
    );
}

#[test]
fn account_status_shape() {
    use launcher_net::account::AccountStatus;
    let status = AccountStatus::from(&fixed_config());
    assert_golden("account_status.json", &status);
}

#[test]
fn load_settings_shape() {
    // `maxRamMb` is 3/4 of host RAM, so no fixed golden is possible: the file
    // holds the placeholder `"$MAX_RAM_MB"` (quoted, replaced by the number).
    let state = test_state(fixed_config());
    let response = launcher_app::ops::settings::load_settings(&state).unwrap();
    let live = launcher_core::config::max_ram_mb().to_string();
    let raw = std::fs::read_to_string(goldens_dir().join("settings_response.json")).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(&raw.replace("\"$MAX_RAM_MB\"", &live)).unwrap();
    assert_eq!(
        serde_json::to_value(&response).unwrap(),
        expected,
        "settings_response.json",
    );
}

#[test]
fn server_status_shape() {
    use launcher_net::status::*;
    assert_golden(
        "server_status.json",
        &ServerStatus {
            online: true,
            rooms: Some(vec![GameRoom {
                battle: Some(RoomBattle {
                    red: 1,
                    blue: 2,
                    points: vec![CapturePoint {
                        id: "mill".into(),
                        name: "Mill".into(),
                        owner: "RED".into(),
                        claiming: "NONE".into(),
                        capturing: "BLUE".into(),
                        progress: 42,
                    }],
                }),
                id: "room-1".into(),
                name: "Casual #1".into(),
                mode: "casual".into(),
                map: "plains".into(),
                phase: "lobby".into(),
                players: 5,
                joinable: true,
                format: Some("casual".into()),
                admission: Some("open".into()),
                ready: Some(RoomReady {
                    ready: 3,
                    required: 6,
                }),
            }]),
            game_available: Some(true),
            maintenance: None,
            players_online: Some(128),
            players_max: Some(200),
            minecraft_version: Some("1.20.1".into()),
            motd: Some(serde_json::json!({"text": "Blockfield"})),
            host: "mc.blockfield.pro".into(),
            port: 25565,
            region_code: "EU".into(),
            location_name: "Frankfurt".into(),
            server_latency_ms: Some(31),
            checked_at: 1700000000,
        },
    );
}

#[test]
fn game_status_shape() {
    let state = test_state(fixed_config());
    assert_golden(
        "game_status.json",
        &launcher_app::status_ops::game_status(&state),
    );
}

#[test]
fn dev_status_shape() {
    assert_golden(
        "dev_status.json",
        &launcher_core::dev::DevStatus {
            server: "127.0.0.1:25599".into(),
            freeze_pack: true,
            jvm_args: "-Xmx2G -Dtest=1".into(),
            pack_url: "https://cdn.example/pack".into(),
        },
    );
}

#[test]
fn skin_shapes() {
    use launcher_net::skins::{SkinPreview, SkinUploadResult};
    assert_golden(
        "skin_upload_result.json",
        &SkinUploadResult {
            skin_url: Some("https://skins.example/textures/skin.png".into()),
            cape_url: None,
        },
    );
    assert_golden(
        "skin_preview.json",
        &SkinPreview {
            skin: Some("data:image/png;base64,iVBORw0KGgo=".into()),
            cape: None,
            slim: true,
            source: "custom",
        },
    );
}

#[test]
fn event_payload_shapes() {
    use launcher_core::game::{GamePhase, GameStatus};
    use launcher_net::download::{DownloadProgressPayload, ProgressUnit};
    assert_golden(
        "launcher_status_event.json",
        &LauncherStatusPayload {
            phase: "verify",
            message: "Проверка файлов".into(),
            cancelable: true,
        },
    );
    assert_golden(
        "download_progress.json",
        &DownloadProgressPayload {
            unit: ProgressUnit::Bytes,
            file_path: "mods/core.jar".into(),
            file_index: 2,
            file_count: 9,
            bytes_downloaded: 1048576,
            file_bytes_total: 2097152,
            total_bytes_downloaded: 12582912,
            total_bytes_all: 33554432,
            speed_bytes_per_sec: 8388608,
        },
    );
    assert_golden(
        "download_mirror_event.json",
        &launcher_app::DownloadMirrorPayload {
            name: "mirror-1".into(),
            speed: Some("12.5 MB/s".into()),
        },
    );
    assert_golden(
        "game_status.json",
        &GameStatus {
            phase: GamePhase::Idle,
            revision: 0,
        },
    );
    // Unit variants travel as `()` today, i.e. JSON null.
    for (name, event) in [
        ("room_pending.json", LauncherEvent::RoomPending),
        ("account_expired.json", LauncherEvent::AccountExpired),
    ] {
        let payload = match event {
            LauncherEvent::RoomPending | LauncherEvent::AccountExpired => serde_json::Value::Null,
            _ => unreachable!(),
        };
        assert_eq!(serde_json::to_value(&payload).unwrap(), golden_value(name));
    }
}

#[test]
fn trivial_return_shapes() {
    // `verify_files` returns the missing-file list; `pending_room` the queued
    // room id; stats commands pass their JSON through as-is.
    let missing: Vec<String> = vec!["mods/a.jar".into(), "config/options.txt".into()];
    assert_golden("verify_files.json", &missing);
    assert_golden("pending_room.json", &Some("lobby".to_string()));
    let stats: Option<serde_json::Value> =
        Some(serde_json::json!({"entries": [{"name": "Steve", "score": 42}], "season": "s1"}));
    assert_golden("stats_sample.json", &stats);
    let no_stats: Option<serde_json::Value> = None;
    assert_eq!(serde_json::to_string(&no_stats).unwrap(), "null");
}
