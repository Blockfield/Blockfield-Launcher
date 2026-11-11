//! Second-launch focus + deep-link forwarding through a per-user IPC endpoint.
//!
//! Tauri parity (`src-tauri/src/lib.rs:47-49` + `single-instance` plugin with
//! the `deep-link` feature): exactly one launcher owns the window. A second
//! process forwards its invite-link argv to the running instance and exits;
//! the owner shows its window and routes each link to the pending-room store.
//! Windows + Linux only (macOS left the native target).
//!
//! The endpoint is per-user so one user can never steer or block another
//! user's launcher: Linux uses a Unix socket under `$XDG_RUNTIME_DIR` (dir
//! `0700`, socket `0600`, with a `0700` per-UID fallback under the temp dir);
//! Windows uses a named pipe whose name carries the user SID (default DACL,
//! i.e. current user only). Tests override the directory with
//! `BLOCKFIELD_SINGLE_INSTANCE_DIR`.
//!
//! Every message starts with a magic + version header and a bounded length,
//! so an unrelated listener on our endpoint never looks like our launcher:
//! a bad header/ack is an error, and a bound/connect failure that is not
//! ours never makes the launch exit (the process continues alone). Payloads
//! are capped at 64 KiB and every read has an overall deadline.

use slint::ComponentHandle;
use std::{
    io::{Read, Write},
    time::{Duration, Instant},
};

/// Wire magic identifying our launcher (`BF1I`); an unrelated listener
/// never sends this header, so we never mistake it for the owner.
const MAGIC: [u8; 4] = *b"BF1I";
/// Only version 1 exists; bump on any framing change and accept just ours.
const PROTOCOL_VERSION: u16 = 1;
/// `MAGIC ++ VERSION(le) ++ LEN(le)`; `LEN` bounds the payload that follows.
const HEADER_LEN: usize = 10;
/// Largest invite payload a second launch may deliver.
pub const MAX_PAYLOAD: usize = 64 * 1024;
/// Overall deadline for one forward/read round-trip (payloads are tiny).
const ROUND_TRIP: Duration = Duration::from_secs(10);
/// Per-call write cap; the ack wait is bounded by `ROUND_TRIP`.
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

fn encode_header(payload_len: u32) -> [u8; HEADER_LEN] {
    let mut header = [0u8; HEADER_LEN];
    header[..4].copy_from_slice(&MAGIC);
    header[4..6].copy_from_slice(&PROTOCOL_VERSION.to_le_bytes());
    header[6..10].copy_from_slice(&payload_len.to_le_bytes());
    header
}

/// Payload length after checking magic + version, or `None` for a stranger.
fn parse_header(header: &[u8; HEADER_LEN]) -> Option<usize> {
    if header[..4] != MAGIC {
        return None;
    }
    if u16::from_le_bytes([header[4], header[5]]) != PROTOCOL_VERSION {
        return None;
    }
    let len = u32::from_le_bytes([header[6], header[7], header[8], header[9]]) as usize;
    (len <= MAX_PAYLOAD).then_some(len)
}

/// Full second-launch message, or `None` when the payload exceeds the cap
/// (argv cannot realistically reach it; the owner then handles links locally).
fn encode_message(links: &[String]) -> Option<Vec<u8>> {
    let payload = encode_links(links);
    if payload.len() > MAX_PAYLOAD {
        return None;
    }
    let mut message = Vec::with_capacity(HEADER_LEN + payload.len());
    message.extend_from_slice(&encode_header(payload.len() as u32));
    message.extend_from_slice(&payload);
    Some(message)
}

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

fn encode_links(links: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    for link in links {
        out.extend_from_slice(link.as_bytes());
        out.push(b'\n');
    }
    out
}

fn decode_links(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Owner election on the per-user endpoint: `Primary` keeps the listener,
/// `Secondary` must forward and exit.
pub enum Claim {
    Primary(Listener),
    Secondary,
}

/// Accept-loop handle; the platform listener lives here so `claim`,
/// `forward` and `spawn_listener` stay uniform across OSes.
pub struct Listener(imp::RawListener);

/// Pipe name from a SID string, kept pure so unit tests pin it on any OS.
#[cfg(any(windows, test))]
fn pipe_name_for_sid(sid: &str, dev: bool) -> String {
    format!(
        r"\\.\pipe\blockfield-launcher-{sid}{}",
        if dev { "-dev" } else { "" }
    )
}

#[cfg(unix)]
mod imp {
    use super::*;
    use std::{
        os::unix::fs::{DirBuilderExt, PermissionsExt},
        os::unix::net::{UnixListener, UnixStream},
        path::PathBuf,
    };

    pub type RawListener = UnixListener;

    const SOCKET_NAME: &str = "blockfield-launcher.sock";
    const SOCKET_NAME_DEV: &str = "blockfield-launcher-dev.sock";

    fn socket_path() -> std::io::Result<PathBuf> {
        let dir = if let Ok(dir) = std::env::var("BLOCKFIELD_SINGLE_INSTANCE_DIR") {
            let dir = PathBuf::from(dir);
            if dir.as_os_str().is_empty() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "BLOCKFIELD_SINGLE_INSTANCE_DIR is empty",
                ));
            }
            dir
        } else if let Some(dir) = dirs::runtime_dir() {
            dir
        } else {
            // Outside a login session there is no per-user runtime dir; fall
            // back to a per-UID dir under the temp dir with the same perms.
            // SAFETY: getuid has no failure mode and takes no arguments.
            let uid = unsafe { libc::getuid() };
            std::env::temp_dir().join(format!("blockfield-launcher-{uid}"))
        };
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)?;
        // create() is a no-op for an existing dir, so enforce 0700 explicitly.
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
        Ok(dir.join(if launcher_core::dev::is_dev_build() {
            SOCKET_NAME_DEV
        } else {
            SOCKET_NAME
        }))
    }

    pub fn claim() -> std::io::Result<Claim> {
        let path = socket_path()?;
        match UnixListener::bind(&path) {
            Ok(listener) => {
                // The dir is 0700 but the socket inherits umask; lock it down.
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
                Ok(Claim::Primary(Listener(listener)))
            }
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                match UnixStream::connect(&path) {
                    // A live owner holds the socket: we are the second launch.
                    Ok(_) => Ok(Claim::Secondary),
                    // Stale socket from a dead owner: reclaim it and lead.
                    Err(_) => {
                        let _ = std::fs::remove_file(&path);
                        match UnixListener::bind(&path) {
                            Ok(listener) => {
                                let _ = std::fs::set_permissions(
                                    &path,
                                    std::fs::Permissions::from_mode(0o600),
                                );
                                Ok(Claim::Primary(Listener(listener)))
                            }
                            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                                Ok(Claim::Secondary)
                            }
                            Err(error) => Err(error),
                        }
                    }
                }
            }
            Err(error) => Err(error),
        }
    }

    /// Second-instance exit path: hand every link to the owner, whose ack
    /// proves the listener is really our launcher (an unrelated socket never
    /// answers our header). Any failure lets the caller retry the election.
    pub fn forward(links: &[String]) -> std::io::Result<()> {
        let path = socket_path()?;
        forward_to(&path, links, ROUND_TRIP)
    }

    fn forward_to(
        path: &std::path::Path,
        links: &[String],
        ack_timeout: Duration,
    ) -> std::io::Result<()> {
        let mut stream = UnixStream::connect(path)?;
        stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
        let message = encode_message(links).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invite payload exceeds the 64 KiB cap",
            )
        })?;
        stream.write_all(&message)?;
        stream.set_read_timeout(Some(ack_timeout))?;
        let mut ack = encode_header(0);
        stream.read_exact(&mut ack).map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("single-instance endpoint is not our launcher: {error}"),
            )
        })?;
        if ack != encode_header(0) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "single-instance endpoint answered without our protocol header",
            ));
        }
        Ok(())
    }

    /// One bounded read: the per-call timeout always leaves room inside the
    /// overall deadline, so slow drips cannot hold the connection open.
    fn read_exact_within(
        mut stream: &UnixStream,
        mut buf: &mut [u8],
        start: Instant,
        limit: Duration,
    ) -> std::io::Result<()> {
        while !buf.is_empty() {
            let remaining = limit.checked_sub(start.elapsed()).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "single-instance read deadline",
                )
            })?;
            stream.set_read_timeout(Some(remaining))?;
            match stream.read(buf) {
                Ok(0) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "single-instance sender went away mid-message",
                    ));
                }
                Ok(n) => buf = &mut buf[n..],
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    fn read_message_with_deadline(stream: UnixStream, limit: Duration) -> Option<Vec<String>> {
        let start = Instant::now();
        let mut header = [0u8; HEADER_LEN];
        read_exact_within(&stream, &mut header, start, limit).ok()?;
        let len = parse_header(&header)?;
        let mut payload = vec![0u8; len];
        read_exact_within(&stream, &mut payload, start, limit).ok()?;
        // The forwarder waits for this ack before exiting; a stranger that
        // connected here gets nothing it can mistake for an answer.
        let _ = (&stream).write_all(&encode_header(0));
        Some(decode_links(&payload))
    }

    fn read_message(stream: UnixStream) -> Option<Vec<String>> {
        read_message_with_deadline(stream, ROUND_TRIP)
    }

    /// Owner loop: every connection is one second launch (links or bare
    /// focus). Callbacks run on the Slint thread, so links land after
    /// `wire_app` too. Connections without our header are ignored outright:
    /// they must not steal focus.
    pub fn spawn_listener(listener: Listener, weak: slint::Weak<crate::AppWindow>) {
        std::thread::spawn(move || {
            for stream in listener.0.incoming() {
                let Ok(stream) = stream else { continue };
                let links = read_message(stream);
                let Some(links) = links else { continue };
                let _ = weak.upgrade_in_event_loop(move |app| {
                    if links.is_empty() {
                        focus_window(&app);
                    } else {
                        for link in &links {
                            handle_link(&app, link);
                        }
                    }
                });
            }
        });
    }

    #[cfg(test)]
    pub fn test_socket_path() -> std::io::Result<std::path::PathBuf> {
        socket_path()
    }

    #[cfg(test)]
    pub fn test_forward_to(
        path: &std::path::Path,
        links: &[String],
        ack_timeout: Duration,
    ) -> std::io::Result<()> {
        forward_to(path, links, ack_timeout)
    }

    #[cfg(test)]
    pub fn test_read_message_with_deadline(
        stream: UnixStream,
        limit: Duration,
    ) -> Option<Vec<String>> {
        read_message_with_deadline(stream, limit)
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, ERROR_BROKEN_PIPE, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GetLastError,
            HANDLE, INVALID_HANDLE_VALUE,
        },
        Security::{
            ConvertSidToStringSidW, GetTokenInformation, OpenProcessToken, TOKEN_QUERY, TOKEN_USER,
            TokenUser,
        },
        Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, GENERIC_READ,
            GENERIC_WRITE, OPEN_EXISTING, ReadFile, WriteFile,
        },
        System::{
            Pipes::{ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_ACCESS_DUPLEX},
            Threading::GetCurrentProcess,
        },
    };

    /// `CloseHandle` on drop; blocking Win32 pipe I/O has no per-call
    /// timeout, so reads run on a helper thread and the owner closes this to
    /// abort them (see `read_with_deadline`).
    struct Handle(HANDLE);
    // SAFETY: the handle is used from one thread at a time; the helper thread
    // never closes, only the owning thread does.
    unsafe impl Send for Handle {}
    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: valid pipe handle owned by us.
            unsafe { CloseHandle(self.0) };
        }
    }

    pub struct PipeListener {
        handle: Handle,
    }
    // SAFETY: moved whole into the listener thread, never shared.
    unsafe impl Send for PipeListener {}

    pub type RawListener = PipeListener;

    fn wide(name: &str) -> Vec<u16> {
        name.encode_utf16().chain([0]).collect()
    }

    fn last_error(context: &str) -> std::io::Error {
        // SAFETY: GetLastError reads thread-local Win32 state.
        let code = unsafe { GetLastError() };
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("{context}: Win32 error {code}"),
        )
    }

    /// `S-1-5-...` of the process token owner; the pipe name carries it so
    /// two users never share an endpoint.
    fn current_user_sid() -> std::io::Result<String> {
        // SAFETY: well-formed Win32 calls; every pointer is valid for its
        // call and every handle is closed exactly once.
        unsafe {
            let mut token: HANDLE = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err(std::io::Error::last_os_error());
            }
            let mut needed = 0u32;
            GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed);
            let mut info = vec![0u8; needed as usize];
            let ok = GetTokenInformation(
                token,
                TokenUser,
                info.as_mut_ptr().cast(),
                needed,
                &mut needed,
            );
            CloseHandle(token);
            if ok == 0 {
                return Err(std::io::Error::last_os_error());
            }
            let sid = (*info.as_ptr().cast::<TOKEN_USER>()).User.Sid;
            let mut wide_sid: *mut u16 = std::ptr::null_mut();
            if ConvertSidToStringSidW(sid, &mut wide_sid) == 0 {
                return Err(std::io::Error::last_os_error());
            }
            let len = (0..).take_while(|&i| *wide_sid.add(i) != 0).count();
            let sid = String::from_utf16_lossy(std::slice::from_raw_parts(wide_sid, len));
            windows_sys::Win32::Foundation::LocalFree(wide_sid.cast());
            Ok(sid)
        }
    }

    fn pipe_name() -> std::io::Result<String> {
        let sid = current_user_sid()?;
        let dev = launcher_core::dev::is_dev_build();
        Ok(super::pipe_name_for_sid(&sid, dev))
    }

    /// `nMaxInstances = 1`: a second creator fails atomically, so exactly one
    /// process can own the endpoint. NULL security = default DACL, i.e. only
    /// this user (plus admins, who can do anything anyway).
    fn create_instance(name: &str) -> std::io::Result<Handle> {
        let name = wide(name);
        // SAFETY: valid name buffer, default security attributes.
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX,
                0, // PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT
                1,
                65536,
                65536,
                0,
                std::ptr::null(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(last_error("cannot create single-instance pipe"));
        }
        Ok(Handle(handle))
    }

    /// `TimedOut` here means "the owner is alive but busy", i.e. Secondary.
    fn open_pipe(name: &str) -> std::io::Result<Handle> {
        let name = wide(name);
        // SAFETY: valid name buffer, synchronous open.
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            // SAFETY: reads thread-local Win32 state.
            let code = unsafe { GetLastError() };
            if code == ERROR_PIPE_BUSY {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "single-instance pipe is busy",
                ));
            }
            return Err(std::io::Error::last_os_error());
        }
        Ok(Handle(handle))
    }

    fn write_all_blocking(handle: HANDLE, mut buf: &[u8]) -> std::io::Result<()> {
        while !buf.is_empty() {
            let mut written = 0u32;
            // SAFETY: buffer is valid for `buf.len()` bytes, synchronous call.
            let ok = unsafe {
                WriteFile(
                    handle,
                    buf.as_ptr(),
                    buf.len().min(u32::MAX as usize) as u32,
                    &mut written,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                return Err(std::io::Error::last_os_error());
            }
            buf = &buf[written as usize..];
        }
        Ok(())
    }

    fn read_exact_blocking(handle: HANDLE, mut buf: &mut [u8]) -> std::io::Result<()> {
        while !buf.is_empty() {
            let mut read = 0u32;
            // SAFETY: buffer is valid for `buf.len()` bytes, synchronous call.
            let ok = unsafe {
                ReadFile(
                    handle,
                    buf.as_mut_ptr(),
                    buf.len().min(u32::MAX as usize) as u32,
                    &mut read,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                // SAFETY: reads thread-local Win32 state.
                let code = unsafe { GetLastError() };
                if code == ERROR_BROKEN_PIPE {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "single-instance sender went away mid-message",
                    ));
                }
                return Err(std::io::Error::last_os_error());
            }
            if read == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "single-instance pipe closed mid-message",
                ));
            }
            buf = &mut buf[read as usize..];
        }
        Ok(())
    }

    /// Blocking Win32 reads have no per-call timeout, so the read runs on a
    /// helper thread while the caller enforces the overall deadline; on
    /// timeout the caller disconnects, which aborts the helper's read.
    fn read_with_deadline(
        handle: HANDLE,
        len: usize,
        remaining: Duration,
    ) -> std::io::Result<Vec<u8>> {
        let raw = handle as usize;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = vec![0u8; len];
            let result = read_exact_blocking(raw as HANDLE, &mut buf).map(|()| buf);
            let _ = tx.send(result);
        });
        rx.recv_timeout(remaining).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "single-instance read deadline",
            )
        })?
    }

    fn read_message(handle: HANDLE, start: Instant) -> Option<Vec<String>> {
        let elapsed = || ROUND_TRIP.checked_sub(start.elapsed());
        let header = read_with_deadline(handle, HEADER_LEN, elapsed()?).ok()?;
        let header: [u8; HEADER_LEN] = header.try_into().ok()?;
        let len = parse_header(&header)?;
        let payload = read_with_deadline(handle, len, elapsed()?).ok()?;
        // The forwarder waits for this ack before exiting.
        write_all_blocking(handle, &encode_header(0)).ok()?;
        Some(decode_links(&payload))
    }

    pub fn claim() -> std::io::Result<Claim> {
        let name = pipe_name()?;
        match create_instance(&name) {
            Ok(handle) => Ok(Claim::Primary(Listener(PipeListener { handle }))),
            Err(_) => match open_pipe(&name) {
                Ok(_) => Ok(Claim::Secondary),
                Err(error) if error.kind() == std::io::ErrorKind::TimedOut => Ok(Claim::Secondary),
                Err(error) => Err(error),
            },
        }
    }

    /// Second-instance exit path: the owner's ack proves the pipe is really
    /// our launcher. Any failure lets the caller retry the election.
    pub fn forward(links: &[String]) -> std::io::Result<()> {
        use windows_sys::Win32::System::Pipes::WaitNamedPipeW;
        let name = pipe_name()?;
        let wide_name = wide(&name);
        // SAFETY: valid name buffer.
        let ready = unsafe {
            WaitNamedPipeW(
                wide_name.as_ptr(),
                ROUND_TRIP.as_millis().min(u32::MAX as u128) as u32,
            )
        };
        if ready == 0 {
            return Err(last_error("no owner answers the single-instance pipe"));
        }
        let handle = open_pipe(&name)?;
        let message = encode_message(links).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invite payload exceeds the 64 KiB cap",
            )
        })?;
        write_all_blocking(handle.0, &message)?;
        let ack = read_with_deadline(handle.0, HEADER_LEN, ROUND_TRIP)?;
        if ack.as_slice() != encode_header(0) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "single-instance endpoint answered without our protocol header",
            ));
        }
        Ok(())
    }

    /// Owner loop: every connection is one second launch (links or bare
    /// focus). Callbacks run on the Slint thread. Connections without our
    /// header are ignored outright: they must not steal focus.
    pub fn spawn_listener(listener: Listener, weak: slint::Weak<crate::AppWindow>) {
        std::thread::spawn(move || {
            let Listener(listener) = listener;
            loop {
                // SAFETY: synchronous connect on our instance.
                let connected =
                    unsafe { ConnectNamedPipe(listener.handle.0, std::ptr::null_mut()) };
                if connected == 0 {
                    // SAFETY: reads thread-local Win32 state.
                    let code = unsafe { GetLastError() };
                    if code != ERROR_PIPE_CONNECTED {
                        // Owner is going away or broken; stop instead of
                        // spinning on a dead endpoint.
                        // SAFETY: synchronous disconnect.
                        unsafe { DisconnectNamedPipe(listener.handle.0) };
                        break;
                    }
                }
                let start = Instant::now();
                let links = read_message(listener.handle.0, start);
                // Ack-less strangers get no answer; abort the read for the
                // next instance either way.
                // SAFETY: synchronous disconnect.
                unsafe { DisconnectNamedPipe(listener.handle.0) };
                let Some(links) = links else { continue };
                let _ = weak.upgrade_in_event_loop(move |app| {
                    if links.is_empty() {
                        focus_window(&app);
                    } else {
                        for link in &links {
                            handle_link(&app, link);
                        }
                    }
                });
            }
        });
    }
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

    /// Point the endpoint at a fresh temp dir; caller must hold `serial()`.
    /// Unix-only: only Unix transports are exercised in unit tests.
    #[cfg(unix)]
    fn with_temp_endpoint() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: single-threaded under the serial guard; each test sets its
        // own dir before touching the endpoint.
        unsafe { std::env::set_var("BLOCKFIELD_SINGLE_INSTANCE_DIR", dir.path()) };
        dir
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
    fn channel_payload_round_trips_links_and_bare_focus() {
        assert!(decode_links(&encode_links(&[])).is_empty());
        let links = vec![
            "blockfield://join/lobby".to_string(),
            "blockfield://join/12345678-1234-1234-1234-123456789abc".to_string(),
        ];
        assert_eq!(decode_links(&encode_links(&links)), links);
        let message = encode_message(&links).unwrap();
        let (header, payload) = message.split_at(HEADER_LEN);
        assert_eq!(
            parse_header(header.try_into().unwrap()),
            Some(payload.len())
        );
        assert_eq!(decode_links(payload), links);
    }

    #[test]
    fn strangers_without_our_header_are_rejected() {
        let mut bad = encode_header(0);
        bad[0] ^= 0xff;
        assert_eq!(parse_header(&bad), None);
        let mut future = encode_header(0);
        future[4..6].copy_from_slice(&99u16.to_le_bytes());
        assert_eq!(parse_header(&future), None);
        let mut huge = encode_header(0);
        huge[6..10].copy_from_slice(&(MAX_PAYLOAD as u32 + 1).to_le_bytes());
        assert_eq!(parse_header(&huge), None);
        assert!(encode_message(&["x".repeat(MAX_PAYLOAD + 1)]).is_none());
    }

    #[test]
    fn pipe_name_carries_the_user_sid() {
        assert_eq!(
            pipe_name_for_sid("S-1-5-21-1-2-3-1001", false),
            r"\\.\pipe\blockfield-launcher-S-1-5-21-1-2-3-1001"
        );
        assert_eq!(
            pipe_name_for_sid("S-1-5-21-1-2-3-1001", true),
            r"\\.\pipe\blockfield-launcher-S-1-5-21-1-2-3-1001-dev"
        );
    }

    #[test]
    #[cfg(unix)]
    fn second_instance_forwards_links_to_the_owner() {
        let _serial = serial();
        let _dir = with_temp_endpoint();
        let Claim::Primary(listener) = claim().unwrap() else {
            panic!("first claim on a fresh dir must elect a primary");
        };
        let received = std::thread::spawn(move || {
            let (stream, _) = listener.0.accept().unwrap();
            imp::test_read_message_with_deadline(stream, ROUND_TRIP)
        });
        forward(&["blockfield://join/lobby".to_string()]).unwrap();
        assert_eq!(
            received.join().unwrap(),
            Some(vec!["blockfield://join/lobby".to_string()])
        );
    }

    #[test]
    #[cfg(unix)]
    fn owner_election_reclaims_a_stale_socket() {
        let _serial = serial();
        let _dir = with_temp_endpoint();
        let path = imp::test_socket_path().unwrap();
        // A dead owner leaves its socket file behind; the next launch leads.
        drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
        assert!(matches!(claim().unwrap(), Claim::Primary(_)));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    #[cfg(unix)]
    fn unrelated_listener_is_never_mistaken_for_the_owner() {
        let _serial = serial();
        let _dir = with_temp_endpoint();
        let path = imp::test_socket_path().unwrap();
        // A stranger holds our path but speaks no protocol: claim still says
        // Secondary, but forward fails instead of delivering links to it.
        let stranger = std::os::unix::net::UnixListener::bind(&path).unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = stranger.accept().unwrap();
            use std::io::Read;
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            // No ack: not our launcher.
        });
        assert!(matches!(claim().unwrap(), Claim::Secondary));
        let result = imp::test_forward_to(
            &path,
            &["blockfield://join/lobby".into()],
            Duration::from_secs(2),
        );
        assert!(
            result.is_err(),
            "forward to a stranger must fail: {result:?}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn oversized_payload_is_ignored() {
        use std::os::unix::net::UnixStream;
        let (reader, mut writer) = UnixStream::pair().unwrap();
        // The header claims more than the cap: no allocation, no links.
        let mut header = encode_header(0);
        header[6..10].copy_from_slice(&(MAX_PAYLOAD as u32 + 1).to_le_bytes());
        writer.write_all(&header).unwrap();
        drop(writer);
        assert_eq!(
            imp::test_read_message_with_deadline(reader, ROUND_TRIP),
            None
        );
    }

    #[test]
    #[cfg(unix)]
    fn stalled_sender_hits_the_read_deadline() {
        use std::os::unix::net::UnixStream;
        let (reader, mut writer) = UnixStream::pair().unwrap();
        // A valid header and a partial payload, then silence: the owner must
        // not wait out the full window. The writer stays alive so only the
        // deadline can end the read.
        writer.write_all(&encode_header(10)).unwrap();
        writer.write_all(b"12").unwrap();
        let start = Instant::now();
        let result = imp::test_read_message_with_deadline(reader, Duration::from_millis(300));
        assert_eq!(result, None);
        assert!(
            start.elapsed() < ROUND_TRIP,
            "deadline must cut the wait short"
        );
    }

    #[test]
    #[cfg(unix)]
    fn election_is_retried_after_the_owner_dies() {
        let _serial = serial();
        let _dir = with_temp_endpoint();
        // The primary answers claim, then dies before forward connects — the
        // exact window the review covers. Forward fails, the retry elects us
        // primary, and the invite survives via the local handle path.
        let Claim::Primary(first) = claim().unwrap() else {
            panic!("first claim on a fresh dir must elect a primary");
        };
        assert!(matches!(claim().unwrap(), Claim::Secondary));
        drop(first);
        let _ = std::fs::remove_file(imp::test_socket_path().unwrap());
        assert!(forward(&["blockfield://join/lobby".to_string()]).is_err());
        let retried = claim().unwrap();
        assert!(
            matches!(retried, Claim::Primary(_)),
            "retry after a failed forward must elect us primary"
        );
        let _ = std::fs::remove_file(imp::test_socket_path().unwrap());
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
