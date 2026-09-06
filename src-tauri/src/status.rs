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
    match target.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => (host.to_string(), port.parse().unwrap_or(25565)),
        _ => (target.to_string(), 25565),
    }
}

pub fn ping(host: &str, port: u16) -> ServerStatus {
    let checked_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let offline = ServerStatus {
        online: false,
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

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
