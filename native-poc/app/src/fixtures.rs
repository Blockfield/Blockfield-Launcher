use slint::{ModelRc, SharedString, VecModel};

use crate::AppWindow;

pub fn apply(app: &AppWindow) {
    app.set_chrome_title("BLOCKFIELD LAUNCHER".into());
    app.set_brand("BLOCKFIELD".into());
    app.set_subtitle("КОМАНДНЫЕ БОИ".into());
    app.set_username("—".into());
    app.set_role("ИГРОК".into());
    app.set_launcher_label("ЛАУНЧЕР v1.0.19".into());
    app.set_server_label("IP · play.blockfield.pro".into());
    app.set_support_label("ПОДДЕРЖКА".into());
    app.set_network_label("АККАУНТ BLOCKFIELD".into());
    app.set_nav_labels(ModelRc::new(VecModel::from(
        ["ИГРАТЬ", "ОБНОВЛЕНИЯ", "ТОП", "НАСТРОЙКИ"]
            .map(SharedString::from)
            .to_vec(),
    )));
    app.set_dev_build(cfg!(debug_assertions));
}

pub const STATES: &[&str] = &[
    "main",
    "main-launching",
    "offline",
    "error",
    "shell",
    "settings-skin",
];
pub const SCENARIOS: &[&str] = &[
    "default",
    "workshop",
    "builder",
    "checking",
    "busy",
    "running",
    "finishing",
    "profile-changing",
    "profile-blocked",
    "profile-error",
    "empty",
    "unavailable",
    "maintenance",
    "updating",
    "update",
    "setup",
    "retry",
    "closed",
];

pub fn apply_main(app: &AppWindow, state: &str, scenario: &str) -> Result<(), String> {
    use crate::{MainData, Room};
    if !STATES.contains(&state) || !SCENARIOS.contains(&scenario) {
        return Err(format!("Unknown state/scenario: {state}/{scenario}"));
    }
    app.set_show_main(state != "shell");
    if state == "shell" {
        return Ok(());
    }
    // Values copied from native/capture data.ts; that fixture's player role hides profile selection.
    app.set_username("CaptureOperator".into());
    app.set_update_label("Доступна 1.0.20".into());
    let mut data = MainData {
        online: state != "offline",
        rooms_available: true,
        players: if state == "offline" { "—" } else { "24" }.into(),
        ping: if state == "offline" { "—" } else { "32" }.into(),
        region: "EU".into(),
        location: "Frankfurt".into(),
        installed: if state == "error" { "..." } else { "2.75" }.into(),
        latest: if state == "error" { "..." } else { "2.75" }.into(),
        size: if state == "error" { "—" } else { "1.0 GB" }.into(),
        checked: state != "error",
        check_error: if state == "error" {
            "Capture fixture: manifest unavailable"
        } else {
            ""
        }
        .into(),
        phase: if state == "main-launching" {
            "launching"
        } else {
            "idle"
        }
        .into(),
        progress: -1,
        ..Default::default()
    };
    match scenario {
        "builder" | "workshop" => {
            data.can_workshop = true;
            app.set_role("СТРОИТЕЛЬ".into());
            app.set_workshop(scenario == "workshop");
        }
        "checking" => {
            data.checking = true;
            data.checked = false;
        }
        "busy" => {
            data.launching = true;
            data.launch_status = "Подготовка файлов".into();
            data.progress = 42;
        }
        "running" => data.phase = "running".into(),
        "finishing" => data.phase = "finishing".into(),
        "profile-changing" => {
            data.can_workshop = true;
            data.profile_changing = true;
        }
        "profile-blocked" => {
            data.can_workshop = true;
            data.profile_blocked = true;
        }
        "profile-error" => {
            data.can_workshop = true;
            data.profile_blocked = true;
            data.profile_error = "Не удалось переключить профиль.".into();
        }
        "maintenance" => data.maintenance = "Плановое обслуживание".into(),
        "updating" => data.updating = true,
        "unavailable" => data.rooms_available = false,
        "update" => {
            data.needs_setup = true;
            data.needs_update = true;
            data.installed = "2.74".into();
        }
        "setup" => data.needs_setup = true,
        "retry" => data.launch_error = "Fixture: launch failed".into(),
        _ => {}
    }
    let no_rooms = state == "offline"
        || ["empty", "unavailable", "maintenance", "updating"].contains(&scenario);
    let rooms = if no_rooms {
        vec![]
    } else {
        vec![
            Room {
                id: "capture-casual".into(),
                name: "ALPHA".into(),
                description: "Casual · room · Citadel · waiting".into(),
                players: 12,
                ready: "Готовы: 8/12".into(),
                join: if scenario == "closed" {
                    "Мест нет"
                } else {
                    "Войти"
                }
                .into(),
                joinable: scenario != "closed",
            },
            Room {
                id: "capture-ranked".into(),
                name: "BRAVO".into(),
                description: "Ranked · campaign · Crossroads · running".into(),
                players: 12,
                ready: "".into(),
                join: "Встать в очередь".into(),
                joinable: true,
            },
        ]
    };
    app.set_room_names(ModelRc::new(VecModel::from(
        rooms.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
    )));
    app.set_rooms(ModelRc::new(VecModel::from(rooms)));
    app.set_main_data(data);
    Ok(())
}

pub fn apply_settings(app: &AppWindow) -> Result<(), String> {
    app.set_show_main(false);
    // Same shell fixture values as the main capture: the operator role hides profile selection.
    app.set_username("CaptureOperator".into());
    app.set_update_label("Доступна 1.0.20".into());
    app.set_active(3);
    app.set_settings_tab(3);
    app.set_settings_tabs(ModelRc::new(VecModel::from(
        ["ОБЩИЕ", "JAVA И ПАМЯТЬ", "КОМАНДЫ", "СКИН", "ЛАУНЧЕР"]
            .map(SharedString::from)
            .to_vec(),
    )));
    app.set_settings_title("НАСТРОЙКИ".into());
    app.set_settings_subtitle("/ ПАРАМЕТРЫ ЛАУНЧЕРА".into());
    // Fixture preview_skin is custom with empty file rows, classic model.
    app.set_skin_path("".into());
    app.set_cape_path("".into());
    app.set_slim(false);
    app.set_skin_caption("ВАШ СКИН".into());
    app.set_skin_error("".into());
    Ok(())
}

pub fn find_option(options: &slint::ModelRc<slint::SharedString>, search: &str) -> i32 {
    use slint::Model;
    options
        .iter()
        .position(|option| option.to_lowercase().starts_with(&search.to_lowercase()))
        .map(|index| index as i32)
        .unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn select_typeahead_handles_cyrillic_and_prefixes() {
        let options = ModelRc::new(VecModel::from(vec!["Игра".into(), "Мастерская".into()]));
        assert_eq!(find_option(&options, "ма"), 1);
        assert_eq!(find_option(&options, "ИГ"), 0);
        assert_eq!(find_option(&options, "нет"), -1);
    }
}
