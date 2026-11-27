//! Fail-before parity tests for D2 (UpdateScreen, LauncherUpdatePanel,
//! MainScreen error/routing, fatal init error screen).
//!
//! Uses `init_no_event_loop` once per test binary, driving the real Slint UI
//! tree and production wiring (`wire_app`, `bridge::apply`, `backend::apply_init_error`).

use blockfield_native::{
    AppWindow, LauncherUpdateData, MainData, UpdateData, backend, bridge, fixtures, update,
    wire_app,
};
use i_slint_backend_testing::ElementHandle;
use launcher_app::{DownloadMirrorPayload, LauncherAppState, LauncherEvent};
use launcher_core::game::{GamePhase, GameStatus};
use launcher_net::download::{DownloadProgressPayload, ProgressUnit};
use slint::{ComponentHandle, Model};

fn has_text(app: &AppWindow, text: &str) -> bool {
    ElementHandle::find_by_accessible_label(app, text)
        .next()
        .is_some()
}

#[test]
fn update_flow_parity_suite() {
    i_slint_backend_testing::init_no_event_loop();

    test_f04_bridge_updates_update_screen();
    test_f04_default_update_data_matches_tauri();
    test_r7_idle_update_screen_ignores_main_launch_events();
    test_r11_core_main_ops_routing_and_profile_block_guard();
    test_f08_footer_click_and_launcher_update_panel_sync();
    test_f11_main_screen_errors_use_friendly_error();
    test_f19_fatal_init_error_dedicated_screen_with_retry();
    test_f22_file_progress_hint_banner();
    test_r13_error_summary_visible_above_800px();
    test_f26_launcher_panel_labels_and_version();
    test_f28_progress_precision_and_cancel_label();
}

fn test_f04_bridge_updates_update_screen() {
    let app = AppWindow::new().unwrap();
    wire_app(&app, backend::Backend::Fixtures).unwrap();
    app.set_show_main(true);
    app.set_active(1);
    app.set_update_data(UpdateData {
        phase: "downloading".into(),
        mirror_name: "blockfield.pro".into(),
        ..Default::default()
    });

    bridge::apply(
        &app,
        LauncherEvent::GameStatus(GameStatus {
            phase: GamePhase::Running,
            revision: 1,
        }),
    );
    let data = app.get_update_data();
    assert_eq!(
        data.game_phase, "running",
        "F04: GameStatus reaches UpdateData"
    );
    assert_eq!(data.game_label, "В ИГРЕ", "F04: game_label matches Tauri");
    assert_eq!(
        data.game_sub, "Закройте игру, чтобы запустить снова",
        "F04: game_sub matches Tauri"
    );

    let _active = update::enter_active_run();

    bridge::apply(
        &app,
        LauncherAppState::status_event("modpack", "Синхронизация модов", true),
    );
    let data = app.get_update_data();
    assert_eq!(data.phase, "downloading", "F04: Status updates phase");
    assert_eq!(data.status_message, "Синхронизация модов");
    assert!(data.can_cancel, "F04: cancelable flag forwarded");
    assert_eq!(data.step1, 1, "F04: modpack phase activates step1");
    assert!(
        data.log.row_count() >= 1,
        "F04: Status appends to update log"
    );

    bridge::apply(
        &app,
        LauncherEvent::DownloadMirror(DownloadMirrorPayload {
            name: "Gcore CDN".into(),
            speed: Some("12.4".into()),
        }),
    );
    let data = app.get_update_data();
    assert_eq!(
        data.mirror_name, "Gcore CDN",
        "F04: DownloadMirror updates mirror_name"
    );
    assert_eq!(
        data.mirror_speed, "12.4 MB/s",
        "Review #10: DownloadMirror preserves fallback mirror_speed"
    );
    assert_eq!(
        data.speed, "—",
        "Review #10: DownloadMirror resets live throughput speed"
    );

    let mut progress = DownloadProgressPayload::files("mods/blockfield.jar".into(), 1, 4);
    progress.unit = ProgressUnit::Bytes;
    progress.total_bytes_downloaded = 512;
    progress.total_bytes_all = 2048;
    bridge::apply(&app, LauncherEvent::DownloadProgress(progress));
    let data = app.get_update_data();
    assert_eq!(data.progress_unit, "bytes");
    assert_eq!(data.current_file, "mods/blockfield.jar");
    assert_eq!(data.downloaded, "512 B");
    assert_eq!(data.total, "2 KB");
    assert_eq!(data.remaining, "3");
    assert!((data.progress - 25.0).abs() < 0.01);

    bridge::apply(
        &app,
        LauncherAppState::status_event("runtime", "Подготовка Minecraft", false),
    );
    let data = app.get_update_data();
    assert_eq!(data.step1, 2, "F04: runtime marks step1 done");
    assert_eq!(data.step2, 1, "F04: runtime activates step2");
    assert!(!data.can_cancel);
}

fn test_f04_default_update_data_matches_tauri() {
    let data = update::default_data();
    assert_eq!(
        data.patch, "/ ПАТЧ ...",
        "F28: ASCII ellipsis in initial patch"
    );
    assert_eq!(data.downloaded, "0 B", "F04: initial downloaded bytes");
    assert_eq!(data.total, "0 B", "F04: initial total bytes");
    assert_eq!(data.remaining, "—", "F04: initial remaining placeholder");
    assert_eq!(data.speed, "—", "F04: initial speed placeholder");
    assert_eq!(
        data.description,
        "Лаунчер скачает обновления и проверит файлы игры. Дождитесь завершения перед запуском.",
        "Review #13: default_data().description matches Tauri i18n.tsx"
    );
}

fn test_r7_idle_update_screen_ignores_main_launch_events() {
    let app = AppWindow::new().unwrap();
    wire_app(&app, backend::Backend::Fixtures).unwrap();
    app.set_show_main(true);
    app.set_active(0);
    app.set_update_data(UpdateData {
        phase: "uptodate".into(),
        mirror_name: "mirror.blockfield.pro".into(),
        step0: 2,
        step1: 2,
        step2: 2,
        step3: 0,
        game_phase: "idle".into(),
        ..Default::default()
    });

    bridge::apply(
        &app,
        LauncherAppState::status_event("launch", "Запуск игры", false),
    );
    bridge::apply(
        &app,
        LauncherEvent::DownloadMirror(DownloadMirrorPayload {
            name: "Gcore CDN".into(),
            speed: Some("4.1".into()),
        }),
    );
    bridge::apply(
        &app,
        LauncherEvent::DownloadProgress(DownloadProgressPayload::files(
            "mods/blockfield.jar".into(),
            1,
            4,
        )),
    );
    bridge::apply(
        &app,
        LauncherEvent::GameStatus(GameStatus {
            phase: GamePhase::Launching,
            revision: 1,
        }),
    );
    assert_eq!(
        app.get_update_data().game_phase,
        "launching",
        "Review #7: GameStatus stays active when UpdateScreen is idle"
    );
    bridge::apply(
        &app,
        LauncherEvent::GameStatus(GameStatus {
            phase: GamePhase::Idle,
            revision: 2,
        }),
    );
    let data = app.get_update_data();
    assert_eq!(
        data.phase, "uptodate",
        "Review #7: MainScreen launch Status must not wedge idle UpdateScreen in launching"
    );
    assert_eq!(data.step3, 0, "Review #7: idle UpdateScreen step3 stays 0");
    assert_eq!(
        data.mirror_name, "mirror.blockfield.pro",
        "Review #7: idle UpdateScreen ignores unrelated DownloadMirror"
    );
    assert_eq!(data.game_phase, "idle");
}

fn test_r11_core_main_ops_routing_and_profile_block_guard() {
    use std::{cell::RefCell, rc::Rc, sync::Arc};
    let rt = blockfield_native::runtime::new().unwrap();
    let temp_dir = std::env::temp_dir().join(format!("bf-d2-core-parity-{}", std::process::id()));
    let downloader = Arc::new(launcher_net::download::Downloader::new(
        Arc::new(|_| {}),
        Arc::new(|_, _| {}),
        launcher_net::download::noop_guard_sink(),
    ));
    let state = Arc::new(LauncherAppState {
        cancel_flag: downloader.cancel_flag.clone(),
        downloader,
        pack: tokio::sync::RwLock::new(None),
        config: tokio::sync::RwLock::new(launcher_core::config::LauncherConfig::default()),
        app_data_dir: temp_dir.clone(),
        game: Arc::new(launcher_core::game::GameState::new(|_| {})),
        operation: tokio::sync::Mutex::new(()),
        events: Arc::new(|_| {}),
    });

    let app = AppWindow::new().unwrap();
    let routed = Rc::new(RefCell::new(Vec::<bool>::new()));
    let routed_spy = routed.clone();
    app.on_open_updates(move |auto_launch| routed_spy.borrow_mut().push(auto_launch));
    blockfield_native::main_ops::attach(&app, state.clone(), rt.handle().clone());

    let mut main = MainData {
        phase: "idle".into(),
        checked: true,
        checking: false,
        needs_setup: true,
        ..Default::default()
    };
    app.set_main_data(main.clone());

    let pending = blockfield_native::main_ops::shared_pending();
    *pending.0.lock().unwrap() = None;
    app.invoke_deploy();
    assert_eq!(
        *routed.borrow(),
        vec![false],
        "Review #11 / Revix: plain ОБНОВИТЬ routes with auto_launch = false"
    );

    *pending.0.lock().unwrap() = Some("room-1".into());
    app.invoke_deploy();
    assert_eq!(
        *routed.borrow(),
        vec![false, true],
        "Review #11: invite-driven Deploy routes with auto_launch = true"
    );

    app.invoke_download_modpack();
    assert_eq!(
        *routed.borrow(),
        vec![false, true, false],
        "Review #11: explicit Download never arms auto_launch even with pending invite"
    );
    *pending.0.lock().unwrap() = None;

    // Attach Core update handlers and verify profile_blocked guards open_updates.
    update::attach_core(&app, state.clone(), rt.handle().clone());
    app.set_active(0);
    main.profile_blocked = true;
    app.set_main_data(main);
    app.invoke_open_updates(true);
    assert_eq!(
        app.get_active(),
        0,
        "Review #11: open_updates must ignore navigation while profile is blocked"
    );

    rt.shutdown_timeout(std::time::Duration::from_secs(1));
    let _ = std::fs::remove_dir_all(temp_dir);
}

fn test_f08_footer_click_and_launcher_update_panel_sync() {
    let app = AppWindow::new().unwrap();
    wire_app(&app, backend::Backend::Fixtures).unwrap();
    app.set_show_main(true);
    app.set_active(0);
    app.set_settings_tab(0);

    // Clicking the footer version button opens Settings -> Лаунчер (tab 4).
    app.invoke_launcher_update();
    assert_eq!(app.get_active(), 3, "F08: footer click opens Settings");
    assert_eq!(
        app.get_settings_tab(),
        4,
        "F08: footer click opens Settings tab 4 (Лаунчер)"
    );

    // Default launcher update data carries the real crate version, not "—".
    let fresh = update::default_launcher_data();
    assert_eq!(
        fresh.current_version,
        env!("CARGO_PKG_VERSION"),
        "F08: current_version initialized from crate version"
    );

    // Panel phase changes mirror into Shell footer update_label.
    fixtures::apply_launcher_update(&app, "idle").unwrap();
    app.set_update_label("".into());
    app.invoke_launcher_check();
    assert_eq!(
        app.get_update_label(),
        "Доступна 1.0.20",
        "F08: available update mirrors to footer update_label"
    );
    app.invoke_launcher_install();
    assert_eq!(
        app.get_update_label(),
        "Перезапустить",
        "F08: installed update mirrors to footer update_label"
    );
}

fn test_f11_main_screen_errors_use_friendly_error() {
    let app = AppWindow::new().unwrap();
    wire_app(&app, backend::Backend::Fixtures).unwrap();
    fixtures::apply_main(&app, "main", "default").unwrap();
    app.show().unwrap();

    // Network check error uses network-specific friendly message, not static fallback.
    let mut data: MainData = app.get_main_data();
    data.check_error = "error trying to connect: dns error".into();
    data.launch_error = "".into();
    app.set_main_data(data);
    assert!(
        has_text(
            &app,
            "Нет доступа к серверу сборки blockfield.pro — проверьте интернет и повторите."
        ),
        "F11: check_error renders network friendly_error message"
    );

    // PACK_REPAIR launch error uses repair wording from friendly_error, not generic launch failure.
    let mut data = app.get_main_data();
    data.check_error = "".into();
    data.launch_error = "PACK_REPAIR: pack files changed or are missing.".into();
    app.set_main_data(data);
    assert!(
        has_text(
            &app,
            "Файлы игры повреждены — откройте «Обновления», чтобы восстановить их."
        ),
        "F11: launch_error renders PACK_REPAIR friendly_error message"
    );
}

fn test_f19_fatal_init_error_dedicated_screen_with_retry() {
    let app = AppWindow::new().unwrap();
    wire_app(&app, backend::Backend::Fixtures).unwrap();
    app.show().unwrap();
    backend::apply_init_error(&app, "Cannot initialize launcher: permission denied".into());
    assert!(
        !app.get_show_main(),
        "F19: fatal init error must not land on MainScreen"
    );
    assert!(
        has_text(
            &app,
            "Не удалось загрузить настройки. Проверьте доступ к папке настроек и повторите."
        ),
        "F19: fatal init error shows SettingsLoad friendly message"
    );
    assert!(
        has_text(&app, "Повторить"),
        "F19: fatal init error screen offers Повторить button"
    );
    let retried = std::rc::Rc::new(std::cell::Cell::new(false));
    let retried_flag = retried.clone();
    app.on_retry_init(move || retried_flag.set(true));
    let retry_btn = ElementHandle::find_by_accessible_label(&app, "Повторить")
        .find(|el| el.accessible_role() != Some(i_slint_backend_testing::AccessibleRole::Text))
        .expect("F19: Повторить button exists");
    retry_btn.invoke_accessible_default_action();
    assert!(retried.get(), "F19: clicking Повторить invokes retry_init");
}

fn test_f22_file_progress_hint_banner() {
    let app = AppWindow::new().unwrap();
    wire_app(&app, backend::Backend::Fixtures).unwrap();
    fixtures::apply_update(&app, "default").unwrap();
    let mut data = app.get_update_data();
    data.phase = "downloading".into();
    data.progress_unit = "files".into();
    app.set_update_data(data);
    app.show().unwrap();
    assert!(
        has_text(
            &app,
            "Крупные файлы скачиваются дольше. Текущий файл, объём и зеркало показаны ниже."
        ),
        "F22: files-mode downloading renders fileProgressHint banner"
    );
}

fn test_r13_error_summary_visible_above_800px() {
    let app = AppWindow::new().unwrap();
    wire_app(&app, backend::Backend::Fixtures).unwrap();
    fixtures::apply_update(&app, "error").unwrap();
    let msg = "Не удалось загрузить обновление";
    app.show().unwrap();
    app.window()
        .set_size(slint::LogicalSize::new(1280.0, 720.0));
    let at_720 = ElementHandle::find_by_accessible_label(&app, msg).count();
    app.window()
        .set_size(slint::LogicalSize::new(1280.0, 960.0));
    let at_960 = ElementHandle::find_by_accessible_label(&app, msg).count();
    assert!(
        at_960 > at_720,
        "Review #13: UpdateScreen error summary text renders above 800px height (720p={at_720}, 960p={at_960})"
    );
}

fn test_f26_launcher_panel_labels_and_version() {
    let app = AppWindow::new().unwrap();
    wire_app(&app, backend::Backend::Fixtures).unwrap();
    fixtures::apply_settings(&app).unwrap();
    app.set_settings_tab(4);
    app.set_launcher_update_data(LauncherUpdateData {
        phase: "available".into(),
        current_version: "2.0.1".into(),
        update_version: "2.0.2".into(),
        ..Default::default()
    });
    app.show().unwrap();
    assert!(
        has_text(&app, "Установить 2.0.2"),
        "F26: install button interpolates target version in sentence case"
    );
    app.set_launcher_update_data(LauncherUpdateData {
        phase: "downloading".into(),
        current_version: "2.0.1".into(),
        update_version: "2.0.2".into(),
        ..Default::default()
    });
    assert!(
        has_text(&app, "Обновление выполняется"),
        "F26: install button shows busy label while downloading"
    );
    app.set_launcher_update_data(LauncherUpdateData {
        phase: "installed".into(),
        current_version: "2.0.1".into(),
        update_version: "2.0.2".into(),
        installed: true,
        ..Default::default()
    });
    assert!(
        has_text(&app, "Перезапустить лаунчер"),
        "F26: restart button uses Tauri sentence case"
    );
}

fn test_f28_progress_precision_and_cancel_label() {
    let app = AppWindow::new().unwrap();
    wire_app(&app, backend::Backend::Fixtures).unwrap();
    fixtures::apply_update(&app, "complete").unwrap();
    app.show().unwrap();
    assert!(
        has_text(&app, "100.0"),
        "F28: progress readout keeps 1 fixed decimal (100.0%)"
    );
    fixtures::apply_update(&app, "default").unwrap();
    assert!(
        has_text(&app, "CANCEL"),
        "F28: cancel button label matches Tauri CANCEL"
    );
}
