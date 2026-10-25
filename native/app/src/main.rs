use blockfield_native::{AppWindow, backend, fixtures, idle_self_check, runtime, skin};
use slint::ComponentHandle;

// Resident set size in KiB (Linux only; None elsewhere) for the RENDERER_PROBE cost line.
#[cfg(target_os = "linux")]
fn rss_kb() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|value| value.split_whitespace().next()?.parse().ok())
}
#[cfg(not(target_os = "linux"))]
fn rss_kb() -> Option<u64> {
    None
}

fn probe_renderer() -> Result<bool, Box<dyn std::error::Error>> {
    use std::{
        process::Stdio,
        time::{Duration, Instant},
    };
    let mut child = launcher_core::host_env::command(std::env::current_exe()?)
        .arg("--renderer-probe")
        .stdout(Stdio::piped())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait()?.is_none() {
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            return Err("Renderer probe timed out".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err("Renderer probe failed".into());
    }
    match std::str::from_utf8(&output.stdout)?.trim() {
        "software" => Ok(true),
        "hardware" => Ok(false),
        _ => Err("Invalid renderer probe response".into()),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--renderer-probe") {
        println!(
            "{}",
            if skin3d::software_adapter_only() {
                "software"
            } else {
                "hardware"
            }
        );
        return Ok(());
    }
    // Partial rendering is opt-in upstream but pixel-identical here (u10: AE=0 on
    // captures; real-GPU run with the flag); default it on so GPU-less GL/llvmpipe
    // setups do not repaint the whole window every animation frame.
    if std::env::var_os("SLINT_SKIA_PARTIAL_RENDERING").is_none() {
        // SAFETY: single-threaded before any thread spawn.
        unsafe { std::env::set_var("SLINT_SKIA_PARTIAL_RENDERING", "1") };
    }
    // An explicit SLINT_BACKEND wins (Layer-2 e2e pins winit-software for
    // bit-stable goldens). Otherwise probe once: on software-only GL
    // (llvmpipe, WARP) Skia GL rasters on the CPU behind a per-frame texture
    // upload (u33: settings-skin +30% vs plain software), so ask for Skia CPU
    // raster directly; real GPUs keep Skia GL/Metal/D3D. Probe objects are
    // confined to a child process; cost is logged as RENDERER_PROBE.
    let mut selector = slint::BackendSelector::new().backend_name("winit".into());
    if std::env::var_os("SLINT_BACKEND").is_none() {
        let probe_start = std::time::Instant::now();
        let rss_before = rss_kb();
        let software = probe_renderer().unwrap_or_else(|error| {
            eprintln!("{error}; using software renderer");
            true
        });
        let rss_after = rss_kb();
        eprintln!(
            "RENDERER_PROBE software_only={software} elapsed_ms={} rss_delta_kb={}",
            probe_start.elapsed().as_millis(),
            rss_before
                .and_then(|b| rss_after.map(|a| a as i64 - b as i64))
                .map_or("-".into(), |d| d.to_string()),
        );
        selector = selector.renderer_name(if software { "skia-software" } else { "skia" }.into());
    }
    selector.select()?;
    let app = AppWindow::new()?;
    let mut state = std::env::var("BLOCKFIELD_CAPTURE_STATE").unwrap_or_else(|_| "main".into());
    let mut scenario = "default".to_string();
    let mut capture_dir = None;
    let mut e2e_dir = None;
    let mut e2e_golden = None;
    let mut e2e_update = false;
    let mut idle_seconds = None;
    let mut dev = false;
    let mut self_check = false;
    let mut idle_self_check_flag = false;
    let args: Vec<_> = std::env::args().skip(1).collect();
    let use_fixtures = backend::fixtures_requested(&args);
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--capture-dir" => capture_dir = Some(args.next().ok_or("--capture-dir needs a path")?),
            "--e2e" => e2e_dir = Some(args.next().ok_or("--e2e needs an output dir")?),
            "--e2e-golden" => e2e_golden = Some(args.next().ok_or("--e2e-golden needs a dir")?),
            "--e2e-update-goldens" => e2e_update = true,
            "--idle-seconds" => {
                idle_seconds = Some(
                    args.next()
                        .ok_or("--idle-seconds needs a duration")?
                        .parse::<u64>()?,
                )
            }
            "--state" => state = args.next().ok_or("--state needs a name")?,
            "--scenario" => scenario = args.next().ok_or("--scenario needs a name")?,
            "--freeze-motion" => app
                .global::<blockfield_native::MainMotion>()
                .set_enabled(false),
            "--self-check" => self_check = true,
            "--idle-self-check" => idle_self_check_flag = true,
            "--dev" => dev = true,
            _ => return Err(format!("Unknown argument: {arg}").into()),
        }
    }
    let runtime = (!use_fixtures).then(runtime::new).transpose()?;
    let backend = match &runtime {
        Some(runtime) => backend::Backend::Core(runtime.handle().clone()),
        None => backend::Backend::Fixtures,
    };
    let (skin_host, activity) = blockfield_native::wire_app(&app, backend)?;
    if dev {
        app.set_dev_build(true);
    }
    if use_fixtures {
        if state == "settings-skin" {
            fixtures::apply_settings(&app)?;
        } else if state == "stats" {
            fixtures::apply_stats(&app, &scenario)?;
        } else {
            fixtures::apply_main(&app, &state, &scenario)?;
        }
    }
    if self_check {
        skin::arm_self_check(&app, &skin_host);
    }
    if idle_self_check_flag {
        idle_self_check::arm(&app, &activity);
    }
    let _capture_timer = capture_dir
        .map(|path| blockfield_native::capture::start(&app, &skin_host, path.into()))
        .transpose()?;
    let _e2e_timer = match (e2e_dir, e2e_golden) {
        (Some(out), Some(golden)) => Some(blockfield_native::e2e::start(
            &app,
            &skin_host,
            out.into(),
            golden.into(),
            e2e_update,
        )?),
        (Some(_), None) => return Err("--e2e needs --e2e-golden <dir>".into()),
        (None, _) => None,
    };
    if let Some(seconds) = idle_seconds {
        slint::Timer::single_shot(std::time::Duration::from_secs(seconds), || {
            slint::quit_event_loop().expect("finish idle run");
        });
    }
    app.show()?;
    skin::sync_visibility(&app, &skin_host);
    println!("READY pid={}", std::process::id());
    let result = app.run();
    if let Some(runtime) = runtime {
        runtime.shutdown_timeout(std::time::Duration::from_secs(2));
    }
    result?;
    Ok(())
}
