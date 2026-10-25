//! Stats ("ТОП") screen backend: filter state, query and cache-key building
//! identical to `src/lib/stats.ts`, a stale-while-error file cache in the app
//! data dir, live loading through `launcher-app status_ops`, and fixture
//! states for e2e. Live loads run on the worker runtime, never on the Slint
//! thread; the tree only renders pre-formatted strings.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use launcher_app::{LauncherAppState, ops::pack_check, status_ops};
use launcher_core::fmt;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};
use tokio::runtime::Handle;

use crate::{AppWindow, StatsFilter, StatsMatch, StatsRating, StatsRow};

pub const PAGE_SIZE: i64 = 25;
pub const RATING_SHOWN: usize = 5;
pub const RECENT_MATCHES_SHOWN: usize = 4;

/// Cache file layout, mirroring the localStorage values of `stats.ts`
/// (`{savedAt, data}`) but stored in the app data dir:
/// `<app_data_dir>/stats-cache/<sanitized key>.json`.
pub fn cache_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("stats-cache")
}

/// `bf.stats.<kind>[.<uuid>][?<sorted query>]` — byte-identical scheme to
/// `stats.ts cacheKey` (localStorage and file caches stay disjoint because
/// the file name is sanitized, see below).
pub fn cache_key(kind: &str, uuid: Option<&str>, query: &BTreeMap<String, String>) -> String {
    let params: Vec<String> = query
        .iter()
        .map(|(key, value)| format!("{}={}", url_encode(key), url_encode(value)))
        .collect();
    format!(
        "bf.stats.{kind}{}{}",
        uuid.map_or(String::new(), |id| format!(".{id}")),
        if params.is_empty() {
            String::new()
        } else {
            format!("?{}", params.join("&"))
        }
    )
}

fn url_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn sanitize(key: &str) -> String {
    key.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn cache_path(dir: &Path, key: &str) -> PathBuf {
    dir.join(format!("{}.json", sanitize(key)))
}

pub fn read_cache(dir: &Path, key: &str) -> Option<(i64, serde_json::Value)> {
    let raw = std::fs::read(cache_path(dir, key)).ok()?;
    let parsed: serde_json::Value = serde_json::from_slice(&raw).ok()?;
    let saved_at = parsed.get("savedAt")?.as_i64()?;
    Some((saved_at, parsed.get("data")?.clone()))
}

pub fn write_cache(dir: &Path, key: &str, saved_at_ms: i64, data: &serde_json::Value) {
    let _ = std::fs::create_dir_all(dir);
    let body = serde_json::json!({"savedAt": saved_at_ms, "data": data});
    let _ = std::fs::write(cache_path(dir, key), body.to_string());
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Machine local UTC offset in minutes, east-positive (Moscow +180), for
/// `format_stats_time`. `chrono` is already in the lock file (transitive).
pub fn local_offset_minutes() -> i32 {
    chrono::Local::now().offset().local_minus_utc() / 60
}

/// Filter state; defaults mirror `DEFAULT_FILTERS` (`StatsScreen.tsx:59`).
#[derive(Clone, Debug)]
pub struct StatsFilters {
    pub mode: String,
    pub period: String,
    pub sort: String,
    pub page: i64,
}

impl Default for StatsFilters {
    fn default() -> Self {
        Self {
            mode: String::new(),
            period: "all".to_owned(),
            sort: "score".to_owned(),
            page: 1,
        }
    }
}

impl StatsFilters {
    /// `filterQuery` (`StatsScreen.tsx:61-67`): mode + period or season.
    /// `season:<id>` periods become a `season` key, like the TS.
    pub fn base_query(&self) -> BTreeMap<String, String> {
        let mut query = BTreeMap::new();
        if !self.mode.is_empty() {
            query.insert("mode".to_owned(), self.mode.clone());
        }
        if let Some(season) = self.period.strip_prefix("season:") {
            query.insert("season".to_owned(), season.to_owned());
        } else {
            query.insert("period".to_owned(), self.period.clone());
        }
        query
    }

    /// Leaderboard query: base + sort/limit/page (`StatsScreen.tsx:87`).
    /// The player request uses only the base query (`StatsScreen.tsx:89-91`).
    pub fn query(&self) -> BTreeMap<String, String> {
        let mut query = self.base_query();
        query.insert("sort".to_owned(), self.sort.clone());
        query.insert("limit".to_owned(), PAGE_SIZE.to_string());
        query.insert("page".to_owned(), self.page.to_string());
        query
    }
}

pub fn page_count(total: i64, limit: i64) -> i64 {
    (total + limit - 1).max(1) / limit.max(1)
}

/// Outcome labels, mirroring `StatsScreen.tsx:390-411` (blockfield-site
/// reasons map): interrupted matches have no winner; deathmatch tracks a
/// winning player rather than a team.
fn interrupted(reason: &str) -> bool {
    matches!(reason, "abandoned" | "stopped" | "shutdown" | "error")
}

pub fn result_label(
    reason: &str,
    winner: &str,
    team: &str,
    mode: &str,
    winner_uuid: Option<&str>,
    own_uuid: &str,
) -> &'static str {
    if reason == "draw" {
        return "Ничья";
    }
    if interrupted(reason) {
        return "Прерван";
    }
    if reason != "completed" {
        return "—";
    }
    if mode == "deathmatch" {
        return match winner_uuid {
            None => "—",
            Some(id) if id.to_lowercase() == own_uuid.to_lowercase() => "Победа",
            Some(_) => "Поражение",
        };
    }
    if winner == "NONE" || team == "NONE" || winner.is_empty() {
        return "Ничья";
    }
    if winner == team {
        "Победа"
    } else {
        "Поражение"
    }
}

/// 1 = win (green), -1 = loss (red), 0 = neutral (gray).
pub fn outcome_tone(label: &str) -> i32 {
    match label {
        "Победа" => 1,
        "Поражение" => -1,
        _ => 0,
    }
}

pub fn mode_label(mode: &str) -> &str {
    match mode {
        "room" => "Casual",
        "campaign" => "Ranked",
        "deathmatch" => "Deathmatch",
        other => other,
    }
}

#[derive(Clone, Copy)]
pub enum StatsKind {
    Leaderboard,
    Seasons,
    Player,
}

pub struct StatsOutcome {
    pub data: Option<serde_json::Value>,
    pub saved_at: Option<i64>,
    pub stale: bool,
    pub error: Option<String>,
}

/// `loadStats` (`stats.ts:133-160`): fetch, cache on success, fall back to
/// the file cache stale-while-error. `data: None` without error means the API
/// answered 404 (unknown player / no seasons), like the TS.
pub async fn load_stats(
    kind: StatsKind,
    pack_base: &str,
    uuid: &str,
    query: &BTreeMap<String, String>,
    cache: &Path,
) -> StatsOutcome {
    let kind_name = match kind {
        StatsKind::Leaderboard => "leaderboard",
        StatsKind::Seasons => "seasons",
        StatsKind::Player => "player",
    };
    let key = cache_key(
        kind_name,
        matches!(kind, StatsKind::Player).then_some(uuid),
        query,
    );
    let fetched = match kind {
        StatsKind::Leaderboard => status_ops::stats_leaderboard(pack_base, query).await,
        StatsKind::Seasons => status_ops::stats_seasons(pack_base).await,
        StatsKind::Player => status_ops::stats_player(pack_base, uuid, query).await,
    };
    match fetched {
        Ok(data) => {
            let saved_at = now_ms();
            write_cache(cache, &key, saved_at, &data.clone().unwrap_or_default());
            StatsOutcome {
                data,
                saved_at: Some(saved_at),
                stale: false,
                error: None,
            }
        }
        Err(error) => match read_cache(cache, &key) {
            // A cached null is the same as no cache (404 was cached).
            Some((saved_at, data)) if !data.is_null() => StatsOutcome {
                data: Some(data),
                saved_at: Some(saved_at),
                stale: true,
                error: Some(error),
            },
            _ => StatsOutcome {
                data: None,
                saved_at: None,
                stale: false,
                error: Some(error),
            },
        },
    }
}

fn stale_label(saved_at: i64) -> String {
    format!(
        "Статистика недоступна · данные на {}",
        fmt::format_stats_time(saved_at, local_offset_minutes())
    )
}

fn get_str(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned()
}

fn get_i64(value: &serde_json::Value, key: &str) -> i64 {
    value.get(key).and_then(|v| v.as_i64()).unwrap_or(0)
}

fn mmr_of(value: &serde_json::Value) -> Option<fmt::StatsMmr> {
    let obj = value.as_object()?;
    Some(fmt::StatsMmr {
        rating: obj.get("rating")?.as_f64()?,
        rd: obj.get("rd").and_then(|v| v.as_f64()).unwrap_or(0.0),
        provisional: obj
            .get("provisional")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    })
}

/// Filter option models (`MODES`/`PERIODS`/`SORTS`, `StatsScreen.tsx:27-45`);
/// seasons append `season:<id>` periods once loaded.
pub fn apply_filter_options(app: &AppWindow) {
    app.set_stats_modes(str_pairs(&[
        ("", "Все"),
        ("room", "Casual"),
        ("campaign", "Ranked"),
        ("deathmatch", "Deathmatch"),
    ]));
    app.set_stats_periods(str_pairs(&[
        ("all", "Всё время"),
        ("week", "7 дней"),
        ("month", "30 дней"),
    ]));
    app.set_stats_sorts(str_pairs(&[
        ("score", "Очки"),
        ("rating", "MMR"),
        ("kd", "K/D"),
        ("captures", "Захваты"),
    ]));
}

fn str_pairs(pairs: &[(&str, &str)]) -> ModelRc<StatsFilter> {
    ModelRc::new(VecModel::from(
        pairs
            .iter()
            .map(|(id, label)| StatsFilter {
                id: (*id).into(),
                label: (*label).into(),
            })
            .collect::<Vec<_>>(),
    ))
}

pub fn apply_seasons(app: &AppWindow, seasons: &serde_json::Value) {
    let mut periods = vec![
        ("all".to_owned(), "Всё время".to_owned()),
        ("week".to_owned(), "7 дней".to_owned()),
        ("month".to_owned(), "30 дней".to_owned()),
    ];
    if let Some(list) = seasons.get("seasons").and_then(|v| v.as_array()) {
        for season in list {
            let id = get_str(season, "id");
            if id.is_empty() {
                continue;
            }
            periods.push((format!("season:{id}"), get_str(season, "name")));
        }
    }
    app.set_stats_periods(ModelRc::new(VecModel::from(
        periods
            .into_iter()
            .map(|(id, label)| StatsFilter {
                id: id.into(),
                label: label.into(),
            })
            .collect::<Vec<_>>(),
    )));
}

pub fn apply_board(app: &AppWindow, outcome: &StatsOutcome, uuid: &str, page: i64) {
    // Stale banner lives in the header above every board state (StatsScreen.tsx:150-154).
    let stale = outcome.stale;
    app.set_stats_board_stale(stale);
    app.set_stats_board_stale_label(if stale {
        outcome
            .saved_at
            .map_or_else(String::new, stale_label)
            .into()
    } else {
        SharedString::default()
    });
    if outcome.data.is_none() {
        app.set_stats_board_state(1);
        app.set_stats_board_error(outcome.error.clone().unwrap_or_default().into());
        app.set_stats_rows(ModelRc::default());
        return;
    }
    let Some(board) = outcome.data.as_ref() else {
        app.set_stats_board_state(1);
        app.set_stats_board_error(outcome.error.clone().unwrap_or_default().into());
        app.set_stats_rows(ModelRc::default());
        return;
    };
    let players = board
        .get("players")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if players.is_empty() {
        app.set_stats_board_state(2);
        app.set_stats_rows(ModelRc::default());
        return;
    }
    let limit = board
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(PAGE_SIZE)
        .max(1);
    let total = board.get("total").and_then(|v| v.as_i64()).unwrap_or(0);
    let own = uuid.to_lowercase();
    let rows: Vec<StatsRow> = players
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let mmr = row.get("mmr").and_then(mmr_of);
            StatsRow {
                rank: fmt::display_rank(
                    row.get("rank").and_then(|v| v.as_i64()),
                    index as i64,
                    page,
                    limit,
                )
                .to_string()
                .into(),
                name: get_str(row, "name").into(),
                mmr: fmt::format_mmr(mmr.as_ref()).into(),
                score: fmt::format_score(row.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0))
                    .into(),
                kd: fmt::format_kd(get_i64(row, "kills"), get_i64(row, "deaths")).into(),
                captures: get_i64(row, "captures").to_string().into(),
                battles: fmt::format_battles(get_i64(row, "matches"), get_i64(row, "wins")).into(),
                own: get_str(row, "uuid").to_lowercase() == own,
            }
        })
        .collect();
    app.set_stats_rows(ModelRc::new(VecModel::from(rows)));
    app.set_stats_board_state(3);
    app.set_stats_board_error(SharedString::default());
    app.set_stats_page(page as i32);
    app.set_stats_pages(page_count(total, limit) as i32);
    app.set_stats_total(total as i32);
    app.set_stats_has_prev(page > 1);
    app.set_stats_has_next(page * limit < total);
}

pub fn apply_self(app: &AppWindow, outcome: &StatsOutcome, uuid: &str) {
    let failed = |app: &AppWindow, error: &str| {
        app.set_stats_self_state(2);
        app.set_stats_self_error(error.into());
    };
    if let Some(error) = outcome.error.as_deref()
        && outcome.data.is_none()
    {
        failed(app, error);
        return;
    }
    let Some(player) = outcome.data.as_ref() else {
        match outcome.error.as_deref() {
            Some(error) => failed(app, error),
            // 404 with no cached copy: never played (StatsScreen.tsx:287-288).
            None => app.set_stats_self_state(1),
        }
        return;
    };
    let totals = player.get("totals");
    let name: SharedString = player
        .get("profile")
        .map(|p| get_str(p, "name"))
        .unwrap_or_default()
        .into();
    let rank = totals.and_then(|t| t.get("rank")).and_then(|v| v.as_i64());
    app.set_stats_self_name(name);
    app.set_stats_self_rank(rank.map_or("—".to_owned(), |r| format!("#{r}")).into());
    app.set_stats_self_rank_code(rank.map_or(String::new(), |r| format!("#{r}")).into());
    let mmr = player
        .get("mmr")
        .and_then(mmr_of)
        .or_else(|| totals.and_then(mmr_of));
    app.set_stats_self_mmr(fmt::format_mmr_range(mmr.as_ref()).into());
    let (battles, kd, captures) = match totals {
        Some(t) => (
            fmt::format_battles(get_i64(t, "matches"), get_i64(t, "wins")),
            fmt::format_kd(get_i64(t, "kills"), get_i64(t, "deaths")),
            get_i64(t, "captures").to_string(),
        ),
        None => ("—".to_owned(), "—".to_owned(), "—".to_owned()),
    };
    app.set_stats_self_battles(battles.into());
    app.set_stats_self_kd(kd.into());
    app.set_stats_self_captures(captures.into());
    let campaigns = player.get("campaigns");
    let completed = campaigns.map(|c| get_i64(c, "completed")).unwrap_or(0);
    let wins = campaigns
        .and_then(|c| c.get("wins"))
        .and_then(|v| v.as_i64());
    app.set_stats_self_campaigns(
        format!(
            "{completed} / {}",
            wins.map_or("—".to_owned(), |w| w.to_string())
        )
        .into(),
    );
    let offset = local_offset_minutes();
    let mut rating_history: Vec<(i64, &serde_json::Value)> = player
        .get("ratingHistory")
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .map(|e| (e.get("ratedAt").and_then(|v| v.as_i64()).unwrap_or(0), e))
                .collect()
        })
        .unwrap_or_default();
    rating_history.sort_by_key(|(at, _)| -at);
    let rating: Vec<StatsRating> = rating_history
        .into_iter()
        .take(RATING_SHOWN)
        .map(|(at, entry)| {
            let after = entry.get("after").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let before = entry.get("before").and_then(|v| v.as_f64()).unwrap_or(0.0);
            #[allow(clippy::cast_possible_truncation)]
            let delta = (after - before).round() as i64;
            StatsRating {
                when: fmt::format_stats_time(at, offset).into(),
                #[allow(clippy::cast_possible_truncation)]
                after: (after.round() as i64).to_string().into(),
                delta: format!("{}{delta}", if delta >= 0 { "+" } else { "" }).into(),
                up: delta >= 0,
            }
        })
        .collect();
    app.set_stats_rating(ModelRc::new(VecModel::from(rating)));
    let mut history: Vec<(i64, &serde_json::Value)> = player
        .get("history")
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .map(|e| (e.get("endedAt").and_then(|v| v.as_i64()).unwrap_or(0), e))
                .collect()
        })
        .unwrap_or_default();
    history.sort_by_key(|(at, _)| -at);
    let own = uuid.to_lowercase();
    let matches: Vec<StatsMatch> = history
        .into_iter()
        .take(RECENT_MATCHES_SHOWN)
        .map(|(at, entry)| {
            let label = result_label(
                entry.get("reason").and_then(|v| v.as_str()).unwrap_or(""),
                entry.get("winner").and_then(|v| v.as_str()).unwrap_or(""),
                entry.get("team").and_then(|v| v.as_str()).unwrap_or(""),
                entry.get("mode").and_then(|v| v.as_str()).unwrap_or(""),
                entry.get("winnerUuid").and_then(|v| v.as_str()),
                &own,
            );
            let tone = outcome_tone(label);
            StatsMatch {
                when: fmt::format_stats_time(at, offset).into(),
                mode: mode_label(entry.get("mode").and_then(|v| v.as_str()).unwrap_or("")).into(),
                outcome: label.into(),
                good: tone == 1,
                bad: tone == -1,
                kd: fmt::format_kd(get_i64(entry, "kills"), get_i64(entry, "deaths")).into(),
            }
        })
        .collect();
    app.set_stats_matches(ModelRc::new(VecModel::from(matches)));
    app.set_stats_self_state(3);
    app.set_stats_self_error(SharedString::default());
    let stale = outcome.stale;
    app.set_stats_self_stale(stale);
    app.set_stats_self_stale_label(if stale {
        outcome
            .saved_at
            .map_or_else(String::new, stale_label)
            .into()
    } else {
        SharedString::default()
    });
}

/// Fixture-mode callbacks: pager and filters mutate the fixture state
/// synchronously (deterministic for Layer 1); refresh re-applies fixtures.
pub fn wire_fixtures(app: &AppWindow) {
    let weak = app.as_weak();
    app.on_stats_page_delta(move |delta| {
        if let Some(app) = weak.upgrade() {
            let page = (app.get_stats_page() + delta)
                .max(1)
                .min(app.get_stats_pages().max(1));
            if page != app.get_stats_page() {
                app.set_stats_page(page);
                crate::fixtures::apply_stats_page(&app, page);
            }
        }
    });
    for (callback, property) in [("mode", 0), ("period", 1), ("sort", 2)] {
        let weak = app.as_weak();
        let set = move |app: &AppWindow, value: SharedString| {
            // Filter change resets to page 1, like StatsScreen.tsx:105.
            match property {
                0 => app.set_stats_mode(value),
                1 => app.set_stats_period(value),
                _ => app.set_stats_sort(value),
            }
            app.set_stats_page(1);
            crate::fixtures::apply_stats_page(&app, 1);
        };
        match callback {
            "mode" => app.on_stats_set_mode(move |v| {
                if let Some(app) = weak.upgrade() {
                    set(&app, v);
                }
            }),
            "period" => app.on_stats_set_period(move |v| {
                if let Some(app) = weak.upgrade() {
                    set(&app, v);
                }
            }),
            _ => app.on_stats_set_sort(move |v| {
                if let Some(app) = weak.upgrade() {
                    set(&app, v);
                }
            }),
        }
    }
    let weak = app.as_weak();
    app.on_stats_refresh(move || {
        if let Some(app) = weak.upgrade() {
            crate::fixtures::apply_stats_page(&app, app.get_stats_page());
        }
    });
    app.on_stats_open_site(|| eprintln!("POC stats: open site is a stub (no backend)"));
    app.on_stats_shown(|| {});
}

/// Live-mode wiring: every callback spawns on the worker runtime and posts
/// back through the event loop; the Slint thread never blocks on network.
pub fn wire_live(app: &AppWindow, handle: Handle, state: Arc<LauncherAppState>) {
    let site_handle = handle.clone();
    let reload = {
        let weak = app.as_weak();
        Arc::new(move |app: &AppWindow| {
            request_reload(&weak, &handle, &state, app);
        })
    };
    let reload_shown = reload.clone();
    let weak_shown = app.as_weak();
    app.on_stats_shown(move || {
        if let Some(app) = weak_shown.upgrade() {
            reload_shown(&app);
        }
    });
    for kind in ["mode", "period", "sort"] {
        let weak = app.as_weak();
        let reload = reload.clone();
        let set_filter = move |app: &AppWindow, value: SharedString| {
            match kind {
                "mode" => app.set_stats_mode(value),
                "period" => app.set_stats_period(value),
                _ => app.set_stats_sort(value),
            }
            // Any filter change resets to page 1 (StatsScreen.tsx:105).
            app.set_stats_page(1);
            reload(app);
        };
        match kind {
            "mode" => app.on_stats_set_mode(move |v| {
                if let Some(app) = weak.upgrade() {
                    set_filter(&app, v);
                }
            }),
            "period" => app.on_stats_set_period(move |v| {
                if let Some(app) = weak.upgrade() {
                    set_filter(&app, v);
                }
            }),
            _ => app.on_stats_set_sort(move |v| {
                if let Some(app) = weak.upgrade() {
                    set_filter(&app, v);
                }
            }),
        }
    }
    let weak_page = app.as_weak();
    let reload_page = reload.clone();
    app.on_stats_page_delta(move |delta| {
        if let Some(app) = weak_page.upgrade() {
            let page = (app.get_stats_page() + delta)
                .max(1)
                .min(app.get_stats_pages().max(1));
            if page != app.get_stats_page() {
                app.set_stats_page(page);
                reload_page(&app);
            }
        }
    });
    let weak_refresh = app.as_weak();
    let reload_refresh = reload.clone();
    app.on_stats_refresh(move || {
        if let Some(app) = weak_refresh.upgrade() {
            reload_refresh(&app);
        }
    });
    let weak_site = app.as_weak();
    app.on_stats_open_site(move || {
        let Some(app) = weak_site.upgrade() else {
            return;
        };
        let uuid = app.get_stats_self_uuid();
        if uuid.is_empty() {
            return;
        }
        let uuid = uuid.to_string().to_lowercase();
        site_handle.spawn(async move {
            let link = match pack_check::pack_base_url() {
                Ok(base) => format!("{base}/players/{uuid}"),
                Err(_) => return,
            };
            let _ = tokio::task::spawn_blocking(move || status_ops::open_url(&link)).await;
        });
    });
}

fn request_reload(
    weak: &Weak<AppWindow>,
    handle: &Handle,
    state: &Arc<LauncherAppState>,
    app: &AppWindow,
) {
    let filters = StatsFilters {
        mode: app.get_stats_mode().to_string(),
        period: app.get_stats_period().to_string(),
        sort: app.get_stats_sort().to_string(),
        page: i64::from(app.get_stats_page()),
    };
    let weak = weak.clone();
    let handle = handle.clone();
    let state = state.clone();
    app.set_stats_board_state(0);
    app.set_stats_self_state(0);
    handle.spawn(async move {
        let config = state.config.read().await.clone();
        let uuid = config.player_uuid.to_lowercase();
        let cache = cache_dir(&state.app_data_dir);
        let board_query = filters.query();
        let self_query = filters.base_query();
        let (board, seasons, player) = tokio::join!(
            async {
                match pack_check::pack_base_url() {
                    Ok(base) => {
                        load_stats(StatsKind::Leaderboard, &base, &uuid, &board_query, &cache).await
                    }
                    Err(error) => StatsOutcome {
                        data: None,
                        saved_at: None,
                        stale: false,
                        error: Some(error),
                    },
                }
            },
            async {
                match pack_check::pack_base_url() {
                    Ok(base) => {
                        load_stats(StatsKind::Seasons, &base, &uuid, &BTreeMap::new(), &cache).await
                    }
                    Err(error) => StatsOutcome {
                        data: None,
                        saved_at: None,
                        stale: false,
                        error: Some(error),
                    },
                }
            },
            async {
                if uuid.is_empty() {
                    return StatsOutcome {
                        data: None,
                        saved_at: None,
                        stale: false,
                        error: None,
                    };
                }
                match pack_check::pack_base_url() {
                    Ok(base) => {
                        load_stats(StatsKind::Player, &base, &uuid, &self_query, &cache).await
                    }
                    Err(error) => StatsOutcome {
                        data: None,
                        saved_at: None,
                        stale: false,
                        error: Some(error),
                    },
                }
            }
        );
        let page = filters.page;
        let _ = weak.upgrade_in_event_loop(move |app| {
            if let Some(data) = seasons.data.as_ref() {
                apply_seasons(&app, data);
            }
            apply_board(&app, &board, &uuid, page);
            apply_self(&app, &player, &uuid);
            // The tree reads the uuid for the site link.
            app.set_stats_self_uuid(uuid.into());
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn cache_key_matches_stats_ts_scheme() {
        // stats.ts: `bf.stats.leaderboard?limit=25&page=1&period=all&sort=score`
        // (URLSearchParams sorts keys).
        let key = cache_key(
            "leaderboard",
            None,
            &query(&[
                ("sort", "score"),
                ("limit", "25"),
                ("page", "1"),
                ("period", "all"),
            ]),
        );
        assert_eq!(
            key,
            "bf.stats.leaderboard?limit=25&page=1&period=all&sort=score"
        );
        let key = cache_key("player", Some("abc"), &query(&[("period", "week")]));
        assert_eq!(key, "bf.stats.player.abc?period=week");
        assert_eq!(cache_key("seasons", None, &query(&[])), "bf.stats.seasons");
    }

    #[test]
    fn cache_path_sanitizes_query_characters() {
        let path = cache_path(Path::new("/data"), "bf.stats.player.abc?period=week&x=a/b");
        assert_eq!(
            path,
            PathBuf::from("/data/bf.stats.player.abc_period_week_x_a_b.json")
        );
    }

    #[test]
    fn filter_query_mirrors_ts_filter_query() {
        let filters = StatsFilters::default();
        assert_eq!(
            filters.query(),
            query(&[
                ("limit", "25"),
                ("page", "1"),
                ("period", "all"),
                ("sort", "score")
            ])
        );
        let filters = StatsFilters {
            mode: "room".to_owned(),
            period: "season:2026".to_owned(),
            sort: "kd".to_owned(),
            page: 3,
        };
        assert_eq!(
            filters.base_query(),
            query(&[("mode", "room"), ("season", "2026")])
        );
        assert_eq!(
            filters.query(),
            query(&[
                ("limit", "25"),
                ("mode", "room"),
                ("page", "3"),
                ("season", "2026"),
                ("sort", "kd")
            ])
        );
    }

    #[test]
    fn outcome_labels_mirror_ts_result_label() {
        assert_eq!(result_label("draw", "A", "A", "room", None, "x"), "Ничья");
        assert_eq!(
            result_label("abandoned", "A", "B", "room", None, "x"),
            "Прерван"
        );
        assert_eq!(
            result_label("shutdown", "A", "B", "room", None, "x"),
            "Прерван"
        );
        assert_eq!(result_label("pending", "A", "B", "room", None, "x"), "—");
        assert_eq!(
            result_label("completed", "", "A", "room", None, "x"),
            "Ничья"
        );
        assert_eq!(
            result_label("completed", "NONE", "A", "room", None, "x"),
            "Ничья"
        );
        assert_eq!(
            result_label("completed", "A", "A", "room", None, "x"),
            "Победа"
        );
        assert_eq!(
            result_label("completed", "A", "B", "room", None, "x"),
            "Поражение"
        );
        assert_eq!(
            result_label("completed", "A", "A", "deathmatch", None, "x"),
            "—"
        );
        assert_eq!(
            result_label("completed", "A", "A", "deathmatch", Some("Own"), "own"),
            "Победа"
        );
        assert_eq!(
            result_label("completed", "A", "A", "deathmatch", Some("other"), "own"),
            "Поражение"
        );
        assert_eq!(outcome_tone("Победа"), 1);
        assert_eq!(outcome_tone("Поражение"), -1);
        assert_eq!(outcome_tone("Ничья"), 0);
        assert_eq!(mode_label("campaign"), "Ranked");
        assert_eq!(mode_label("custom"), "custom");
    }

    #[test]
    fn pager_clamps_and_counts_pages() {
        assert_eq!(page_count(0, 25), 1);
        assert_eq!(page_count(103, 25), 5);
        assert_eq!(page_count(25, 25), 1);
    }
}
