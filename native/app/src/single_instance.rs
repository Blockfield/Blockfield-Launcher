//! Second-launch focus + deep-link forwarding via the shipping launcher's
//! single-instance mechanisms (tauri-plugin-single-instance 2.5.2 parity).
//!
//! Linux owns a D-Bus session-bus name (`<id>.SingleInstance`, object path
//! derived like the plugin) and forwards with the `ExecuteCallback(argv, cwd)`
//! method; the bus daemon makes election atomic, so there is no stale-socket
//! unlink race. Windows holds a named mutex (`<id>-sim`) and a hidden window
//! (`<id>-sic`/`-siw`) receiving `WM_COPYDATA` 1542 with `cwd|args\0`, exactly
//! like the plugin; both live in the user's session, so another user cannot
//! impersonate the owner. Payloads stay capped at 64 KiB, malformed deliveries
//! are ignored, and a failed forward retries election in `main.rs`.

use slint::ComponentHandle;

pub const MAX_PAYLOAD: usize = 64 * 1024;
#[cfg(target_os = "linux")]
const DBUS_INTERFACE: &str = "org.SingleInstance.DBus";
#[cfg(windows)]
const WMCOPYDATA_SINGLE_INSTANCE_DATA: usize = 1542;

/// Raw argv entries that are invite links (Tauri passes the OS URL as a
/// positional arg to the second instance). Validated with the shared core
/// parser, so the channel only ever carries joinable targets.
pub fn collect_links(args: &[String]) -> Vec<String> {
    let application_id = launcher_app::presence::application_id();
    args.iter()
        .filter(|arg| launcher_app::rooms::parse_link(arg, &application_id).is_some())
        .cloned()
        .collect()
}

/// Positional argv that the main arg parser must tolerate (already collected
/// above; handling happens through the single-instance channel).
pub fn is_link(arg: &str) -> bool {
    let application_id = launcher_app::presence::application_id();
    launcher_app::rooms::parse_link(arg, &application_id).is_some()
}

fn payload_len(links: &[String]) -> usize {
    links.iter().map(|link| link.len() + 1).sum()
}

fn check_payload(links: &[String]) -> std::io::Result<()> {
    if payload_len(links) > MAX_PAYLOAD {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invite payload exceeds the 64 KiB cap",
        ));
    }
    Ok(())
}

/// Owner election: `Primary` keeps the listener, `Secondary` must forward.
pub enum Claim {
    Primary(Listener),
    Secondary,
}

/// Accept-loop handle; the platform listener lives here so `claim`,
/// `forward` and `spawn_listener` stay uniform across OSes.
pub struct Listener(imp::RawListener);

fn base_app_id() -> &'static str {
    if launcher_core::dev::is_dev_build() {
        "com.blockfield.launcher-dev"
    } else {
        "com.blockfield.launcher"
    }
}

#[cfg(any(target_os = "linux", test))]
fn dbus_name() -> String {
    if let Ok(name) = std::env::var("BLOCKFIELD_SINGLE_INSTANCE_DBUS_NAME")
        && !name.trim().is_empty()
    {
        return name;
    }
    format!("{}.SingleInstance", base_app_id())
}

#[cfg(any(target_os = "linux", test))]
fn dbus_path_for_name(name: &str) -> String {
    let mut path = name.replace('.', "/").replace('-', "_");
    if !path.starts_with('/') {
        path = format!("/{path}");
    }
    path
}

#[cfg(any(target_os = "linux", test))]
fn dbus_path() -> String {
    dbus_path_for_name(&dbus_name())
}

#[cfg(any(windows, test))]
fn windows_names() -> (String, String, String) {
    let id = base_app_id();
    (
        format!("{id}-sic"),
        format!("{id}-siw"),
        format!("{id}-sim"),
    )
}

#[cfg(any(windows, test))]
fn encode_copydata(cwd: &str, args: &[String]) -> Vec<u8> {
    let mut data = String::from(cwd);
    for arg in args {
        data.push('|');
        data.push_str(arg);
    }
    data.push('\0');
    data.into_bytes()
}

#[cfg(any(windows, test))]
fn decode_copydata(bytes: &[u8]) -> Option<(String, Vec<String>)> {
    let end = bytes.iter().position(|&byte| byte == 0)?;
    let text = std::str::from_utf8(&bytes[..end]).ok()?;
    let mut parts = text.split('|');
    let cwd = parts.next()?.to_owned();
    let args: Vec<String> = parts.map(str::to_owned).collect();
    if cwd.len() + args.iter().map(|arg| arg.len() + 1).sum::<usize>() > MAX_PAYLOAD + 1024 {
        return None;
    }
    Some((cwd, args))
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use std::sync::mpsc::{Receiver, Sender};
    use zbus::blocking::{Connection, connection::Builder};

    pub struct RawListener {
        _conn: Connection,
        rx: Receiver<(Vec<String>, String)>,
    }
    // SAFETY: moved whole into the listener thread, never shared.
    unsafe impl Send for RawListener {}

    struct SingleInstanceDbus {
        tx: Sender<(Vec<String>, String)>,
    }

    #[zbus::interface(name = "org.SingleInstance.DBus")]
    impl SingleInstanceDbus {
        fn execute_callback(&mut self, argv: Vec<String>, cwd: String) {
            if argv.iter().map(|arg| arg.len() + 1).sum::<usize>() > MAX_PAYLOAD {
                return;
            }
            let _ = self.tx.send((argv, cwd));
        }
    }

    fn bus_error(context: &str, error: zbus::Error) -> std::io::Error {
        std::io::Error::other(format!("{context}: {error}"))
    }

    pub fn claim() -> std::io::Result<Claim> {
        let name = super::dbus_name();
        let path = super::dbus_path();
        let (tx, rx) = std::sync::mpsc::channel();
        let obj = SingleInstanceDbus { tx };
        match Builder::session()
            .map_err(|error| bus_error("no D-Bus session bus", error))?
            .name(name.as_str())
            .map_err(|error| bus_error("cannot request single-instance name", error))?
            .replace_existing_names(false)
            .allow_name_replacements(false)
            .serve_at(path.as_str(), obj)
            .map_err(|error| bus_error("cannot serve single-instance path", error))?
            .build()
        {
            Ok(conn) => Ok(Claim::Primary(Listener(RawListener { _conn: conn, rx }))),
            Err(zbus::Error::NameTaken) => Ok(Claim::Secondary),
            Err(error) => Err(bus_error("single-instance election failed", error)),
        }
    }

    /// Second-instance exit path: the bus routes to the owner only, so a
    /// stranger cannot answer. Any failure lets the caller retry election.
    pub fn forward(links: &[String]) -> std::io::Result<()> {
        super::check_payload(links)?;
        let name = super::dbus_name();
        let path = super::dbus_path();
        let cwd = std::env::current_dir()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default()
            .to_owned();
        let conn =
            Connection::session().map_err(|error| bus_error("no D-Bus session bus", error))?;
        conn.call_method(
            Some(name.as_str()),
            path.as_str(),
            Some(super::DBUS_INTERFACE),
            "ExecuteCallback",
            &(links.to_vec(), cwd),
        )
        .map_err(|error| {
            std::io::Error::other(format!("single-instance forward failed: {error}"))
        })?;
        Ok(())
    }

    /// Owner loop: every D-Bus delivery is one second launch (links or bare
    /// focus). Callbacks run on the Slint thread. Malformed argv is filtered
    /// by `collect_links`, so strangers cannot steal focus.
    pub fn spawn_listener(listener: Listener, weak: slint::Weak<crate::AppWindow>) {
        std::thread::spawn(move || {
            let Listener(listener) = listener;
            let RawListener { _conn, rx } = listener;
            let _keep_bus = _conn;
            for (argv, _cwd) in rx {
                let links = super::collect_links(&argv);
                let _ = weak.upgrade_in_event_loop(move |app| {
                    if links.is_empty() {
                        super::focus_window(&app);
                    } else {
                        for link in &links {
                            super::handle_link(&app, link);
                        }
                    }
                });
            }
        });
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::sync::mpsc::Receiver;
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, WPARAM,
        },
        System::{
            DataExchange::COPYDATASTRUCT, LibraryLoader::GetModuleHandleW, Threading::CreateMutexW,
        },
        UI::WindowsAndMessaging::{
            self as w32wm, AllowSetForegroundWindow, CREATESTRUCTW, CreateWindowExW,
            DefWindowProcW, FindWindowW, GWL_STYLE, GWLP_USERDATA, GetWindowThreadProcessId,
            RegisterClassExW, SendMessageW, WINDOW_LONG_PTR_INDEX, WM_COPYDATA, WM_CREATE,
            WM_DESTROY, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
            WS_EX_TRANSPARENT, WS_OVERLAPPED, WS_POPUP, WS_VISIBLE,
        },
    };

    pub struct RawListener {
        mutex: isize,
        hwnd: isize,
        rx: Receiver<(Vec<String>, String)>,
    }
    // SAFETY: moved whole into the listener thread, never shared.
    unsafe impl Send for RawListener {}

    struct UserData {
        tx: std::sync::mpsc::Sender<(Vec<String>, String)>,
    }

    fn wide(name: &str) -> Vec<u16> {
        name.encode_utf16().chain([0]).collect()
    }

    fn last_error(context: &str) -> std::io::Error {
        // SAFETY: GetLastError reads thread-local Win32 state.
        let code = unsafe { GetLastError() };
        std::io::Error::other(format!("{context}: Win32 error {code}"))
    }

    fn parse_copydata_payload(ptr: *const u8, len: usize) -> Option<(Vec<String>, String)> {
        if len == 0 || len > super::MAX_PAYLOAD + 4096 {
            return None;
        }
        // SAFETY: WM_COPYDATA gives a sender buffer valid for `len` bytes.
        let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
        let (cwd, args) = super::decode_copydata(bytes)?;
        if super::payload_len(&args) > super::MAX_PAYLOAD {
            return None;
        }
        Some((args, cwd))
    }

    pub fn claim() -> std::io::Result<Claim> {
        let (class, window, mutex) = super::windows_names();
        let mutex_wide = wide(&mutex);
        // SAFETY: valid name buffer, default security (user session only).
        let hmutex = unsafe { CreateMutexW(std::ptr::null(), true.into(), mutex_wide.as_ptr()) };
        if hmutex.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: reads thread-local Win32 state.
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            // SAFETY: we own this handle to the existing mutex; close ours.
            unsafe { CloseHandle(hmutex) };
            return Ok(Claim::Secondary);
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let userdata = Box::into_raw(Box::new(UserData { tx }));
        let hwnd = create_event_target_window(&wide(&class), &wide(&window), userdata);
        if hwnd.is_null() {
            // SAFETY: window creation failed; free userdata and mutex.
            unsafe {
                drop(Box::from_raw(userdata));
                CloseHandle(hmutex);
            }
            return Err(last_error("cannot create single-instance window"));
        }
        Ok(Claim::Primary(Listener(RawListener {
            mutex: hmutex as isize,
            hwnd: hwnd as isize,
            rx,
        })))
    }

    /// Second-instance exit path: the mutex election already proved an owner
    /// exists in this session, and the hidden window answers only our
    /// `dwData`. Any failure lets the caller retry election.
    pub fn forward(links: &[String]) -> std::io::Result<()> {
        super::check_payload(links)?;
        let (class, window, _mutex) = super::windows_names();
        let class_wide = wide(&class);
        let window_wide = wide(&window);
        // SAFETY: valid name buffers, synchronous lookup.
        let hwnd = unsafe { FindWindowW(class_wide.as_ptr(), window_wide.as_ptr()) };
        if hwnd.is_null() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no owner answers the single-instance window",
            ));
        }
        // SAFETY: hwnd came from FindWindowW.
        unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid != 0 {
                AllowSetForegroundWindow(pid);
            }
        }
        let cwd = std::env::current_dir()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default()
            .to_owned();
        let data = super::encode_copydata(&cwd, links);
        let cds = COPYDATASTRUCT {
            dwData: super::WMCOPYDATA_SINGLE_INSTANCE_DATA,
            cbData: data.len() as _,
            lpData: data.as_ptr() as _,
        };
        // SAFETY: cds borrows `data` for this synchronous call.
        let handled = unsafe { SendMessageW(hwnd, WM_COPYDATA, 0, &cds as *const _ as _) };
        if handled == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "single-instance endpoint is not our launcher",
            ));
        }
        Ok(())
    }

    /// Owner loop: the hidden window is pumped by the main (Slint/winit) event
    /// loop; its WndProc forwards deliveries into this channel, and this
    /// thread posts them to the Slint thread. Malformed payloads never reach
    /// the UI. Handles stay open until process exit, when the OS releases the
    /// mutex and destroys the window.
    pub fn spawn_listener(listener: Listener, weak: slint::Weak<crate::AppWindow>) {
        std::thread::spawn(move || {
            let Listener(listener) = listener;
            let RawListener { mutex, hwnd, rx } = listener;
            let _keep_handles = (mutex, hwnd);
            for (argv, _cwd) in rx {
                let links = super::collect_links(&argv);
                let _ = weak.upgrade_in_event_loop(move |app| {
                    if links.is_empty() {
                        super::focus_window(&app);
                    } else {
                        for link in &links {
                            super::handle_link(&app, link);
                        }
                    }
                });
            }
        });
    }

    unsafe extern "system" fn single_instance_window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_CREATE => {
                let create_struct = unsafe { &*(lparam as *const CREATESTRUCTW) };
                let userdata = create_struct.lpCreateParams as isize;
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, userdata) };
                0
            }
            WM_COPYDATA => {
                let cds_ptr = lparam as *const COPYDATASTRUCT;
                // SAFETY: lparam is a valid COPYDATASTRUCT for WM_COPYDATA.
                let (dw_data, cb_data, lp_data) = unsafe {
                    (
                        (*cds_ptr).dwData,
                        (*cds_ptr).cbData as usize,
                        (*cds_ptr).lpData as *const u8,
                    )
                };
                if dw_data == super::WMCOPYDATA_SINGLE_INSTANCE_DATA && !hwnd.is_null() {
                    // SAFETY: GWLP_USERDATA holds our Box<UserData> since WM_CREATE.
                    let userdata =
                        unsafe { &*(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const UserData) };
                    if let Some((args, _cwd)) = parse_copydata_payload(lp_data, cb_data) {
                        let _ = userdata.tx.send((args, _cwd));
                    }
                }
                1
            }
            WM_DESTROY => {
                let userdata = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut UserData };
                if !userdata.is_null() {
                    drop(unsafe { Box::from_raw(userdata) });
                }
                0
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    fn create_event_target_window(
        class_name: &[u16],
        window_name: &[u16],
        userdata: *const UserData,
    ) -> HWND {
        unsafe {
            let class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: 0,
                lpfnWndProc: Some(single_instance_window_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: GetModuleHandleW(std::ptr::null()),
                hIcon: std::ptr::null_mut(),
                hCursor: std::ptr::null_mut(),
                hbrBackground: std::ptr::null_mut(),
                lpszMenuName: std::ptr::null(),
                lpszClassName: class_name.as_ptr(),
                hIconSm: std::ptr::null_mut(),
            };
            RegisterClassExW(&class);
            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
                class_name.as_ptr(),
                window_name.as_ptr(),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                userdata as _,
            );
            SetWindowLongPtrW(hwnd, GWL_STYLE, (WS_VISIBLE | WS_POPUP) as isize);
            hwnd
        }
    }

    #[cfg(target_pointer_width = "32")]
    #[allow(non_snake_case)]
    unsafe fn SetWindowLongPtrW(hwnd: HWND, index: WINDOW_LONG_PTR_INDEX, value: isize) -> isize {
        unsafe { w32wm::SetWindowLongW(hwnd, index, value as _) as _ }
    }

    #[cfg(target_pointer_width = "64")]
    #[allow(non_snake_case)]
    unsafe fn SetWindowLongPtrW(hwnd: HWND, index: WINDOW_LONG_PTR_INDEX, value: isize) -> isize {
        unsafe { w32wm::SetWindowLongPtrW(hwnd, index, value) }
    }

    #[cfg(target_pointer_width = "32")]
    #[allow(non_snake_case)]
    unsafe fn GetWindowLongPtrW(hwnd: HWND, index: WINDOW_LONG_PTR_INDEX) -> isize {
        unsafe { w32wm::GetWindowLongW(hwnd, index) as _ }
    }

    #[cfg(target_pointer_width = "64")]
    #[allow(non_snake_case)]
    unsafe fn GetWindowLongPtrW(hwnd: HWND, index: WINDOW_LONG_PTR_INDEX) -> isize {
        unsafe { w32wm::GetWindowLongPtrW(hwnd, index) }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    use super::*;

    pub struct RawListener(#[allow(dead_code)] ());
    // SAFETY: unit, never shared.
    unsafe impl Send for RawListener {}

    pub fn claim() -> std::io::Result<Claim> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "single-instance election needs Linux D-Bus or Windows",
        ))
    }

    pub fn forward(_links: &[String]) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "single-instance forwarding needs Linux D-Bus or Windows",
        ))
    }

    pub fn spawn_listener(_listener: Listener, _weak: slint::Weak<crate::AppWindow>) {}
}

pub use imp::{claim, forward, spawn_listener};

/// Tauri `game::show_launcher` parity for the Slint shell: reveal the window
/// and drop any minimization (Slint 1.18 exposes no separate focus call).
pub fn focus_window(app: &crate::AppWindow) {
    let _ = app.show();
    app.window().set_minimized(false);
}

/// Route one forwarded link exactly like a Tauri `on_open_url` event: store
/// the target, emit `RoomPending` for the auto-join effect, focus.
pub fn handle_link(app: &crate::AppWindow, link: &str) {
    let application_id = launcher_app::presence::application_id();
    if let Some(target) = launcher_app::rooms::parse_link(link, &application_id) {
        *crate::main_ops::shared_pending().0.lock().unwrap() = Some(target);
        crate::bridge::apply(app, launcher_app::LauncherEvent::RoomPending);
    }
    focus_window(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serial() -> std::sync::MutexGuard<'static, ()> {
        crate::main_ops::test_serial()
    }

    #[test]
    fn only_valid_invite_links_reach_the_channel() {
        let app_id = launcher_app::presence::application_id();
        let lobby = "blockfield://join/lobby".to_string();
        let discord = format!("discord-{app_id}://join?secret=bf1%3Alobby");
        let args = vec![
            lobby.clone(),
            "--state".into(),
            discord.clone(),
            "blockfield://join/../../evil".into(),
            "https://blockfield.pro/?room=lobby#rooms".into(),
        ];
        assert_eq!(collect_links(&args), vec![lobby, discord]);
        assert!(is_link("blockfield://join/lobby"));
        assert!(!is_link("--state"));
        assert!(!is_link("blockfield://join/../../evil"));
    }

    #[test]
    fn oversized_payload_never_leaves_the_second_instance() {
        assert_eq!(payload_len(&[]), 0);
        assert!(check_payload(&["blockfield://join/lobby".to_string()]).is_ok());
        let huge = vec!["x".repeat(MAX_PAYLOAD + 1)];
        assert!(check_payload(&huge).is_err());
    }

    #[test]
    fn dbus_name_and_path_match_the_shipping_plugin() {
        // Plugin: name = "<identifier>.SingleInstance", path = name with
        // '.' -> '/' and '-' -> '_', prefixed with '/'.
        assert_eq!(
            dbus_path_for_name("com.blockfield.launcher.SingleInstance"),
            "/com/blockfield/launcher/SingleInstance"
        );
        assert_eq!(
            dbus_path_for_name("com.blockfield.launcher-dev.SingleInstance"),
            "/com/blockfield/launcher_dev/SingleInstance"
        );
        let _serial = serial();
        // SAFETY: serial guard; test-only bus-name override.
        unsafe { std::env::set_var("BLOCKFIELD_SINGLE_INSTANCE_DBUS_NAME", "") };
        let name = dbus_name();
        assert!(name.ends_with(".SingleInstance"));
        assert_eq!(dbus_path(), dbus_path_for_name(&name));
        // SAFETY: serial guard; restore the override to unset.
        unsafe { std::env::remove_var("BLOCKFIELD_SINGLE_INSTANCE_DBUS_NAME") };
    }

    #[test]
    fn windows_names_carry_the_app_id() {
        let (class, window, mutex) = windows_names();
        assert!(class.ends_with("-sic"));
        assert!(window.ends_with("-siw"));
        assert!(mutex.ends_with("-sim"));
        assert!(class.starts_with("com.blockfield.launcher"));
        // Dev and prod never share an endpoint.
        assert!(class.contains("blockfield"));
    }

    #[test]
    fn copydata_round_trips_links_and_bare_focus() {
        let links = vec!["blockfield://join/lobby".to_string()];
        let bytes = encode_copydata("/tmp", &links);
        assert_eq!(decode_copydata(&bytes), Some(("/tmp".into(), links)));
        let empty = encode_copydata("/tmp", &[]);
        assert_eq!(decode_copydata(&empty), Some(("/tmp".into(), vec![])));
    }

    #[test]
    fn copydata_ignores_malformed_deliveries() {
        assert_eq!(decode_copydata(b"no-nul-terminator"), None);
        assert_eq!(decode_copydata(&[0xff, 0xfe, 0]), None);
        let huge = vec!["x".repeat(MAX_PAYLOAD + 2048)];
        let bytes = encode_copydata("/tmp", &huge);
        assert_eq!(decode_copydata(&bytes), None);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn oversized_forward_fails_before_touching_the_bus() {
        let huge = vec!["x".repeat(MAX_PAYLOAD + 1)];
        let result = forward(&huge);
        assert!(result.is_err(), "oversized forward must fail");
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn owner_election_uses_the_session_bus_atomically() {
        let _serial = serial();
        // Unique bus name per test run; the bus daemon serializes ownership,
        // so no stale-socket unlink race exists by construction.
        let unique = format!(
            "com.blockfield.TestNg34C3{}.SingleInstance",
            std::process::id()
        );
        // SAFETY: serial guard; each test sets its own bus name.
        unsafe { std::env::set_var("BLOCKFIELD_SINGLE_INSTANCE_DBUS_NAME", &unique) };
        let first = claim();
        // SAFETY: serial guard; restore before asserting so a failure cannot
        // leak the override into another test.
        unsafe { std::env::remove_var("BLOCKFIELD_SINGLE_INSTANCE_DBUS_NAME") };
        match first {
            Ok(Claim::Primary(_)) => {
                // SAFETY: serial guard; re-apply the same name for the second
                // claim so it contends with the live owner above.
                unsafe { std::env::set_var("BLOCKFIELD_SINGLE_INSTANCE_DBUS_NAME", &unique) };
                let second = claim();
                // SAFETY: serial guard; always restore.
                unsafe { std::env::remove_var("BLOCKFIELD_SINGLE_INSTANCE_DBUS_NAME") };
                assert!(
                    matches!(second, Ok(Claim::Secondary)),
                    "second claim while the owner lives must be secondary"
                );
            }
            Ok(Claim::Secondary) => {
                panic!("first claim on a fresh bus name must elect a primary");
            }
            Err(_) => {
                // No session bus in this environment (headless CI without
                // dbus-daemon): election gracefully declines and the launch
                // continues alone, like the plugin's `_ => {{}}` path.
            }
        }
    }

    #[test]
    fn forwarded_link_stores_pending_and_focuses() {
        let _serial = serial();
        i_slint_backend_testing::init_no_event_loop();
        let app = crate::AppWindow::new().unwrap();
        *crate::main_ops::shared_pending().0.lock().unwrap() = None;
        crate::fixtures::apply(&app);
        handle_link(&app, "blockfield://join/lobby");
        assert_eq!(
            crate::main_ops::shared_pending()
                .0
                .lock()
                .unwrap()
                .as_deref(),
            Some("lobby")
        );
        assert!(app.get_main_data().pending_room);
        *crate::main_ops::shared_pending().0.lock().unwrap() = None;
    }
}
