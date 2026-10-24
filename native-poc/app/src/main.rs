mod capture;
mod fixtures;

use slint::ComponentHandle;

slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .select()?;
    let app = AppWindow::new()?;
    fixtures::apply(&app);
    let weak = app.as_weak();
    app.on_minimize(move || {
        if let Some(app) = weak.upgrade() {
            app.window().set_minimized(true);
        }
    });
    let weak = app.as_weak();
    app.on_maximize(move || {
        if let Some(app) = weak.upgrade() {
            app.window().set_maximized(!app.window().is_maximized());
        }
    });
    app.on_request_close(|| {
        if let Err(error) = slint::quit_event_loop() {
            eprintln!("Cannot close launcher: {error}");
        }
    });
    let weak = app.as_weak();
    app.on_navigate(move |index| {
        if let Some(app) = weak.upgrade() {
            app.set_active(index);
        }
    });
    let weak = app.as_weak();
    app.on_launcher_update(move || {
        if let Some(app) = weak.upgrade() {
            app.set_active(3);
        }
    });
    app.on_support(|| eprintln!("POC support action: https://discord.gg/cAUcpeCVDM"));
    app.global::<SelectSearch>()
        .on_find(|options, search| fixtures::find_option(&options, &search));
    let weak = app.as_weak();
    app.on_deploy(move || {
        if let Some(app) = weak.upgrade() {
            let mut data = app.get_main_data();
            if data.needs_setup {
                app.set_room_message(
                    "POC: подготовка сборки (UpdateScreen вне области этого экрана).".into(),
                );
            } else {
                data.launching = true;
                data.phase = "launching".into();
                app.set_main_data(data);
            }
        }
    });
    let weak = app.as_weak();
    app.on_verify(move || {
        if let Some(app) = weak.upgrade() {
            let mut data = app.get_main_data();
            data.checking = true;
            app.set_main_data(data);
            let weak = app.as_weak();
            slint::Timer::single_shot(std::time::Duration::from_secs(1), move || {
                if let Some(app) = weak.upgrade() {
                    let mut data = app.get_main_data();
                    data.checking = false;
                    app.set_main_data(data);
                }
            });
        }
    });
    let weak = app.as_weak();
    app.on_join(move |id| {
        if let Some(app) = weak.upgrade() {
            app.set_room_message(
                if app.get_main_data().phase == "running" {
                    "Запрос передан игре. Результат входа появится в игровом меню."
                } else if id == "lobby" {
                    "После подготовки игра подключится к лобби."
                } else {
                    "После подготовки игра подключится к выбранному матчу."
                }
                .into(),
            );
            if app.get_main_data().phase != "running" {
                app.invoke_deploy();
            }
        }
    });
    let mut state = std::env::var("BLOCKFIELD_CAPTURE_STATE").unwrap_or_else(|_| "main".into());
    let mut scenario = "default".to_string();
    let mut capture_dir = None;
    let mut idle_seconds = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--capture-dir" => capture_dir = Some(args.next().ok_or("--capture-dir needs a path")?),
            "--idle-seconds" => {
                idle_seconds = Some(
                    args.next()
                        .ok_or("--idle-seconds needs a duration")?
                        .parse::<u64>()?,
                )
            }
            "--state" => state = args.next().ok_or("--state needs a name")?,
            "--scenario" => scenario = args.next().ok_or("--scenario needs a name")?,
            "--freeze-motion" => app.global::<MainMotion>().set_enabled(false),
            "--dev" => app.set_dev_build(true),
            _ => return Err(format!("Unknown argument: {arg}").into()),
        }
    }
    fixtures::apply_main(&app, &state, &scenario)?;
    let _capture_timer = capture_dir
        .map(|path| capture::start(&app, path.into()))
        .transpose()?;
    if let Some(seconds) = idle_seconds {
        slint::Timer::single_shot(std::time::Duration::from_secs(seconds), || {
            slint::quit_event_loop().expect("finish idle run");
        });
    }
    app.show()?;
    println!("READY pid={}", std::process::id());
    app.run()?;
    Ok(())
}
