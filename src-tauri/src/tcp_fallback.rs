//! TCP fallback after a RakNet connect failure (proxy#74 U3).
//!
//! The proxy accepts TCP 25565 logins through the same Drasl gate as RakNet and
//! always routes them to the Game. When the RakNet/UDP path is broken for a
//! player, the next launch uses TCP `host:25565`. The choice is remembered per
//! server host for 24 h, then the next launch retries RakNet.
//!
//! The launcher cannot observe the in-game RakNet handshake (the address is
//! handed to the game via `--quickPlayMultiplayer`; only the process outcome
//! is visible), so the observable signal is a pre-launch RakNet unconnected
//! ping to the resolved address: no valid pong means the RakNet path is down.
//! A wrong "down" verdict still logs the player in (via TCP); a wrong "up" is
//! near-impossible (the pong must echo our id and the RakNet magic).
use std::collections::BTreeMap;
use std::net::{ToSocketAddrs, UdpSocket};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// TCP login port the proxy routes to the Game.
pub const TCP_PORT: u16 = 25565;
/// RakNet port the proxy routes to the Game. Other RakNet ports (e.g. the
/// Workshop's 25567) have no TCP equivalent: TCP 25565 would land the player
/// in the Game, so they never fall back.
const GAME_RAKNET_PORT: u16 = 25566;
/// A remembered fallback expires after this; the next launch retries RakNet.
pub const FALLBACK_TTL_SECS: u64 = 24 * 3600;
const STATE_FILE: &str = "tcp_fallback.json";
const PROBE_TIMEOUT: Duration = Duration::from_millis(1200);
const RAKNET_MAGIC: [u8; 16] = [
    0x00, 0xff, 0xff, 0x00, 0xfe, 0xfe, 0xfe, 0xfe, 0xfd, 0xfd, 0xfd, 0xfd, 0x12, 0x34, 0x56, 0x78,
];

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `raknet;host:25566` → `(host, 25566)`; anything without a Game-routeable
/// RakNet endpoint (plain TCP, Workshop port, malformed) → `None`.
pub fn raknet_endpoint(target: &str) -> Option<(String, u16)> {
    let rest = target.trim().strip_prefix("raknet;")?;
    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }
    let port = if let Some(stripped) = rest.strip_prefix('[') {
        let (host, tail) = stripped.split_once(']')?;
        if host.is_empty() {
            return None;
        }
        if tail.is_empty() {
            GAME_RAKNET_PORT
        } else {
            tail.strip_prefix(':')?.parse::<u16>().ok()?
        }
    } else {
        match rest.rsplit_once(':') {
            Some((host, port_str)) if !host.is_empty() => port_str.parse::<u16>().ok()?,
            _ => GAME_RAKNET_PORT,
        }
    };
    if port != GAME_RAKNET_PORT {
        return None;
    }
    // Normalized (lowercase, brackets/DNS rules) or rejected, same as connect.
    let host = crate::config::server_host(&format!("raknet;{rest}")).ok()?;
    Some((host, port))
}

/// TCP address replacing a Game RakNet target, or `None` when the target has
/// no TCP equivalent (plain address, Workshop port, malformed).
pub fn tcp_target_for(target: &str) -> Option<String> {
    let (host, _) = raknet_endpoint(target)?;
    if host.parse::<std::net::Ipv6Addr>().is_ok() {
        Some(format!("[{host}]:{TCP_PORT}"))
    } else {
        Some(format!("{host}:{TCP_PORT}"))
    }
}

/// Address to hand the game: TCP when a fresh fallback is remembered for the
/// host, otherwise the target unchanged.
pub fn resolve_target(target: &str, state: &FallbackState, now: u64) -> String {
    match raknet_endpoint(target) {
        // tcp_target_for cannot fail here: the endpoint just parsed.
        Some((host, _)) if state.is_fresh(&host, now) => {
            tcp_target_for(target).unwrap_or_else(|| target.to_string())
        }
        _ => target.to_string(),
    }
}

/// Remembered RakNet failures per (normalized) server host.
#[derive(Debug, Default)]
pub struct FallbackState {
    entries: BTreeMap<String, u64>,
}

impl FallbackState {
    pub fn load(app_data_dir: &Path) -> Self {
        let data = std::fs::read(app_data_dir.join(STATE_FILE)).unwrap_or_default();
        let parsed: BTreeMap<String, u64> = serde_json::from_slice(&data).unwrap_or_default();
        let mut state = Self {
            entries: BTreeMap::new(),
        };
        for (host, at) in parsed {
            if !host.is_empty() && host.len() <= 253 && at > 0 {
                state.entries.insert(host, at);
            }
        }
        state
    }

    pub fn save(&self, app_data_dir: &Path, now: u64) -> std::io::Result<()> {
        let fresh: BTreeMap<&String, &u64> = self
            .entries
            .iter()
            .filter(|(_, at)| now.saturating_sub(**at) < FALLBACK_TTL_SECS)
            .collect();
        std::fs::write(
            app_data_dir.join(STATE_FILE),
            serde_json::to_vec(&fresh).unwrap_or_default(),
        )
    }

    pub fn record(&mut self, host: &str, now: u64) {
        if !host.is_empty() {
            self.entries.insert(host.to_string(), now);
        }
    }

    pub fn is_fresh(&self, host: &str, now: u64) -> bool {
        self.entries
            .get(host)
            .is_some_and(|at| now.saturating_sub(*at) < FALLBACK_TTL_SECS)
    }
}

fn build_ping(ping_id: i64, client_guid: i64) -> [u8; 33] {
    let mut packet = [0u8; 33];
    packet[0] = 0x01;
    packet[1..9].copy_from_slice(&ping_id.to_be_bytes());
    packet[9..25].copy_from_slice(&RAKNET_MAGIC);
    packet[25..33].copy_from_slice(&client_guid.to_be_bytes());
    packet
}

fn is_valid_pong(packet: &[u8], ping_id: i64) -> bool {
    packet.len() >= 33
        && packet[0] == 0x1c
        && packet[1..9] == ping_id.to_be_bytes()
        && packet[17..33] == RAKNET_MAGIC
}

/// True when the RakNet listener answers an unconnected ping. Any error
/// (timeout, refused, garbage reply) means the path is unusable from here.
pub fn raknet_reachable(host: &str, port: u16) -> bool {
    let host_part = match host.parse::<std::net::Ipv6Addr>() {
        Ok(_) => format!("[{host}]"),
        Err(_) => host.to_string(),
    };
    let mut addrs = match format!("{host_part}:{port}").to_socket_addrs() {
        Ok(addrs) => addrs,
        Err(_) => return false,
    };
    let addr = match addrs.next() {
        Some(addr) => addr,
        None => return false,
    };
    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(socket) => socket,
        Err(_) => return false,
    };
    if socket.set_read_timeout(Some(PROBE_TIMEOUT)).is_err() {
        return false;
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as i64);
    let packet = build_ping(nanos, nanos ^ 0x5f3759df);
    if socket.send_to(&packet, addr).is_err() {
        return false;
    }
    let mut buf = [0u8; 1500];
    match socket.recv_from(&mut buf) {
        Ok((n, _)) => is_valid_pong(&buf[..n], nanos),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_raknet_target_maps_to_tcp_25565() {
        assert_eq!(
            tcp_target_for("raknet;play.example:25566"),
            Some("play.example:25565".into())
        );
        assert_eq!(
            tcp_target_for("raknet;PLAY.Example.:25566"),
            Some("play.example:25565".into())
        );
    }

    #[test]
    fn non_game_targets_have_no_tcp_equivalent() {
        for target in [
            "raknet;workshop.example:25567",
            "raknet;game.example:25568",
            "play.example:25565",
            "play.example",
            "raknet;",
            "raknet;[::1]:25567",
            "",
            "raknet;game.example:notaport",
        ] {
            assert_eq!(tcp_target_for(target), None, "{target}");
        }
        assert_eq!(
            tcp_target_for("raknet;[::1]:25566"),
            Some("[::1]:25565".into())
        );
    }

    #[test]
    fn fresh_fallback_uses_tcp_expired_retries_raknet() {
        let target = "raknet;play.example:25566";
        let mut state = FallbackState::default();
        let now = 1_800_000_000;
        assert_eq!(resolve_target(target, &state, now), target);
        state.record("play.example", now);
        assert_eq!(resolve_target(target, &state, now), "play.example:25565");
        // Exactly at the TTL the entry expires: RakNet is retried.
        assert_eq!(
            resolve_target(target, &state, now + FALLBACK_TTL_SECS),
            target
        );
        assert_eq!(
            resolve_target(target, &state, now + FALLBACK_TTL_SECS - 1),
            "play.example:25565"
        );
    }

    #[test]
    fn fallback_is_per_host() {
        let mut state = FallbackState::default();
        state.record("other.example", 1_800_000_000);
        let target = "raknet;play.example:25566";
        assert_eq!(resolve_target(target, &state, 1_800_000_000), target);
    }

    #[test]
    fn corrupted_state_loads_empty_and_save_prunes_expired() {
        let dir =
            std::env::temp_dir().join(format!("blockfield_tcp_fallback_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(STATE_FILE), b"{not json").unwrap();
        assert!(FallbackState::load(&dir).entries.is_empty());
        let mut state = FallbackState::load(&dir);
        let now = 1_800_000_000;
        state.record("fresh.example", now);
        state.record("stale.example", now - FALLBACK_TTL_SECS);
        state.save(&dir, now).unwrap();
        let reloaded = FallbackState::load(&dir);
        assert!(reloaded.is_fresh("fresh.example", now));
        assert!(!reloaded.is_fresh("stale.example", now));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn pong_validation_rejects_garbage() {
        let id = 123456789i64;
        let mut pong = vec![0x1cu8];
        pong.extend_from_slice(&id.to_be_bytes());
        pong.extend_from_slice(&42i64.to_be_bytes());
        pong.extend_from_slice(&RAKNET_MAGIC);
        assert!(is_valid_pong(&pong, id));
        let mut wrong_id = pong.clone();
        wrong_id[1] ^= 0xff;
        assert!(!is_valid_pong(&wrong_id, id));
        let mut wrong_magic = pong.clone();
        wrong_magic[20] ^= 0xff;
        assert!(!is_valid_pong(&wrong_magic, id));
        assert!(!is_valid_pong(&pong[..20], id));
        assert!(!is_valid_pong(&[], id));
    }

    #[test]
    fn loopback_responder_is_reachable_closed_port_is_not() {
        let responder = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = responder.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let mut buf = [0u8; 1500];
            let (n, from) = responder.recv_from(&mut buf).unwrap();
            let id = i64::from_be_bytes(buf[1..9].try_into().unwrap());
            let mut pong = vec![0x1cu8];
            pong.extend_from_slice(&id.to_be_bytes());
            pong.extend_from_slice(&7i64.to_be_bytes());
            pong.extend_from_slice(&RAKNET_MAGIC);
            pong.extend_from_slice(&buf[..n.min(8)]);
            responder.send_to(&pong, from).unwrap();
        });
        assert!(raknet_reachable("127.0.0.1", port));
        handle.join().unwrap();
        // A port nothing listens on answers no pong.
        let closed = UdpSocket::bind("127.0.0.1:0").unwrap();
        let closed_port = closed.local_addr().unwrap().port();
        drop(closed);
        assert!(!raknet_reachable("127.0.0.1", closed_port));
    }
}
