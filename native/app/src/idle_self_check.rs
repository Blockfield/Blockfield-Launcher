use std::{rc::Rc, time::Duration};

use slint::{ComponentHandle, Timer};

use crate::{AppWindow, WindowActivity, window_activity::Activity};

pub fn arm(app: &AppWindow, activity: &Rc<Activity>) {
    cycle(app, activity.clone(), false);
}

fn cycle(app: &AppWindow, activity: Rc<Activity>, busy: bool) {
    let weak = app.as_weak();
    let visible = activity.redraws.get();
    Timer::single_shot(Duration::from_secs(2), move || {
        let app = weak.upgrade().unwrap();
        assert!(
            activity.redraws.get() > visible + 10,
            "visible animation delivers frames"
        );
        app.window().set_minimized(true);
        let weak = app.as_weak();
        Timer::single_shot(Duration::from_secs(1), move || {
            let app = weak.upgrade().unwrap();
            assert!(
                !app.global::<WindowActivity>().get_live(),
                "minimize detected"
            );
            assert!(
                !app.window().has_active_animations(),
                "minimized motion stops"
            );
            let count = activity.redraws.get();
            println!("IDLE_SELF_CHECK minimized busy={busy} redraws={count}");
            let weak = app.as_weak();
            Timer::single_shot(Duration::from_secs(5), move || {
                let app = weak.upgrade().unwrap();
                assert_eq!(
                    activity.redraws.get(),
                    count,
                    "no frames for five minimized seconds"
                );
                assert!(
                    !app.window().has_active_animations(),
                    "animation stays stopped"
                );
                app.window().set_minimized(false);
                let weak = app.as_weak();
                Timer::single_shot(Duration::from_secs(1), move || {
                    let app = weak.upgrade().unwrap();
                    assert!(
                        app.global::<WindowActivity>().get_live(),
                        "restore detected"
                    );
                    assert!(
                        // Distinguish sustained animation from restore's one-off repaint.
                        activity.redraws.get() > count + 10,
                        "restored animation delivers frames"
                    );
                    println!(
                        "IDLE_SELF_CHECK cycle passed: busy={busy}, five seconds without frames/animation, restore resumes"
                    );
                    if busy {
                        println!("IDLE_SELF_CHECK passed: pulse and spinner minimize/restore");
                        slint::quit_event_loop().expect("finish idle self-check");
                    } else {
                        let mut data = app.get_main_data();
                        data.online = false;
                        data.launching = true;
                        data.phase = "launching".into();
                        app.set_main_data(data);
                        cycle(&app, activity, true);
                    }
                });
            });
        });
    });
}
