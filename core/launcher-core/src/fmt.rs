//! Pure display formatters ported 1:1 from the frontend TypeScript.
//!
//! Sources (base b4583ca): `UpdateScreen.tsx:43-66` (`formatSpeed`,
//! `formatMirror`, `formatBytes`), `stats.ts:164-180`, `game-state.ts:66`.
//! Proven equal by `tests/parity.rs` over JSON fixtures recorded from the TS.

/// `Math.round`: ties go up (toward +inf). `f64::round` goes away from zero,
/// which differs for negative ties, so spell out the JS semantics.
pub(crate) fn js_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

/// `Number.prototype.toFixed`: round half away from zero on the binary value
/// (verified against Node 26, incl. `"-0"` for negative values rounding to
/// zero), then pad with trailing zeros. Non-finite inputs use the JS spellings.
fn js_to_fixed(value: f64, digits: usize) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value.is_sign_positive() {
            "Infinity".to_owned()
        } else {
            "-Infinity".to_owned()
        };
    }
    // f64 arithmetic matches the JS doubles exactly; UI magnitudes stay far
    // below 2^53 where integer precision would go.
    let factor = 10f64.powi(digits as i32);
    let rounded = ((value.abs() * factor + 0.5).floor() / factor).copysign(value);
    format!("{rounded:.digits$}")
}

/// `String(n)`: shortest round-trip, plain notation below 1e21, `-0` as `"0"`.
fn js_num_string(value: f64) -> String {
    if value == 0.0 {
        return "0".to_owned();
    }
    if value.fract() == 0.0 && value.abs() < 1e21 {
        return format!("{}", value as i128);
    }
    format!("{value}")
}

/// `UpdateScreen.tsx:44` — bytes/sec as `"123 B/s"`, `"1.5 KB/s"`, `"2.4 MB/s"`.
pub fn format_speed(bytes_per_sec: f64) -> String {
    if bytes_per_sec < 1024.0 {
        return format!("{} B/s", js_num_string(js_round(bytes_per_sec)));
    }
    if bytes_per_sec < 1024.0 * 1024.0 {
        return format!("{} KB/s", js_to_fixed(bytes_per_sec / 1024.0, 1));
    }
    format!("{} MB/s", js_to_fixed(bytes_per_sec / (1024.0 * 1024.0), 1))
}

/// `UpdateScreen.tsx:50` — host of a mirror URL, or the input/`"—"` when unset.
pub fn format_mirror(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(parsed) => match parsed.host_str() {
            Some(host) => match parsed.port() {
                Some(port) => format!("{host}:{port}"),
                None => host.to_owned(),
            },
            None => {
                if url.is_empty() {
                    "—".to_owned()
                } else {
                    url.to_owned()
                }
            }
        },
        Err(_) => {
            if url.is_empty() {
                "—".to_owned()
            } else {
                url.to_owned()
            }
        }
    }
}

/// `UpdateScreen.tsx:59` — bytes as `"512 B"`, `"1024 KB"`, `"1.5 MB"`, `"1.50 GB"`.
pub fn format_bytes(bytes: f64) -> String {
    if !bytes.is_finite() || bytes < 0.0 {
        return "—".to_owned();
    }
    if bytes < 1024.0 {
        return format!("{} B", js_num_string(bytes));
    }
    if bytes < 1024.0 * 1024.0 {
        return format!("{} KB", js_to_fixed(bytes / 1024.0, 0));
    }
    if bytes < 1024.0 * 1024.0 * 1024.0 {
        return format!("{} MB", js_to_fixed(bytes / (1024.0 * 1024.0), 1));
    }
    format!("{} GB", js_to_fixed(bytes / (1024.0 * 1024.0 * 1024.0), 2))
}

/// MMR value behind `stats.ts` formatters (`ratedCampaigns` is display-irrelevant).
pub struct StatsMmr {
    pub rating: f64,
    pub rd: f64,
    pub provisional: bool,
}

/// `stats.ts:164` — `"1500"`, `"1500?"`, `"—"`.
pub fn format_mmr(mmr: Option<&StatsMmr>) -> String {
    match mmr {
        Some(mmr) => format!(
            "{}{}",
            js_num_string(js_round(mmr.rating)),
            if mmr.provisional { "?" } else { "" }
        ),
        None => "—".to_owned(),
    }
}

/// `stats.ts:167` — `"1500 ± 100"`, `"—"`.
pub fn format_mmr_range(mmr: Option<&StatsMmr>) -> String {
    match mmr {
        Some(mmr) => format!(
            "{} ± {}",
            format_mmr(Some(mmr)),
            js_num_string(js_round(2.0 * mmr.rd))
        ),
        None => "—".to_owned(),
    }
}

/// `stats.ts:170` — `"5 / 3"`.
pub fn format_kd(kills: i64, deaths: i64) -> String {
    format!("{kills} / {deaths}")
}

/// `stats.ts:172` — score rounded to cents, `"1.23"`, `"100"`.
pub fn format_score(score: f64) -> String {
    js_num_string(js_round(score * 100.0) / 100.0)
}

/// `stats.ts:174` — `"10 / 3"`.
pub fn format_battles(matches: i64, wins: i64) -> String {
    format!("{matches} / {wins}")
}

/// `stats.ts:177` — stored rank or the 1-based position on the page.
pub fn display_rank(rank: Option<i64>, index: i64, page: i64, limit: i64) -> i64 {
    rank.unwrap_or((page - 1) * limit + index + 1)
}

/// `game-state.ts:66` — Russian play-button/phase label. `None` for phases the
/// TS union cannot hold.
pub fn game_state_label(phase: &str) -> Option<&'static str> {
    match phase {
        "running" => Some("В ИГРЕ"),
        "launching" => Some("ЗАПУСК"),
        "finishing" => Some("ЗАВЕРШЕНИЕ"),
        "unknown" => Some("ПРОВЕРКА…"),
        "idle" => Some("ИГРАТЬ"),
        _ => None,
    }
}
