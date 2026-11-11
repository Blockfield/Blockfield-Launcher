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
        "settings-skin" => 3,
        "settings-launcher" => 4,
        _ => return Err(format!("Unknown settings state: {state}")),
    });
    let form = app.global::<SettingsForm>();
    match scenario {
        "default" => {}
        "edited" => {
            match app.get_settings_tab() {
                0 => {
                    let mut data = form.get_data();
                    data.auto_update = true;
                    data.discord_presence = true;
                    data.hide_while_playing = true;
                    form.set_data(data);
                    form.set_password_open(true);
                    form.invoke_edited();
                }
                1 => {
                    let mut data = form.get_data();
                    data.ram = 8;
                    data.java_path = "/capture/java/bin/java".into();
                    form.set_data(data);
                    form.invoke_edited();
                }
                2 => {
                    let mut data = form.get_data();
                    data.pre_launch = "echo preparing\necho \"$INST_DIR\"".into();
                    data.post_exit = "echo finished".into();
                    form.set_data(data);
                    form.invoke_edited();
                }
                3 => {
                    // Skin picks live outside the settings form: choosing a
                    // file must not enable the form save (upload is separate).
                    app.set_skin_path("/capture/skin.png".into());
                    app.set_slim(true);
                }
                _ => {
                    crate::fixtures::apply_launcher_update(app, "available")
                        .expect("launcher fixtures");
                }
            }
        }
        "error" => match app.get_settings_tab() {
            0 => {
                form.set_password_open(true);
                form.set_password("short".into());
                form.invoke_change_password();
            }
            3 => {
                app.set_skin_error("Не удалось загрузить предпросмотр".into());
            }
            4 => {
                crate::fixtures::apply_launcher_update(app, "error").expect("launcher fixtures");
            }
            _ => {
                let mut data = form.get_data();
                data.java_path = "/missing/java".into();
                form.set_data(data);
                form.invoke_edited();
                form.invoke_save();
            }
        },
        _ => return Err(format!("Unknown settings scenario: {scenario}")),
    }
    Ok(())
}
