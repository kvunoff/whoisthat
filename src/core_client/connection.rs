use log::{error, info};
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use super::protocol::TcpMessage;

pub struct CoreConnection {
    stream: TcpStream,
}

impl CoreConnection {
    pub async fn connect(host: &str, port: u16) -> std::io::Result<Self> {
        let addr = format!("{}:{}", host, port);
        let stream = TcpStream::connect(&addr).await?;
        stream.set_nodelay(true)?;
        info!("Connected to core at {}", addr);
        Ok(Self { stream })
    }

    pub async fn send(&mut self, msg: &str, data: &impl Serialize) -> std::io::Result<()> {
        let payload = serde_json::json!({
            "msg": msg,
            "data": data,
        });
        let json = serde_json::to_vec(&payload)?;

        let len = json.len() as u32;
        if len == 0 || len > 100 * 1024 * 1024 {
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
        if len == 0 || len > 100 * 1024 * 1024 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid message length: {}", len),
            ));
        }

        let mut payload = vec![0u8; len as usize];
        self.stream.read_exact(&mut payload).await?;

        let msg: TcpMessage = serde_json::from_slice(&payload).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
        })?;

        info!("[core] {}", msg.msg);
        Ok(msg)
    }
}
