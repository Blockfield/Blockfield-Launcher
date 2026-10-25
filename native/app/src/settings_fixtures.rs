//! Deterministic capture data; called only by the explicit fixture backend.
use crate::{AppWindow, SettingsForm};
use slint::ComponentHandle;

pub fn defaults(app: &AppWindow) {
    crate::settings_tabs::apply_response(
        app,
        &serde_json::json!({
            "activeProfile": "game", "workshopServer": "workshop.example.test:25566",
            "gameDir": "/capture/game", "javaPath": "", "ramMb": 4096, "maxRamMb": 12288,
            "autoUpdate": false, "discordPresence": false, "hideWhilePlaying": false,
            "preLaunchCommand": "", "postExitCommand": "", "lang": "ru",
        }),
    );
    let form = app.global::<SettingsForm>();
    form.set_password_open(false);
    form.set_password("".into());
    form.set_account_error("".into());
    form.set_account_raw_error("".into());
    form.set_password_changed(false);
    form.set_account_busy(false);
    form.set_saved(false);
    form.set_saving(false);
    form.set_blocked(false);
}

pub fn apply(app: &AppWindow, state: &str, scenario: &str) -> Result<(), String> {
    crate::fixtures::apply_settings(app)?;
    defaults(app);
    app.set_settings_tab(match state {
        "settings-general" => 0,
        "settings-runtime" => 1,
        "settings-commands" => 2,
        _ => return Err(format!("Unknown settings state: {state}")),
    });
    let form = app.global::<SettingsForm>();
    match scenario {
        "default" => {}
        "edited" => {
            let mut data = form.get_data();
            match app.get_settings_tab() {
                0 => {
                    data.auto_update = true;
                    data.discord_presence = true;
                    data.hide_while_playing = true;
                    form.set_password_open(true);
                }
                1 => {
                    data.ram = 8;
                    data.java_path = "/capture/java/bin/java".into();
                }
                _ => {
                    data.pre_launch = "echo preparing\necho \"$INST_DIR\"".into();
                    data.post_exit = "echo finished".into();
                }
            }
            form.set_data(data);
            form.invoke_edited();
        }
        "error" => {
            if app.get_settings_tab() == 0 {
                form.set_password_open(true);
                form.set_password("short".into());
                form.invoke_change_password();
            } else {
                let mut data = form.get_data();
                data.java_path = "/missing/java".into();
                form.set_data(data);
                form.invoke_edited();
                form.invoke_save();
            }
        }
        _ => return Err(format!("Unknown settings scenario: {scenario}")),
    }
    Ok(())
}
