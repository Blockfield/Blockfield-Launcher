//! Phase 2 close-out: pins the Tauri adapter contract from the shell side.
//!
//! - `REGISTERED_COMMANDS` (src-tauri/src/lib.rs, next to `generate_handler!`)
//!   equals the exact set of names the frontend `invoke()`s, in both
//!   directions: renaming a command or a call site on either side fails CI.
//! - Every `LauncherEvent` variant maps to today's event name and payload, and
//!   the set of names equals what the frontend `listen()`s to. Goldens compare
//!   as parsed JSON (`serde_json::Value`): key order is not contracted (the
//!   frontend uses `JSON.parse`), but field names, casing, null/absent and
//!   enum spellings must match.
//!
//! Frontend fixtures were generated with:
//!   grep -rhoP "invoke(<[^;]*?>)?\s*\(\s*['\"][a-z_]+['\"]" src/ \
//!     | grep -oP "['\"][a-z_]+['\"]" | tr -d "'\"" | sort -u
//! plus `login`/`register` (invoked via the `mode` variable in
//! `src/app/components/LoginScreen.tsx`), and the `listen('…')` /
//! `listenUntilUnsubscribed('…')` event names in `src/lib/events.ts`,
//! `src/lib/game-state.ts`, `src/app/App.tsx` and
//! `src/app/components/MainScreen.tsx`.

use blockfield_launcher_lib::{event_name, REGISTERED_COMMANDS};
use launcher_app::LauncherEvent;
use std::path::{Path, PathBuf};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn fixture_list(name: &str) -> Vec<String> {
    let raw = std::fs::read_to_string(fixtures_dir().join(name)).unwrap();
    serde_json::from_str(&raw).unwrap()
}

#[test]
fn registered_commands_match_frontend_invoke_calls() {
    let mut registered = REGISTERED_COMMANDS.to_vec();
    registered.sort_unstable();
    let mut frontend = fixture_list("frontend_commands.json");
    frontend.sort_unstable();
    assert_eq!(
        registered.len(),
        27,
        "adapter must serve exactly 27 commands"
    );
    assert_eq!(
        frontend, registered,
        "invoke() names differ from the adapter"
    );
}

fn all_events() -> Vec<(LauncherEvent, &'static str, &'static str)> {
    use launcher_app::{DownloadMirrorPayload, LauncherStatusPayload};
    use launcher_core::game::{GamePhase, GameStatus};
    use launcher_net::download::{DownloadProgressPayload, ProgressUnit};
    vec![
        (
            LauncherEvent::Status(LauncherStatusPayload {
                phase: "verify",
                message: "Проверка файлов".into(),
                cancelable: true,
            }),
            "launcher://status",
            include_str!("../../core/launcher-app/tests/goldens/launcher_status_event.json"),
        ),
        (
            LauncherEvent::DownloadProgress(DownloadProgressPayload {
                unit: ProgressUnit::Bytes,
                file_path: "mods/core.jar".into(),
                file_index: 2,
                file_count: 9,
                bytes_downloaded: 1048576,
                file_bytes_total: 2097152,
                total_bytes_downloaded: 12582912,
                total_bytes_all: 33554432,
                speed_bytes_per_sec: 8388608,
            }),
            "download://progress",
            include_str!("../../core/launcher-app/tests/goldens/download_progress.json"),
        ),
        (
            LauncherEvent::DownloadMirror(DownloadMirrorPayload {
                name: "mirror-1".into(),
                speed: Some("12.5 MB/s".into()),
            }),
            "download://mirror",
            include_str!("../../core/launcher-app/tests/goldens/download_mirror_event.json"),
        ),
        (
            LauncherEvent::GameStatus(GameStatus {
                phase: GamePhase::Idle,
                revision: 0,
            }),
            "game://status",
            include_str!("../../core/launcher-app/tests/goldens/game_status.json"),
        ),
        (
            LauncherEvent::RoomPending,
            "room://pending",
            include_str!("../../core/launcher-app/tests/goldens/room_pending.json"),
        ),
        (
            LauncherEvent::AccountExpired,
            "account://expired",
            include_str!("../../core/launcher-app/tests/goldens/account_expired.json"),
        ),
    ]
}

/// Test-local mirror of the sink arms in `src-tauri/src/lib.rs`: the sink
/// emits these same typed values, so pinning them here pins the wire shape.
fn event_payload_value(event: &LauncherEvent) -> serde_json::Value {
    match event {
        LauncherEvent::Status(payload) => serde_json::to_value(payload).unwrap(),
        LauncherEvent::DownloadProgress(payload) => serde_json::to_value(payload).unwrap(),
        LauncherEvent::DownloadMirror(mirror) => serde_json::json!({
            "name": mirror.name,
            "speed": mirror.speed,
        }),
        LauncherEvent::GameStatus(status) => serde_json::to_value(status).unwrap(),
        // Unit variants travel as `()` today, i.e. JSON null.
        LauncherEvent::RoomPending | LauncherEvent::AccountExpired => serde_json::Value::Null,
    }
}

#[test]
fn events_map_to_todays_names_and_payloads() {
    for (event, name, golden) in all_events() {
        assert_eq!(event_name(&event), name);
        let expected: serde_json::Value = serde_json::from_str(golden.trim_end()).unwrap();
        assert_eq!(event_payload_value(&event), expected, "{name}");
    }
}

#[test]
fn event_names_match_frontend_listen_calls() {
    let mut names: Vec<&str> = all_events()
        .iter()
        .map(|(event, _, _)| event_name(event))
        .collect();
    names.sort_unstable();
    names.dedup();
    let mut frontend = fixture_list("frontend_events.json");
    frontend.sort_unstable();
    assert_eq!(frontend, names, "listen() names differ from the adapter");
}
