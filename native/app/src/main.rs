use blockfield_native::{
    AppWindow, backend, deep_link, fixtures, idle_self_check, runtime, single_instance, skin,
};
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

/// True when the OpenGL stack Slint renders through is forced onto a CPU
/// rasterizer (headless llvmpipe benchmarks, GPU-less CI). The
/// Vulkan-adapter probe cannot see this — a real GPU stays visible to
/// Vulkan while GL is software — but Skia GL would still raster on the CPU
/// behind a per-frame texture upload, so prefer Skia's CPU raster directly.
/// Env only; the GL-context probe below covers the same case without env.
fn gl_forced_software() -> bool {
    matches!(
        std::env::var("LIBGL_ALWAYS_SOFTWARE").as_deref(),
        Ok("1") | Ok("true")
    ) || matches!(
        std::env::var("GALLIUM_DRIVER").as_deref(),
        Ok("llvmpipe") | Ok("swrast") | Ok("softpipe")
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--renderer-probe") {
        println!(
            "{}",
            // Cheapest signals first; the GL-context probe only runs when
            // neither env nor the Vulkan adapters already said software.
            if skin3d::software_adapter_only()
                || gl_forced_software()
                || skin3d::gl_software_rasterizer()
            {
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
    // Empty counts as unset: CI's `VAR=${VAR-}` prefix delivers SLINT_BACKEND=""
    // for default-renderer runs, and skipping the probe there left GPU-less
    // runners on the flaky Skia-GL fallback (exit 127, coordination#43).
    if std::env::var("SLINT_BACKEND")
        .map(|value| value.is_empty())
        .unwrap_or(true)
    {
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
    // Single instance (Tauri plugin parity): the owner keeps the window, a
    // second launch forwards its invite links and exits. Probe children exit
    // above, fixture/capture/e2e runs bypass the lock like parallel harnesses.
    let cli_args: Vec<String> = std::env::args().skip(1).collect();
    let harness_run = backend::fixtures_requested(&cli_args);
    let cli_links = single_instance::collect_links(&cli_args);
    let primary_listener = if harness_run {
        None
    } else {
        match single_instance::claim() {
            Ok(single_instance::Claim::Secondary) => {
                match single_instance::forward(&cli_links) {
                    Ok(()) => return Ok(()),
                    // The owner died between election and forward, or the
                    // endpoint is not ours: elect once more instead of
                    // dropping the invite, and never exit on a stranger.
                    Err(error) => {
                        eprintln!(
                            "Cannot forward to the running launcher ({error}); retrying election"
                        );
                        match single_instance::claim() {
                            Ok(single_instance::Claim::Primary(listener)) => Some(listener),
                            Ok(single_instance::Claim::Secondary) => {
                                eprintln!(
                                    "Another launcher owns this session; continuing alone: {error}"
                                );
                                None
                            }
                            Err(error) => {
                                eprintln!("Single-instance lock failed, continuing alone: {error}");
                                None
                            }
                        }
                    }
                }
            }
            Ok(single_instance::Claim::Primary(listener)) => Some(listener),
            Err(error) => {
                eprintln!("Single-instance lock failed, continuing alone: {error}");
                None
            }
        }
    };
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
    let use_fixtures = harness_run;
    let mut args = cli_args.into_iter();
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
            // Invite links travel through the single-instance channel; the
            // parser tolerates them here, handling happens below.
            _ if single_instance::is_link(&arg) => {}
            _ => return Err(format!("Unknown argument: {arg}").into()),
        }
    }
    let runtime = (!use_fixtures).then(runtime::new).transpose()?;
    let backend = match &runtime {
        Some(runtime) => backend::Backend::Core(runtime.handle().clone()),
        None => backend::Backend::Fixtures,
    };
    let (skin_host, activity) = blockfield_native::wire_app(&app, backend)?;
    if let Some(listener) = primary_listener {
        single_instance::spawn_listener(listener, app.as_weak());
    }
    if !harness_run {
        for link in &cli_links {
            single_instance::handle_link(&app, link);
        }
        if let Err(error) = deep_link::register() {
            eprintln!("Cannot register deep links: {error}");
        }
    }
    if dev {
        app.set_dev_build(true);
    }
    if use_fixtures {
        if ["settings-general", "settings-runtime", "settings-commands"].contains(&state.as_str()) {
            blockfield_native::settings_fixtures::apply(&app, &state, &scenario)?;
        } else if state == "settings-skin" {
            fixtures::apply_settings(&app)?;
        } else if state == "stats" {
            fixtures::apply_stats(&app, &scenario)?;
        } else if state == "login" {
            fixtures::apply_login(&app, &scenario)?;
        } else if state == "firstrun" {
            fixtures::apply_firstrun(&app)?;
        } else if state == "update" {
            fixtures::apply_update(&app, &scenario)?;
        } else {
            fixtures::apply_main(&app, &state, &scenario)?;
        }
    }
    if use_fixtures {
        app.set_dev_build(dev);
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

#[cfg(test)]
mod probe_env_tests {
    use super::gl_forced_software;

    // SAFETY: single-threaded body — no other test reads these two vars, so
    // save/set/restore around each assertion is race-free.
    fn with_vars(libgl: Option<&str>, gallium: Option<&str>, check: impl FnOnce()) {
        let old_libgl = std::env::var("LIBGL_ALWAYS_SOFTWARE").ok();
        let old_gallium = std::env::var("GALLIUM_DRIVER").ok();
        unsafe {
            match libgl {
                Some(value) => std::env::set_var("LIBGL_ALWAYS_SOFTWARE", value),
                None => std::env::remove_var("LIBGL_ALWAYS_SOFTWARE"),
            }
            match gallium {
                Some(value) => std::env::set_var("GALLIUM_DRIVER", value),
                None => std::env::remove_var("GALLIUM_DRIVER"),
            }
        }
        check();
        unsafe {
            match old_libgl {
                Some(value) => std::env::set_var("LIBGL_ALWAYS_SOFTWARE", value),
                None => std::env::remove_var("LIBGL_ALWAYS_SOFTWARE"),
            }
            match old_gallium {
                Some(value) => std::env::set_var("GALLIUM_DRIVER", value),
                None => std::env::remove_var("GALLIUM_DRIVER"),
            }
        }
    }

    #[test]
    fn forced_software_gl_env_detected() {
        // launcher#59 u-bench env: probe must see software despite a real GPU.
        with_vars(
            Some("1"),
            Some("llvmpipe"),
            || assert!(gl_forced_software()),
        );
        with_vars(None, Some("swrast"), || assert!(gl_forced_software()));
        with_vars(Some("true"), None, || assert!(gl_forced_software()));
        with_vars(None, Some("radeonsi"), || assert!(!gl_forced_software()));
        with_vars(None, None, || assert!(!gl_forced_software()));
    }
}
