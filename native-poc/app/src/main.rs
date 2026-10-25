use blockfield_native_poc::{AppWindow, fixtures, idle_self_check, skin};
use slint::ComponentHandle;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Partial rendering is opt-in upstream but pixel-identical here (u10: AE=0 on
    // captures; real-GPU run with the flag); default it on so GPU-less GL/llvmpipe
    // setups do not repaint the whole window every animation frame.
    if std::env::var_os("SLINT_SKIA_PARTIAL_RENDERING").is_none() {
        // SAFETY: single-threaded before any thread spawn.
        unsafe { std::env::set_var("SLINT_SKIA_PARTIAL_RENDERING", "1") };
    }
    // An explicit SLINT_BACKEND wins (Layer-2 e2e pins winit-software for
    // bit-stable goldens). Otherwise use Skia: GPU (GL/Metal/D3D) first,
    // Slint-internal fallback to Skia's CPU raster — never the plain software
    // renderer, which idles ~3x hotter on GPU-less machines (u33; same pixels
    // here, u10 AE=0). select() cannot fail over renderers (suspended
    // creation is infallible); the fallback happens at window resume inside
    // Slint and is logged there as "Backend: ...".
    let mut selector = slint::BackendSelector::new().backend_name("winit".into());
    if std::env::var_os("SLINT_BACKEND").is_none() {
        selector = selector.renderer_name("skia".into());
    }
    selector.select()?;
    let app = AppWindow::new()?;
    let (skin_host, activity) = blockfield_native_poc::wire_app(&app)?;
    let mut state = std::env::var("BLOCKFIELD_CAPTURE_STATE").unwrap_or_else(|_| "main".into());
    let mut scenario = "default".to_string();
    let mut capture_dir = None;
    let mut e2e_dir = None;
    let mut e2e_golden = None;
    let mut e2e_update = false;
    let mut idle_seconds = None;
    let mut self_check = false;
    let mut idle_self_check_flag = false;
    let mut args = std::env::args().skip(1);
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
                .global::<blockfield_native_poc::MainMotion>()
                .set_enabled(false),
            "--self-check" => self_check = true,
            "--idle-self-check" => idle_self_check_flag = true,
            "--dev" => app.set_dev_build(true),
            _ => return Err(format!("Unknown argument: {arg}").into()),
        }
    }
    if state == "settings-skin" {
        fixtures::apply_settings(&app)?;
    } else {
        fixtures::apply_main(&app, &state, &scenario)?;
    }
    if self_check {
        skin::arm_self_check(&app, &skin_host);
    }
    if idle_self_check_flag {
        idle_self_check::arm(&app, &activity);
    }
    let _capture_timer = capture_dir
        .map(|path| blockfield_native_poc::capture::start(&app, &skin_host, path.into()))
        .transpose()?;
    let _e2e_timer = match (e2e_dir, e2e_golden) {
        (Some(out), Some(golden)) => Some(blockfield_native_poc::e2e::start(
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
    app.run()?;
    Ok(())
}
