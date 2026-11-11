use std::fs;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::config::AppConfig;
use crate::core_client::connection::{CoreConnection, Endpoint};
use crate::core_client::protocol::*;
use crate::doctor::types::{CheckCategory, CheckItem};
use crate::platform::{self, InitServiceState};

pub(super) async fn check_ipc_and_daemon(cfg: &AppConfig) -> CheckCategory {
    let mut items = Vec::new();

    // 1. XDG_RUNTIME_DIR
    match std::env::var("XDG_RUNTIME_DIR") {
        Ok(dir) => {
            let p = Path::new(&dir);
            if p.exists() {
                items.push(CheckItem::pass("XDG_RUNTIME_DIR", dir));
            } else {
                items.push(CheckItem::warn(
                    "XDG_RUNTIME_DIR",
                    format!("directory does not exist: {}", dir),
                    None,
                ));
            }
        }
        Err(_) => {
            items.push(CheckItem::warn(
                "XDG_RUNTIME_DIR",
                "env variable not set, using fallback /tmp/whoisthat-<uid>",
                None,
            ));
        }
    }

    // 2. IPC Socket
    let endpoint = cfg.endpoint();
    match &endpoint {
        Endpoint::Unix(sock_path) => {
            let p = Path::new(sock_path);
            if p.exists() {
                if let Ok(meta) = fs::metadata(p) {
                    let is_sock = meta.file_type().is_socket();
                    let mode = meta.permissions().mode() & 0o777;
                    if is_sock {
                        if mode == 0o600 {
                            items.push(CheckItem::pass(
                                "IPC socket",
                                format!("{} (mode 0600, secure)", sock_path),
                            ));
                        } else {
                            items.push(CheckItem::warn(
                                "IPC socket",
                                format!("{} (mode {:04o}, expected 0600)", sock_path, mode),
                                Some(format!("chmod 0600 {}", sock_path)),
                            ));
                        }
                    } else {
                        items.push(CheckItem::fail(
                            "IPC socket",
                            format!("{} exists but is not a unix domain socket", sock_path),
                            Some(format!("rm -f {}", sock_path)),
                        ));
                    }
                } else {
                    items.push(CheckItem::pass("IPC socket", sock_path.clone()));
                }
            } else {
                items.push(CheckItem::info(
                    "IPC socket",
                    format!("{} (not created — daemon is offline)", sock_path),
                ));
            }
        }
        Endpoint::Tcp { host, port } => {
            items.push(CheckItem::info(
                "IPC transport",
                format!("TCP mode configured on {}:{}", host, port),
            ));
        }
    }

    // 3. Live daemon connection
    let conn_attempt = tokio::time::timeout(
        Duration::from_millis(1500),
        CoreConnection::connect_endpoint(&endpoint),
    )
    .await;

    match conn_attempt {
        Ok(Ok(conn)) => {
            let (mut read_half, mut write_half) = conn.into_split();
            let query_res = async {
                write_half
                    .send("get-application-state", &GetApplicationStateData {})
                    .await?;
                let state = tokio::time::timeout(Duration::from_millis(2000), async {
                    while let Ok(msg) = read_half.recv().await {
                        if msg.msg == "application-state"
                            && let Ok(state) = serde_json::from_value::<ApplicationState>(msg.data)
                        {
                            return Ok(state);
                        }
                    }
                    Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "state message not received",
                    ))
                })
                .await??;
                Ok::<ApplicationState, std::io::Error>(state)
            }
            .await;

            match query_res {
                Ok(state) => {
                    let conn_status = &state.connection_status.connection;
                    let tun_str = if state.tun_status {
                        "TUN active"
                    } else {
                        "Proxy mode"
                    };
                    let prof_str = state
                        .connection_status
                        .profile
                        .as_ref()
                        .map(|p| format!("profile #{} ({})", p.id, p.name))
                        .unwrap_or_else(|| "none".to_string());

                    items.push(CheckItem::pass(
                        "daemon connection",
                        format!(
                            "active & responsive (VPN: {}, {}, profile: {})",
                            conn_status, tun_str, prof_str
                        ),
                    ));
                }
                Err(e) => {
                    items.push(CheckItem::warn(
                        "daemon connection",
                        format!("connected but state query failed: {}", e),
                        None,
                    ));
                }
            }
        }
        _ => {
            items.push(CheckItem::info(
                "daemon connection",
                "core is not running (start with 'whoisthat')",
            ));
        }
    }

    // 4. Autostart backend. Never call systemctl/loginctl on a host without a
    // running systemd: Artix and friends may still ship the binary.
    if platform::has_systemd() {
        let s_active = Command::new("systemctl")
            .arg("--user")
            .arg("is-active")
            .arg("whoisthat-core.service")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        let s_enabled = Command::new("systemctl")
            .arg("--user")
            .arg("is-enabled")
            .arg("whoisthat-core.service")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        items.push(CheckItem::info(
            "systemd user service",
            format!(
                "whoisthat-core.service (status: {}, autostart: {})",
                s_active, s_enabled
            ),
        ));
    } else {
        let uid = crate::config::uid();
        let state = match platform::init_service_state(uid) {
            InitServiceState::Configured => "configured".to_string(),
            InitServiceState::NotConfigured => "not configured".to_string(),
            InitServiceState::Unknown(reason) => format!("unknown ({reason})"),
        };
        let running = platform::init_service_running(uid);
        items.push(CheckItem::info(
            "autostart",
            format!(
                "systemd not detected; desktop autostart only until the init backend is set up \
                 (init: {}, whoisthat init service: {}, {})",
                platform::detect_init().name(),
                state,
                if running { "running" } else { "not running" }
            ),
        ));
    }

    CheckCategory::new("IPC & Daemon", items)
}
