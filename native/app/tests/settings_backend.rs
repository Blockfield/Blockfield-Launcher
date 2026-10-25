//! Exercise the native form payload through the production save op in a disposable directory.
use blockfield_native::{SettingsData, settings_tabs::config_from_form};
use launcher_app::{LauncherAppState, ops::settings};
use launcher_core::{
    config::{LauncherConfig, load_config},
    game::GameState,
};
use launcher_net::download::{Downloader, noop_guard_sink};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

#[test]
fn form_save_preserves_account_and_other_profile_and_rejects_invalid_writes() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache/u42")
        .join(format!("settings-backend-{}", std::process::id()));
    std::fs::create_dir_all(root.join("game")).unwrap();
    std::fs::create_dir_all(root.join("workshop")).unwrap();
    let config = LauncherConfig {
        game_dir: root.join("game").to_string_lossy().into(),
        workshop_game_dir: root.join("workshop").to_string_lossy().into(),
        username: "FixtureOperator".into(),
        access_token: "fixture-token".into(),
        api_token: "fixture-api".into(),
        discord_presence: false,
        ..Default::default()
    };
    let downloader = Arc::new(Downloader::new(
        Arc::new(|_| {}),
        Arc::new(|_, _| {}),
        noop_guard_sink(),
    ));
    let state = LauncherAppState {
        cancel_flag: downloader.cancel_flag.clone(),
        downloader,
        config: RwLock::new(config.clone()),
        app_data_dir: root.clone(),
        pack: RwLock::new(None),
        game: Arc::new(GameState::new(|_| {})),
        operation: Mutex::new(()),
        events: Arc::new(|_| {}),
    };
    let data = SettingsData {
        game_dir: config.game_dir.clone().into(),
        ram: 4,
        max_ram: 12,
        lang: "ru".into(),
        pre_launch: "echo before\necho after".into(),
        workshop_server: config.workshop_server.clone().into(),
        ..Default::default()
    };
    settings::save_settings(&state, &root, config_from_form(&data).unwrap()).unwrap();
    let saved = load_config(&root);
    assert_eq!(saved.access_token, config.access_token);
    assert_eq!(saved.api_token, config.api_token);
    assert_eq!(saved.username, config.username);
    assert_eq!(saved.workshop_game_dir, config.workshop_game_dir);
    assert_eq!(saved.pre_launch_command, "echo before\necho after");
    let before = std::fs::read(launcher_core::config::config_path(&root)).unwrap();
    let mut invalid = data.clone();
    invalid.java_path = root.join("missing-java").to_string_lossy().as_ref().into();
    assert_eq!(
        settings::save_settings(&state, &root, config_from_form(&invalid).unwrap()).unwrap_err(),
        "Selected Java executable does not exist"
    );
    invalid = data.clone();
    invalid.workshop = true;
    assert_eq!(
        settings::save_settings(&state, &root, config_from_form(&invalid).unwrap()).unwrap_err(),
        "Профиль изменился. Откройте настройки заново."
    );
    let guard = state.operation.try_lock().unwrap();
    assert_eq!(
        settings::save_settings(&state, &root, config_from_form(&data).unwrap()).unwrap_err(),
        "Дождитесь завершения текущей операции."
    );
    drop(guard);
    let game = state.game.begin().unwrap();
    assert_eq!(
        settings::save_settings(&state, &root, config_from_form(&data).unwrap()).unwrap_err(),
        "Закройте игру перед изменением настроек."
    );
    drop(game);
    assert_eq!(
        std::fs::read(launcher_core::config::config_path(&root)).unwrap(),
        before
    );
    invalid.max_ram = 1;
    assert!(config_from_form(&invalid).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
