//! Default / edited / error screenshots use the same fixture callbacks as Layer 1.
use crate::AppWindow;
use slint::{ComponentHandle, Timer, TimerMode};
use std::{path::PathBuf, time::Duration};

pub fn start(
    app: &AppWindow,
    out: PathBuf,
    goldens: PathBuf,
    update: bool,
) -> std::io::Result<Timer> {
    std::fs::create_dir_all(&out)?;
    let state = match app.get_settings_tab() {
        0 => "settings-general",
        1 => "settings-runtime",
        _ => "settings-commands",
    };
    let weak = app.as_weak();
    let timer = Timer::default();
    let mut step = 0;
    let mut failures = 0;
    timer.start(TimerMode::Repeated, Duration::from_millis(300), move || {
        let app = weak.upgrade().expect("settings e2e window");
        if step < 6 {
            let scenario = ["default", "edited", "error"][step / 2];
            if step % 2 == 0 {
                crate::settings_fixtures::apply(&app, state, scenario).expect("settings fixture");
            } else {
                failures += crate::e2e::check_shot(&app, &out, &goldens, "", scenario, update);
            }
            step += 1;
        } else {
            assert_eq!(failures, 0, "settings golden failures");
            println!("PASS settings e2e: {state}, 3 states");
            slint::quit_event_loop().expect("settings e2e complete");
        }
    });
    Ok(timer)
}
