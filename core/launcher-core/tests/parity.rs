//! Snapshot parity against the TS: every case in `fixtures/*.json` (recorded
//! from the real TS by `fixtures/record.test.ts`) must match the Rust port.
//! See unit u31-core-fmt; `cargo test -p launcher-core` runs this.

use launcher_core::errors::{friendly_error, ErrorContext};
use launcher_core::fmt::{
    display_rank, format_battles, format_bytes, format_kd, format_mirror, format_mmr,
    format_mmr_range, format_score, format_speed, game_state_label, StatsMmr,
};
use launcher_core::roles::has_workshop_access;
use launcher_core::validate::{
    clamp_ram_gb, is_valid_username, ram_limit_gb, selected_game_directory, USERNAME_HINT,
};
use serde_json::Value;

fn load(name: &str) -> Vec<Value> {
    let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing {path}"));
    serde_json::from_str::<Value>(&text).unwrap()["cases"]
        .as_array()
        .unwrap()
        .clone()
}

/// JSON numbers or the strings "NaN"/"Infinity"/"-Infinity" the recorder emits.
fn f64_of(value: &Value) -> f64 {
    match value {
        Value::Number(n) => n.as_f64().unwrap(),
        Value::String(s) => match s.as_str() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            _ => panic!("unexpected numeric string {s}"),
        },
        _ => panic!("expected number, got {value}"),
    }
}

fn num_eq(actual: f64, expected: &Value) {
    if expected.is_string() {
        assert!(actual.is_nan(), "expected NaN, got {actual}");
    } else {
        assert_eq!(actual, f64_of(expected));
    }
}

#[test]
fn fmt_update_parity() {
    for case in load("fmt-update") {
        let expected = case["expected"].as_str().unwrap();
        let actual = match case["fn"].as_str().unwrap() {
            "format_speed" => format_speed(f64_of(&case["input"])),
            "format_bytes" => format_bytes(f64_of(&case["input"])),
            "format_mirror" => format_mirror(case["input"].as_str().unwrap()),
            other => panic!("unknown fn {other}"),
        };
        assert_eq!(actual, expected, "case {case}");
    }
}

#[test]
fn fmt_stats_parity() {
    for case in load("fmt-stats") {
        if case["fn"] == "display_rank" {
            continue;
        }
        let expected = case["expected"].as_str().unwrap();
        let actual = match case["fn"].as_str().unwrap() {
            "format_mmr" | "format_mmr_range" => {
                // A missing key means the TS input was `undefined`, same branch as null.
                let mmr = case.get("input").and_then(|v| {
                    if v.is_null() {
                        None
                    } else {
                        Some(StatsMmr {
                            rating: f64_of(&v["rating"]),
                            rd: f64_of(&v["rd"]),
                            provisional: v["provisional"].as_bool().unwrap(),
                        })
                    }
                });
                if case["fn"] == "format_mmr" {
                    format_mmr(mmr.as_ref())
                } else {
                    format_mmr_range(mmr.as_ref())
                }
            }
            "format_kd" => format_kd(
                case["input"]["kills"].as_i64().unwrap(),
                case["input"]["deaths"].as_i64().unwrap(),
            ),
            "format_score" => format_score(f64_of(&case["input"])),
            "format_battles" => format_battles(
                case["input"]["matches"].as_i64().unwrap(),
                case["input"]["wins"].as_i64().unwrap(),
            ),
            other => panic!("unknown fn {other}"),
        };
        assert_eq!(actual, expected, "case {case}");
    }
    for case in load("fmt-stats") {
        if case["fn"] != "display_rank" {
            continue;
        }
        assert_eq!(
            display_rank(
                case["input"]["row"].get("rank").and_then(Value::as_i64),
                case["input"]["index"].as_i64().unwrap(),
                case["input"]["page"].as_i64().unwrap(),
                case["input"]["limit"].as_i64().unwrap(),
            ),
            case["expected"].as_i64().unwrap(),
            "case {case}"
        );
    }
}

#[test]
fn game_state_label_parity() {
    for case in load("fmt-game-state") {
        let label = game_state_label(case["input"].as_str().unwrap());
        assert_eq!(
            label,
            Some(case["expected"].as_str().unwrap()),
            "case {case}"
        );
    }
    assert_eq!(game_state_label("bogus"), None);
}

#[test]
fn roles_parity() {
    for case in load("roles") {
        let input = case.get("input").and_then(Value::as_str);
        assert_eq!(
            has_workshop_access(input),
            case["expected"].as_bool().unwrap(),
            "case {case}"
        );
    }
}

#[test]
fn errors_parity() {
    for case in load("errors") {
        let context = ErrorContext::from_slug(case["input"]["context"].as_str().unwrap()).unwrap();
        let out = friendly_error(case["input"]["raw"].as_str().unwrap(), context);
        assert_eq!(
            out.message,
            case["expected"].as_str().unwrap(),
            "case {case}"
        );
        assert_eq!(
            out.raw,
            case["input"]["raw"].as_str().unwrap(),
            "raw {case}"
        );
    }
}

#[test]
fn validate_parity() {
    for case in load("validate") {
        match case["fn"].as_str().unwrap() {
            "ram_limit_gb" => num_eq(
                ram_limit_gb(f64_of(&case["input"]["value"])),
                &case["expected"],
            ),
            "clamp_ram_gb" => num_eq(
                clamp_ram_gb(
                    f64_of(&case["input"]["value"]),
                    f64_of(&case["input"]["max"]),
                ),
                &case["expected"],
            ),
            "is_valid_username" => assert_eq!(
                is_valid_username(case["input"].as_str().unwrap()),
                case["expected"].as_bool().unwrap(),
                "case {case}"
            ),
            "selected_game_directory" => {
                let expected = case["expected"]
                    .as_str()
                    .unwrap()
                    .replace('/', std::path::MAIN_SEPARATOR_STR);
                assert_eq!(
                    selected_game_directory(case["input"].as_str().unwrap()),
                    expected,
                    "case {case}"
                );
            }
            other => panic!("unknown fn {other}"),
        }
    }
    assert_eq!(USERNAME_HINT, "Ник в Minecraft (3-16 букв, цифр или _).");
}
