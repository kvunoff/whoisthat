use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::config::{self, AppConfig};
use crate::core_client::connection::{CoreConnection, CoreReadHalf, CoreWriteHalf, Endpoint};
use crate::core_client::protocol::*;

use super::args::GlobalArgs;
use super::error::CliError;
use super::output::Out;

pub enum Reply<'a> {
    /// Wait for a frame carrying our request-id and one of these message types.
    Id {
        expect: &'a [&'a str],
        fatal_warns: &'a [&'a str],
    },
    /// Wait for a broadcast frame matching the predicate; id-warns are still
    /// checked so `connect`/`enable-tun` can fail fast.
    Broadcast {
        pred: Box<dyn Fn(&TcpMessage) -> bool + Send + 'a>,
        fatal_warns: &'a [&'a str],
    },
}

pub struct Session {
    read: CoreReadHalf,
    write: CoreWriteHalf,
    pub cfg: AppConfig,
    pub timeout: Duration,
    warnings: Vec<Warning>,
}

impl Session {
    pub async fn open(g: &GlobalArgs) -> Result<Self, CliError> {
        let cfg = config::load_config();
        let endpoint = endpoint(g, &cfg)?;
        let conn = CoreConnection::connect_endpoint(&endpoint)
            .await
            .map_err(|e| {
                CliError::Offline(format!(
                    "cannot reach whoisthat-core at {}: {e}",
                    endpoint.describe()
                ))
            })?;
        Ok(Self::from_conn(conn, cfg, g))
    }

    /// Connect, optionally spawning a local core. Spawning is only attempted
    /// for the default local endpoint: `--tcp`/`--socket` point elsewhere.
    pub async fn open_or_spawn(g: &GlobalArgs) -> Result<Self, CliError> {
        match Self::open(g).await {
            Ok(session) => Ok(session),
            Err(err) if !spawn_allowed(g) => Err(err),
            Err(_) => {
                let cfg = config::load_config();
                crate::core_spawn::spawn_core(&cfg.log_level).map_err(|e| {
                    CliError::Offline(format!("failed to start whoisthat-core: {e}"))
                })?;
                for _ in 0..300 {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    if let Ok(session) = Self::open(g).await {
                        return Ok(session);
                    }
                }
                Err(CliError::Offline(
                    "whoisthat-core did not become reachable within 30s".into(),
                ))
            }
        }
    }

    pub fn from_conn(conn: CoreConnection, cfg: AppConfig, g: &GlobalArgs) -> Self {
        let (read, write) = conn.into_split();
        Self {
            read,
            write,
            cfg,
            timeout: Duration::from_secs(g.timeout.unwrap_or(10)),
            warnings: Vec::new(),
        }
    }

    #[cfg(test)]
    pub fn from_connection(read: CoreReadHalf, write: CoreWriteHalf) -> Self {
        Self {
            read,
            write,
            cfg: AppConfig::default(),
            timeout: Duration::from_secs(1),
            warnings: Vec::new(),
        }
    }

    pub fn set_timeout(&mut self, secs: u64) {
        self.timeout = Duration::from_secs(secs);
    }

    pub async fn send(&mut self, msg: &str, data: &impl Serialize) -> Result<String, CliError> {
        let id = next_id();
        self.write.send_with_id(&id, msg, data).await?;
        Ok(id)
    }

    pub async fn request<T: DeserializeOwned>(
        &mut self,
        msg: &str,
        data: &impl Serialize,
        reply: Reply<'_>,
    ) -> Result<T, CliError> {
        let id = self.send(msg, data).await?;
        self.wait(&id, reply).await
    }

    pub async fn state(&mut self) -> Result<ApplicationState, CliError> {
        self.request(
            "get-application-state",
            &GetApplicationStateData {},
            Reply::Id {
                expect: &["application-state"],
                fatal_warns: &[],
            },
        )
        .await
    }

    pub async fn wait<T: DeserializeOwned>(
        &mut self,
        id: &str,
        reply: Reply<'_>,
    ) -> Result<T, CliError> {
        loop {
            let msg = tokio::time::timeout(self.timeout, self.read.recv())
                .await
                .map_err(|_| CliError::Timeout(format!("waiting for reply to '{id}'")))??;

            if msg.msg == "warn" {
                if msg.id.as_deref() == Some(id) {
                    if let Ok(w) = serde_json::from_value::<Warning>(msg.data.clone()) {
                        let fatal = match &reply {
                            Reply::Id { fatal_warns, .. } => fatal_warns.contains(&w.key.as_str()),
                            Reply::Broadcast { fatal_warns, .. } => {
                                fatal_warns.contains(&w.key.as_str())
                            }
                        };
                        if fatal {
                            return Err(CliError::Core(w.content));
                        }
                        self.warnings.push(w);
                    }
                }
                continue;
            }

            let matched = match &reply {
                Reply::Id { expect, .. } => {
                    msg.id.as_deref() == Some(id) && expect.contains(&msg.msg.as_str())
                }
                Reply::Broadcast { pred, .. } => pred(&msg),
            };
            if matched {
                return serde_json::from_value(msg.data).map_err(CliError::from);
            }
        }
    }

    pub fn take_warnings(&mut self) -> Vec<Warning> {
        std::mem::take(&mut self.warnings)
    }

    /// Print collected non-fatal warnings as notes.
    pub fn flush_warnings(&mut self, out: &Out) {
        for w in self.take_warnings() {
            if w.content.is_empty() {
                continue;
            }
            if w.key.is_empty() {
                out.note(&w.content);
            } else {
                out.note(format!("note: {}", w.content));
            }
        }
    }
}

fn spawn_allowed(g: &GlobalArgs) -> bool {
    !g.no_spawn && g.tcp.is_none() && g.socket.is_none()
}

pub fn endpoint(g: &GlobalArgs, cfg: &AppConfig) -> Result<Endpoint, CliError> {
    if let Some(tcp) = &g.tcp {
        return parse_tcp(tcp);
    }
    if let Some(path) = &g.socket {
        return Ok(Endpoint::Unix(path.to_string_lossy().into_owned()));
    }
    Ok(cfg.endpoint())
}

fn parse_tcp(value: &str) -> Result<Endpoint, CliError> {
    let value = value.trim();
    let (host, port) = if let Some(rest) = value.strip_prefix('[') {
        let (host, port) = rest
            .split_once("]:")
            .ok_or_else(|| CliError::Usage(format!("invalid TCP endpoint: {value}")))?;
        (host, port)
    } else {
        value
            .rsplit_once(':')
            .ok_or_else(|| CliError::Usage(format!("invalid TCP endpoint: {value}")))?
    };
    let port: u16 = port
        .parse()
        .map_err(|_| CliError::Usage(format!("invalid TCP port in: {value}")))?;
    Ok(Endpoint::Tcp {
        host: host.to_string(),
        port,
    })
}

fn next_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}-{}-{}", std::process::id(), nanos, n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_client::connection::CoreConnection;
    use crate::core_client::protocol::TcpMessage;
    use serde_json::json;
    use tokio::net::UnixStream;

    fn pair() -> (Session, CoreWriteHalf) {
        let (client, server) = UnixStream::pair().unwrap();
        let (read, write) = CoreConnection::from_unix(client).into_split();
        let (_, peer) = CoreConnection::from_unix(server).into_split();
        (Session::from_connection(read, write), peer)
    }

    #[tokio::test]
    async fn ignores_unrelated_frames_and_other_ids() {
        let (mut session, mut peer) = pair();
        let id = session.send("ping", &json!({})).await.unwrap();
        peer.send("status-changed", &json!({})).await.unwrap();
        peer.send_with_id("other", "ok", &json!(0)).await.unwrap();
        peer.send_with_id(&id, "ok", &json!(1)).await.unwrap();

        let value: serde_json::Value = session
            .wait(
                &id,
                Reply::Id {
                    expect: &["ok"],
                    fatal_warns: &[],
                },
            )
            .await
            .unwrap();
        assert_eq!(value, json!(1));
    }

    #[tokio::test]
    async fn fatal_warn_for_our_id_aborts_the_wait() {
        let (mut session, mut peer) = pair();
        let id = session.send("connect", &json!({})).await.unwrap();
        peer.send_with_id(
            &id,
            "warn",
            &json!({"key": "connect-failed", "content": "nope"}),
        )
        .await
        .unwrap();

        let err = session
            .wait::<serde_json::Value>(
                &id,
                Reply::Broadcast {
                    pred: Box::new(|_: &TcpMessage| false),
                    fatal_warns: &["connect-failed"],
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, CliError::Core(msg) if msg == "nope"));
    }

    #[tokio::test]
    async fn non_fatal_warn_is_collected() {
        let (mut session, mut peer) = pair();
        let id = session.send("connect", &json!({})).await.unwrap();
        peer.send_with_id(
            &id,
            "warn",
            &json!({"key": "set-kill-switch-failed", "content": "not armed"}),
        )
        .await
        .unwrap();
        peer.send_with_id(&id, "ok", &json!(true)).await.unwrap();

        let _: serde_json::Value = session
            .wait(
                &id,
                Reply::Id {
                    expect: &["ok"],
                    fatal_warns: &["connect-failed"],
                },
            )
            .await
            .unwrap();
        let warnings = session.take_warnings();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].key, "set-kill-switch-failed");
    }

    #[test]
    fn parses_tcp_endpoints() {
        match parse_tcp("127.0.0.1:4897").unwrap() {
            Endpoint::Tcp { host, port } => {
                assert_eq!(host, "127.0.0.1");
                assert_eq!(port, 4897);
            }
            _ => panic!("expected TCP"),
        }
        match parse_tcp("[::1]:4897").unwrap() {
            Endpoint::Tcp { host, port } => {
                assert_eq!(host, "::1");
                assert_eq!(port, 4897);
            }
            _ => panic!("expected TCP"),
        }
        assert!(parse_tcp("nope").is_err());
    }
}
