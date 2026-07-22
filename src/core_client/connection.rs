use log::error;
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UnixStream};

use super::protocol::TcpMessage;

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

    async fn read_exact(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        match self {
            Stream::Unix(s) => s.read_exact(buf).await.map(|_| ()),
            Stream::Tcp(s) => s.read_exact(buf).await.map(|_| ()),
        }
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
                let s = TcpStream::connect(format!("{}:{}", host, port)).await?;
                s.set_nodelay(true)?;
                Stream::Tcp(s)
            }
        };
        Ok(Self { stream })
    }

    /// Legacy TCP-only connect, retained for call sites that only speak TCP.
    pub async fn connect(host: &str, port: u16) -> std::io::Result<Self> {
        Self::connect_endpoint(&Endpoint::Tcp {
            host: host.to_string(),
            port,
        })
        .await
    }

    pub async fn send(&mut self, msg: &str, data: &impl Serialize) -> std::io::Result<()> {
        let payload = serde_json::json!({
            "msg": msg,
            "data": data,
        });
        let json = serde_json::to_vec(&payload)?;

        let len = json.len() as u32;
        if len == 0 || len > 10 * 1024 * 1024 {
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

    pub async fn recv(&mut self) -> std::io::Result<TcpMessage> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;

        let len = u32::from_be_bytes(len_buf);
        if len == 0 || len > 10 * 1024 * 1024 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid message length: {}", len),
            ));
        }

        let mut payload = vec![0u8; len as usize];
        self.stream.read_exact(&mut payload).await?;

        let msg: TcpMessage = serde_json::from_slice(&payload)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

        Ok(msg)
    }
}
