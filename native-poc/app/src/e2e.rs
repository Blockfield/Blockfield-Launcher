use std::{fs, path::PathBuf, rc::Rc, time::Duration};

use image::{ImageBuffer, Rgb};
use skin3d::slint::PreviewHost;
use slint::{ComponentHandle, Timer, TimerMode};

use crate::AppWindow;

/// Layer 2 acceptance walk: the release binary in a real window visits every
/// ported screen, screenshots each state, and compares pixels against
/// per-OS goldens. Any mismatch, missing element, or hang fails the process
/// (non-zero exit) so CI turns red. `--e2e-update-goldens` writes candidates
/// instead of comparing (bootstrap only; never gate on it).
///
/// Determinism: run with `--freeze-motion` (fixed skin clock, no UI motion)
/// and the software renderer (`SLINT_BACKEND=winit-software`, the one
/// renderer all three CI runners support) so reruns are bit-stable apart
/// from text rasterization; tolerance below.
const FRAME_MS: u64 = 250;
const MAX_FRAMES: u32 = 400;
// Same-OS reruns are near-identical (frozen motion); text AA shimmer is the
// only noise. u17 measured cross-OS RMSE 0.01-0.04; same-OS rerun diff stays
// an order of magnitude under these gates (measured in the u18 report).
const MEAN_ABS_GATE: f32 = 2.0;
const MAX_FRAC_COARSE: f32 = 0.002; // pixels with any-channel diff > 8
const COARSE_DELTA: u8 = 8;

pub fn start(
    app: &AppWindow,
    host: &Rc<PreviewHost>,
    out_dir: PathBuf,
    golden_dir: PathBuf,
    update: bool,
) -> std::io::Result<Timer> {
    fs::create_dir_all(&out_dir)?;
    let os = std::env::consts::OS;
    let log_path = out_dir.join("e2e.log");
    let mut log = String::new();
    let push = |log: &mut String, line: String| {
        println!("{line}");
        log.push_str(&line);
        log.push('\n');
    };
    push(
        &mut log,
        format!(
            "E2E os={os} backend={} scale={} update={update}",
            std::env::var("SLINT_BACKEND").unwrap_or_else(|_| "default".into()),
            app.window().scale_factor(),
        ),
    );
    fs::write(&log_path, &log)?;
    let timer = Timer::default();
    let weak = app.as_weak();
    let host = host.clone();
    let start_delivered = host.delivered();
    // Plan depends on the entry state: main-like states walk settings tabs and
    // come back; settings-skin stays and walks tabs from the skin side.
    let from_settings = app.get_active() == 3;
    let mut frame: u32 = 0;
    let mut step: usize = 0;
    let mut tab: i32 = 0;
    let mut shot_count = 0u32;
    let mut failures = 0u32;
    // Minimize probe state.
    let mut probe_delivered = 0u64;
    timer.start(
        TimerMode::Repeated,
        Duration::from_millis(FRAME_MS),
        move || {
            let app = weak.upgrade().expect("e2e window");
            if frame > MAX_FRAMES {
                panic!(
                    "E2E hang: walk did not finish in {}s",
                    MAX_FRAMES * 250 / 1000
                );
            }
            frame += 1;
            if host.failed() {
                panic!("E2E FAIL: skin renderer failed, cannot produce meaningful shots");
            }
            // Skin tab needs a few delivered frames before pixels are meaningful.
            let skin_settled = !(app.get_active() == 3 && app.get_settings_tab() == 3)
                || host.delivered() >= start_delivered + 3;
            if !skin_settled {
                return;
            }
            match step {
                // 0: screenshot entry state.
                0 => {
                    shot_count += 1;
                    failures +=
                        check_shot(&app, &out_dir, &golden_dir, &os_name(), "state", update);
                    step = 1;
                }
                // 1..: walk settings tabs 0..=4, one shot each.
                1 => {
                    if tab <= 4 {
                        app.invoke_settings_tab_changed(tab);
                        crate::skin::sync_visibility(&app, &host);
                        // Next tick screenshots after the tab switch settles.
                        step = 2;
                    } else if from_settings {
                        step = 4;
                    } else {
                        app.invoke_navigate(0);
                        crate::skin::sync_visibility(&app, &host);
                        step = 3;
                    }
                }
                2 => {
                    let name = format!("tab-{tab}");
                    shot_count += 1;
                    failures += check_shot(&app, &out_dir, &golden_dir, &os_name(), &name, update);
                    assert_eq!(app.get_settings_tab(), tab, "tab {tab} stays selected");
                    tab += 1;
                    step = 1;
                }
                // 3: back on main after the walk.
                3 => {
                    assert_eq!(app.get_active(), 0, "walk returns to main");
                    shot_count += 1;
                    failures +=
                        check_shot(&app, &out_dir, &golden_dir, &os_name(), "back-main", update);
                    step = 4;
                }
                // 4: minimize must freeze animation timers (idle self-check logic).
                4 => {
                    probe_delivered = host.delivered();
                    assert!(
                        !app.window().is_minimized(),
                        "window starts restored for the minimize probe"
                    );
                    app.window().set_minimized(true);
                    step = 5;
                }
                5 => {
                    if !frame.is_multiple_of(6) {
                        return; // ~1.5 s minimized before asserting.
                    }
                    if !app.window().is_minimized() {
                        // Headless/CI window managers may refuse minimization; record and move on.
                        let line = "E2E minimize refused by the window manager; probe skipped";
                        println!("{line}");
                        append_log(&log_path, line);
                        step = 6;
                        return;
                    }
                    if app.window().has_active_animations() {
                        failures += 1;
                        let line = "E2E FAIL minimize: animations still active";
                        println!("{line}");
                        append_log(&log_path, line);
                    }
                    if host.delivered() != probe_delivered {
                        failures += 1;
                        let line = "E2E FAIL minimize: skin kept delivering frames";
                        println!("{line}");
                        append_log(&log_path, line);
                    }
                    app.window().set_minimized(false);
                    append_log(&log_path, "E2E minimize probe passed");
                    step = 6;
                }
                // 6: done.
                _ => {
                    let line = format!("E2E shots={shot_count} failures={failures}");
                    println!("{line}");
                    append_log(&log_path, &line);
                    if failures > 0 {
                        panic!("E2E failed with {failures} failure(s)");
                    }
                    println!("PASS e2e walk");
                    slint::quit_event_loop().expect("finish e2e walk");
                }
            }
        },
    );
    Ok(timer)
}

fn os_name() -> String {
    // Match the golden layout tests/golden/<os>/; runner OS values.
    match std::env::consts::OS {
        "windows" => "windows".into(),
        "macos" => "macos".into(),
        _ => "linux".into(),
    }
}

fn append_log(path: &std::path::Path, line: &str) {
    use std::fmt::Write as _;
    let mut cur = fs::read_to_string(path).unwrap_or_default();
    let _ = writeln!(cur, "{line}");
    let _ = fs::write(path, cur);
}

fn snapshot_pixels(app: &AppWindow) -> (u32, u32, Vec<u8>) {
    let pixels = app.window().take_snapshot().expect("renderer snapshot");
    let (w, h) = (pixels.width(), pixels.height());
    let rgb: Vec<u8> = pixels
        .as_slice()
        .iter()
        .flat_map(|p| [p.r, p.g, p.b])
        .collect();
    (w, h, rgb)
}

/// Screenshot `name`, compare with `golden_dir/<state-prefix>-<name>.png`
/// (state is encoded by the caller via out naming), return 0/1 failures.
/// In update mode the shot is written as the golden candidate instead.
fn check_shot(
    app: &AppWindow,
    out_dir: &std::path::Path,
    golden_dir: &std::path::Path,
    _os: &str,
    name: &str,
    update: bool,
) -> u32 {
    let (w, h, rgb) = snapshot_pixels(app);
    let img: ImageBuffer<Rgb<u8>, _> =
        ImageBuffer::from_raw(w, h, rgb.clone()).expect("snapshot dimensions");
    let actual_path = out_dir.join(format!("e2e-{name}.png"));
    img.save(&actual_path).expect("write e2e shot");
    // Golden file carries the entry state in its name via the directory run;
    // CI runs one --state per invocation, so <name> alone is unique per run.
    // The state prefix is added by the wrapper script (see tests/README.md).
    let prefix = std::env::var("BLOCKFIELD_E2E_STATE").unwrap_or_else(|_| "state".into());
    let golden_path = golden_dir.join(format!("{prefix}-{name}.png"));
    if update {
        let dest = out_dir.join(format!("golden-{prefix}-{name}.png"));
        img.save(&dest).expect("write golden candidate");
        println!("E2E update: candidate {dest:?} (from {golden_path:?})");
        return 0;
    }
    let Ok(golden_bytes) = fs::read(&golden_path) else {
        println!("E2E FAIL {name}: missing golden {golden_path:?}");
        return 1;
    };
    let golden = image::load_from_memory(&golden_bytes)
        .expect("decode golden")
        .to_rgb8();
    if golden.width() != w || golden.height() != h {
        println!(
            "E2E FAIL {name}: size {}x{} != golden {}x{}",
            w,
            h,
            golden.width(),
            golden.height()
        );
        return 1;
    }
    let g = golden.into_raw();
    assert_eq!(g.len(), rgb.len());
    let mut sum = 0u64;
    let mut coarse = 0u64;
    let mut worst = 0u8;
    for i in 0..rgb.len() {
        let d = rgb[i].abs_diff(g[i]);
        sum += d as u64;
        worst = worst.max(d);
        if i % 3 == 2
            && (rgb[i - 2].abs_diff(g[i - 2]) > COARSE_DELTA
                || rgb[i - 1].abs_diff(g[i - 1]) > COARSE_DELTA
                || d > COARSE_DELTA)
        {
            coarse += 1;
        }
    }
    let n = rgb.len() as f32;
    let mean = sum as f32 / n;
    let frac = coarse as f32 / (n / 3.0);
    let line =
        format!("E2E shot {name}: {w}x{h} mean_abs={mean:.3} coarse_frac={frac:.5} worst={worst}");
    println!("{line}");
    append_log(&out_dir.join("e2e.log"), &line);
    if mean > MEAN_ABS_GATE || frac > MAX_FRAC_COARSE {
        // Write a red-tinted diff for the artifact bundle.
        let mut diff = ImageBuffer::new(w, h);
        for (x, y, p) in diff.enumerate_pixels_mut() {
            let i = ((y * w + x) * 3) as usize;
            let bad = rgb[i].abs_diff(g[i]) > COARSE_DELTA
                || rgb[i + 1].abs_diff(g[i + 1]) > COARSE_DELTA
                || rgb[i + 2].abs_diff(g[i + 2]) > COARSE_DELTA;
            *p = if bad {
                Rgb([255, 0, 0])
            } else {
                Rgb([g[i], g[i + 1], g[i + 2]])
            };
        }
        let _ = diff.save(out_dir.join(format!("e2e-{name}-diff.png")));
        println!(
            "E2E FAIL {name}: mean {mean:.3} (gate {MEAN_ABS_GATE}), coarse {frac:.5} (gate {MAX_FRAC_COARSE})"
        );
        return 1;
    }
    0
}
