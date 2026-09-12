use serde_json::{json, Value};
use std::{io, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const MAX_FRAME: usize = 64 * 1024;
trait Transport: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Transport for T {}

pub struct Connection {
    stream: Box<dyn Transport>,
    buffer: Vec<u8>,
    nonce: u64,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

impl Connection {
    pub async fn connect(id: &str) -> io::Result<Self> {
        let stream = connect_transport().await?;
        let mut connection = Self {
            stream,
            buffer: Vec::new(),
            nonce: 0,
        };
        connection.send(0, &json!({"v":1,"client_id":id})).await?;
        tokio::time::timeout(Duration::from_secs(5), async {
            let (op, value) = connection.next().await?;
            if op != 1 || value["evt"] != "READY" {
                return Err(invalid("Discord rejected handshake"));
            }
            Ok::<_, io::Error>(())
        })
        .await??;
        Ok(connection)
    }

    pub async fn command(
        &mut self,
        name: &str,
        args: Value,
        event: Option<&str>,
    ) -> io::Result<()> {
        self.nonce += 1;
        let mut value = json!({"cmd":name,"args":args,"nonce":self.nonce.to_string()});
        if let Some(event) = event {
            value["evt"] = event.into();
        }
        self.send(1, &value).await
    }

    pub async fn send(&mut self, op: u32, value: &Value) -> io::Result<()> {
        let body = serde_json::to_vec(value)?;
        if body.len() > MAX_FRAME {
            return Err(invalid("Discord frame too large"));
        }
        let mut frame = Vec::with_capacity(body.len() + 8);
        frame.extend_from_slice(&op.to_le_bytes());
        frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
        frame.extend(body);
        tokio::time::timeout(Duration::from_secs(3), self.stream.write_all(&frame)).await?
    }

    // Keep partial frames across select cancellation when an activity update wins the race.
    pub async fn next(&mut self) -> io::Result<(u32, Value)> {
        loop {
            if let Some(frame) = decode(&mut self.buffer)? {
                return Ok(frame);
            }
            let mut chunk = [0; 4096];
            let count = self.stream.read(&mut chunk).await?;
            if count == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            self.buffer.extend_from_slice(&chunk[..count]);
        }
    }
}

fn decode(buffer: &mut Vec<u8>) -> io::Result<Option<(u32, Value)>> {
    if buffer.len() < 8 {
        return Ok(None);
    }
    let op = u32::from_le_bytes(buffer[..4].try_into().unwrap());
    let length = u32::from_le_bytes(buffer[4..8].try_into().unwrap()) as usize;
    if length > MAX_FRAME {
        return Err(invalid("Discord frame too large"));
    }
    if buffer.len() < length + 8 {
        return Ok(None);
    }
    let value = serde_json::from_slice(&buffer[8..8 + length])?;
    buffer.drain(..8 + length);
    Ok(Some((op, value)))
}

#[cfg(unix)]
async fn connect_transport() -> io::Result<Box<dyn Transport>> {
    let mut roots: Vec<std::path::PathBuf> = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .iter()
        .filter_map(std::env::var_os)
        .filter(|s| !s.is_empty())
        .map(Into::into)
        .collect();
    roots.push("/tmp".into());
    for root in roots {
        for subdir in [
            "",
            "app/com.discordapp.Discord",
            "app/dev.vencord.Vesktop",
            ".flatpak/com.discordapp.Discord/xdg-run",
            ".flatpak/dev.vencord.Vesktop/xdg-run",
        ] {
            for n in 0..10 {
                let path = root.join(subdir).join(format!("discord-ipc-{n}"));
                if let Ok(Ok(stream)) = tokio::time::timeout(
                    Duration::from_millis(200),
                    tokio::net::UnixStream::connect(path),
                )
                .await
                {
                    return Ok(Box::new(stream));
                }
            }
        }
    }
    Err(io::ErrorKind::NotFound.into())
}

#[cfg(windows)]
async fn connect_transport() -> io::Result<Box<dyn Transport>> {
    for n in 0..10 {
        if let Ok(pipe) = tokio::net::windows::named_pipe::ClientOptions::new()
            .open(format!(r"\\?\pipe\discord-ipc-{n}"))
        {
            return Ok(Box::new(pipe));
        }
    }
    Err(io::ErrorKind::NotFound.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fragmented_event_survives_cancelled_read_and_another_frame() {
        let (client, mut server) = tokio::io::duplex(4096);
        let mut connection = Connection {
            stream: Box::new(client),
            buffer: vec![],
            nonce: 0,
        };
        let value = json!({"cmd":"DISPATCH","evt":"ACTIVITY_JOIN","data":{"secret":"abc"}});
        let body = serde_json::to_vec(&value).unwrap();
        let mut frame = 1u32.to_le_bytes().to_vec();
        frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
        frame.extend(body);
        server.write_all(&frame[..11]).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(10), connection.next())
                .await
                .is_err()
        );
        server.write_all(&frame[11..]).await.unwrap();
        server.write_all(&frame).await.unwrap();
        assert_eq!(connection.next().await.unwrap(), (1, value.clone()));
        assert_eq!(connection.next().await.unwrap(), (1, value));
    }

    #[test]
    fn rejects_oversized_and_invalid_json_without_allocating_payload() {
        let mut header = 1u32.to_le_bytes().to_vec();
        header.extend_from_slice(&((MAX_FRAME + 1) as u32).to_le_bytes());
        assert!(decode(&mut header).is_err());
        assert!(decode(&mut vec![1, 0, 0, 0, 1, 0, 0, 0, b'!']).is_err());
    }
}
