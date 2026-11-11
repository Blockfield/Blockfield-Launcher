use slint::{ModelRc, SharedString, VecModel};

use crate::AppWindow;

pub fn apply(app: &AppWindow) {
    app.set_chrome_title("BLOCKFIELD LAUNCHER".into());
    app.set_brand("BLOCKFIELD".into());
    app.set_subtitle("КОМАНДНЫЕ БОИ".into());
    app.set_username("—".into());
    app.set_role("ИГРОК".into());
    app.set_launcher_label("ЛАУНЧЕР v1.0.19".into());
    app.set_server_label("IP · play.blockfield.pro".into());
    app.set_support_label("ПОДДЕРЖКА".into());
    app.set_network_label("АККАУНТ BLOCKFIELD".into());
    app.set_update_data(crate::update::default_data());
    app.set_launcher_update_data(crate::update::default_launcher_data());
    app.set_nav_labels(ModelRc::new(VecModel::from(
        ["ИГРАТЬ", "ОБНОВЛЕНИЯ", "ТОП", "НАСТРОЙКИ"]
            .map(SharedString::from)
            .to_vec(),
    )));
    app.set_dev_build(cfg!(debug_assertions));
    apply_dev(app);
}

/// Dev panel fixture: rows of a dev build with the standard dev environment
/// (mirrors the Tauri DevPanel; production hides the panel via empty server).
pub fn apply_dev(app: &AppWindow) {
    app.set_dev_data(crate::DevData {
        server: "127.0.0.1:25565".into(),
        freeze: "пак заморожен".into(),
        jvm_args: "-Xmx2G -Dtestbot=true".into(),
        pack_url: "https://packs.blockfield.pro/dev.zip".into(),
    });
}

pub const STATES: &[&str] = &[
    "main",
    "main-launching",
    "offline",
    "error",
    "shell",
    "settings-skin",
    "stats",
    "login",
    "firstrun",
    "update",
];
pub const SCENARIOS: &[&str] = &[
    "default",
    "workshop",
    "builder",
    "checking",
    "busy",
    "running",
    "finishing",
    "profile-changing",
    "profile-blocked",
    "profile-error",
    "empty",
    "loading",
    "stale",
    "pager",
    "unavailable",
    "maintenance",
    "updating",
    "update",
    "setup",
    "retry",
    "inflight",
    "stale",
    "closed",
];

pub fn apply_main(app: &AppWindow, state: &str, scenario: &str) -> Result<(), String> {
    use crate::{BattlePoint, MainData, Room};
    if !STATES.contains(&state) || !SCENARIOS.contains(&scenario) {
        return Err(format!("Unknown state/scenario: {state}/{scenario}"));
    }
    app.set_show_main(state != "shell");
    if state == "shell" {
        return Ok(());
    }
    // Values copied from native/capture data.ts; that fixture's player role hides profile selection.
    app.set_username("CaptureOperator".into());
    app.set_update_label("Доступна 1.0.20".into());
    let mut data = MainData {
        online: state != "offline",
        rooms_available: true,
        players: if state == "offline" { "—" } else { "24" }.into(),
        ping: if state == "offline" { "—" } else { "32" }.into(),
        region: "EU".into(),
        location: "Frankfurt".into(),
        installed: if state == "error" { "..." } else { "2.75" }.into(),
        latest: if state == "error" { "..." } else { "2.75" }.into(),
        size: if state == "error" { "—" } else { "1.0 GB" }.into(),
        checked: state != "error",
        check_error: if state == "error" {
            "Capture fixture: manifest unavailable"
        } else {
            ""
        }
        .into(),
        phase: if state == "main-launching" {
            "launching"
        } else {
            "idle"
        }
        .into(),
        progress: -1,
        ..Default::default()
    };
    match scenario {
        "builder" | "workshop" => {
            data.can_workshop = true;
            app.set_role("СТРОИТЕЛЬ".into());
            app.set_workshop(scenario == "workshop");
        }
        "checking" => {
            data.checking = true;
            data.checked = false;
        }
        "busy" => {
            data.launching = true;
            data.launch_status = "Подготовка файлов".into();
            data.progress = 42;
        }
        "running" => data.phase = "running".into(),
        "finishing" => data.phase = "finishing".into(),
        "profile-changing" => {
            data.can_workshop = true;
            data.profile_changing = true;
        }
        "profile-blocked" => {
            data.can_workshop = true;
            data.profile_blocked = true;
        }
        "profile-error" => {
            data.can_workshop = true;
            data.profile_blocked = true;
            data.profile_error = "Не удалось переключить профиль.".into();
        }
        "maintenance" => data.maintenance = "Плановое обслуживание".into(),
        "updating" => data.updating = true,
        "unavailable" => data.rooms_available = false,
        "update" => {
            data.needs_setup = true;
            data.needs_update = true;
            data.installed = "2.74".into();
        }
        "setup" => data.needs_setup = true,
        "retry" => data.launch_error = "Fixture: launch failed".into(),
        // Mirrors PR #64 inflight progress (core ops::install): while a large
        // jar still downloads, the live file leads the launch status.
        "inflight" => {
            data.launching = true;
            data.phase = "launching".into();
            data.launch_status =
                "12.3/40.1 MB - blockfield-client-1.56.0+mc1.21.1.jar - Gcore CDN".into();
            data.progress = 8;
        }
        // Mirrors the PACK_STALE launch refusal (core ops::launch): the main
        // screen shows the friendly update prompt instead of a raw error.
        "stale" => {
            data.launch_error = "PACK_STALE: installed pack is outdated (remote 2.80.0). Open Updates to install it, then launch again.".into();
            data.launch_stale = true;
        }
        _ => {}
    }
    let no_rooms = state == "offline"
        || ["empty", "unavailable", "maintenance", "updating"].contains(&scenario);
    let rooms = if no_rooms {
        vec![]
    } else {
        vec![
            Room {
                id: "capture-casual".into(),
                name: "ALPHA".into(),
                description: "Casual · room · Citadel · waiting".into(),
                players: 12,
                ready: "Готовы: 8/12".into(),
                join: if scenario == "closed" {
                    "Мест нет"
                } else {
                    "Войти"
                }
                .into(),
                joinable: scenario != "closed",
                has_battle: false,
                battle_red: 0,
                battle_blue: 0,
                points: slint::ModelRc::default(),
            },
            Room {
                id: "capture-ranked".into(),
                name: "BRAVO".into(),
                description: "Ranked · campaign · Crossroads · running".into(),
                players: 12,
                ready: "".into(),
                join: "Встать в очередь".into(),
                joinable: true,
                has_battle: true,
                battle_red: 2,
                battle_blue: 5,
                // Four points over two pager pages (Tauri BattlePoints parity).
                points: slint::ModelRc::new(slint::VecModel::from(vec![
                    BattlePoint {
                        label: "Высота · Синие".into(),
                        sub: "82%".into(),
                        progress: 82,
                        tone: 2,
                    },
                    BattlePoint {
                        label: "Мост · Красные".into(),
                        sub: "45% · Синие нейтрализуют".into(),
                        progress: 45,
                        tone: 1,
                    },
                    BattlePoint {
                        label: "Завод · Нейтральная".into(),
                        sub: "10%".into(),
                        progress: 10,
                        tone: 0,
                    },
                    BattlePoint {
                        label: "Холм · Синие".into(),
                        sub: "100%".into(),
                        progress: 100,
                        tone: 2,
                    },
                ])),
            },
        ]
    };
    app.set_room_names(ModelRc::new(VecModel::from(
        rooms.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
    )));
    app.set_rooms(ModelRc::new(VecModel::from(rooms)));
    app.set_main_data(data);
    crate::avatars::apply_fixture_faces(app);
    Ok(())
}

/// Login fixtures: `variant` is default (empty form), register (mode
/// toggle) or password (migration: username fixed, logout link shown).
pub fn apply_login(app: &AppWindow, variant: &str) -> Result<(), String> {
    if !["default", "register", "password"].contains(&variant) {
        return Err(format!("Unknown login variant: {variant}"));
    }
    app.set_show_main(false);
    app.set_show_firstrun(false);
    app.set_login_mode(
        if variant == "register" {
            "register"
        } else {
            "login"
        }
        .into(),
    );
    app.set_login_set_password(variant == "password");
    app.set_login_username(
        if variant == "password" {
            "CaptureOperator"
        } else {
            ""
        }
        .into(),
    );
    app.set_login_password("".into());
    app.set_login_busy(false);
    app.set_login_error("".into());
    app.set_login_name_valid(true);
    app.set_login_can_submit(false);
    app.set_show_login(true);
    Ok(())
}

pub fn apply_firstrun(app: &AppWindow) -> Result<(), String> {
    app.set_show_main(false);
    app.set_show_login(false);
    app.set_firstrun_game_dir("/home/operator/.blockfield".into());
    app.set_firstrun_java_path("".into());
    app.set_firstrun_ram(4);
    app.set_firstrun_maxram(16);
    app.set_firstrun_presets(ModelRc::new(VecModel::from(vec![4, 6, 8, 12, 16])));
    app.set_firstrun_saving(false);
    app.set_firstrun_error("".into());
    app.set_show_firstrun(true);
    Ok(())
}

pub fn apply_settings(app: &AppWindow) -> Result<(), String> {
    app.set_show_main(false);
    // Same shell fixture values as the main capture: the operator role hides profile selection.
    app.set_username("CaptureOperator".into());
    app.set_update_label("Доступна 1.0.20".into());
    app.set_active(3);
    app.set_settings_tab(3);
    app.set_settings_tabs(ModelRc::new(VecModel::from(
        ["ОБЩИЕ", "JAVA И ПАМЯТЬ", "КОМАНДЫ", "СКИН", "ЛАУНЧЕР"]
            .map(SharedString::from)
            .to_vec(),
    )));
    app.set_settings_title("НАСТРОЙКИ".into());
    app.set_settings_subtitle("/ ПАРАМЕТРЫ ЛАУНЧЕРА".into());
    // Fixture preview_skin is custom with empty file rows, classic model.
    app.set_skin_path("".into());
    app.set_cape_path("".into());
    app.set_slim(false);
    app.set_skin_caption("ВАШ СКИН".into());
    app.set_skin_error("".into());
    crate::avatars::apply_fixture_faces(app);
    Ok(())
}

/// Stats ("ТОП") fixtures. Payloads are canned op-shaped JSON (as if from
/// the stats API) passed through the real `stats::apply_*` mapping, so
/// goldens exercise the same code as live loads — not hand-set rows.
pub const STATS_UUID: &str = "c0ffee00-0000-4000-8000-000000000001";

fn stats_row(index: i64, own: bool) -> serde_json::Value {
    serde_json::json!({
        "uuid": if own { STATS_UUID.to_owned() } else { format!("c0ffee00-0000-4000-8000-{index:012}") },
        "name": if own { "CaptureOperator".to_owned() } else { format!("Player-{index:03}") },
        "score": 1200.5 + index as f64 * 17.25,
        "kills": 40 + index,
        "deaths": 12 + index % 5,
        "captures": 9 - index % 4,
        "neutralizations": 3,
        "pointSeconds": 500,
        "playSeconds": 3600,
        "suicides": 0,
        "teamKills": 0,
        "maxKillstreak": 7,
        "matches": 30 + index,
        "wins": 18 + index % 6,
        "kd": 0.0,
        "mmr": if index % 7 == 6 { serde_json::Value::Null } else {
            serde_json::json!({"rating": 1500.0 + index as f64 * 7.0, "rd": 60.0, "ratedCampaigns": 9, "provisional": index % 5 == 4})
        },
    })
}

fn stats_board_json(total: i64, rows: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({"schemaVersion": 1, "page": 1, "limit": 25, "total": total, "players": rows})
}

fn stats_self_json() -> serde_json::Value {
    serde_json::json!({
        "schemaVersion": 1,
        "profile": {"uuid": STATS_UUID, "name": "CaptureOperator", "lastSeen": 1761300000000i64},
        "totals": {
            "uuid": STATS_UUID, "name": "CaptureOperator", "score": 2450.75,
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
            {"eventId": "m4", "kind": "match", "mode": "deathmatch", "endedAt": 1761289200000i64, "reason": "completed", "winner": "", "winnerUuid": STATS_UUID, "team": "", "score": 20, "kills": 20, "deaths": 6},
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

fn stats_seasons_json() -> serde_json::Value {
    serde_json::json!({
        "current": "2026",
        "seasons": [
            {"id": "2026", "name": "Сезон 2026", "startsAt": 1767225600000i64, "endsAt": null},
            {"id": "2025", "name": "Сезон 2025", "startsAt": 1735689600000i64, "endsAt": 1767225600000i64}
        ]
    })
}

pub fn apply_stats(app: &AppWindow, scenario: &str) -> Result<(), String> {
    if !STATES.contains(&"stats") || !SCENARIOS.contains(&scenario) {
        return Err(format!("Unknown state/scenario: stats/{scenario}"));
    }
    app.set_show_main(false);
    app.set_active(2);
    app.set_username("CaptureOperator".into());
    app.set_role("ИГРОК".into());
    app.set_update_label("Доступна 1.0.20".into());
    crate::stats::apply_filter_options(app);
    let defaults = crate::stats::StatsFilters::default();
    app.set_stats_mode(defaults.mode.into());
    app.set_stats_period(defaults.period.into());
    app.set_stats_sort(defaults.sort.into());
    app.set_stats_self_uuid(STATS_UUID.into());
    crate::stats::apply_seasons(app, &stats_seasons_json());
    let error_outcome = || crate::stats::StatsOutcome {
        data: None,
        saved_at: None,
        stale: false,
        error: Some("Capture fixture: stats unavailable".to_owned()),
    };
    match scenario {
        "loading" => {
            app.set_stats_board_state(0);
            app.set_stats_self_state(0);
        }
        "error" => {
            let outcome = error_outcome();
            crate::stats::apply_board(app, &outcome, STATS_UUID, 1);
            crate::stats::apply_self(app, &outcome, STATS_UUID);
        }
        "empty" => {
            crate::stats::apply_board(
                app,
                &crate::stats::StatsOutcome {
                    data: Some(stats_board_json(0, vec![])),
                    saved_at: Some(1_761_300_000_000),
                    stale: false,
                    error: None,
                },
                STATS_UUID,
                1,
            );
            // Fresh 404, no cached copy: never played.
            crate::stats::apply_self(
                app,
                &crate::stats::StatsOutcome {
                    data: None,
                    saved_at: None,
                    stale: false,
                    error: None,
                },
                STATS_UUID,
            );
        }
        "stale" => {
            let stale_at = 1_761_300_000_000;
            crate::stats::apply_board(
                app,
                &crate::stats::StatsOutcome {
                    data: Some(stats_board_json(
                        2,
                        vec![stats_row(1, false), stats_row(2, true)],
                    )),
                    saved_at: Some(stale_at),
                    stale: true,
                    error: Some("Capture fixture: stats unavailable".to_owned()),
                },
                STATS_UUID,
                1,
            );
            crate::stats::apply_self(
                app,
                &crate::stats::StatsOutcome {
                    data: Some(stats_self_json()),
                    saved_at: Some(stale_at),
                    stale: true,
                    error: Some("Capture fixture: stats unavailable".to_owned()),
                },
                STATS_UUID,
            );
        }
        "pager" => {
            app.set_stats_total(103);
            apply_stats_page(app, 2);
            crate::stats::apply_self(
                app,
                &crate::stats::StatsOutcome {
                    data: Some(stats_self_json()),
                    saved_at: Some(1_761_300_000_000),
                    stale: false,
                    error: None,
                },
                STATS_UUID,
            );
        }
        _ => {
            app.set_stats_total(6);
            apply_stats_page(app, 1);
            crate::stats::apply_self(
                app,
                &crate::stats::StatsOutcome {
                    data: Some(stats_self_json()),
                    saved_at: Some(1_761_300_000_000),
                    stale: false,
                    error: None,
                },
                STATS_UUID,
            );
        }
    }
    crate::avatars::apply_fixture_faces(app);
    Ok(())
}

/// Regenerate the board fixture for `page` using the current total.
/// Pager/filter callbacks call this, so Layer 1 drives real mapping.
pub fn apply_stats_page(app: &AppWindow, page: i32) {
    let total = i64::from(app.get_stats_total()).max(0);
    let start = (i64::from(page) - 1) * crate::stats::PAGE_SIZE + 1;
    let end = (start + crate::stats::PAGE_SIZE - 1).min(total);
    let rows: Vec<serde_json::Value> = if end < start {
        vec![]
    } else {
        (start..=end)
            .map(|index| stats_row(index, page == 1 && index == 3))
            .collect()
    };
    crate::stats::apply_board(
        app,
        &crate::stats::StatsOutcome {
            data: Some(stats_board_json(total, rows)),
            saved_at: Some(1_761_300_000_000),
            stale: false,
            error: None,
        },
        STATS_UUID,
        i64::from(page),
    );
    crate::avatars::apply_fixture_faces(app);
}

pub fn find_option(options: &slint::ModelRc<slint::SharedString>, search: &str) -> i32 {
    use slint::Model;
    options
        .iter()
        .position(|option| option.to_lowercase().starts_with(&search.to_lowercase()))
        .map(|index| index as i32)
        .unwrap_or(-1)
}

pub const UPDATE_SCENARIOS: &[&str] = &[
    "default",
    "checking",
    "verifying",
    "complete",
    "uptodate",
    "error",
    "running",
];

/// Pack-sync screen on nav index 1; main fixtures stay loaded so the e2e
/// walk back to main still screenshots a populated screen.
pub fn apply_update(app: &AppWindow, scenario: &str) -> Result<(), String> {
    if !UPDATE_SCENARIOS.contains(&scenario) {
        return Err(format!("Unknown update scenario: {scenario}"));
    }
    apply_main(app, "main", "default")?;
    app.set_active(1);
    let mut data = crate::UpdateData {
        operation: "BLOCKFIELD".into(),
        patch: "/ ПАТЧ 2.75".into(),
        description:
            "Лаунчер скачает обновления и проверит файлы игры. Дождитесь завершения перед запуском."
                .into(),
        mirror_name: "mirror.blockfield.pro".into(),
        game_phase: "idle".into(),
        ..Default::default()
    };
    match scenario {
        "checking" => {
            data.phase = "checking".into();
            data.step0 = 1;
            data.log = crate::update::checking_log();
        }
        "verifying" => {
            data.phase = "verifying".into();
            data.progress = 100.0;
            data.downloaded = "1.2 ГБ".into();
            data.total = "1.2 ГБ".into();
            data.step0 = 2;
            data.step1 = 2;
            data.step2 = 1;
        }
        "complete" => {
            data.phase = "complete".into();
            data.progress = 100.0;
            data.downloaded = "1.2 ГБ".into();
            data.total = "1.2 ГБ".into();
            data.mirror_status = "mirror.blockfield.pro".into();
            data.step0 = 2;
            data.step1 = 2;
            data.step2 = 2;
        }
        "uptodate" => {
            data.phase = "uptodate".into();
            data.progress = 100.0;
            data.downloaded = "1.2 ГБ".into();
            data.total = "1.2 ГБ".into();
            data.step0 = 2;
            data.step1 = 2;
            data.step2 = 2;
        }
        "error" => {
            data.phase = "error".into();
            data.progress = 37.0;
            data.error = "Не удалось загрузить обновление".into();
            data.step0 = 2;
            data.step1 = 1;
            data.log = crate::update::downloading_log();
        }
        "running" => {
            data.phase = "downloading".into();
            data.game_phase = "running".into();
            data.game_label = "В ИГРЕ".into();
            data.game_sub = "Игра запущена".into();
        }
        _ => {
            data.phase = "downloading".into();
            data.status_message = "ЗАГРУЗКА МОДПАКА".into();
            data.can_cancel = true;
            data.progress = 42.5;
            data.file_index = 18;
            data.file_count = 42;
            data.current_file = "mods/blockfield-content.jar".into();
            data.downloaded = "512 МБ".into();
            data.total = "1.2 ГБ".into();
            data.remaining = "24".into();
            data.speed = "8.4 МБ/с".into();
            data.step0 = 2;
            data.step1 = 1;
            data.log = crate::update::downloading_log();
        }
    }
    app.set_update_data(data);
    Ok(())
}

pub const LAUNCHER_UPDATE_SCENARIOS: &[&str] = &[
    "idle",
    "current",
    "available",
    "downloading",
    "installed",
    "error",
];

/// Launcher self-update panel (settings launcher tab).
pub fn apply_launcher_update(app: &AppWindow, scenario: &str) -> Result<(), String> {
    if !LAUNCHER_UPDATE_SCENARIOS.contains(&scenario) {
        return Err(format!("Unknown launcher-update scenario: {scenario}"));
    }
    let mut data = crate::LauncherUpdateData {
        current_version: "1.0.19".into(),
        update_version: "1.0.20".into(),
        update_body: "Версия 1.0.20: ускорена проверка файлов, исправлены мелкие ошибки.".into(),
        checked_at: "06.11.2026, 11:00:00".into(),
        ..Default::default()
    };
    match scenario {
        "current" => {
            data.phase = "current".into();
            data.update_version = "".into();
            data.update_body = "".into();
        }
        "available" => {
            data.phase = "available".into();
        }
        "downloading" => {
            data.phase = "downloading".into();
            data.downloaded = "12 МБ".into();
            data.total = "21.4 МБ".into();
            data.percent = 56;
            data.has_total = true;
        }
        "installed" => {
            data.phase = "installed".into();
            data.installed = true;
        }
        "error" => {
            data.phase = "error".into();
            data.error = "Не удалось скачать обновление".into();
        }
        _ => {
            data.phase = "idle".into();
        }
    }
    app.set_launcher_update_data(data);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn select_typeahead_handles_cyrillic_and_prefixes() {
        let options = ModelRc::new(VecModel::from(vec!["Игра".into(), "Мастерская".into()]));
        assert_eq!(find_option(&options, "ма"), 1);
        assert_eq!(find_option(&options, "ИГ"), 0);
        assert_eq!(find_option(&options, "нет"), -1);
    }
}
