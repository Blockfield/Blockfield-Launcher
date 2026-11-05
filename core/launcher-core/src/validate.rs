//! Small validators, ported from `src/lib/ram.ts`, `src/lib/username.ts` and
//! `src/lib/game-directory.ts`. Proven equal by `tests/parity.rs`.

/// `ram.ts:1` — whole GiB steps the UI may offer, capped at 32.
pub fn ram_limit_gb(max_ram_mb: f64) -> f64 {
    // JS Math.min/Math.max propagate NaN; Rust f64::min/max do not, so keep it.
    if max_ram_mb.is_nan() {
        return f64::NAN;
    }
    0f64.max(32f64.min((max_ram_mb / 1024.0).floor()))
}

/// `ram.ts:5` — clamp to `[2, max]`, non-finite input falls back to 2.
/// Nesting matches the TS (`Math.min(max, Math.max(2, …))`).
pub fn clamp_ram_gb(value: f64, max: f64) -> f64 {
    // `Number.isFinite(value) ? Math.round(value) : 2`: ±Infinity falls back to 2.
    let rounded = if value.is_finite() {
        crate::fmt::js_round(value)
    } else {
        2.0
    };
    rounded.max(2.0).min(max)
}

/// `username.ts:5` — same rule as the backend; reuses `config::valid_username`
/// instead of duplicating the regex (brief: do not duplicate).
pub fn is_valid_username(name: &str) -> bool {
    crate::config::valid_username(name.trim())
}

/// `username.ts:9` hint shown under the nickname field.
pub const USERNAME_HINT: &str = "Ник в Minecraft (3-16 букв, цифр или _).";

/// `game-directory.ts:4` — keep a `Blockfield` folder, else append one.
/// Join mirrors the Tauri reference (`@tauri-apps/api/path` goes through the
/// backend `plugin:path|join`, tauri 2.12.1 `src/path/plugin.rs`): append the
/// platform separator unless empty/trailing-sep, then normalize separators
/// (`PathBuf::push` alone keeps `/` on Windows, which Tauri never emits).
pub fn selected_game_directory(parent: &str) -> String {
    let name = parent
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("");
    if name.to_lowercase() == "blockfield" {
        return parent.to_owned();
    }
    let mut out = parent.to_owned();
    if !parent.is_empty() && !parent.ends_with('/') && !parent.ends_with('\\') {
        out.push(std::path::MAIN_SEPARATOR);
    }
    out.push_str("Blockfield");
    if std::path::MAIN_SEPARATOR == '\\' {
        out.replace('/', "\\")
    } else {
        out
    }
}
