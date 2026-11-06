use i_slint_backend_testing::ElementHandle;
use slint::{
    ComponentHandle,
    platform::{Key, WindowEvent},
};

slint::slint! {
    import { ActionArea } from "../ui/controls.slint";
    export component ControlsProbe inherits Window {
        width: 100px; height: 100px;
        in property <bool> enabled: true;
        out property <int> activations;
        out property <bool> pressed: button.pressed;
        forward-focus: button;
        button := ActionArea {
            width: 80px; height: 40px;
            label: "probe";
            enabled: root.enabled;
            clicked => { root.activations += 1; }
        }
    }
}

#[test]
fn controls_input_contract() {
    i_slint_backend_testing::init_no_event_loop();
    let app = ControlsProbe::new().unwrap();
    app.show().unwrap();
    slint::platform::update_timers_and_animations();
    let button = ElementHandle::find_by_accessible_label(&app, "probe")
        .next()
        .unwrap();
    button.invoke_accessible_default_action();
    assert_eq!(app.get_activations(), 1);
    app.set_enabled(false);
    slint::platform::update_timers_and_animations();
    button.invoke_accessible_default_action();
    assert_eq!(
        app.get_activations(),
        1,
        "disabled accessibility activation is inert"
    );
    app.set_enabled(true);
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Tab.into(),
    });
    app.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Tab.into(),
    });
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Return.into(),
    });
    assert_eq!(app.get_activations(), 2, "Enter activates on keydown");
    app.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Return.into(),
    });
    assert_eq!(
        app.get_activations(),
        2,
        "Enter keyup does not activate twice"
    );
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Space.into(),
    });
    assert!(app.get_pressed(), "Space exposes pressed state");
    assert_eq!(app.get_activations(), 2, "Space waits for keyup");
    app.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Space.into(),
    });
    assert!(!app.get_pressed());
    assert_eq!(app.get_activations(), 3);
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Space.into(),
    });
    app.set_enabled(false);
    slint::platform::update_timers_and_animations();
    app.set_enabled(true);
    app.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Space.into(),
    });
    assert_eq!(
        app.get_activations(),
        3,
        "disable cancels pending Space activation"
    );
}
