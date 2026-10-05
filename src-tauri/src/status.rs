//! Minecraft Server List Ping over plain TCP (std only), run on a blocking thread.
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const PROTOCOL_1_20_1: i32 = 763;
const TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    pub online: bool,
    pub rooms: Option<Vec<GameRoom>>,
    /// Site API view of the game server behind the proxy; `None` when the API is unreachable.
    pub game_available: Option<bool>,
    /// Set by the host controller while draining/workshop; implies `game_available == Some(false)`.
    pub maintenance: Option<Maintenance>,
    pub players_online: Option<u64>,
    pub players_max: Option<u64>,
    pub minecraft_version: Option<String>,
    pub motd: Option<serde_json::Value>,
    pub host: String,
    pub port: u16,
    pub region_code: String,
    pub location_name: String,
    pub server_latency_ms: Option<u64>,
    pub checked_at: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Maintenance {
    pub message: String,
    pub since: Option<u64>,
}

impl Maintenance {
    fn parse(value: &serde_json::Value) -> Option<Self> {
        if !value.is_object() {
            return None;
        }
        Some(Self {
            message: value["message"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .chars()
                .take(500)
                .collect(),
            since: value["since"].as_u64(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameRoom {
    #[serde(default)]
    pub battle: Option<RoomBattle>,
    pub id: String,
    pub name: String,
    pub mode: String,
    pub map: String,
    pub phase: String,
    pub players: u32,
    pub joinable: bool,
    /// `casual` or `ranked`; absent on older servers, which only ever ran Casual rooms.
    #[serde(default)]
    pub format: Option<String>,
    /// `open` (join now), `waiting` (queue only) or `closed` (full or campaign running).
    #[serde(default)]
    pub admission: Option<String>,
    #[serde(default)]
    pub ready: Option<RoomReady>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomReady {
    pub ready: u32,
    pub required: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomBattle {
    pub red: u32,
    pub blue: u32,
    pub points: Vec<CapturePoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturePoint {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub claiming: String,
    pub capturing: String,
    pub progress: u8,
}

impl RoomBattle {
    fn valid(&self) -> bool {
        self.points.len() <= 100
            && self.points.iter().all(|point| {
                point.id.len() <= 256
                    && point.name.len() <= 256
                    && point.progress <= 100
                    && ["RED", "BLUE", "NEUTRAL"].contains(&point.owner.as_str())
                    && ["RED", "BLUE", "NONE"].contains(&point.claiming.as_str())
                    && ["RED", "BLUE", "NONE"].contains(&point.capturing.as_str())
            })
    }
}

#[derive(Clone, Default, Deserialize)]
struct GeoLocation {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    country_code: String,
    #[serde(default)]
    country: String,
    #[serde(default)]
    city: String,
}

struct CachedLocation {
    host: String,
    checked: Instant,
    location: GeoLocation,
}

static LOCATION_CACHE: tokio::sync::Mutex<Option<CachedLocation>> =
    tokio::sync::Mutex::const_new(None);

pub async fn locate(host: &str, port: u16) -> (String, String) {
    let mut cache = LOCATION_CACHE.lock().await;
    if let Some(entry) = cache.as_ref() {
        let ttl = if entry.location.success { 86_400 } else { 300 };
        if entry.host == host && entry.checked.elapsed().as_secs() < ttl {
            return location_labels(&entry.location);
        }
    }
    let target = (host.to_string(), port);
    let address = tokio::task::spawn_blocking(move || {
        (target.0.as_str(), target.1)
            .to_socket_addrs()
            .ok()?
            .map(|addr| addr.ip())
            .find(|ip| public_address(*ip))
    })
    .await
    .ok()
    .flatten();
    let location = match address {
        Some(ip) => async {
            reqwest::Client::builder()
                .timeout(TIMEOUT)
                .build()
                .ok()?
                .get(format!("https://ipwho.is/{ip}"))
                .query(&[("fields", "success,country_code,country,city")])
                .send()
                .await
                .ok()?
                .error_for_status()
                .ok()?
                .json::<GeoLocation>()
                .await
                .ok()
        }
        .await
        .unwrap_or_default(),
        None => GeoLocation::default(),
    };
    let labels = location_labels(&location);
    *cache = Some(CachedLocation {
        host: host.to_string(),
        checked: Instant::now(),
        location,
    });
    labels
}

fn public_address(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ip) => {
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && !ip.is_broadcast()
                && !ip.is_documentation()
        }
        std::net::IpAddr::V6(ip) => ip
            .to_ipv4_mapped()
            .map(|ip| public_address(ip.into()))
            .unwrap_or_else(|| {
                !ip.is_loopback()
                    && !ip.is_unspecified()
                    && !ip.is_unique_local()
                    && !ip.is_unicast_link_local()
                    && !ip.is_multicast()
            }),
    }
}

fn location_labels(location: &GeoLocation) -> (String, String) {
    if !location.success
        || location.country_code.len() != 2
        || !location
            .country_code
            .bytes()
            .all(|b| b.is_ascii_alphabetic())
    {
        return (String::new(), String::new());
    }
    let name = [&location.city, &location.country]
        .into_iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    (location.country_code.to_uppercase(), name)
}

pub fn split_host_port(target: &str) -> (String, u16) {
    let raknet = target.strip_prefix("raknet;");
    let target = raknet.unwrap_or(target);
    let (host, port) = match target.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => (host.to_string(), port.parse().unwrap_or(25565)),
        _ => (target.to_string(), 25565),
    };
    // Status uses TCP SLP; Blockfield's RakNet UDP listener has a separate port.
    (host, if raknet.is_some() { 25565 } else { port })
}

pub fn ping(host: &str, port: u16) -> ServerStatus {
    let checked_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let offline = ServerStatus {
        online: false,
        rooms: None,
        game_available: None,
        maintenance: None,
        players_online: None,
        players_max: None,
        minecraft_version: None,
        motd: None,
        host: host.to_string(),
        port,
        region_code: String::new(),
        location_name: String::new(),
        server_latency_ms: None,
        checked_at,
    };
    match query(host, port) {
        Ok((json, latency)) => ServerStatus {
            online: true,
            players_online: json["players"]["online"].as_u64(),
            players_max: json["players"]["max"].as_u64(),
            minecraft_version: json["version"]["name"].as_str().map(str::to_string),
            motd: Some(json["description"].clone()),
            server_latency_ms: Some(latency),
            ..offline
        },
        Err(error) => {
            log::info!("Server ping {host}:{port} failed: {error}");
            offline
        }
    }
}

fn query(host: &str, port: u16) -> Result<(serde_json::Value, u64), String> {
    let addr = (host, port)
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .next()
        .ok_or("no address")?;
    let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(TIMEOUT)).ok();
    stream.set_write_timeout(Some(TIMEOUT)).ok();

    let mut handshake = vec![0x00];
    write_varint(&mut handshake, PROTOCOL_1_20_1);
    write_varint(&mut handshake, host.len() as i32);
    handshake.extend_from_slice(host.as_bytes());
    handshake.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut handshake, 1);
    write_packet(&mut stream, &handshake)?;

    let started = Instant::now();
    write_packet(&mut stream, &[0x00])?;
    let response = read_packet(&mut stream)?;
    let latency = started.elapsed().as_millis() as u64;

    let mut cursor = 0;
    let packet_id = read_varint(&response, &mut cursor)?;
    if packet_id != 0 {
        return Err(format!("unexpected packet id {packet_id}"));
    }
    let len = read_varint(&response, &mut cursor)? as usize;
    let json = response
        .get(cursor..cursor + len)
        .ok_or("truncated status json")?;
    let json = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    Ok((json, latency))
}

fn write_packet(stream: &mut TcpStream, payload: &[u8]) -> Result<(), String> {
    let mut packet = Vec::with_capacity(payload.len() + 5);
    write_varint(&mut packet, payload.len() as i32);
    packet.extend_from_slice(payload);
    stream.write_all(&packet).map_err(|e| e.to_string())
}

fn read_packet(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut length = 0i32;
    for shift in (0..35).step_by(7) {
        let mut byte = [0u8];
        stream.read_exact(&mut byte).map_err(|e| e.to_string())?;
        length |= i32::from(byte[0] & 0x7f) << shift;
        if byte[0] & 0x80 == 0 {
            break;
        }
    }
    if !(1..=1 << 20).contains(&length) {
        return Err(format!("bad packet length {length}"));
    }
    let mut payload = vec![0u8; length as usize];
    stream.read_exact(&mut payload).map_err(|e| e.to_string())?;
    Ok(payload)
}

fn write_varint(out: &mut Vec<u8>, mut value: i32) {
    loop {
        let byte = (value & 0x7f) as u8;
        value = ((value as u32) >> 7) as i32;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn read_varint(data: &[u8], cursor: &mut usize) -> Result<i32, String> {
    let mut value = 0i32;
    for shift in (0..35).step_by(7) {
        let byte = *data.get(*cursor).ok_or("truncated varint")?;
        *cursor += 1;
        value |= i32::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err("varint too long".to_string())
}

/// Rooms come from the site API because the proxy answers pings even while the game server is down.
pub async fn fetch_rooms(base: &str) -> Option<RoomsResponse> {
    let value = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .ok()?
        .get(format!("{base}/api/rooms"))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json::<serde_json::Value>()
        .await
        .ok()?;
    rooms_response(&value)
}

const STATS_TIMEOUT: Duration = Duration::from_secs(5);
const STATS_QUERY_KEYS: [&str; 6] = ["mode", "period", "season", "sort", "limit", "page"];

/// Only the public read endpoints; the player segment must be a lowercase dashed UUID.
fn valid_stats_path(path: &str) -> bool {
    match path.strip_prefix("players/") {
        Some(uuid) => {
            uuid.len() == 36
                && uuid.bytes().enumerate().all(|(i, b)| match i {
                    8 | 13 | 18 | 23 => b == b'-',
                    _ => matches!(b, b'0'..=b'9' | b'a'..=b'f'),
                })
        }
        None => matches!(path, "leaderboard" | "seasons"),
    }
}

/// Stats are read from the site API, independent of the game server. 404 is `Ok(None)`.
pub async fn fetch_stats(
    base: &str,
    path: &str,
    query: &std::collections::BTreeMap<String, String>,
) -> Result<Option<serde_json::Value>, String> {
    if !valid_stats_path(path)
        || query
            .keys()
            .any(|k| !STATS_QUERY_KEYS.contains(&k.as_str()))
    {
        return Err(format!("Invalid stats request: {path}"));
    }
    let response = reqwest::Client::builder()
        .timeout(STATS_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?
        .get(format!("{base}/api/stats/{path}"))
        .query(query)
        .send()
        .await
        .map_err(|e| format!("Stats API unavailable: {e}"))?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let body = response.json::<serde_json::Value>().await;
    if !status.is_success() {
        let detail = body
            .ok()
            .and_then(|value| value["error"].as_str().map(str::to_string))
            .unwrap_or_else(|| status.to_string());
        return Err(format!("Stats API: {detail}"));
    }
    body.map(Some)
        .map_err(|e| format!("Invalid stats response: {e}"))
}

pub type RoomsResponse = (bool, Vec<GameRoom>, Option<Maintenance>);

fn rooms_response(value: &serde_json::Value) -> Option<RoomsResponse> {
    let available = value["available"].as_bool()?;
    if let Some(maintenance) = Maintenance::parse(&value["maintenance"]) {
        return Some((false, Vec::new(), Some(maintenance)));
    }
    if !available {
        return Some((false, Vec::new(), None));
    }
    room_snapshot(value).map(|rooms| (true, rooms, None))
}

fn room_snapshot(value: &serde_json::Value) -> Option<Vec<GameRoom>> {
    let checked = value["checkedAt"].as_u64()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;
    if now.abs_diff(checked) > 30_000 || !value["rooms"].is_array() {
        return None;
    }
    let rooms: Vec<GameRoom> = serde_json::from_value(value["rooms"].clone()).ok()?;
    if rooms.len() > 100
        || rooms.iter().any(|room| {
            room.battle.as_ref().is_some_and(|battle| !battle.valid())
                || !crate::rooms::valid_id(&room.id)
                || !["IDLE", "VOTING", "PREPARING", "GAME", "MATCH_END"]
                    .contains(&room.phase.as_str())
                || room.name.is_empty()
                || room.name.len() > 256
                || room.mode.len() > 256
                || room.map.len() > 256
                || room
                    .format
                    .as_deref()
                    .is_some_and(|f| !["casual", "ranked"].contains(&f))
                || room
                    .admission
                    .as_deref()
                    .is_some_and(|a| !["open", "waiting", "closed"].contains(&a))
        })
    {
        return None;
    }
    Some(rooms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_and_malformed_rooms_are_unavailable() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let mut json = serde_json::json!({"available": true, "checkedAt": now, "rooms": []});
        assert_eq!(
            rooms_response(&json).map(|(up, rooms, m)| (up, rooms.len(), m)),
            Some((true, 0, None))
        );
        json["rooms"] = serde_json::json!([null]);
        assert!(rooms_response(&json).is_none());
        json["rooms"] = serde_json::json!([]);
        json["checkedAt"] = serde_json::json!(now - 60_000);
        assert!(rooms_response(&json).is_none());
        json["available"] = serde_json::json!(false);
        assert_eq!(
            rooms_response(&json).map(|(up, rooms, m)| (up, rooms.len(), m)),
            Some((false, 0, None))
        );
        assert!(rooms_response(&serde_json::json!({"rooms": []})).is_none());
    }

    #[test]
    fn maintenance_is_passed_through_and_forces_unavailable() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let room = serde_json::json!({"id": "12345678-1234-1234-1234-123456789abc", "name": "Лобби", "mode": "", "map": "", "phase": "IDLE", "players": 1, "joinable": true});
        let mut json = serde_json::json!({"available": false, "checkedAt": now, "rooms": [], "lobby": room, "maintenance": {"message": "  Карта обновляется  ", "since": 1_700_000_000_000u64}});
        let summary = |json: &serde_json::Value| {
            rooms_response(json).map(|(up, rooms, m)| (up, rooms.len(), m))
        };
        assert_eq!(
            summary(&json),
            Some((
                false,
                0,
                Some(Maintenance {
                    message: "Карта обновляется".into(),
                    since: Some(1_700_000_000_000)
                })
            ))
        );

        json["available"] = serde_json::json!(true);
        json["rooms"] = serde_json::json!([room]);
        json["maintenance"] = serde_json::json!({"message": "x".repeat(2_000), "since": "soon"});
        let (up, rooms, maintenance) = rooms_response(&json).unwrap();
        let maintenance = maintenance.unwrap();
        assert!(!up && rooms.is_empty());
        assert_eq!((maintenance.message.len(), maintenance.since), (500, None));

        json["maintenance"] = serde_json::json!({"message": 42});
        assert_eq!(
            summary(&json).and_then(|(_, _, m)| m).map(|m| m.message),
            Some(String::new())
        );
        json["maintenance"] = serde_json::Value::Null;
        assert_eq!(summary(&json), Some((true, 1, None)));
    }

    #[test]
    fn rooms_with_extended_fields_are_accepted() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let mut json = serde_json::json!({"available": true, "checkedAt": now, "rooms": [{
            "id": "12345678-1234-1234-1234-123456789abc", "name": "Комната 1", "mode": "Захват точек",
            "map": "Город", "phase": "PREPARING", "players": 4, "joinable": true,
            "capacity": 16, "format": "ranked", "admission": "open", "round": 2,
            "front": {"index": 1, "length": 3}, "ready": {"ready": 3, "required": 4}, "reserved": 2
        }]});
        let (up, rooms, maintenance) = rooms_response(&json).unwrap();
        assert!(up && maintenance.is_none());
        assert_eq!(rooms[0].players, 4);
        assert_eq!(rooms[0].format.as_deref(), Some("ranked"));
        assert_eq!(rooms[0].admission.as_deref(), Some("open"));
        let ready = rooms[0].ready.as_ref().unwrap();
        assert_eq!((ready.ready, ready.required), (3, 4));

        json["rooms"][0]["format"] = serde_json::json!("5v5");
        assert!(rooms_response(&json).is_none());
        json["rooms"][0]["format"] = serde_json::json!("ranked");
        json["rooms"][0]["admission"] = serde_json::json!("OPEN");
        assert!(rooms_response(&json).is_none());
    }

    #[test]
    fn stats_paths_are_limited_to_public_read_endpoints() {
        for path in [
            "leaderboard",
            "seasons",
            "players/f3e1b9a6-2270-3dee-8782-b7cdd8519efe",
        ] {
            assert!(valid_stats_path(path), "{path}");
        }
        for path in [
            "",
            "players/",
            "players/F3E1B9A6-2270-3DEE-8782-B7CDD8519EFE",
            "players/f3e1b9a622703dee8782b7cdd8519efe",
            "players/f3e1b9a6-2270-3dee-8782-b7cdd8519ef",
            "players/f3e1b9a6-2270-3dee-8782-b7cdd8519efe/x",
            "players/../../admin/xxxxxxxxxxxxxxxxxxxxxxxxxxx",
            "leaderboard/../ingest",
            "ingest",
        ] {
            assert!(!valid_stats_path(path), "{path}");
        }
    }

    #[test]
    fn location_errors_and_private_addresses_do_not_produce_a_region() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "192.168.1.1",
            "::1",
            "fc00::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(!public_address(ip.parse().unwrap()));
        }
        assert!(public_address("8.8.8.8".parse().unwrap()));
        assert_eq!(
            location_labels(&GeoLocation::default()),
            (String::new(), String::new())
        );
        let location: GeoLocation = serde_json::from_value(serde_json::json!({"success": true, "country_code": "ua", "country": "Ukraine", "city": "Kyiv"})).unwrap();
        assert_eq!(
            location_labels(&location),
            ("UA".into(), "Kyiv, Ukraine".into())
        );
    }

    #[test]
    fn varint_roundtrip_and_host_port_split() {
        for value in [0, 1, 127, 128, 300, PROTOCOL_1_20_1, i32::MAX] {
            let mut buf = Vec::new();
            write_varint(&mut buf, value);
            let mut cursor = 0;
            assert_eq!(read_varint(&buf, &mut cursor).unwrap(), value);
            assert_eq!(cursor, buf.len());
        }
        assert_eq!(
            split_host_port("mc.example:25566"),
            ("mc.example".into(), 25566)
        );
        assert_eq!(split_host_port("mc.example"), ("mc.example".into(), 25565));
        assert_eq!(
            split_host_port("raknet;mc.example:25566"),
            ("mc.example".into(), 25565)
        );
        assert_eq!(
            split_host_port("raknet;mc.example"),
            ("mc.example".into(), 25565)
        );
        assert_eq!(
            split_host_port("raknet;[::1]:25566"),
            ("[::1]".into(), 25565)
        );
    }
}
