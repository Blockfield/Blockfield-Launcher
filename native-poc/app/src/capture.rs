use std::{fs, io::Write, path::PathBuf, time::Duration};

use slint::{ComponentHandle, LogicalPosition, Timer, TimerMode, platform::WindowEvent};

use crate::AppWindow;

pub fn start(app: &AppWindow, directory: PathBuf) -> std::io::Result<Timer> {
    fs::create_dir_all(&directory)?;
    let timer = Timer::default();
    let weak = app.as_weak();
    let mut frame = 0;
    timer.start(TimerMode::Repeated, Duration::from_millis(250), move || {
        let app = weak.upgrade().expect("capture window");
        let snapshot = |name| {
            let pixels = app.window().take_snapshot().expect("renderer snapshot");
            let scale = app.window().scale_factor();
            assert_eq!(pixels.width(), (1280.0 * scale).round() as u32);
            assert_eq!(pixels.height(), (720.0 * scale).round() as u32);
            let mut file =
                fs::File::create(directory.join(format!("{name}.ppm"))).expect("create capture");
            write!(file, "P6\n{} {}\n255\n", pixels.width(), pixels.height())
                .expect("write capture header");
            let rgb: Vec<u8> = pixels
                .as_slice()
                .iter()
                .flat_map(|p| [p.r, p.g, p.b])
                .collect();
            file.write_all(&rgb).expect("write capture pixels");
        };
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
                app.set_show_pulse(true);
            }
            8 => snapshot("backdrop-dev-pulse-a"),
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
