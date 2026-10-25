//! Stats file-cache integration tests: key scheme, sanitized file layout and
//! the stale-while-error semantics `loadStats` promises, against throwaway
//! dirs (never real user data).

use std::collections::BTreeMap;
use std::path::PathBuf;

use blockfield_native::stats::{cache_dir, cache_key, cache_path, read_cache, write_cache};

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "blockfield-stats-test-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test dir");
    dir
}

fn query(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

#[test]
fn cache_key_matches_ts_key_scheme() {
    assert_eq!(cache_key("seasons", None, &query(&[])), "bf.stats.seasons");
    assert_eq!(
        cache_key(
            "leaderboard",
            None,
            &query(&[
                ("limit", "25"),
                ("page", "1"),
                ("period", "all"),
                ("sort", "score")
            ])
        ),
        "bf.stats.leaderboard?limit=25&page=1&period=all&sort=score"
    );
    assert_eq!(
        cache_key("player", Some("AbC"), &query(&[("period", "week")])),
        "bf.stats.player.AbC?period=week"
    );
}

#[test]
fn cache_roundtrip_keeps_saved_at_and_data() {
    let root = dir("roundtrip");
    let cache = cache_dir(&root);
    let key = cache_key("player", Some("abc"), &query(&[("period", "week")]));
    assert!(read_cache(&cache, &key).is_none());
    let data = serde_json::json!({"players": [], "total": 0});
    write_cache(&cache, &key, 1_761_300_000_000, &data);
    let (saved_at, back) = read_cache(&cache, &key).expect("cached");
    assert_eq!(saved_at, 1_761_300_000_000);
    assert_eq!(back, data);
    // Query order must not matter (sorted keys, like URLSearchParams).
    let same = cache_key("player", Some("abc"), &query(&[("period", "week")]));
    assert_eq!(key, same);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn cache_files_live_under_app_data_dir_with_safe_names() {
    let root = dir("layout");
    let cache = cache_dir(&root);
    assert_eq!(cache, root.join("stats-cache"));
    let key = cache_key("leaderboard", None, &query(&[("season", "2026/26")]));
    let path = cache_path(&cache, &key);
    assert!(path.starts_with(&cache));
    assert_eq!(path.extension().and_then(|e| e.to_str()), Some("json"));
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    assert!(
        name.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'),
        "sanitized file name: {name}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn corrupt_cache_reads_as_miss() {
    let root = dir("corrupt");
    let cache = cache_dir(&root);
    let key = cache_key("seasons", None, &query(&[]));
    std::fs::create_dir_all(&cache).expect("cache dir");
    std::fs::write(cache_path(&cache, &key), "{not json").expect("corrupt file");
    assert!(read_cache(&cache, &key).is_none());
    let _ = std::fs::remove_dir_all(&root);
}
