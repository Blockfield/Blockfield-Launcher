//! Stats screen rendering: filters/sort/page/stale cases through the real
//! `stats::apply_*` mapping (the same code live loads use), plus the fixture
//! pager/filter callbacks. Fail-before cover for the self-rank parity fix:
//! the TSX `Место` cell shows the plain rank (`3`), `#{rank}` lives only in
//! the header code.

use std::sync::Once;

use blockfield_native::{AppWindow, fixtures, stats};
use slint::{Model, ModelRc};

fn init_once() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        i_slint_backend_testing::init_no_event_loop();
    });
}

fn test_app() -> AppWindow {
    init_once();
    AppWindow::new().expect("test window")
}

fn board_json(total: i64, rows: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({"schemaVersion": 1, "page": 1, "limit": 25, "total": total, "players": rows})
}

fn row(uuid: &str, name: &str, rank: Option<i64>) -> serde_json::Value {
    let mut v = serde_json::json!({
        "uuid": uuid, "name": name, "score": 1200.5, "kills": 40, "deaths": 12,
        "captures": 9, "neutralizations": 3, "pointSeconds": 500, "playSeconds": 3600,
        "suicides": 0, "teamKills": 0, "maxKillstreak": 7, "matches": 30, "wins": 18,
        "kd": 0.0, "mmr": {"rating": 1500.0, "rd": 60.0, "ratedCampaigns": 9, "provisional": false},
    });
    if let Some(rank) = rank {
        v["rank"] = rank.into();
    }
    v
}

fn ok(data: serde_json::Value) -> stats::StatsOutcome {
    stats::StatsOutcome {
        data: Some(data),
        saved_at: Some(1_761_300_000_000),
        stale: false,
        error: None,
    }
}

fn self_json() -> serde_json::Value {
    serde_json::json!({
        "schemaVersion": 1,
        "profile": {"uuid": "own-uuid", "name": "CaptureOperator", "lastSeen": 1761300000000i64},
        "totals": {
            "uuid": "own-uuid", "name": "CaptureOperator", "score": 2450.75,
            "kills": 210, "deaths": 96, "captures": 44, "neutralizations": 11,
            "pointSeconds": 900, "playSeconds": 7200, "suicides": 1, "teamKills": 0,
            "maxKillstreak": 12, "matches": 64, "wins": 41, "kd": 0.0,
            "mmr": null, "rank": 3
        },
        "classes": [],
        "campaigns": {"completed": 12, "wins": 7},
        "history": [
            {"eventId": "m1", "kind": "match", "mode": "room", "endedAt": 1761300000000i64, "reason": "completed", "winner": "RED", "winnerUuid": null, "team": "RED", "score": 10, "kills": 9, "deaths": 2},
            {"eventId": "m2", "kind": "match", "mode": "campaign", "endedAt": 1761296400000i64, "reason": "completed", "winner": "BLUE", "winnerUuid": null, "team": "RED", "score": 4, "kills": 3, "deaths": 8},
            {"eventId": "m3", "kind": "match", "mode": "room", "endedAt": 1761292800000i64, "reason": "draw", "winner": "NONE", "winnerUuid": null, "team": "NONE", "score": 5, "kills": 5, "deaths": 5},
            {"eventId": "m4", "kind": "match", "mode": "deathmatch", "endedAt": 1761289200000i64, "reason": "completed", "winner": "", "winnerUuid": "own-uuid", "team": "", "score": 20, "kills": 20, "deaths": 6},
            {"eventId": "m5", "kind": "match", "mode": "room", "endedAt": 1761285600000i64, "reason": "abandoned", "winner": "", "winnerUuid": null, "team": "RED", "score": 1, "kills": 1, "deaths": 9}
        ],
        "mmr": {"rating": 1648.4, "rd": 71.0, "ratedCampaigns": 14, "provisional": false},
        "ratingHistory": [
            {"campaignSessionId": "r6", "ratedAt": 1761300000000i64, "team": "RED", "score": 10, "before": 1600.0, "after": 1612.6, "rd": 70.0},
            {"campaignSessionId": "r5", "ratedAt": 1761296400000i64, "team": "BLUE", "score": 4, "before": 1612.6, "after": 1601.2, "rd": 70.5},
            {"campaignSessionId": "r4", "ratedAt": 1761292800000i64, "team": "RED", "score": 8, "before": 1615.9, "after": 1612.6, "rd": 71.0},
            {"campaignSessionId": "r3", "ratedAt": 1761289200000i64, "team": "RED", "score": 9, "before": 1609.1, "after": 1615.9, "rd": 71.5},
            {"campaignSessionId": "r2", "ratedAt": 1761285600000i64, "team": "BLUE", "score": 3, "before": 1618.4, "after": 1609.1, "rd": 72.0},
            {"campaignSessionId": "r1", "ratedAt": 1761282000000i64, "team": "RED", "score": 10, "before": 1605.0, "after": 1618.4, "rd": 72.5}
        ]
    })
}

fn scenario_board_states_and_pager() {
    let app = test_app();
    // Loading fixture: both halves spin.
    fixtures::apply_stats(&app, "loading").expect("loading fixtures");
    assert_eq!(app.get_stats_board_state(), 0, "board loading");
    assert_eq!(app.get_stats_self_state(), 0, "self loading");

    // Error without cache: board error, message carried.
    let app = test_app();
    stats::apply_board(
        &app,
        &stats::StatsOutcome {
            data: None,
            saved_at: None,
            stale: false,
            error: Some("boom".to_owned()),
        },
        "own-uuid",
        1,
    );
    assert_eq!(app.get_stats_board_state(), 1, "board error");
    assert_eq!(app.get_stats_board_error().to_string(), "boom");

    // Empty board: dedicated empty state like the TSX.
    let app = test_app();
    stats::apply_board(&app, &ok(board_json(0, vec![])), "own-uuid", 1);
    assert_eq!(app.get_stats_board_state(), 2, "board empty");
    assert_eq!(app.get_stats_rows().row_count(), 0);

    // Data: stored rank wins, missing rank falls back to page position,
    // own row matches case-insensitively, pager counts derive from total.
    let app = test_app();
    stats::apply_board(
        &app,
        &ok(board_json(
            103,
            vec![
                row("some-other", "Player-001", Some(1)),
                row("OWN-UUID", "CaptureOperator", None),
            ],
        )),
        "own-uuid",
        2,
    );
    assert_eq!(app.get_stats_board_state(), 3, "board data");
    let rows = app.get_stats_rows();
    assert_eq!(rows.row_count(), 2);
    let first = rows.row_data(0).expect("row 0");
    let own = rows.row_data(1).expect("row 1");
    assert_eq!(first.rank.to_string(), "1", "stored rank kept");
    assert!(!first.own, "other row not own");
    assert_eq!(
        own.rank.to_string(),
        "27",
        "rank falls back to page position"
    );
    assert!(own.own, "own uuid matches case-insensitively");
    assert_eq!(app.get_stats_page(), 2);
    assert_eq!(app.get_stats_pages(), 5, "ceil(103/25)");
    assert_eq!(app.get_stats_total(), 103);
    assert!(app.get_stats_has_prev(), "page 2 has prev");
    assert!(app.get_stats_has_next(), "page 2 of 5 has next");

    // Stale cache: table stays visible with the stale banner above it.
    let app = test_app();
    let stale = stats::StatsOutcome {
        data: Some(board_json(2, vec![row("a", "A", None)])),
        saved_at: Some(1_761_300_000_000),
        stale: true,
        error: Some("offline".to_owned()),
    };
    stats::apply_board(&app, &stale, "own-uuid", 1);
    assert_eq!(app.get_stats_board_state(), 3, "stale keeps the table");
    assert!(app.get_stats_board_stale(), "stale flag set");
    assert!(
        app.get_stats_board_stale_label()
            .to_string()
            .starts_with("Статистика недоступна · данные на "),
        "stale label mirrors the TSX banner"
    );
}

fn scenario_self_card_states_and_rank_parity() {
    // Fresh 404, no cached copy: never played (StatsScreen.tsx:287-288).
    let app = test_app();
    stats::apply_self(
        &app,
        &stats::StatsOutcome {
            data: None,
            saved_at: None,
            stale: false,
            error: None,
        },
        "own-uuid",
    );
    assert_eq!(app.get_stats_self_state(), 1, "never played");

    // Error without cache: profile error.
    let app = test_app();
    stats::apply_self(
        &app,
        &stats::StatsOutcome {
            data: None,
            saved_at: None,
            stale: false,
            error: Some("boom".to_owned()),
        },
        "own-uuid",
    );
    assert_eq!(app.get_stats_self_state(), 2, "self error");
    assert_eq!(app.get_stats_self_error().to_string(), "boom");

    // Data: the Место cell is the plain rank like the TSX (`3`), while the
    // header code keeps the `#` prefix (`#3`).
    let app = test_app();
    stats::apply_self(&app, &ok(self_json()), "OWN-UUID");
    assert_eq!(app.get_stats_self_state(), 3, "self data");
    assert_eq!(app.get_stats_self_name().to_string(), "CaptureOperator");
    assert_eq!(
        app.get_stats_self_rank().to_string(),
        "3",
        "cell: plain rank"
    );
    assert_eq!(
        app.get_stats_self_rank_code().to_string(),
        "#3",
        "header: #-prefixed code"
    );
    assert_eq!(app.get_stats_self_mmr().to_string(), "1648 ± 142");
    assert_eq!(app.get_stats_self_battles().to_string(), "64 / 41");
    assert_eq!(app.get_stats_self_kd().to_string(), "210 / 96");
    assert_eq!(app.get_stats_self_captures().to_string(), "44");
    assert_eq!(app.get_stats_self_campaigns().to_string(), "12 / 7");
    let rating = app.get_stats_rating();
    assert_eq!(rating.row_count(), 5, "rating capped at 5");
    let top = rating.row_data(0).expect("rating 0");
    assert_eq!(top.delta.to_string(), "+13", "newest delta first");
    assert!(top.up, "gain points up");
    let matches = app.get_stats_matches();
    assert_eq!(matches.row_count(), 4, "matches capped at 4");
    let outcomes: Vec<String> = (0..4)
        .map(|i| matches.row_data(i).expect("match row").outcome.to_string())
        .collect();
    assert_eq!(outcomes, vec!["Победа", "Поражение", "Ничья", "Победа"]);
    assert!(!app.get_stats_self_stale(), "fresh load not stale");

    // Stale self: card stays visible with the stale banner.
    let app = test_app();
    stats::apply_self(
        &app,
        &stats::StatsOutcome {
            data: Some(self_json()),
            saved_at: Some(1_761_300_000_000),
            stale: true,
            error: Some("offline".to_owned()),
        },
        "own-uuid",
    );
    assert_eq!(app.get_stats_self_state(), 3, "stale keeps the card");
    assert!(app.get_stats_self_stale(), "self stale flag set");
    assert!(
        app.get_stats_self_stale_label()
            .to_string()
            .starts_with("Статистика недоступна · данные на "),
        "self stale label mirrors the TSX banner"
    );
}

fn scenario_fixture_callbacks_drive_filters_sort_and_page() {
    // Given the pager fixture (total 103, on page 2).
    let win = test_app();
    fixtures::apply_stats(&win, "pager").expect("pager fixtures");
    stats::wire_fixtures(&win);
    assert_eq!(win.get_stats_total(), 103);
    assert_eq!(win.get_stats_page(), 2);
    assert_eq!(win.get_stats_rows().row_count(), 25);
    assert_eq!(
        win.get_stats_rows()
            .row_data(0)
            .expect("row 0")
            .rank
            .to_string(),
        "26",
        "page 2 starts at rank 26"
    );

    // When the operator picks a filter, the page resets to 1 like the TSX.
    win.invoke_stats_set_mode("room".into());
    assert_eq!(win.get_stats_mode().to_string(), "room");
    assert_eq!(win.get_stats_page(), 1, "filter resets page");
    win.invoke_stats_set_sort("kd".into());
    assert_eq!(win.get_stats_sort().to_string(), "kd");
    assert_eq!(win.get_stats_page(), 1, "sort resets page");
    win.invoke_stats_set_period("season:2026".into());
    assert_eq!(win.get_stats_period().to_string(), "season:2026");
    assert_eq!(win.get_stats_page(), 1, "period resets page");

    // Pager walks the fixture board and clamps at both ends.
    win.invoke_stats_page_delta(1);
    assert_eq!(win.get_stats_page(), 2);
    assert_eq!(
        win.get_stats_rows()
            .row_data(0)
            .expect("row 0")
            .rank
            .to_string(),
        "26"
    );
    win.set_stats_page(5);
    win.invoke_stats_page_delta(1);
    assert_eq!(win.get_stats_page(), 5, "clamped at last page");
    win.set_stats_page(1);
    win.invoke_stats_page_delta(-1);
    assert_eq!(win.get_stats_page(), 1, "clamped at first page");

    // Refresh re-applies the current page fixture.
    win.set_stats_page(3);
    win.invoke_stats_refresh();
    assert_eq!(win.get_stats_page(), 3, "refresh keeps the page");
    assert_eq!(
        win.get_stats_rows()
            .row_data(0)
            .expect("row 0")
            .rank
            .to_string(),
        "51",
        "page 3 starts at rank 51"
    );
}

fn scenario_stale_fixture_renders_both_banners() {
    let win = test_app();
    fixtures::apply_stats(&win, "stale").expect("stale fixtures");
    assert!(win.get_stats_board_stale(), "board stale banner");
    assert!(win.get_stats_self_stale(), "self stale banner");
    assert_eq!(win.get_stats_board_state(), 3, "stale board renders");
    assert_eq!(win.get_stats_self_state(), 3, "stale self renders");
}

fn scenario_season_periods_extend_the_filter_model() {
    let win = test_app();
    stats::apply_filter_options(&win);
    assert_eq!(win.get_stats_periods().row_count(), 3, "base periods");
    stats::apply_seasons(
        &win,
        &serde_json::json!({"current": "2026", "seasons": [
            {"id": "2026", "name": "Сезон 2026"},
            {"id": "", "name": "Без id"},
            {"id": "2025", "name": "Сезон 2025"},
        ]}),
    );
    let periods = win.get_stats_periods();
    assert_eq!(periods.row_count(), 5, "seasons appended");
    let ids: Vec<String> = (0..5)
        .map(|i| periods.row_data(i).expect("period row").id.to_string())
        .collect();
    assert_eq!(
        ids,
        vec!["all", "week", "month", "season:2026", "season:2025"]
    );
    // The fixture seasons helper keeps the same scheme for live loads.
    let live = test_app();
    fixtures::apply_stats(&live, "default").expect("default fixtures");
    assert_eq!(live.get_stats_periods().row_count(), 5);
}

/// Filter option labels stay byte-identical to the TSX MODES/PERIODS/SORTS.
fn scenario_filter_options_match_tsx_labels() {
    let win = test_app();
    stats::apply_filter_options(&win);
    let labels = |model: ModelRc<blockfield_native::StatsFilter>| -> Vec<String> {
        (0..model.row_count())
            .map(|i| model.row_data(i).expect("option").label.to_string())
            .collect()
    };
    assert_eq!(
        labels(win.get_stats_modes()),
        vec!["Все", "Casual", "Ranked", "Deathmatch"]
    );
    assert_eq!(
        labels(win.get_stats_periods()),
        vec!["Всё время", "7 дней", "30 дней"]
    );
    assert_eq!(
        labels(win.get_stats_sorts()),
        vec!["Очки", "MMR", "K/D", "Захваты"]
    );
}

#[test]
fn stats_render_walk() {
    scenario_board_states_and_pager();
    scenario_self_card_states_and_rank_parity();
    scenario_fixture_callbacks_drive_filters_sort_and_page();
    scenario_stale_fixture_renders_both_banners();
    scenario_season_periods_extend_the_filter_model();
    scenario_filter_options_match_tsx_labels();
    println!("PASS stats render walk (6 scenarios)");
}
