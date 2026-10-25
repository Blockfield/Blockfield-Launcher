//! Additional Main states share the existing per-OS golden walk.
use slint::ComponentHandle;
use std::path::Path;

use crate::{AppWindow, fixtures};

const SHOTS: &[&str] = &[
    "pager-second",
    "verify-spinner",
    "status-slice",
    "join-running",
    "join-finishing",
    "join-closed",
    "join-updating",
    "join-check-error",
    "deploy-focus",
];

pub(crate) fn step(
    app: &AppWindow,
    out: &Path,
    golden: &Path,
    update: bool,
    step: &mut usize,
    shots: &mut u32,
    failures: &mut u32,
) -> bool {
    let index = *step / 2;
    if index >= SHOTS.len() {
        if index == SHOTS.len() {
            // Recreate Main after its keyboard probe so later baseline shots have no focus ring.
            if (*step).is_multiple_of(2) {
                app.set_show_main(false);
            } else {
                fixtures::apply_main(app, "main", "default").unwrap();
            }
            *step += 1;
            return true;
        }
        return false;
    }
    if (*step).is_multiple_of(2) {
        fixtures::apply_main(
            app,
            "main",
            match index {
                1 => "default",
                2 => "busy",
                3 => "running",
                4 => "finishing",
                5 => "closed",
                _ => "default",
            },
        )
        .unwrap();
        app.set_room_index(0);
        match index {
            0 => app.set_room_index(1),
            1 => app.invoke_verify(),
            2 => {
                let mut data = app.get_main_data();
                data.launch_status = "Подготовка файлов: mods/очень-длинный-путь-к-файлу-для-проверки-ограничения.jar".into();
                app.set_main_data(data);
            }
            6 => {
                let mut data = app.get_main_data();
                data.updating = true;
                app.set_main_data(data);
            }
            7 => fixtures::apply_main(app, "error", "default").unwrap(),
            8 => {
                // Chrome (3), brand (1) and navigation (4) precede Deploy.
                for _ in 0..9 {
                    app.window()
                        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
                            text: slint::platform::Key::Tab.into(),
                        });
                    app.window()
                        .dispatch_event(slint::platform::WindowEvent::KeyReleased {
                            text: slint::platform::Key::Tab.into(),
                        });
                }
            }
            _ => {}
        }
    } else {
        *shots += 1;
        *failures += crate::e2e::check_shot(app, out, golden, "", SHOTS[index], update);
    }
    *step += 1;
    true
}
