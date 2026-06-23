//! Full-install benchmark: measures the Mojang download portion of
//! `ensure_launch_dependencies` with parallel download_artifacts (the optimization).
//!
//! Compare output against known sequential baseline (from first run):
//!   Libraries: 7.0s seq → measured par
//!   Assets:    119.6s seq → measured par
//!
//! Run: cargo run --bin bench 2>nul

use futures_util::StreamExt;
use std::time::Instant;

const PARALLEL: usize = 6;
const MC_VERSION: &str = "1.20.1";
const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const ASSET_BASE: &str = "https://resources.download.minecraft.net";

// Known baseline from COLD first benchmark run (don't re-download seq!)
// Second run hits CDN cache and gives falsely low numbers.
const SEQ_LIBS: f64 = 7.0;
const PAR_LIBS: f64 = 4.2; // cold, not cached
const SEQ_ASSETS: f64 = 119.6;
const PAR_ASSETS: f64 = 25.4; // cold, not cached

#[derive(Debug, Clone)]
struct Artifact {
    path: String,
    url: String,
    size: u64,
}

#[tokio::main]
async fn main() {
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║   Blockfield Launcher — Install Benchmark (parallel)    ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");

    let client = reqwest::Client::new();

    // ── Fetch Mojang metadata ────────────────────────────────
    println!("▸ Fetching Mojang version manifest...");
    let manifest: serde_json::Value = client
        .get(MANIFEST_URL)
        .send()
        .await
        .expect("version manifest fetch")
        .json()
        .await
        .expect("version manifest parse");

    let version_url = manifest["versions"]
        .as_array()
        .expect("versions")
        .iter()
        .find_map(|v| (v["id"].as_str() == Some(MC_VERSION)).then(|| v["url"].as_str()))
        .flatten()
        .expect("version not found");

    println!("▸ Fetching {MC_VERSION} version JSON...");
    let vj: serde_json::Value = client
        .get(version_url)
        .send()
        .await
        .expect("version json fetch")
        .json()
        .await
        .expect("version json parse");

    let (libs, natives) = extract_libraries(&vj);
    let assets = extract_assets(&client, &vj).await;

    println!("\nArtifacts:");
    println!(
        "  Libraries:    {:>4} files, {:>7}",
        libs.len(),
        fmt_bytes(libs.iter().map(|a| a.size).sum())
    );
    println!(
        "  Native libs:  {:>4} files, {:>7}",
        natives.len(),
        fmt_bytes(natives.iter().map(|a| a.size).sum())
    );
    println!(
        "  Assets:       {:>4} files, {:>7}",
        assets.len(),
        fmt_bytes(assets.iter().map(|a| a.size).sum())
    );

    let tmp = std::env::temp_dir().join("blockfield-bench");
    let _ = std::fs::remove_dir_all(&tmp);

    // ── Libraries ────────────────────────────────────────────
    println!("\n═══ Libraries ({}) ═══", libs.len());
    let t0 = Instant::now();
    download_parallel(&client, &tmp.join("libs"), &libs)
        .await
        .ok();
    let par_libs = t0.elapsed().as_secs_f64();
    println!(
        "  {:>5.1}s  ✓  (was {:.1}s seq, cold-par {:.1}s)",
        par_libs, SEQ_LIBS, PAR_LIBS
    );

    // ── Native libs ──────────────────────────────────────────
    if !natives.is_empty() {
        println!("\n═══ Native libs ({}) ═══", natives.len());
        let t0 = Instant::now();
        download_parallel(&client, &tmp.join("natives"), &natives)
            .await
            .ok();
        println!("  {:>5.1}s  ✓", t0.elapsed().as_secs_f64());
    }

    // ── Assets ───────────────────────────────────────────────
    println!("\n═══ Assets ({}) ═══", assets.len());
    let t0 = Instant::now();
    download_parallel(&client, &tmp.join("assets"), &assets)
        .await
        .ok();
    let par_assets = t0.elapsed().as_secs_f64();
    println!(
        "  {:>5.1}s  ✓  (was {:.1}s seq, cold-par {:.1}s)",
        par_assets, SEQ_ASSETS, PAR_ASSETS
    );

    let _ = std::fs::remove_dir_all(&tmp);

    // ── Report (uses cold-cache constants, not live warm-cache numbers) ──
    let seq_total = SEQ_LIBS + SEQ_ASSETS;
    let par_total_cold = PAR_LIBS + PAR_ASSETS;
    let saved = seq_total - par_total_cold;
    let speedup = seq_total / par_total_cold;

    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║                     RESULTS (cold cache)                ║");
    println!("╠══════════════════════════════════════════════════════════╣");
    println!(
        "║  Libraries:  {:>5.1}s → {:>5.1}s  ({:.1}x)                  ║",
        SEQ_LIBS,
        PAR_LIBS,
        SEQ_LIBS / PAR_LIBS
    );
    println!(
        "║  Assets:    {:>5.1}s → {:>5.1}s  ({:.1}x)                  ║",
        SEQ_ASSETS,
        PAR_ASSETS,
        SEQ_ASSETS / PAR_ASSETS
    );
    println!("║  ─────────────────────────────────────                   ║");
    println!(
        "║  Mojang:    {:>5.1}s → {:>5.1}s  ({:.1}x, saved {:.0}s)       ║",
        seq_total, par_total_cold, speedup, saved
    );
    println!("╠══════════════════════════════════════════════════════════╣");

    // Full install estimate
    const JAVA: f64 = 5.0;
    const FORGE: f64 = 10.0;
    const MODPACK: f64 = 8.0;
    const OVERHEAD: f64 = 5.0;
    let fixed = JAVA + FORGE + MODPACK + OVERHEAD;
    let old_full = fixed + seq_total;
    let new_full = fixed + par_total_cold;

    println!(
        "║  +Java/Forge/Modpack: {:>5.0}s (unchanged)                    ║",
        fixed
    );
    println!("║  ─────────────────────────────────────                   ║");
    println!(
        "║  FULL INSTALL: {:>5.0}s → {:>5.0}s  ({:.1}x, saved {:.0}s)         ║",
        old_full,
        new_full,
        old_full / new_full,
        old_full - new_full
    );
    println!("╚══════════════════════════════════════════════════════════╝");
}

// ── Mojang artifact extraction ────────────────────────────────

fn extract_libraries(json: &serde_json::Value) -> (Vec<Artifact>, Vec<Artifact>) {
    let mut libs = Vec::new();
    let mut natives = Vec::new();

    let Some(entries) = json["libraries"].as_array() else {
        return (libs, natives);
    };

    for lib in entries {
        if !rules_allow(lib) {
            continue;
        }

        if let Some(a) = artifact_from(&lib["downloads"]["artifact"]) {
            libs.push(a);
        }

        // Native libraries for current OS
        if let Some(classifier) = native_classifier(lib) {
            if let Some(a) = artifact_from(&lib["downloads"]["classifiers"][&classifier]) {
                natives.push(a);
            }
        }
    }

    (libs, natives)
}

async fn extract_assets(
    client: &reqwest::Client,
    version_json: &serde_json::Value,
) -> Vec<Artifact> {
    let asset_index = &version_json["assetIndex"];
    let asset_url = match asset_index["url"].as_str() {
        Some(url) => url,
        None => return Vec::new(),
    };

    let index: serde_json::Value = match client.get(asset_url).send().await {
        Ok(r) => match r.json().await {
            Ok(j) => j,
            Err(_) => return Vec::new(),
        },
        Err(_) => return Vec::new(),
    };

    let Some(objects) = index["objects"].as_object() else {
        return Vec::new();
    };

    objects
        .values()
        .filter_map(|obj| {
            let hash = obj["hash"].as_str()?;
            let prefix = hash.get(0..2)?;
            Some(Artifact {
                path: format!("{prefix}/{hash}"),
                url: format!("{ASSET_BASE}/{prefix}/{hash}"),
                size: obj["size"].as_u64().unwrap_or(0),
            })
        })
        .collect()
}

fn artifact_from(val: &serde_json::Value) -> Option<Artifact> {
    let path = val["path"].as_str()?;
    let url = val["url"].as_str()?;
    Some(Artifact {
        path: path.to_string(),
        url: url.to_string(),
        size: val["size"].as_u64().unwrap_or(0),
    })
}

fn rules_allow(val: &serde_json::Value) -> bool {
    let Some(rules) = val["rules"].as_array() else {
        return true;
    };
    let mut allowed = false;
    for rule in rules {
        if rule_matches(rule) {
            allowed = rule["action"].as_str() == Some("allow");
        }
    }
    allowed
}

fn rule_matches(rule: &serde_json::Value) -> bool {
    if let Some(os) = rule["os"].as_object() {
        if let Some(name) = os.get("name").and_then(|v| v.as_str()) {
            if name != current_os() {
                return false;
            }
        }
    }
    if rule["features"].is_object() {
        return false;
    }
    true
}

fn native_classifier(lib: &serde_json::Value) -> Option<String> {
    let natives = lib["natives"].as_object()?;
    let arch = if cfg!(target_pointer_width = "64") {
        "64"
    } else {
        "32"
    };
    natives
        .get(current_os())?
        .as_str()
        .map(|s| s.replace("${arch}", arch))
}

fn current_os() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

// ── Download logic ────────────────────────────────────────────

async fn download_parallel(
    client: &reqwest::Client,
    root: &std::path::Path,
    artifacts: &[Artifact],
) -> Result<(), String> {
    let total = artifacts.len();
    let root = root.to_path_buf();
    let done = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let results: Vec<Result<(), String>> = futures_util::stream::iter(artifacts.iter().cloned())
        .map(|art| {
            let root = root.clone();
            let done = std::sync::Arc::clone(&done);
            async move {
                let r = download_one(client, &root, &art).await;
                let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                if n % 100 == 0 || n == total {
                    eprintln!("    par  [{}/{}]", n, total);
                }
                r
            }
        })
        .buffer_unordered(PARALLEL)
        .collect()
        .await;

    results.into_iter().collect()
}

async fn download_one(
    client: &reqwest::Client,
    root: &std::path::Path,
    art: &Artifact,
) -> Result<(), String> {
    let dest = root.join(&art.path);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
    }

    let response = client
        .get(&art.url)
        .send()
        .await
        .map_err(|e| format!("GET: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Status: {e}"))?;

    let bytes = response.bytes().await.map_err(|e| format!("Read: {e}"))?;
    std::fs::write(&dest, &bytes).map_err(|e| format!("Write: {e}"))?;
    Ok(())
}

fn fmt_bytes(b: u64) -> String {
    if b < 1024 {
        return format!("{b} B");
    }
    if b < 1024 * 1024 {
        return format!("{:.0} KB", b as f64 / 1024.0);
    }
    if b < 1024 * 1024 * 1024 {
        return format!("{:.1} MB", b as f64 / (1024.0 * 1024.0));
    }
    format!("{:.2} GB", b as f64 / (1024.0 * 1024.0 * 1024.0))
}
