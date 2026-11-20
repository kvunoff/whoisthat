use std::time::Duration;

use log::error;
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UnixStream};

use super::protocol::{EXPECTED_CORE_PROTOCOL_VERSION, TcpMessage};

/// Maximum accepted IPC frame. Kept in sync with
/// core/core/lib/TCPServer/TCPServer.go:MaxFrameSize (64 MiB).
const MAX_FRAME_SIZE: usize = 64 * 1024 * 1024;

/// Maximum accepted TCP hello response. The handshake payload is a tiny
/// `{msg, data}` object; keeping it far below the normal frame cap stops a
/// misbehaving peer from making the client allocate 64 MiB before it has
/// authenticated.
const MAX_HELLO_SIZE: usize = 64 * 1024;

/// Bound the TCP handshake the same way the core bounds its auth read.
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(serde::Deserialize)]
struct HelloFrame {
    msg: String,
    #[serde(default)]
    data: HelloData,
}

#[derive(serde::Deserialize, Default)]
struct HelloData {
    #[serde(default)]
    protocol_version: Option<u32>,
    #[serde(default)]
    error: Option<String>,
}

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

    /// Wait for the first frame the predicate accepts, skipping unrelated
    /// broadcasts (preflight warns, traffic stats, …).
    ///
    /// NOT cancel-safe: a timeout can abort mid-frame and desync the stream, so
    /// the caller must drop the connection after an error. Intended for one-shot
    /// CLI/deeplink connections; the live TUI read loop reads frames itself.
    pub async fn recv_until<F>(
        &mut self,
        timeout: Duration,
        mut matches: F,
    ) -> std::io::Result<TcpMessage>
    where
        F: FnMut(&TcpMessage) -> bool,
    {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            if left.is_zero() {
                return Err(wait_timed_out());
            }
            let msg = tokio::time::timeout(left, self.recv())
                .await
                .map_err(|_| wait_timed_out())??;
            if matches(&msg) {
                return Ok(msg);
            }
            log::debug!("ignoring IPC frame '{}' while waiting", msg.msg);
        }
    }
}

fn wait_timed_out() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "timed out waiting for IPC message",
    )
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
        self.send_frame(msg, data, None).await
    }

    /// Send a request with an optional correlation id. The core echoes it on
    /// notifications produced by this request, which lets CLI commands ignore
    /// unrelated broadcasts (traffic stats, other clients' actions).
    pub async fn send_with_id(
        &mut self,
        id: &str,
        msg: &str,
        data: &impl Serialize,
    ) -> std::io::Result<()> {
        self.send_frame(msg, data, Some(id)).await
    }

    async fn send_frame(
        &mut self,
        msg: &str,
        data: &impl Serialize,
        id: Option<&str>,
    ) -> std::io::Result<()> {
        let mut payload = serde_json::json!({
            "msg": msg,
            "data": data,
        });
        if let Some(id) = id {
            payload["id"] = serde_json::Value::String(id.to_string());
        }
        let json = serde_json::to_vec(&payload)?;
        // Message name only: payloads may contain subscription URIs/tokens.
        log::debug!("ipc -> {msg} (id={})", id.unwrap_or("-"));

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
///
/// The token is a parameter so the loopback test can drive the handshake
/// without touching XDG_RUNTIME_DIR.
async fn tcp_handshake(stream: &mut TcpStream, token: &str) -> std::io::Result<()> {
    tcp_handshake_with_timeout(stream, token, HELLO_TIMEOUT).await
}

async fn tcp_handshake_with_timeout(
    stream: &mut TcpStream,
    token: &str,
    timeout: Duration,
) -> std::io::Result<()> {
    tokio::time::timeout(timeout, tcp_handshake_io(stream, token))
        .await
        .map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::TimedOut, "TCP IPC handshake timed out")
        })?
}

async fn tcp_handshake_io(stream: &mut TcpStream, token: &str) -> std::io::Result<()> {
    let payload = serde_json::json!({
        "msg": "hello",
        "data": {
            "token": token,
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
    let len = hello_len(u32::from_be_bytes(len_buf))?;
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    classify_hello_response(&buf)
}

/// Read the TCP IPC token from the user's runtime directory.
fn read_ipc_token() -> std::io::Result<String> {
    let token_path = crate::config::ipc_token_path();
    std::fs::read_to_string(&token_path).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "cannot read TCP IPC token at {}: {}. Start whoisthat-core with tcp-enabled first.",
                token_path, e
            ),
        )
    })
}

/// Bound the hello frame before allocating its body.
fn hello_len(len: u32) -> std::io::Result<usize> {
    if len == 0 || len as usize > MAX_HELLO_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid hello response length: {len}"),
        ));
    }
    Ok(len as usize)
}

/// Strip control characters and cap remote-controlled text before it reaches a
/// CLI/TUI error message.
fn sanitize_reason(reason: String) -> String {
    let cleaned: String = reason
        .chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect();
    if cleaned.is_empty() {
        "TCP IPC authentication failed".to_string()
    } else {
        cleaned
    }
}

/// Parse and validate the TCP hello response. A size cap only bounds the
/// allocation; this keeps the payload typed and checks the handshake contract.
fn classify_hello_response(buf: &[u8]) -> std::io::Result<()> {
    let frame: HelloFrame = serde_json::from_slice(buf)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

    match frame.msg.as_str() {
        "hello-ok" => match frame.data.protocol_version {
            Some(v) if v == EXPECTED_CORE_PROTOCOL_VERSION => Ok(()),
            Some(v) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unexpected core protocol version: {v}"),
            )),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "hello response is missing protocol_version",
            )),
        },
        "hello-error" => {
            let reason = frame
                .data
                .error
                .unwrap_or_else(|| "TCP IPC authentication failed".to_string());
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                sanitize_reason(reason),
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
                let token = read_ipc_token()?;
                tcp_handshake(&mut s, token.trim()).await?;
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

    /// Build a connection from an already-connected Unix stream. Test-only:
    /// lets CLI tests exercise `Session` without a real daemon.
    #[cfg(test)]
    pub fn from_unix(stream: UnixStream) -> Self {
        Self {
            stream: Stream::Unix(stream),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn recv_until_skips_unrelated_frames() {
        let (client, server) = UnixStream::pair().unwrap();
        let (mut read, _) = CoreConnection {
            stream: Stream::Unix(client),
        }
        .into_split();
        let (_, mut write) = CoreConnection {
            stream: Stream::Unix(server),
        }
        .into_split();

        write
            .send("warn", &serde_json::json!({"key": "x", "content": "y"}))
            .await
            .unwrap();
        write
            .send("traffic-stats", &serde_json::json!({}))
            .await
            .unwrap();
        write
            .send(
                "application-state",
                &serde_json::json!({
                    "groups": [],
                    "connection-status": {"connection": "disconnected"}
                }),
            )
            .await
            .unwrap();

        let msg = read
            .recv_until(Duration::from_secs(1), |m| m.msg == "application-state")
            .await
            .unwrap();
        assert_eq!(msg.msg, "application-state");
    }

    #[tokio::test]
    async fn recv_until_times_out_without_match() {
        let (client, _server) = UnixStream::pair().unwrap();
        let (mut read, _) = CoreConnection {
            stream: Stream::Unix(client),
        }
        .into_split();

        let err = read
            .recv_until(Duration::from_millis(20), |m| m.msg == "application-state")
            .await
            .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
    }
}

#[cfg(test)]
mod hello_response_tests {
    use super::*;

    fn classify(json: serde_json::Value) -> std::io::Result<()> {
        classify_hello_response(&serde_json::to_vec(&json).unwrap())
    }

    fn ok(version: u32) -> serde_json::Value {
        serde_json::json!({"msg": "hello-ok", "data": {"protocol_version": version}})
    }

    #[test]
    fn hello_len_enforces_the_handshake_cap() {
        assert!(hello_len(0).is_err());
        assert!(hello_len(MAX_HELLO_SIZE as u32 + 1).is_err());
        assert!(hello_len(u32::MAX).is_err());
        assert_eq!(hello_len(MAX_HELLO_SIZE as u32).unwrap(), MAX_HELLO_SIZE);
        assert_eq!(hello_len(128).unwrap(), 128);
    }

    #[test]
    fn accepts_matching_hello_ok() {
        classify(ok(EXPECTED_CORE_PROTOCOL_VERSION)).unwrap();
    }

    #[test]
    fn rejects_mismatched_or_missing_version() {
        let mismatched = classify(ok(EXPECTED_CORE_PROTOCOL_VERSION + 1)).unwrap_err();
        assert_eq!(mismatched.kind(), std::io::ErrorKind::InvalidData);

        for value in [
            serde_json::json!({"msg": "hello-ok"}),
            serde_json::json!({"msg": "hello-ok", "data": null}),
            serde_json::json!({"msg": "hello-ok", "data": {}}),
            serde_json::json!({"msg": "hello-ok", "data": {"protocol_version": "5"}}),
            serde_json::json!({"msg": "hello-ok", "data": {"protocol_version": -1}}),
            serde_json::json!({"msg": "hello-ok", "data": {"protocol_version": 5.0}}),
        ] {
            let err = classify(value.clone()).unwrap_err();
            assert_eq!(err.kind(), std::io::ErrorKind::InvalidData, "case {value}");
        }
    }

    #[test]
    fn hello_error_uses_the_server_reason() {
        let err = classify(serde_json::json!({
            "msg": "hello-error",
            "data": {"protocol_version": 0, "error": "invalid authentication token"}
        }))
        .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(err.to_string(), "invalid authentication token");
    }

    #[test]
    fn hello_error_without_reason_uses_the_fallback() {
        let err = classify(serde_json::json!({
            "msg": "hello-error",
            "data": {"protocol_version": 0}
        }))
        .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(err.to_string(), "TCP IPC authentication failed");
    }

    #[test]
    fn hello_error_with_a_non_string_reason_is_invalid() {
        let err = classify(serde_json::json!({
            "msg": "hello-error",
            "data": {"protocol_version": 0, "error": 123}
        }))
        .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn unknown_message_and_malformed_payloads_are_rejected() {
        for value in [
            serde_json::json!({"msg": "hello-other", "data": {}}),
            serde_json::json!({"data": {"protocol_version": 5}}),
        ] {
            let err = classify(value).unwrap_err();
            assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        }

        assert!(classify_hello_response(b"not json").is_err());
        assert!(classify_hello_response(&[0xff, 0xfe]).is_err());

        let mut trailing = serde_json::to_vec(&ok(EXPECTED_CORE_PROTOCOL_VERSION)).unwrap();
        trailing.extend_from_slice(b"trailing");
        assert!(classify_hello_response(&trailing).is_err());
    }

    #[test]
    fn hello_error_strips_control_characters_and_truncates() {
        let err = classify(serde_json::json!({
            "msg": "hello-error",
            "data": {"protocol_version": 0, "error": "\u{1b}[31mboom\n"}
        }))
        .unwrap_err();
        assert_eq!(err.to_string(), "[31mboom");

        let long = "x".repeat(300);
        let err = classify(serde_json::json!({
            "msg": "hello-error",
            "data": {"protocol_version": 0, "error": long}
        }))
        .unwrap_err();
        assert_eq!(err.to_string().len(), 200);

        let err = classify(serde_json::json!({
            "msg": "hello-error",
            "data": {"protocol_version": 0, "error": "\u{1b}\n"}
        }))
        .unwrap_err();
        assert_eq!(err.to_string(), "TCP IPC authentication failed");
    }

    #[test]
    fn unknown_fields_are_ignored_for_forward_compatibility() {
        classify(serde_json::json!({
            "msg": "hello-ok",
            "id": "ignored",
            "data": {
                "protocol_version": EXPECTED_CORE_PROTOCOL_VERSION,
                "future_field": true
            }
        }))
        .unwrap();
    }
}
