//! Second-launch focus + deep-link forwarding through one local channel.
//!
//! Tauri parity (`src-tauri/src/lib.rs:47-49` + `single-instance` plugin with
//! the `deep-link` feature): exactly one launcher owns the window. A second
//! process forwards its invite-link argv to the running instance and exits;
//! the owner shows its window and routes each link to the pending-room store.
//! Windows + Linux only (macOS left the native target).

use slint::ComponentHandle;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    time::Duration,
};

/// Loopback port for the release identifier.
pub const PORT_PROD: u16 = 41731;
/// Separate channel so a dev build never steals a release launch.
pub const PORT_DEV: u16 = 41732;

/// Channel for this build; `BLOCKFIELD_SINGLE_INSTANCE_PORT` overrides it
/// (tests pin an ephemeral port through `claim` directly).
pub fn port() -> u16 {
    if let Ok(raw) = std::env::var("BLOCKFIELD_SINGLE_INSTANCE_PORT")
        && let Ok(port) = raw.parse::<u16>()
        && port != 0
    {
        return port;
    }
    if launcher_core::dev::is_dev_build() {
        PORT_DEV
    } else {
        PORT_PROD
    }
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

/// Owner election on the loopback channel: `Primary` keeps the listener,
/// `Secondary` must forward and exit.
pub enum Claim {
    Primary(TcpListener),
    Secondary,
}

pub fn claim(port: u16) -> std::io::Result<Claim> {
    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => Ok(Claim::Primary(listener)),
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => Ok(Claim::Secondary),
        Err(error) => Err(error),
    }
}

/// Second-instance exit path: hand every link to the owner (an empty payload
/// still wakes it, so a bare second launch focuses the window like Tauri).
pub fn forward(port: u16, links: &[String]) -> std::io::Result<()> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(&encode_links(links))?;
    stream.shutdown(std::net::Shutdown::Write)?;
    Ok(())
}

fn read_links(stream: TcpStream) -> Vec<String> {
    let mut stream = stream;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf);
    decode_links(&buf)
}

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

/// Owner loop: every connection is one second launch (links or bare focus).
/// Callbacks run on the Slint thread, so links land after `wire_app` too.
pub fn spawn_listener(listener: TcpListener, weak: slint::Weak<crate::AppWindow>) {
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let links = read_links(stream);
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
mod tests {
    use super::*;

    fn serial() -> std::sync::MutexGuard<'static, ()> {
        crate::main_ops::test_serial()
    }

    #[test]
    fn only_valid_invite_links_reach_the_channel() {
        let app_id = launcher_app::presence::application_id();
        let lobby = "blockfield://join/lobby".to_string();
        let discord = format!("discord-{}://join?secret=bf1%3Alobby", app_id);
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
    }

    #[test]
    fn second_instance_forwards_links_to_the_owner() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let received = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            read_links(stream)
        });
        forward(port, &["blockfield://join/lobby".to_string()]).unwrap();
        assert_eq!(
            received.join().unwrap(),
            vec!["blockfield://join/lobby".to_string()]
        );
    }

    #[test]
    fn owner_election_follows_the_bound_port() {
        let holder = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = holder.local_addr().unwrap().port();
        assert!(matches!(claim(port).unwrap(), Claim::Secondary));
        drop(holder);
        let Claim::Primary(listener) = claim(port).unwrap() else {
            panic!("freed loopback port must elect a primary");
        };
        drop(listener);
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
