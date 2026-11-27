//! Fail-before parity tests for D6: stats (F07, F21), skin upload/paths (F20),
//! room maintenance banner (F17), commands hook-error alert (F13).

use blockfield_native::{AppWindow, bridge, fixtures, stats, wire_app};
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use launcher_app::{LauncherAppState, LauncherEvent};
use launcher_core::game::{GamePhase, GameStatus};
use slint::{ComponentHandle, Model, ModelRc};

fn test_app() -> AppWindow {
    crate::init_slint();
    AppWindow::new().expect("test window")
}

fn ok(data: serde_json::Value) -> stats::StatsOutcome {
    stats::StatsOutcome {
        data: Some(data),
        saved_at: Some(1_761_300_000_000),
        stale: false,
        error: None,
    }
}

/// F07: SelfCard must show "—" when `totals` is JSON `null`, and must fall back
/// to `totals.mmr` when top-level `player.mmr` is `null`.
#[test]
fn f07_stats_self_null_totals_and_totals_mmr_fallback() {
    let app = test_app();
    let null_totals = serde_json::json!({
        "schemaVersion": 1,
        "profile": {"uuid": "u1", "name": "NetherG", "lastSeen": 1761300000000i64},
        "totals": null,
        "classes": [],
        "campaigns": {"completed": 0, "wins": null},
        "history": [],
        "mmr": null,
        "ratingHistory": []
    });
    stats::apply_self(&app, &ok(null_totals), "u1");
    assert_eq!(app.get_stats_self_state(), 3);
    assert_eq!(app.get_stats_self_rank().to_string(), "—");
    assert_eq!(app.get_stats_self_mmr().to_string(), "—");
    assert_eq!(app.get_stats_self_battles().to_string(), "—");
    assert_eq!(app.get_stats_self_kd().to_string(), "—");
    assert_eq!(app.get_stats_self_captures().to_string(), "—");
    assert_eq!(app.get_stats_self_campaigns().to_string(), "0 / —");

    // Top-level mmr is null, totals.mmr carries the player's rating.
    let totals_mmr = serde_json::json!({
        "schemaVersion": 1,
        "profile": {"uuid": "u2", "name": "PlayerTwo", "lastSeen": 1761300000000i64},
        "totals": {
            "uuid": "u2", "name": "PlayerTwo", "score": 100.0,
            "kills": 10, "deaths": 5, "captures": 2, "matches": 4, "wins": 3,
            "mmr": {"rating": 1600.0, "rd": 50.0, "provisional": false},
            "rank": 5
        },
        "classes": [],
        "campaigns": {"completed": 2, "wins": 1},
        "history": [],
        "mmr": null,
        "ratingHistory": []
    });
    stats::apply_self(&app, &ok(totals_mmr), "u2");
    assert_eq!(
        app.get_stats_self_mmr().to_string(),
        "1600 ± 100",
        "totals.mmr fallback must populate SelfCard MMR"
    );
    assert_eq!(app.get_stats_self_battles().to_string(), "4 / 3");
    assert_eq!(app.get_stats_self_kd().to_string(), "10 / 5");
    assert_eq!(app.get_stats_self_captures().to_string(), "2");
}

/// F21: Leaderboard uses server-returned `page` for pager & synthetic ranks,
/// and both board/self errors render expandable `FormError` ("Подробности").
#[test]
fn f21_stats_server_page_and_error_detail() {
    let app = test_app();
    let board = serde_json::json!({
        "schemaVersion": 1,
        "page": 3,
        "limit": 25,
        "total": 103,
        "players": [
            {
                "uuid": "p1", "name": "PageThreePlayer", "score": 50.0,
                "kills": 5, "deaths": 2, "captures": 1, "matches": 3, "wins": 2,
                "mmr": null
            }
        ]
    });
    // Requested page was 1, server answered page 3.
    stats::apply_board(&app, &ok(board), "own-uuid", 1);
    assert_eq!(
        app.get_stats_page(),
        3,
        "server page overrides requested page"
    );
    assert!(app.get_stats_has_prev(), "page 3 has prev");
    assert!(app.get_stats_has_next(), "page 3 of 5 has next");
    assert_eq!(
        app.get_stats_rows().row_data(0).unwrap().rank.to_string(),
        "51",
        "synthetic rank derives from server page 3: (3-1)*25 + 1 = 51"
    );

    // ErrorDetail spoiler renders for both board and self errors.
    app.set_active(2);
    let err = stats::StatsOutcome {
        data: None,
        saved_at: None,
        stale: false,
        error: Some("Stats API: 503 Service Unavailable".to_owned()),
    };
    stats::apply_board(&app, &err, "own-uuid", 1);
    stats::apply_self(&app, &err, "own-uuid");
    let details: Vec<_> = ElementHandle::find_by_accessible_label(&app, "Подробности")
        .filter(|el| el.accessible_role() != Some(AccessibleRole::Text))
        .collect();
    assert_eq!(
        details.len(),
        2,
        "both board and self errors must expose expandable Подробности controls"
    );
}

/// F20: Skin and Cape paths are editable text inputs without dirtying SettingsForm,
/// and upload errors render FormError (friendly + expandable raw error).
#[test]
fn f20_skin_editable_paths_and_upload_error_detail() {
    let app = test_app();
    let (_host, _activity) =
        wire_app(&app, blockfield_native::backend::Backend::Fixtures).expect("wire");
    fixtures::apply_settings(&app).expect("settings fixtures");
    assert_eq!((app.get_active(), app.get_settings_tab()), (3, 3));

    let skin_input = ElementHandle::find_by_accessible_label(&app, "СКИН")
        .find(|el| el.accessible_role() == Some(AccessibleRole::TextInput))
        .expect("skin path must be an editable TextInput");
    let cape_input = ElementHandle::find_by_accessible_label(&app, "ПЛАЩ")
        .find(|el| el.accessible_role() == Some(AccessibleRole::TextInput))
        .expect("cape path must be an editable TextInput");

    let form = app.global::<blockfield_native::SettingsForm>();
    assert!(!form.get_dirty());
    skin_input.set_accessible_value("/tmp/custom-skin.png");
    assert_eq!(app.get_skin_path().to_string(), "/tmp/custom-skin.png");
    cape_input.set_accessible_value("/tmp/custom-cape.png");
    assert_eq!(app.get_cape_path().to_string(), "/tmp/custom-cape.png");
    assert!(
        !form.get_dirty(),
        "editing skin/cape paths must not dirty SettingsForm"
    );

    // Upload error exposes FormError with expandable raw detail.
    app.set_skin_error("Не удалось загрузить скин.".into());
    app.set_skin_raw_error("Drasl: 500 Internal Server Error".into());
    let details: Vec<_> = ElementHandle::find_by_accessible_label(&app, "Подробности")
        .filter(|el| el.accessible_role() != Some(AccessibleRole::Text))
        .collect();
    assert!(
        !details.is_empty(),
        "skin upload error must render FormError with Подробности toggle"
    );
}

/// F17: Room maintenance banner must not double-print "Идут технические работы"
/// when the server message is empty, and must not add a double period "..".
#[test]
fn f17_room_maintenance_banner_no_double_print() {
    let app = test_app();
    let (_host, _activity) =
        wire_app(&app, blockfield_native::backend::Backend::Fixtures).expect("wire");
    fixtures::apply_main(&app, "main", "maintenance").expect("maintenance fixture");
    app.set_rooms(ModelRc::default());

    // Empty server maintenance message (backend sentinel "Идут технические работы").
    let mut data = app.get_main_data();
    data.maintenance = "Идут технические работы".into();
    app.set_main_data(data);
    assert!(
        ElementHandle::find_by_accessible_label(
            &app,
            "Идут технические работы. Матчи откроются после завершения работ.",
        )
        .next()
        .is_some(),
        "empty maintenance must not double-print 'Идут технические работы'"
    );

    // Custom message ending with a period must not produce "..".
    let mut data = app.get_main_data();
    data.maintenance = "Обновляем карту.".into();
    app.set_main_data(data);
    assert!(
        ElementHandle::find_by_accessible_label(
            &app,
            "Идут технические работы: Обновляем карту. Матчи откроются после завершения работ.",
        )
        .next()
        .is_some(),
        "custom maintenance ending in '.' must not produce double period '..'"
    );
}

/// F13: `hook-error` status event must populate a persistent dismissible alert in Shell
/// rather than being overwritten by subsequent launch status or GamePhase::Idle.
#[test]
fn f13_hook_error_shows_persistent_dismissible_alert() {
    let app = test_app();
    let (_host, _activity) =
        wire_app(&app, blockfield_native::backend::Backend::Fixtures).expect("wire");
    fixtures::apply_main(&app, "main", "default").expect("main fixture");

    let msg = "Команда перед запуском: exit code 1";
    bridge::apply(
        &app,
        LauncherAppState::status_event("hook-error", msg, false),
    );
    // Subsequent launch status and Idle must NOT wipe the hook-error alert.
    bridge::apply(
        &app,
        LauncherAppState::status_event("launch", "Запуск игры", false),
    );
    bridge::apply(
        &app,
        LauncherEvent::GameStatus(GameStatus {
            phase: GamePhase::Idle,
            revision: 1,
        }),
    );

    assert!(
        ElementHandle::find_by_accessible_label(&app, msg)
            .next()
            .is_some(),
        "hook-error message must remain visible in Shell after launch/idle events"
    );

    // Skip the WindowChrome titlebar close button (y < 50); click the Shell alert's dismiss button.
    let close = ElementHandle::find_by_accessible_label(&app, "Закрыть")
        .find(|el| {
            el.accessible_role() != Some(AccessibleRole::Text) && el.absolute_position().y > 50.0
        })
        .expect("hook-error alert must have a dismiss button");
    close.invoke_accessible_default_action();
    assert!(
        ElementHandle::find_by_accessible_label(&app, msg)
            .next()
            .is_none(),
        "clicking Закрыть must dismiss the hook-error alert"
    );
}

/// F13 + F30: Commands screen pre-launch hint matches engine behavior ("не отменяет запуск"),
/// shell footnote says "sh (Linux)", and DevPanel heading says "DEV-сборка".
#[test]
fn f13_f30_commands_and_dev_panel_copy_parity() {
    let app = test_app();
    let (_host, _activity) =
        wire_app(&app, blockfield_native::backend::Backend::Fixtures).expect("wire");
    fixtures::apply_settings(&app).expect("settings fixtures");
    app.set_settings_tab(2);
    assert!(
        ElementHandle::find_by_accessible_label(&app, "команды не отменяет запуск.")
            .next()
            .is_some(),
        "pre-launch hint must state errors do not cancel launch"
    );
    assert!(
        ElementHandle::find_by_accessible_label(
            &app,
            "Команды выполняются в папке игры через sh (Linux) или cmd (Windows). Доступны INST_DIR, INST_MC_DIR, INST_JAVA и INST_EXIT_CODE после выхода. Обращение к переменным: $INST_DIR в sh, %INST_DIR% в cmd. Вывод: logs/launcher-hooks.log.",
        )
        .next()
        .is_some(),
        "commands footer must say sh (Linux)"
    );
    app.set_settings_tab(4);
    assert!(
        ElementHandle::find_by_accessible_label(&app, "DEV-сборка")
            .next()
            .is_some(),
        "DevPanel heading must match Tauri 'DEV-сборка'"
    );
}
