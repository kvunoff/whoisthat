use log::error;
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UnixStream};

use super::protocol::{TcpMessage, EXPECTED_CORE_PROTOCOL_VERSION};

/// Maximum accepted IPC frame. Kept in sync with
/// core/core/lib/TCPServer/TCPServer.go:MaxFrameSize (64 MiB).
const MAX_FRAME_SIZE: usize = 64 * 1024 * 1024;

/// How to reach the core. UDS is the secure default (filesystem-permission
/// gated); TCP is the legacy/remote fallback and only used when configured.
#[derive(Debug, Clone)]
pub enum Endpoint {
    Unix(String),
    Tcp { host: String, port: u16 },
}

impl Endpoint {
    pub fn describe(&self) -> String {
        match self {
            Endpoint::Unix(path) => format!("unix:{}", path),
            Endpoint::Tcp { host, port } => format!("{}:{}", host, port),
        }
    }
}

/// A framed connection to the core over either transport. The wire format
/// (4-byte big-endian length prefix + JSON) is identical on both, so only the
/// underlying stream differs.
enum Stream {
    Unix(UnixStream),
    Tcp(TcpStream),
}

impl Stream {
    async fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        match self {
            Stream::Unix(s) => s.write_all(buf).await,
            Stream::Tcp(s) => s.write_all(buf).await,
        }
    }

    async fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Stream::Unix(s) => s.flush().await,
            Stream::Tcp(s) => s.flush().await,
        }
    }
}

pub struct CoreReadHalf {
    stream: CoreReadStream,
}

pub enum CoreReadStream {
    Unix(tokio::net::unix::OwnedReadHalf),
    Tcp(tokio::net::tcp::OwnedReadHalf),
}

impl CoreReadHalf {
    pub async fn recv(&mut self) -> std::io::Result<TcpMessage> {
        let mut len_buf = [0u8; 4];
        match &mut self.stream {
            CoreReadStream::Unix(s) => s.read_exact(&mut len_buf).await?,
            CoreReadStream::Tcp(s) => s.read_exact(&mut len_buf).await?,
        };

        let len = u32::from_be_bytes(len_buf);
        if len == 0 || len as usize > MAX_FRAME_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid message length: {}", len),
            ));
        }

        let mut payload = vec![0u8; len as usize];
        match &mut self.stream {
            CoreReadStream::Unix(s) => s.read_exact(&mut payload).await?,
            CoreReadStream::Tcp(s) => s.read_exact(&mut payload).await?,
        };

        let msg: TcpMessage = serde_json::from_slice(&payload)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

        Ok(msg)
    }
}

pub struct CoreWriteHalf {
    stream: CoreWriteStream,
}

pub enum CoreWriteStream {
    Unix(tokio::net::unix::OwnedWriteHalf),
    Tcp(tokio::net::tcp::OwnedWriteHalf),
}

impl CoreWriteHalf {
    pub async fn send(&mut self, msg: &str, data: &impl Serialize) -> std::io::Result<()> {
        let payload = serde_json::json!({
            "msg": msg,
            "data": data,
        });
        let json = serde_json::to_vec(&payload)?;

        let len = json.len() as u32;
        if len == 0 || len as usize > MAX_FRAME_SIZE {
            error!("Invalid payload length: {}", len);
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid payload length: {}", len),
            ));
        }

        let len_bytes = len.to_be_bytes();
        match &mut self.stream {
            CoreWriteStream::Unix(s) => {
                s.write_all(&len_bytes).await?;
                s.write_all(&json).await?;
                s.flush().await?;
            }
            CoreWriteStream::Tcp(s) => {
                s.write_all(&len_bytes).await?;
                s.write_all(&json).await?;
                s.flush().await?;
            }
        }
        Ok(())
    }
}

/// Authenticate the opt-in TCP transport with the shared token file written by
/// the core (0600 in the user's runtime directory). UDS is already protected by
/// filesystem permissions and does not use this handshake.
async fn tcp_handshake(stream: &mut TcpStream) -> std::io::Result<()> {
    let token_path = crate::config::ipc_token_path();
    let token = std::fs::read_to_string(&token_path).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "cannot read TCP IPC token at {}: {}. Start whoisthat-core with tcp-enabled first.",
                token_path, e
            ),
        )
    })?;

    let payload = serde_json::json!({
        "msg": "hello",
        "data": {
            "token": token.trim(),
            "protocol_version": EXPECTED_CORE_PROTOCOL_VERSION,
        },
    });
    let json = serde_json::to_vec(&payload)?;
    let len = json.len() as u32;
    if len == 0 || len as usize > MAX_FRAME_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "hello frame too large",
        ));
    }
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(&json).await?;
    stream.flush().await?;

    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf);
    if len == 0 || len as usize > MAX_FRAME_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid hello response length",
        ));
    }
    let mut buf = vec![0u8; len as usize];
    stream.read_exact(&mut buf).await?;
    let msg: TcpMessage = serde_json::from_slice(&buf)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

    match msg.msg.as_str() {
        "hello-ok" => Ok(()),
        "hello-error" => {
            let reason = msg
                .data
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("TCP IPC authentication failed");
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                reason.to_string(),
            ))
        }
        other => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unexpected handshake response: {other}"),
        )),
    }
}

pub struct CoreConnection {
    stream: Stream,
}

impl CoreConnection {
    /// Connect over the given endpoint.
    pub async fn connect_endpoint(endpoint: &Endpoint) -> std::io::Result<Self> {
        let stream = match endpoint {
            Endpoint::Unix(path) => Stream::Unix(UnixStream::connect(path).await?),
            Endpoint::Tcp { host, port } => {
                let mut s = TcpStream::connect(format!("{}:{}", host, port)).await?;
                s.set_nodelay(true)?;
                tcp_handshake(&mut s).await?;
                Stream::Tcp(s)
            }
        };
        Ok(Self { stream })
    }

    /// Split this connection into separate read and write halves for full-duplex IPC.
    pub fn into_split(self) -> (CoreReadHalf, CoreWriteHalf) {
        match self.stream {
            Stream::Unix(s) => {
                let (rh, wh) = s.into_split();
                (
                    CoreReadHalf {
                        stream: CoreReadStream::Unix(rh),
                    },
                    CoreWriteHalf {
                        stream: CoreWriteStream::Unix(wh),
                    },
                )
            }
            Stream::Tcp(s) => {
                let (rh, wh) = s.into_split();
                (
                    CoreReadHalf {
                        stream: CoreReadStream::Tcp(rh),
                    },
                    CoreWriteHalf {
                        stream: CoreWriteStream::Tcp(wh),
                    },
                )
            }
        }
    }

    /// Connect and split into read and write halves in one step.
    pub async fn connect_split(
        endpoint: &Endpoint,
    ) -> std::io::Result<(CoreReadHalf, CoreWriteHalf)> {
        let conn = Self::connect_endpoint(endpoint).await?;
        Ok(conn.into_split())
    }

    pub async fn send(&mut self, msg: &str, data: &impl Serialize) -> std::io::Result<()> {
        let payload = serde_json::json!({
            "msg": msg,
            "data": data,
        });
        let json = serde_json::to_vec(&payload)?;

        let len = json.len() as u32;
        if len == 0 || len as usize > MAX_FRAME_SIZE {
            error!("Invalid payload length: {}", len);
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid payload length: {}", len),
            ));
        }

        self.stream.write_all(&len.to_be_bytes()).await?;
        self.stream.write_all(&json).await?;
        self.stream.flush().await?;
        Ok(())
    }
}
