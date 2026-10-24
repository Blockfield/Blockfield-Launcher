use std::{fs, io::Write, path::PathBuf, time::Duration};

use slint::{ComponentHandle, LogicalPosition, Model, Timer, TimerMode, platform::WindowEvent};

use crate::AppWindow;

fn snapshot(app: &AppWindow, directory: &std::path::Path, name: &str) {
    let pixels = app.window().take_snapshot().expect("renderer snapshot");
    let scale = app.window().scale_factor();
    assert_eq!(pixels.width(), (1280.0 * scale).round() as u32);
    assert_eq!(pixels.height(), (720.0 * scale).round() as u32);
    let mut file = fs::File::create(directory.join(format!("{name}.ppm"))).expect("create capture");
    write!(file, "P6\n{} {}\n255\n", pixels.width(), pixels.height()).expect("capture header");
    let rgb: Vec<u8> = pixels
        .as_slice()
        .iter()
        .flat_map(|p| [p.r, p.g, p.b])
        .collect();
    file.write_all(&rgb).expect("capture pixels");
}

pub fn start(app: &AppWindow, directory: PathBuf) -> std::io::Result<Timer> {
    if app.get_show_main() {
        return start_main(app, directory);
    }
    fs::create_dir_all(&directory)?;
    let timer = Timer::default();
    let weak = app.as_weak();
    let mut frame = 0;
    timer.start(TimerMode::Repeated, Duration::from_millis(250), move || {
        let app = weak.upgrade().expect("capture window");
        let snapshot = |name| snapshot(&app, &directory, name);
        match frame {
            0 => snapshot("shell"),
            1 => app.window().dispatch_event(WindowEvent::PointerMoved {
                position: LogicalPosition::new(400.0, 65.0),
            }),
            2 => snapshot("nav-hover"),
            3 => {
                for (index, x) in [(0, 250.0), (1, 400.0), (2, 490.0), (3, 610.0)] {
                    click(&app, x, 65.0);
                    assert_eq!(app.get_active(), index, "navigation click {index}");
                }
                click(&app, 80.0, 65.0);
                assert_eq!(app.get_active(), 0, "brand returns home");
                click(&app, 100.0, 696.0);
                assert_eq!(app.get_active(), 3, "launcher update opens settings");
                click(&app, 400.0, 65.0);
            }
            4 => snapshot("nav-updates"),
            5 => {
                app.window().dispatch_event(WindowEvent::PointerExited);
                app.window().dispatch_event(WindowEvent::KeyPressed {
                    text: slint::platform::Key::Tab.into(),
                });
                app.window().dispatch_event(WindowEvent::KeyReleased {
                    text: slint::platform::Key::Tab.into(),
                });
            }
            6 => {
                snapshot("keyboard-focus");
                app.window().dispatch_event(WindowEvent::KeyPressed {
                    text: slint::platform::Key::Return.into(),
                });
                app.window().dispatch_event(WindowEvent::KeyReleased {
                    text: slint::platform::Key::Return.into(),
                });
                assert_eq!(app.get_active(), 2, "keyboard activates stats");
            }
            7 => {
                app.set_dev_build(true);
                app.set_show_backdrop(true);
            }
            8 => {
                // Warm backdrop shaders first; sample pulse activation before readback delays shift its phase.
                app.set_show_pulse(true);
                snapshot("backdrop-dev-pulse-a");
            }
            12 => snapshot("backdrop-dev-pulse-b"),
            13 => {
                app.set_show_backdrop(false);
                app.set_show_pulse(false);
                app.window().dispatch_event(WindowEvent::PointerMoved {
                    position: LogicalPosition::new(1250.0, 18.0),
                });
            }
            14 => snapshot("close-hover"),
            15 => {
                app.window().dispatch_event(WindowEvent::PointerPressed {
                    position: LogicalPosition::new(1250.0, 18.0),
                    button: slint::platform::PointerEventButton::Left,
                });
                snapshot("close-pressed");
            }
            16 => app.window().dispatch_event(WindowEvent::PointerReleased {
                position: LogicalPosition::new(1250.0, 18.0),
                button: slint::platform::PointerEventButton::Left,
            }),
            17 => panic!("Close button did not quit the event loop"),
            _ => {}
        }
        frame += 1;
    });
    Ok(timer)
}

fn click(app: &AppWindow, x: f32, y: f32) {
    let position = LogicalPosition::new(x, y);
    app.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: slint::platform::PointerEventButton::Left,
    });
    app.window().dispatch_event(WindowEvent::PointerReleased {
        position,
        button: slint::platform::PointerEventButton::Left,
    });
}

pub fn start_main(app: &AppWindow, directory: PathBuf) -> std::io::Result<Timer> {
    fs::create_dir_all(&directory)?;
    let timer = Timer::default();
    let weak = app.as_weak();
    let mut frame = 0;
    timer.start(TimerMode::Repeated, Duration::from_millis(250), move || {
        let app = weak.upgrade().expect("capture window");
        let data = app.get_main_data();
        let offset = if data.can_workshop { 48.0 } else { 0.0 }
            + if data.profile_error.is_empty() {
                0.0
            } else {
                34.0
            };
        match frame {
            0 => snapshot(&app, &directory, "main"),
            1 => app.window().dispatch_event(WindowEvent::PointerMoved {
                position: LogicalPosition::new(100.0, 295.0 + offset),
            }),
            2 => snapshot(&app, &directory, "deploy-hover"),
            3 => {
                app.window().dispatch_event(WindowEvent::PointerExited);
                if !app.get_workshop() && app.get_rooms().row_count() > 1 {
                    click(&app, 156.0, 635.0);
                    assert_eq!(app.get_room_index(), 1, "next room");
                }
            }
            4 => snapshot(&app, &directory, "room-next"),
            5 => {
                click(
                    &app,
                    if app.get_workshop() { 190.0 } else { 140.0 },
                    402.0 + offset,
                );
                assert_eq!(app.get_details_view(), 1, "briefing tab");
            }
            6 => snapshot(&app, &directory, "briefing"),
            7 => {
                click(&app, 65.0, 402.0 + offset);
                assert_eq!(app.get_details_view(), 0, "rooms tab");
            }
            8 => {
                if data.can_workshop
                    && !data.profile_changing
                    && !data.checking
                    && !data.launching
                    && data.phase == "idle"
                {
                    click(&app, 180.0, 282.0);
                } else if !app.get_workshop() && app.get_rooms().row_count() > 0 {
                    click(&app, 180.0, 442.0 + offset);
                }
            }
            9 => snapshot(&app, &directory, "room-select"),
            10 => {
                if data.can_workshop
                    && !data.profile_changing
                    && !data.checking
                    && !data.launching
                    && data.phase == "idle"
                {
                    key(&app, slint::platform::Key::End.into());
                    key(&app, slint::platform::Key::Return.into());
                    assert!(app.get_workshop(), "permitted Workshop selection");
                } else if !app.get_workshop() && app.get_rooms().row_count() > 0 {
                    key(&app, slint::platform::Key::Home.into());
                    key(&app, slint::platform::Key::Return.into());
                    assert_eq!(app.get_room_index(), 0, "keyboard chooses first room");
                }
                key(&app, slint::platform::Key::Escape.into());
                let data = app.get_main_data();
                if !data.checking && data.phase == "idle" {
                    click(&app, 1000.0, 631.0);
                    assert!(app.get_main_data().checking, "verify starts");
                }
            }
            11 => snapshot(&app, &directory, "verifying"),
            12 => {
                if !app.get_workshop() && app.get_rooms().row_count() > 1 {
                    click(&app, 180.0, 442.0 + offset);
                    key(&app, "b".into());
                    key(&app, slint::platform::Key::Return.into());
                    assert_eq!(app.get_room_index(), 1, "typeahead selects BRAVO");
                }
            }
            13 => snapshot(&app, &directory, "keyboard-select"),
            15 => {
                let before = app.get_main_data();
                click(&app, 100.0, 295.0 + offset);
                if before.checked
                    && !before.checking
                    && !before.launching
                    && before.phase == "idle"
                    && !before.profile_changing
                    && !before.profile_blocked
                    && !before.needs_setup
                {
                    assert!(
                        app.get_main_data().launching,
                        "deploy starts fixture launch"
                    );
                } else {
                    assert_eq!(
                        app.get_main_data().launching,
                        before.launching,
                        "disabled deploy stays unchanged"
                    );
                }
            }
            17 => snapshot(&app, &directory, "launch-after-click"),
            18 => {
                println!("PASS main capture interactions");
                slint::quit_event_loop().expect("finish main capture");
            }
            _ => {}
        }
        frame += 1;
    });
    Ok(timer)
}

fn key(app: &AppWindow, text: slint::SharedString) {
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text });
}
