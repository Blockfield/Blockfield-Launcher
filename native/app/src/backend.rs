//! Fixture selection is explicit; a plain launch reads the shared launcher core.
use std::{
    cell::Cell,
    collections::BTreeMap,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use launcher_app::{
    LauncherAppState, LauncherEvent,
    ops::{pack_check, settings},
    status_ops,
};
use launcher_core::{
    config::ClientProfile,
    errors::{ErrorContext, friendly_error},
    roles::has_workshop_access,
};
use launcher_net::{
    account, minecraft,
    status::{GameRoom, ServerStatus},
};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use tokio::runtime::Handle;

use crate::{AppWindow, BattlePoint, MainData, Room, SettingsForm, bridge, runtime};

const BACKGROUND_PROBE_INTERVAL: Duration = Duration::from_secs(60);

thread_local! {
    /// Suppresses recursive `changed workshop => SettingsForm.profile(...)` callbacks
    /// while Rust updates `app.workshop` programmatically (load, success, or revert).
    static SUPPRESS_WORKSHOP_CHANGE: Cell<bool> = const { Cell::new(false) };
}

fn set_workshop_quietly(app: &AppWindow, workshop: bool) {
    SUPPRESS_WORKSHOP_CHANGE.with(|flag| {
        let prev = flag.replace(true);
        app.set_workshop(workshop);
        flag.set(prev);
    });
}

fn pending_profile_retry() -> &'static Mutex<Option<ClientProfile>> {
    static PENDING: OnceLock<Mutex<Option<ClientProfile>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(None))
}

type SessionHandle = (Arc<LauncherAppState>, Handle);

fn active_session() -> &'static Mutex<Option<SessionHandle>> {
    static SESSION: OnceLock<Mutex<Option<SessionHandle>>> = OnceLock::new();
    SESSION.get_or_init(|| Mutex::new(None))
}

fn pack_check_gen() -> &'static AtomicU64 {
    static GEN: AtomicU64 = AtomicU64::new(0);
    &GEN
}

fn server_refresh_gen() -> &'static AtomicU64 {
    static GEN: AtomicU64 = AtomicU64::new(0);
    &GEN
}

fn role_refresh_in_flight() -> &'static AtomicBool {
    static BUSY: AtomicBool = AtomicBool::new(false);
    &BUSY
}

fn pack_probe_in_flight() -> &'static AtomicBool {
    static BUSY: AtomicBool = AtomicBool::new(false);
    &BUSY
}

struct AtomicReset(&'static AtomicBool);

impl Drop for AtomicReset {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Starts the never-ending background loops (10s server poll, 60s probe, presence)
/// exactly once per process: every enter_main re-runs only the one-shot
/// checks. Tauri starts both loops once at setup the same way.
fn claim_background_loops() -> bool {
    static STARTED: AtomicBool = AtomicBool::new(false);
    !STARTED.swap(true, Ordering::SeqCst)
}

pub enum Backend {
    Fixtures,
    Core(Handle),
}

pub fn fixtures_requested(args: &[String]) -> bool {
    args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "--state"
                | "--scenario"
                | "--e2e"
                | "--capture-dir"
                | "--e2e-golden"
                | "--e2e-update-goldens"
        )
    }) || std::env::var_os("BLOCKFIELD_CAPTURE_STATE").is_some()
}

fn initial_main_data() -> MainData {
    MainData {
        checking: true,
        progress: -1,
        phase: "idle".into(),
        players: "—".into(),
        players_max: "".into(),
        ping: "—".into(),
        region: "—".into(),
        installed: "...".into(),
        latest: "...".into(),
        size: "—".into(),
        ..Default::default()
    }
}

/// Maps canonical site role names to Tauri Shell's uppercase badge labels (`App.tsx:31-36`).
pub fn role_label(role: Option<&str>) -> &'static str {
    match role {
        Some("owner") => "ВЛАДЕЛЕЦ",
        Some("admin") => "АДМИНИСТРАТОР",
        Some("moderator") => "МОДЕРАТОР",
        Some("builder") => "СТРОИТЕЛЬ",
        _ => "ИГРОК",
    }
}

/// Extract `profile.role` from a `stats_player` payload.
pub fn extract_player_role(data: Option<&serde_json::Value>) -> Option<String> {
    data.and_then(|v| v.pointer("/profile/role"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Format footer `server-label` (`App.tsx:357`, `Shell.tsx:56-57,128`).
pub fn format_footer_server_label(
    workshop: bool,
    profile_blocked: bool,
    workshop_server: &str,
    cms_server_ip: &str,
) -> String {
    if !profile_blocked && workshop {
        let addr = workshop_server.trim();
        if addr.is_empty() {
            "Адрес мастерской не задан".to_owned()
        } else {
            format!("IP · {addr}")
        }
    } else {
        let fallback = crate::content::default_server_ip();
        let addr = if cms_server_ip.trim().is_empty() {
            fallback.as_str()
        } else {
            cms_server_ip.trim()
        };
        format!("IP · {addr}")
    }
}

/// Recompute and apply the footer `server-label` from active profile, workshop server, and CMS `serverIp`.
pub fn sync_footer_server_label(app: &AppWindow) {
    let main = app.get_main_data();
    let workshop_server = app.global::<SettingsForm>().get_data().workshop_server;
    let label = format_footer_server_label(
        app.get_workshop(),
        main.profile_blocked,
        workshop_server.as_str(),
        main.server_ip_fallback.as_str(),
    );
    app.set_server_label(label.into());
}

/// Apply a `stats_player` role lookup outcome to the header badge and Workshop access gate.
/// Returns `true` when Workshop access is revoked on an active Workshop profile and the caller
/// must downgrade back to `ClientProfile::Game` (Tauri `App.tsx:52-55,110-115,141-166`).
pub fn apply_role_outcome(app: &AppWindow, outcome: &crate::stats::StatsOutcome) -> bool {
    // A failed lookup (no data and not stale) keeps the previous role so a transient
    // error never triggers the workshop->game auto-downgrade.
    if !outcome.stale && outcome.error.is_some() {
        return false;
    }
    let role = extract_player_role(outcome.data.as_ref());
    let can_workshop = has_workshop_access(role.as_deref());
    app.set_role(role_label(role.as_deref()).into());
    let mut data = app.get_main_data();
    data.can_workshop = can_workshop;
    data.profile_blocked = data.profile_changing
        || (!can_workshop && app.get_workshop())
        || !data.profile_error.is_empty();
    let should_downgrade = !can_workshop
        && app.get_workshop()
        && !data.profile_changing
        && data.profile_error.is_empty();
    app.set_main_data(data);
    sync_footer_server_label(app);
    should_downgrade
}

impl Backend {
    pub fn start(self, app: &AppWindow) {
        let Self::Core(handle) = self else {
            crate::fixtures::apply(app);
            crate::main_ops::attach_fixtures(app);
            crate::auth::attach_fixtures(app);
            crate::update::attach_fixtures(app);
            crate::stats::wire_fixtures(app);
            crate::settings_fixtures::defaults(app);
            crate::settings_tabs::wire(app, None);
            return;
        };
        crate::content::reset_cache();
        app.global::<SettingsForm>().set_loading(true);
        app.set_settings_tab(0);
        // The auth gate owns the window until sign-in: logged-out users must
        // never see the main Shell, even though settings init runs up front.
        app.set_show_main(false);
        app.set_show_login(false);
        app.set_show_firstrun(false);
        app.set_dev_build(launcher_core::dev::is_dev_build());
        apply_dev_status(app);
        app.set_update_label("".into());
        app.set_main_data(initial_main_data());
        crate::content::apply_shell_defaults(app);
        crate::content::spawn_fetch(app.as_weak(), &handle);
        let events = bridge::sink(app);
        let weak = app.as_weak();
        let route_handle = handle.clone();
        handle.spawn(async move {
            let initialized = tokio::task::spawn_blocking(move || {
                let state = runtime::build_state(events)?;
                // SettingsResponse keeps its fields private; serialize the redacted op response.
                let settings = settings::load_settings(&state).map_err(std::io::Error::other)?;
                let settings = serde_json::to_value(settings)?;
                Ok::<_, std::io::Error>((state, settings))
            })
            .await
            .map_err(std::io::Error::other)
            .and_then(|result| result);
            let (state, settings) = match initialized {
                Ok(initialized) => initialized,
                Err(error) => {
                    let message = format!("Cannot initialize launcher: {error}");
                    eprintln!("{message}");
                    let _ = weak.upgrade_in_event_loop(move |app| {
                        apply_init_error(&app, message);
                    });
                    return;
                }
            };
            let status = account_status(&state).await;
            let stored_username = settings["username"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let _ = weak.upgrade_in_event_loop(move |app| {
                let handle = route_handle.clone();
                crate::auth::attach(&app, state.clone(), handle.clone());
                match status {
                    Ok(status) => crate::auth::enter_startup(&app, state, handle, status),
                    Err(error) => {
                        app.set_login_mode("login".into());
                        app.set_login_set_password(false);
                        app.set_login_username(stored_username.into());
                        app.set_login_error(error.into());
                        app.set_show_main(false);
                        app.set_show_firstrun(false);
                        app.set_show_login(true);
                    }
                }
            });
        });
    }
}

/// Show a fatal init failure on the main screen. Startup hides every
/// screen until auth resolves, so without this the error would sit in
/// empty chrome nobody can see.
pub fn apply_init_error(app: &AppWindow, message: String) {
    // The settings overlay would otherwise hide this error and block input.
    app.global::<SettingsForm>().set_loading(false);
    app.set_show_main(true);
    let mut data = app.get_main_data();
    data.checking = false;
    data.check_error = message.clone().into();
    app.set_main_data(data);
    // TOP has no live data without init: show the failure there too
    // instead of leaving the board on its loading spinner.
    app.set_stats_board_state(1);
    app.set_stats_board_error(message.clone().into());
    app.set_stats_self_state(2);
    app.set_stats_self_error(message.into());
}

async fn account_status(state: &LauncherAppState) -> Result<account::AccountStatus, String> {
    let skins = state
        .pack
        .read()
        .await
        .as_ref()
        .and_then(|pack| pack.info.skins.clone());
    account::account_status(&state.config, &state.app_data_dir, skins).await
}

/// Fetch player role via `stats_player(uuid, {})` with stale-while-error cache,
/// update the header role badge & `can_workshop`, and auto-downgrade `workshop -> game` on revocation.
pub fn trigger_role_refresh(app: &AppWindow, state: Arc<LauncherAppState>, handle: Handle) {
    if role_refresh_in_flight().swap(true, Ordering::AcqRel) {
        return;
    }
    let weak = app.as_weak();
    let spawn = handle.clone();
    spawn.spawn(async move {
        let _guard = AtomicReset(role_refresh_in_flight());
        let uuid = state.config.read().await.player_uuid.to_lowercase();
        if uuid.is_empty() {
            return;
        }
        let cache = crate::stats::cache_dir(&state.app_data_dir);
        let outcome = crate::stats::load_stats(
            crate::stats::StatsKind::Player,
            &uuid,
            &BTreeMap::new(),
            &cache,
        )
        .await;
        let _ = weak.upgrade_in_event_loop(move |app| {
            if apply_role_outcome(&app, &outcome) {
                select_profile(&app, state, handle, false);
            }
        });
    });
}

/// Run `check_modpack_version` for the active profile.
/// When `silent` is `true` (60s timer / window focus probe), skips busy states and
/// ignores transient errors so background probes never flicker UI state.
pub fn trigger_pack_check(
    app: &AppWindow,
    state: Arc<LauncherAppState>,
    handle: Handle,
    silent: bool,
) {
    let generation = if silent {
        let data = app.get_main_data();
        if !app.get_show_main()
            || !data.checked
            || data.checking
            || data.launching
            || data.phase != "idle"
            || data.profile_changing
            || data.profile_blocked
        {
            return;
        }
        if pack_probe_in_flight().swap(true, Ordering::AcqRel) {
            return;
        }
        pack_check_gen().load(Ordering::Acquire)
    } else {
        pack_check_gen().fetch_add(1, Ordering::AcqRel) + 1
    };
    let weak = app.as_weak();
    handle.spawn(async move {
        let _probe_guard = silent.then(|| AtomicReset(pack_probe_in_flight()));
        let result = pack_check::check_modpack_version(&state, |dir, meta| match &meta.loader {
            Some(loader) => {
                let id = minecraft::loader_version_id(&meta.minecraft, loader);
                minecraft::loader_profile_exists(dir, &id)
                    && minecraft::has_launch_dependencies(dir, &meta.minecraft, Some(&id))
            }
            None => minecraft::has_launch_dependencies(dir, &meta.minecraft, None),
        })
        .await;
        if pack_check_gen().load(Ordering::Acquire) != generation {
            return;
        }
        if let Err(error) = &result {
            if silent {
                return;
            }
            eprintln!("check_modpack_version: {error}");
        }
        let _ = weak.upgrade_in_event_loop(move |app| {
            if pack_check_gen().load(Ordering::Acquire) != generation {
                return;
            }
            let mut data = app.get_main_data();
            if !silent {
                data.checking = false;
            }
            match result {
                Ok(version) => {
                    data.checked = true;
                    data.needs_update = version.needs_update;
                    data.needs_setup =
                        version.needs_update || !version.java_ok || !version.loader_ok;
                    data.installed = version.installed_version.into();
                    data.latest = version.remote_version.into();
                    data.size = if version.total_size == 0 {
                        "—".into()
                    } else {
                        format!("{:.1} GB", version.total_size as f64 / 1e9).into()
                    };
                    data.check_error = "".into();
                }
                Err(error) => data.check_error = error.into(),
            }
            app.set_main_data(data);
            // Pack check done: a waiting invite retries now (Tauri
            // MainScreen.tsx pending-room effect).
            crate::bridge::retry_pending_join(&app);
        });
    });
}

/// Immediately poll `server_status` for the active profile, discarding any older in-flight poll.
pub fn trigger_server_refresh(app: &AppWindow, state: Arc<LauncherAppState>, handle: Handle) {
    let generation = server_refresh_gen().fetch_add(1, Ordering::AcqRel) + 1;
    let weak = app.as_weak();
    handle.spawn(async move {
        let result = match pack_check::pack_base_url() {
            Ok(base) => status_ops::server_status(&state, &base).await,
            Err(error) => Err(error),
        };
        if server_refresh_gen().load(Ordering::Acquire) != generation {
            return;
        }
        if let Err(error) = &result {
            eprintln!("server_status: {error}");
        }
        let _ = weak.upgrade_in_event_loop(move |app| {
            if server_refresh_gen().load(Ordering::Acquire) == generation {
                apply_server(&app, result);
            }
        });
    });
}

/// Background 60s + window-focus probe (F18): refresh role and silently re-check modpack version.
pub fn probe_background(app: &AppWindow, state: Arc<LauncherAppState>, handle: Handle) {
    if !app.get_show_main() {
        return;
    }
    trigger_role_refresh(app, state.clone(), handle.clone());
    trigger_pack_check(app, state, handle, true);
}

/// Switch active profile (`Game` / `Workshop`), resyncing pack check, server status, and footer IP;
/// on error revert `app.workshop` and surface `main_data.profile_error` (F16).
pub fn select_profile(
    app: &AppWindow,
    state: Arc<LauncherAppState>,
    handle: Handle,
    workshop: bool,
) {
    if SUPPRESS_WORKSHOP_CHANGE.with(Cell::get) {
        return;
    }
    let form = app.global::<SettingsForm>();
    let mut main = app.get_main_data();
    if workshop && !main.can_workshop {
        set_workshop_quietly(app, form.get_data().workshop);
        return;
    }
    if form.get_data().workshop == workshop && main.profile_error.is_empty() {
        return;
    }
    if main.profile_changing || form.get_blocked() {
        return;
    }
    form.set_blocked(true);
    main.profile_changing = true;
    main.profile_blocked = true;
    main.profile_error = "".into();
    app.set_main_data(main);
    sync_footer_server_label(app);
    let weak = app.as_weak();
    let spawn = handle.clone();
    spawn.spawn(async move {
        let profile = if workshop {
            ClientProfile::Workshop
        } else {
            ClientProfile::Game
        };
        let result = settings::select_profile(&state, profile)
            .await
            .and_then(|value| serde_json::to_value(value).map_err(|e| e.to_string()));
        let _ = weak.upgrade_in_event_loop(move |app| match result {
            Ok(value) => {
                crate::settings_tabs::apply_response(&app, &value);
                let actual_workshop = value["activeProfile"] == "workshop";
                set_workshop_quietly(&app, actual_workshop);
                app.global::<SettingsForm>().set_blocked(false);
                *pending_profile_retry()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) = None;

                let mut data = app.get_main_data();
                data.profile_changing = false;
                data.profile_error = "".into();
                data.profile_blocked = !data.can_workshop && actual_workshop;
                // Invalidate stale pack & server status from the previous profile
                // (mirrors Tauri's invalidateModpackCheck() + MainScreen key={profile} remount).
                data.checking = true;
                data.checked = false;
                data.needs_setup = false;
                data.needs_update = false;
                data.installed = "...".into();
                data.latest = "...".into();
                data.size = "—".into();
                data.check_error = "".into();
                data.launch_error = "".into();
                data.launch_stale = false;
                data.online = false;
                data.updating = false;
                data.maintenance = "".into();
                data.rooms_available = false;
                data.players = "—".into();
                data.players_max = "".into();
                data.ping = "—".into();
                data.region = "—".into();
                data.location = "".into();
                data.server_address = "".into();
                app.set_room_index(0);
                app.set_rooms(ModelRc::default());
                app.set_room_names(ModelRc::default());
                app.set_main_data(data);

                sync_footer_server_label(&app);
                trigger_pack_check(&app, state.clone(), handle.clone(), false);
                trigger_server_refresh(&app, state, handle);
                crate::bridge::retry_pending_join(&app);
            }
            Err(error) => {
                let friendly = friendly_error(&error, ErrorContext::SelectProfile);
                let form = app.global::<SettingsForm>();
                let previous_workshop = form.get_data().workshop;
                form.set_error(friendly.message.clone().into());
                form.set_raw_error(friendly.raw.into());
                form.set_blocked(false);
                set_workshop_quietly(&app, previous_workshop);

                let mut data = app.get_main_data();
                data.profile_changing = false;
                data.profile_error = friendly.message.into();
                data.profile_blocked = true;
                app.set_main_data(data);
                *pending_profile_retry()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) = Some(profile);

                sync_footer_server_label(&app);
                crate::bridge::retry_pending_join(&app);
            }
        });
    });
}

/// Retry a failed profile switch once the game returns to `Idle` (Tauri `App.tsx:168-175`).
pub fn retry_pending_profile_if_idle(app: &AppWindow) {
    let target = {
        let guard = pending_profile_retry()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *guard
    };
    let Some(target) = target else {
        return;
    };
    if app.get_main_data().profile_changing {
        return;
    }
    let session = {
        active_session()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    };
    let Some((state, handle)) = session else {
        return;
    };
    let workshop = target == ClientProfile::Workshop && app.get_main_data().can_workshop;
    select_profile(app, state, handle, workshop);
}

/// Resync footer IP, modpack check, and server status after saving Settings (F16).
pub fn on_settings_saved(app: &AppWindow, state: Arc<LauncherAppState>, handle: Handle) {
    sync_footer_server_label(app);
    let mut data = app.get_main_data();
    data.checking = true;
    data.checked = false;
    data.check_error = "".into();
    app.set_main_data(data);
    trigger_pack_check(app, state.clone(), handle.clone(), false);
    trigger_server_refresh(app, state, handle);
}

/// Enter the main Shell: wire MainScreen ops, load settings, then run the
/// pack check, role refresh, server poll and presence loop (moved here so the auth screens
/// own the window until sign-in completes).
pub fn enter_main(app: &AppWindow, state: Arc<LauncherAppState>, handle: Handle) {
    *active_session().lock().unwrap_or_else(|e| e.into_inner()) =
        Some((state.clone(), handle.clone()));
    *pending_profile_retry()
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = None;
    app.set_show_login(false);
    app.set_show_firstrun(false);
    app.set_show_main(true);
    app.set_dev_build(launcher_core::dev::is_dev_build());
    apply_dev_status(app);
    app.set_launcher_label(crate::content::launcher_version_label().into());
    app.set_update_label("".into());
    app.set_main_data(initial_main_data());
    crate::content::apply_cached_or_default(app);
    crate::main_ops::attach(app, state.clone(), handle.clone());
    crate::update::attach_core(app, state.clone(), handle.clone());
    crate::stats::wire_live(app, handle.clone(), state.clone());
    // Window-focus refresh for role + modpack version (F18).
    {
        let weak = app.as_weak();
        let focus_state = state.clone();
        let focus_handle = handle.clone();
        app.on_window_focused(move || {
            if let Some(app) = weak.upgrade() {
                probe_background(&app, focus_state.clone(), focus_handle.clone());
            }
        });
    }
    // The shell avatar resolves as soon as the username is known, without
    // waiting for the first visit to the TOP screen (Tauri Shell parity).
    {
        let weak = app.as_weak();
        let state = state.clone();
        let face_spawn = handle.clone();
        let face_handle = handle.clone();
        face_spawn.spawn(async move {
            let username = state.config.read().await.username.clone();
            let skins = state
                .pack
                .read()
                .await
                .as_ref()
                .and_then(|pack| pack.info.skins.clone());
            let _ = weak.upgrade_in_event_loop(move |app| {
                crate::avatars::refresh_shell(&app);
                crate::avatars::request_faces(&app.as_weak(), &face_handle, skins, vec![username]);
            });
        });
    }
    let settings_handle = handle.clone();
    let weak = app.as_weak();
    handle.spawn(async move {
        let settings = tokio::task::spawn_blocking({
            let state = state.clone();
            move || -> std::io::Result<serde_json::Value> {
                let settings = settings::load_settings(&state).map_err(std::io::Error::other)?;
                serde_json::to_value(settings).map_err(std::io::Error::other)
            }
        })
        .await
        .map_err(std::io::Error::other)
        .and_then(|result| result);
        let settings = match settings {
            Ok(settings) => settings,
            Err(error) => {
                let message = format!("Cannot load settings: {error}");
                eprintln!("{message}");
                let _ = weak.upgrade_in_event_loop(move |app| {
                    let mut data = app.get_main_data();
                    data.checking = false;
                    data.check_error = message.into();
                    app.set_main_data(data);
                });
                return;
            }
        };
        (state.events)(LauncherEvent::GameStatus(status_ops::game_status(&state)));
        let settings_state = state.clone();
        let _ = weak.upgrade_in_event_loop(move |app| {
            crate::settings_tabs::apply_response(&app, &settings);
            crate::settings_tabs::wire(
                &app,
                Some((settings_state.clone(), settings_handle.clone())),
            );
            crate::content::apply_cached_or_default(&app);
            crate::skin_upload::wire_upload(&app, settings_state.clone(), settings_handle.clone());
            app.set_username(settings["username"].as_str().unwrap_or_default().into());
            set_workshop_quietly(&app, settings["activeProfile"] == "workshop");
            sync_footer_server_label(&app);
            // Shell avatar follows the signed-in user like Tauri's Shell block.
            crate::avatars::refresh_shell(&app);
            // F02 + initial pack check + immediate server poll.
            trigger_role_refresh(&app, settings_state.clone(), settings_handle.clone());
            trigger_pack_check(&app, settings_state.clone(), settings_handle.clone(), false);
            trigger_server_refresh(&app, settings_state, settings_handle);
        });
        // Never-ending loops start once per process; re-logins only re-run
        // the one-shot checks above, never piling up duplicate loops.
        if !claim_background_loops() {
            return;
        }
        let server_state = state.clone();
        let server_weak = weak.clone();
        tokio::spawn(async move {
            let mut timer = tokio::time::interval_at(
                tokio::time::Instant::now() + Duration::from_secs(10),
                Duration::from_secs(10),
            );
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                timer.tick().await;
                let generation = server_refresh_gen().load(Ordering::Acquire);
                let result = match pack_check::pack_base_url() {
                    Ok(base) => status_ops::server_status(&server_state, &base).await,
                    Err(error) => Err(error),
                };
                if server_refresh_gen().load(Ordering::Acquire) != generation {
                    continue;
                }
                if let Err(error) = &result {
                    eprintln!("server_status: {error}");
                }
                if server_weak
                    .upgrade_in_event_loop(move |app| {
                        if server_refresh_gen().load(Ordering::Acquire) == generation {
                            apply_server(&app, result);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        // F18: periodic 60s role + modpack version probe.
        let probe_state = state.clone();
        let probe_weak = weak.clone();
        let probe_handle = Handle::current();
        tokio::spawn(async move {
            let mut timer = tokio::time::interval_at(
                tokio::time::Instant::now() + BACKGROUND_PROBE_INTERVAL,
                BACKGROUND_PROBE_INTERVAL,
            );
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                timer.tick().await;
                let state = probe_state.clone();
                let handle = probe_handle.clone();
                if probe_weak
                    .upgrade_in_event_loop(move |app| {
                        probe_background(&app, state, handle);
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        presence(state).await;
    });
}

async fn presence(state: Arc<LauncherAppState>) {
    // Same store the join flow reads: a Discord invite must auto-join through
    // the exact target stored here (Tauri keeps one PendingRoom per app).
    let pending = crate::main_ops::shared_pending();
    loop {
        if state.config.read().await.discord_presence
            && let Err(error) = launcher_app::presence::session(&state, &pending).await
        {
            eprintln!("Discord presence disconnected: {error}");
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Dev panel data (Tauri `dev_status` parity): `None` in production builds,
// so the panel stays invisible exactly like the Tauri DevPanel.
pub fn apply_dev_status(app: &AppWindow) {
    let status = launcher_app::status_ops::dev_status(
        &launcher_app::ops::pack_check::pack_base_url().unwrap_or_default(),
    );
    app.set_dev_data(crate::DevData {
        server: status.as_ref().map_or_else(slint::SharedString::new, |s| {
            if s.server.is_empty() {
                "сервер пака".into()
            } else {
                s.server.as_str().into()
            }
        }),
        freeze: status.as_ref().map_or_else(slint::SharedString::new, |s| {
            if s.freeze_pack {
                "пак заморожен".into()
            } else {
                "выкл".into()
            }
        }),
        jvm_args: status.as_ref().map_or_else(slint::SharedString::new, |s| {
            if s.jvm_args.is_empty() {
                "—".into()
            } else {
                s.jvm_args.as_str().into()
            }
        }),
        pack_url: status
            .as_ref()
            .map_or_else(slint::SharedString::new, |s| s.pack_url.as_str().into()),
    });
}

/// Room subtitle parity with Tauri `RoomList.tsx`: optional format label,
/// mode, map and RU phase name, skipping whatever is absent.
fn room_description(room: &GameRoom) -> String {
    let format = match room.format.as_deref() {
        Some("casual") => Some("Casual"),
        Some("ranked") => Some("Ranked"),
        _ => None,
    };
    let phase = match room.phase.as_str() {
        "IDLE" => "Ожидание игроков",
        "VOTING" => "Голосование",
        "PREPARING" => "Подготовка",
        "GAME" => "Бой",
        "MATCH_END" => "Матч завершён",
        other => other,
    };
    let mut parts = Vec::with_capacity(4);
    if let Some(format) = format {
        parts.push(format);
    }
    if !room.mode.is_empty() {
        parts.push(room.mode.as_str());
    }
    if !room.map.is_empty() {
        parts.push(room.map.as_str());
    }
    if !phase.is_empty() {
        parts.push(phase);
    }
    parts.join(" · ")
}

pub fn apply_server(app: &AppWindow, status: Result<ServerStatus, String>) {
    let selected = app
        .get_rooms()
        .row_data(app.get_room_index() as usize)
        .map(|room| room.id);
    let mut data = app.get_main_data();
    let Ok(status) = status else {
        data.online = false;
        data.rooms_available = false;
        data.updating = false;
        data.players = "—".into();
        data.players_max = "".into();
        data.ping = "—".into();
        data.region = "—".into();
        data.location = "".into();
        data.server_address = "".into();
        data.maintenance = "".into();
        app.set_room_index(0);
        app.set_rooms(ModelRc::default());
        app.set_room_names(ModelRc::default());
        app.set_main_data(data);
        // Maintenance cleared (or still unknown): a waiting invite retries
        // now (Tauri MainScreen.tsx pending-room effect).
        crate::bridge::retry_pending_join(app);
        return;
    };
    data.online = status.online;
    data.rooms_available = status.rooms.is_some();
    data.updating = status.online && status.game_available == Some(false);
    data.players = status
        .players_online
        .map_or("—".into(), |v| v.to_string())
        .into();
    data.players_max = status
        .players_max
        .filter(|&m| m > 0)
        .map_or_else(String::new, |m| format!("/ {m}"))
        .into();
    data.ping = status
        .server_latency_ms
        .map_or("—".into(), |v| v.to_string())
        .into();
    let region = status.region_code.trim();
    data.region = if region.is_empty() { "—" } else { region }.into();
    data.location = status.location_name.into();
    // status host/port resolve from the launcher config target (workshop
    // override or pack server), so the panel shows the real address.
    data.server_address = format!("{}:{}", status.host, status.port).into();
    data.maintenance = if status.online {
        status.maintenance.map_or(String::new(), |m| {
            if m.message.is_empty() {
                "Идут технические работы".into()
            } else {
                m.message
            }
        })
    } else {
        String::new()
    }
    .into();
    let rooms: Vec<_> = status
        .rooms
        .unwrap_or_default()
        .into_iter()
        .map(|room| {
            let battle = room.battle.as_ref();
            let description = room_description(&room);
            Room {
                id: room.id.into(),
                name: room.name.into(),
                description: description.into(),
                players: room.players.min(i32::MAX as u32) as i32,
                ready: room
                    .ready
                    .map_or(String::new(), |r| {
                        format!("Готовы: {}/{}", r.ready, r.required)
                    })
                    .into(),
                join: match room.admission.as_deref() {
                    Some("waiting") => "Встать в очередь",
                    Some("closed") => "Мест нет",
                    _ => "Войти",
                }
                .into(),
                joinable: room.joinable && room.admission.as_deref() != Some("closed"),
                has_battle: battle.is_some(),
                battle_red: battle.map_or(0, |b| b.red.min(i32::MAX as u32) as i32),
                battle_blue: battle.map_or(0, |b| b.blue.min(i32::MAX as u32) as i32),
                points: slint::ModelRc::new(slint::VecModel::from(battle.map_or(vec![], |b| {
                    b.points
                        .iter()
                        .map(|point| {
                            let team = if point.owner == "NEUTRAL" {
                                point.claiming.clone()
                            } else {
                                point.owner.clone()
                            };
                            let owner = match team.as_str() {
                                "RED" => "Красные",
                                "BLUE" => "Синие",
                                _ => "Нейтральная",
                            };
                            let action = if point.capturing != "NONE" && point.capturing != team {
                                match point.capturing.as_str() {
                                    "RED" => " · Красные нейтрализуют",
                                    "BLUE" => " · Синие нейтрализуют",
                                    _ => "",
                                }
                            } else {
                                ""
                            };
                            BattlePoint {
                                label: format!("{} · {}", point.name, owner).into(),
                                sub: format!("{}%{}", point.progress, action).into(),
                                progress: i32::from(point.progress.min(100)),
                                tone: match team.as_str() {
                                    "RED" => 1,
                                    "BLUE" => 2,
                                    _ => 0,
                                },
                            }
                        })
                        .collect::<Vec<_>>()
                }))),
            }
        })
        .collect();
    app.set_room_index(
        rooms
            .iter()
            .position(|room| Some(&room.id) == selected.as_ref())
            .unwrap_or(0) as i32,
    );
    app.set_room_names(ModelRc::new(VecModel::from(
        rooms.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
    )));
    app.set_rooms(ModelRc::new(VecModel::from(rooms)));
    app.set_main_data(data);
    // Server refresh may have cleared maintenance: a waiting invite retries
    // now (Tauri MainScreen.tsx pending-room effect).
    crate::bridge::retry_pending_join(app);
}

#[cfg(test)]
mod tests {
    use super::room_description;
    use launcher_net::status::GameRoom;

    fn subtitle_room(format: Option<&str>, mode: &str, map: &str, phase: &str) -> GameRoom {
        GameRoom {
            id: "room".into(),
            name: "Room".into(),
            mode: mode.into(),
            map: map.into(),
            phase: phase.into(),
            players: 3,
            joinable: true,
            format: format.map(str::to_owned),
            admission: None,
            ready: None,
            battle: None,
        }
    }

    #[test]
    fn room_subtitle_matches_tauri_format_and_ru_phase() {
        // Tauri RoomList.tsx: [format, mode, map, phase].filter(Boolean).join(' · ').
        assert_eq!(
            room_description(&subtitle_room(None, "room", "Map", "IDLE")),
            "room · Map · Ожидание игроков"
        );
        assert_eq!(
            room_description(&subtitle_room(Some("casual"), "room", "Map", "GAME")),
            "Casual · room · Map · Бой"
        );
        assert_eq!(
            room_description(&subtitle_room(
                Some("ranked"),
                "campaign",
                "Crossroads",
                "MATCH_END"
            )),
            "Ranked · campaign · Crossroads · Матч завершён"
        );
        // Unknown format is dropped like Tauri's undefined lookup; unknown
        // phase and empty mode/map fall back to the raw values.
        assert_eq!(
            room_description(&subtitle_room(Some("custom"), "room", "Map", "VOTING")),
            "room · Map · Голосование"
        );
        assert_eq!(
            room_description(&subtitle_room(Some("casual"), "room", "Map", "CUSTOM")),
            "Casual · room · Map · CUSTOM"
        );
        assert_eq!(
            room_description(&subtitle_room(Some("ranked"), "", "", "PREPARING")),
            "Ranked · Подготовка"
        );
    }

    #[test]
    fn background_loops_start_exactly_once() {
        assert!(super::claim_background_loops());
        assert!(!super::claim_background_loops());
    }

    #[test]
    fn server_refresh_preserves_game_state_and_clears_stale_rooms() {
        use launcher_net::status::ServerStatus;
        use slint::Model;
        i_slint_backend_testing::init_no_event_loop();
        let app = crate::AppWindow::new().unwrap();
        app.set_server_label("IP · raknet;workshop.blockfield.pro:25567".into());
        app.set_main_data(crate::MainData {
            phase: "running".into(),
            ..Default::default()
        });
        let mut status = ServerStatus {
            online: true,
            rooms: Some(vec![GameRoom {
                id: "room".into(),
                name: "Room".into(),
                mode: "room".into(),
                map: "Map".into(),
                phase: "IDLE".into(),
                players: 3,
                joinable: true,
                format: None,
                admission: Some("closed".into()),
                ready: None,
                battle: None,
            }]),
            game_available: Some(false),
            maintenance: None,
            players_online: Some(3),
            players_max: Some(200),
            minecraft_version: None,
            motd: None,
            host: "localhost".into(),
            port: 25565,
            region_code: "EU".into(),
            location_name: "Test".into(),
            server_latency_ms: Some(7),
            checked_at: 0,
        };
        super::apply_server(&app, Ok(status.clone()));
        assert_eq!(app.get_main_data().phase, "running");
        assert!(app.get_main_data().updating);
        assert_eq!(app.get_main_data().players, "3");
        assert_eq!(app.get_main_data().players_max, "/ 200");
        assert_eq!(app.get_main_data().ping, "7");
        assert_eq!(app.get_main_data().region, "EU");
        assert_eq!(app.get_main_data().server_address, "localhost:25565");
        // F15: apply_server never overwrites footer server_label with SLP host:port.
        assert_eq!(
            app.get_server_label(),
            "IP · raknet;workshop.blockfield.pro:25567"
        );
        assert!(!app.get_rooms().row_data(0).unwrap().joinable);
        assert_eq!(
            app.get_rooms().row_data(0).unwrap().description,
            "room · Map · Ожидание игроков"
        );
        let mut second = status.rooms.as_ref().unwrap()[0].clone();
        second.id = "second".into();
        status.rooms.as_mut().unwrap().push(second);
        super::apply_server(&app, Ok(status.clone()));
        app.set_room_index(1);
        status.rooms.as_mut().unwrap().reverse();
        super::apply_server(&app, Ok(status.clone()));
        assert_eq!(
            app.get_room_index(),
            0,
            "refresh preserves the selected room ID"
        );
        status.rooms.as_mut().unwrap().remove(0);
        super::apply_server(&app, Ok(status.clone()));
        assert_eq!(
            app.get_room_index(),
            0,
            "removed selection falls back to the first room"
        );

        status.online = false;
        status.rooms = None;
        status.players_online = None;
        status.players_max = None;
        status.region_code = "  ".into();
        status.server_latency_ms = None;
        super::apply_server(&app, Ok(status));
        assert!(!app.get_main_data().online);
        assert!(!app.get_main_data().rooms_available);
        assert!(!app.get_main_data().updating);
        assert_eq!(app.get_main_data().players_max, "");
        assert_eq!(app.get_main_data().region, "—");
        assert_eq!(app.get_rooms().row_count(), 0);
        assert_eq!(app.get_main_data().phase, "running");
        super::apply_server(&app, Err("offline".into()));
        assert_eq!(app.get_main_data().players, "—");
        assert_eq!(app.get_main_data().players_max, "");
        assert_eq!(app.get_main_data().ping, "—");
        assert_eq!(app.get_main_data().region, "—");
        assert!(!app.get_main_data().online);
        // F15: Err clears stale server_address so MainScreen shows the active profile fallback.
        assert_eq!(app.get_main_data().server_address, "");
    }

    #[test]
    fn role_mapping_workshop_gate_and_downgrade_match_tauri() {
        use crate::stats::StatsOutcome;
        i_slint_backend_testing::init_no_event_loop();
        let app = crate::AppWindow::new().unwrap();
        assert_eq!(super::role_label(Some("owner")), "ВЛАДЕЛЕЦ");
        assert_eq!(super::role_label(Some("admin")), "АДМИНИСТРАТОР");
        assert_eq!(super::role_label(Some("moderator")), "МОДЕРАТОР");
        assert_eq!(super::role_label(Some("builder")), "СТРОИТЕЛЬ");
        assert_eq!(super::role_label(Some("player")), "ИГРОК");
        assert_eq!(super::role_label(None), "ИГРОК");

        // Owner role grants workshop access.
        let owner = StatsOutcome {
            data: Some(serde_json::json!({"profile": {"name": "NetherG", "role": "owner"}})),
            saved_at: Some(1),
            stale: false,
            error: None,
        };
        assert!(!super::apply_role_outcome(&app, &owner));
        assert_eq!(app.get_role(), "ВЛАДЕЛЕЦ");
        assert!(app.get_main_data().can_workshop);

        // Transient error (not stale) must preserve previous role and never downgrade workshop.
        super::set_workshop_quietly(&app, true);
        let transient_err = StatsOutcome {
            data: None,
            saved_at: None,
            stale: false,
            error: Some("network error".into()),
        };
        assert!(!super::apply_role_outcome(&app, &transient_err));
        assert_eq!(app.get_role(), "ВЛАДЕЛЕЦ");
        assert!(app.get_main_data().can_workshop);

        // Authoritative revocation on Workshop profile requests auto-downgrade and blocks profile.
        let revoked = StatsOutcome {
            data: Some(serde_json::json!({"profile": {"name": "NetherG", "role": "player"}})),
            saved_at: Some(2),
            stale: false,
            error: None,
        };
        assert!(super::apply_role_outcome(&app, &revoked));
        assert_eq!(app.get_role(), "ИГРОК");
        assert!(!app.get_main_data().can_workshop);
        assert!(app.get_main_data().profile_blocked);
    }

    #[test]
    fn footer_server_label_matches_tauri_profile_rules() {
        assert_eq!(
            super::format_footer_server_label(
                false,
                false,
                "raknet;workshop.blockfield.pro:25567",
                "play.blockfield.pro:25565"
            ),
            "IP · play.blockfield.pro:25565"
        );
        assert_eq!(
            super::format_footer_server_label(
                true,
                false,
                "raknet;workshop.blockfield.pro:25567",
                "play.blockfield.pro:25565"
            ),
            "IP · raknet;workshop.blockfield.pro:25567"
        );
        assert_eq!(
            super::format_footer_server_label(true, false, "   ", "play.blockfield.pro:25565"),
            "Адрес мастерской не задан"
        );
        // Blocked workshop profile falls back to Game server IP (`App.tsx:357`).
        assert_eq!(
            super::format_footer_server_label(
                true,
                true,
                "raknet;workshop.blockfield.pro:25567",
                "play.blockfield.pro:25565"
            ),
            "IP · play.blockfield.pro:25565"
        );
    }

    #[test]
    fn pending_profile_retry_only_fires_on_busy_to_idle_transition() {
        use slint::ComponentHandle;
        let _serial = crate::main_ops::test_serial();
        i_slint_backend_testing::init_no_event_loop();
        let rt = crate::runtime::new().unwrap();
        let app = crate::AppWindow::new().unwrap();
        let temp_dir = std::env::temp_dir().join(format!("bf-bridge-retry-{}", std::process::id()));
        let downloader = std::sync::Arc::new(launcher_net::download::Downloader::new(
            std::sync::Arc::new(|_| {}),
            std::sync::Arc::new(|_, _| {}),
            launcher_net::download::noop_guard_sink(),
        ));
        let state = std::sync::Arc::new(launcher_app::LauncherAppState {
            cancel_flag: downloader.cancel_flag.clone(),
            downloader,
            pack: tokio::sync::RwLock::new(None),
            config: tokio::sync::RwLock::new(launcher_core::config::LauncherConfig::default()),
            app_data_dir: temp_dir.clone(),
            game: std::sync::Arc::new(launcher_core::game::GameState::new(|_| {})),
            operation: tokio::sync::Mutex::new(()),
            events: std::sync::Arc::new(|_| {}),
        });
        *super::active_session().lock().unwrap() = Some((state, rt.handle().clone()));
        *super::pending_profile_retry().lock().unwrap() =
            Some(launcher_core::config::ClientProfile::Workshop);
        let form = app.global::<crate::SettingsForm>();
        form.set_blocked(false);
        form.set_data(crate::SettingsData::default());
        let mut data = app.get_main_data();
        data.phase = "idle".into();
        data.can_workshop = true;
        data.profile_error = "disk error".into();
        data.profile_blocked = true;
        app.set_main_data(data);

        // RoomPending while already idle must not retry a failed profile switch.
        crate::bridge::apply(&app, launcher_app::LauncherEvent::RoomPending);
        assert!(
            !app.get_main_data().profile_changing,
            "RoomPending while idle must not retry a failed profile switch"
        );

        // Non-transition Idle event while already idle must not retry either.
        crate::bridge::apply(
            &app,
            launcher_app::LauncherEvent::GameStatus(launcher_core::game::GameStatus {
                phase: launcher_core::game::GamePhase::Idle,
                revision: 1,
            }),
        );
        assert!(
            !app.get_main_data().profile_changing,
            "non-transition Idle event must not retry a failed profile switch"
        );

        // Transitioning from running -> Idle must retry the pending profile switch.
        let mut data = app.get_main_data();
        data.phase = "running".into();
        app.set_main_data(data);
        crate::bridge::apply(
            &app,
            launcher_app::LauncherEvent::GameStatus(launcher_core::game::GameStatus {
                phase: launcher_core::game::GamePhase::Idle,
                revision: 2,
            }),
        );
        assert!(
            app.get_main_data().profile_changing,
            "busy -> idle GameStatus transition must retry pending profile switch"
        );

        *super::pending_profile_retry().lock().unwrap() = None;
        *super::active_session().lock().unwrap() = None;
        rt.shutdown_timeout(std::time::Duration::from_secs(1));
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn manual_verify_invalidates_in_flight_silent_pack_probe() {
        let _serial = crate::main_ops::test_serial();
        i_slint_backend_testing::init_no_event_loop();
        let rt = crate::runtime::new().unwrap();
        let app = crate::AppWindow::new().unwrap();
        let temp_dir = std::env::temp_dir().join(format!("bf-verify-gen-{}", std::process::id()));
        let downloader = std::sync::Arc::new(launcher_net::download::Downloader::new(
            std::sync::Arc::new(|_| {}),
            std::sync::Arc::new(|_, _| {}),
            launcher_net::download::noop_guard_sink(),
        ));
        let state = std::sync::Arc::new(launcher_app::LauncherAppState {
            cancel_flag: downloader.cancel_flag.clone(),
            downloader,
            pack: tokio::sync::RwLock::new(None),
            config: tokio::sync::RwLock::new(launcher_core::config::LauncherConfig::default()),
            app_data_dir: temp_dir.clone(),
            game: std::sync::Arc::new(launcher_core::game::GameState::new(|_| {})),
            operation: tokio::sync::Mutex::new(()),
            events: std::sync::Arc::new(|_| {}),
        });
        crate::main_ops::attach(&app, state, rt.handle().clone());
        let mut data = app.get_main_data();
        data.phase = "idle".into();
        data.checked = true;
        data.checking = false;
        app.set_main_data(data);

        let before = super::pack_check_gen().load(std::sync::atomic::Ordering::Acquire);
        app.invoke_verify();
        let after = super::pack_check_gen().load(std::sync::atomic::Ordering::Acquire);
        assert!(
            after > before,
            "manual Action::Verify must advance pack_check_gen to discard older in-flight probes"
        );

        rt.shutdown_timeout(std::time::Duration::from_secs(1));
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn explicit_capture_and_test_flags_use_fixtures() {
        for flag in [
            "--state",
            "--scenario",
            "--capture-dir",
            "--e2e",
            "--e2e-golden",
            "--e2e-update-goldens",
        ] {
            assert!(super::fixtures_requested(&[flag.into(), "value".into()]));
        }
        if std::env::var_os("BLOCKFIELD_CAPTURE_STATE").is_none() {
            assert!(!super::fixtures_requested(&[]));
            assert!(!super::fixtures_requested(&[
                "--idle-seconds".into(),
                "1".into()
            ]));
        }
    }
}
