use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config;
use crate::core_client::connection::CoreConnection;
use crate::core_client::protocol::*;

use super::super::args::GlobalArgs;
use super::super::error::CliError;

/// Stable JSON contract for `status --json` (and the NDJSON watch stream).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CliStatus {
    pub core_running: bool,
    pub connected: bool,
    pub status: String,
    pub mode: String,
    pub tun_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xray_version: Option<String>,
    pub rx_speed_bytes: i64,
    pub tx_speed_bytes: i64,
    pub rx_speed_human: String,
    pub tx_speed_human: String,
}

impl CliStatus {
    pub fn offline() -> Self {
        Self {
            core_running: false,
            connected: false,
            status: "offline".to_string(),
            mode: String::new(),
            tun_enabled: false,
            group_id: None,
            group_name: None,
            profile_id: None,
            profile_name: None,
            protocol: None,
            xray_version: None,
            rx_speed_bytes: 0,
            tx_speed_bytes: 0,
            rx_speed_human: "0 B/s".to_string(),
            tx_speed_human: "0 B/s".to_string(),
        }
    }

    pub fn from_app_state(state: &ApplicationState) -> Self {
        let is_conn = state.connection_status.connection == "connected";
        let tun = state.tun_status;
        let mode = if is_conn {
            if tun {
                "tun".to_string()
            } else {
                "proxy".to_string()
            }
        } else {
            String::new()
        };

        let mut group_id = None;
        let mut group_name = None;
        let mut profile_id = None;
        let mut profile_name = None;
        let mut protocol = None;

        if let Some(ref prof) = state.connection_status.profile {
            profile_id = Some(prof.id);
            group_id = Some(prof.group_id);
            profile_name = if prof.name.is_empty() {
                None
            } else {
                Some(prof.name.clone())
            };
            protocol = if prof.protocol.is_empty() {
                None
            } else {
                Some(prof.protocol.clone())
            };

            for g in &state.groups {
                if g.group.id == prof.group_id {
                    group_name = Some(g.group.name.clone());
                    break;
                }
            }
        }

        let xray_version = state.xray_status.as_ref().and_then(|x| {
            if !x.version.is_empty() {
                Some(x.version.clone())
            } else if !x.target_version.is_empty() {
                Some(x.target_version.clone())
            } else {
                None
            }
        });

        Self {
            core_running: true,
            connected: is_conn,
            status: state.connection_status.connection.clone(),
            mode,
            tun_enabled: tun,
            group_id,
            group_name,
            profile_id,
            profile_name,
            protocol,
            xray_version,
            rx_speed_bytes: 0,
            tx_speed_bytes: 0,
            rx_speed_human: "0 B/s".to_string(),
            tx_speed_human: "0 B/s".to_string(),
        }
    }

    pub fn update_traffic(&mut self, stats: &TrafficStats) {
        let rx = stats.proxy_down + stats.direct_down;
        let tx = stats.proxy_up + stats.direct_up;
        self.rx_speed_bytes = rx;
        self.tx_speed_bytes = tx;
        self.rx_speed_human = format_speed_bytes(rx);
        self.tx_speed_human = format_speed_bytes(tx);
    }

    pub fn update_tun_status(&mut self, enabled: bool) {
        self.tun_enabled = enabled;
        if self.connected {
            self.mode = if enabled { "tun" } else { "proxy" }.to_string();
        }
    }

    pub fn format_short(&self) -> String {
        if !self.core_running {
            "[Offline]".to_string()
        } else if !self.connected {
            "[Disconnected]".to_string()
        } else {
            let mode_tag = if self.tun_enabled { "TUN" } else { "Proxy" };
            format!(
                "[Connected] [{}] ↓ {} ↑ {}",
                mode_tag, self.rx_speed_human, self.tx_speed_human
            )
        }
    }

    pub fn format_pretty(&self) -> String {
        if !self.core_running {
            "WhoisThat: Core is offline".to_string()
        } else if !self.connected {
            "WhoisThat: Disconnected".to_string()
        } else {
            let mode_tag = if self.tun_enabled { "TUN" } else { "Proxy" };
            let name = self.profile_name.as_deref().unwrap_or("connected");
            format!(
                "WhoisThat: [Connected] [{}] ({}) ↓ {} ↑ {}",
                mode_tag, name, self.rx_speed_human, self.tx_speed_human
            )
        }
    }
}

/// Kept byte-for-byte for the `status --json` contract used by Waybar/Polybar.
pub fn format_speed_bytes(bytes_per_sec: i64) -> String {
    let bytes = bytes_per_sec.max(0) as f64;
    if bytes < 1024.0 {
        format!("{bytes:.0} B/s")
    } else if bytes < 1024.0 * 1024.0 {
        format!("{:.1} KB/s", bytes / 1024.0)
    } else if bytes < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB/s", bytes / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB/s", bytes / (1024.0 * 1024.0 * 1024.0))
    }
}

pub async fn run(g: &GlobalArgs, short: bool, watch: bool) -> Result<(), CliError> {
    let cfg = config::load_config();
    let endpoint = super::super::client::endpoint(g, &cfg)?;

    let conn = match CoreConnection::connect_endpoint(&endpoint).await {
        Ok(conn) => conn,
        Err(_) => {
            let status = CliStatus::offline();
            print_status(&status, short, g.json)?;
            return Ok(());
        }
    };

    let (mut read_half, mut write_half) = conn.into_split();
    write_half
        .send("get-application-state", &GetApplicationStateData {})
        .await?;

    let state_timeout = Duration::from_secs(g.timeout.unwrap_or(3));
    let state: ApplicationState = tokio::time::timeout(state_timeout, async {
        while let Ok(msg) = read_half.recv().await {
            if msg.msg == "application-state" {
                if let Ok(state) = serde_json::from_value::<ApplicationState>(msg.data) {
                    return Ok(state);
                }
            }
        }
        Err(CliError::Core("failed to receive application state".into()))
    })
    .await
    .map_err(|_| CliError::Timeout("waiting for application-state".into()))??;

    let mut current = CliStatus::from_app_state(&state);

    if current.connected && !watch {
        // Give the 1 Hz stats collector a chance to produce a first sample.
        let _ = tokio::time::timeout(Duration::from_millis(1100), async {
            while let Ok(msg) = read_half.recv().await {
                if msg.msg == "traffic-stats" {
                    if let Ok(stats) = serde_json::from_value::<TrafficStats>(msg.data) {
                        current.update_traffic(&stats);
                        break;
                    }
                }
            }
        })
        .await;
    }

    print_status(&current, short, g.json)?;

    if !watch {
        return Ok(());
    }

    while let Ok(msg) = read_half.recv().await {
        match msg.msg.as_str() {
            "traffic-stats" => {
                if let Ok(stats) = serde_json::from_value::<TrafficStats>(msg.data) {
                    current.update_traffic(&stats);
                    print_status(&current, short, g.json)?;
                }
            }
            "status-changed" => {
                if let Ok(st) = serde_json::from_value::<ProxyStatus>(msg.data) {
                    current.connected = st.connection == "connected";
                    current.status = st.connection.clone();
                    if let Some(prof) = st.profile {
                        current.profile_id = Some(prof.id);
                        current.group_id = Some(prof.group_id);
                        current.profile_name = if prof.name.is_empty() {
                            None
                        } else {
                            Some(prof.name)
                        };
                        current.protocol = if prof.protocol.is_empty() {
                            None
                        } else {
                            Some(prof.protocol)
                        };
                    } else if !current.connected {
                        current.profile_id = None;
                        current.group_id = None;
                        current.profile_name = None;
                        current.protocol = None;
                        current.rx_speed_bytes = 0;
                        current.tx_speed_bytes = 0;
                        current.rx_speed_human = "0 B/s".into();
                        current.tx_speed_human = "0 B/s".into();
                    }
                    print_status(&current, short, g.json)?;
                }
            }
            "tun-status-changed" => {
                if let Ok(enabled) = serde_json::from_value::<bool>(msg.data) {
                    current.update_tun_status(enabled);
                    print_status(&current, short, g.json)?;
                }
            }
            "application-state" => {
                if let Ok(state) = serde_json::from_value::<ApplicationState>(msg.data) {
                    let rx = current.rx_speed_bytes;
                    let tx = current.tx_speed_bytes;
                    current = CliStatus::from_app_state(&state);
                    current.rx_speed_bytes = rx;
                    current.tx_speed_bytes = tx;
                    current.rx_speed_human = format_speed_bytes(rx);
                    current.tx_speed_human = format_speed_bytes(tx);
                    print_status(&current, short, g.json)?;
                }
            }
            _ => {}
        }
    }

    Ok(())
}

fn print_status(status: &CliStatus, short: bool, json: bool) -> Result<(), CliError> {
    if json {
        println!("{}", serde_json::to_string(status)?);
    } else if short {
        println!("{}", status.format_short());
    } else {
        println!("{}", status.format_pretty());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_speed_matches_legacy_contract() {
        assert_eq!(format_speed_bytes(0), "0 B/s");
        assert_eq!(format_speed_bytes(512), "512 B/s");
        assert_eq!(format_speed_bytes(1024), "1.0 KB/s");
        assert_eq!(format_speed_bytes(1536), "1.5 KB/s");
        assert_eq!(format_speed_bytes(1048576), "1.0 MB/s");
        assert_eq!(format_speed_bytes(1048576 * 5 / 2), "2.5 MB/s");
        assert_eq!(format_speed_bytes(1073741824 * 3), "3.00 GB/s");
    }

    #[test]
    fn status_formatting_is_stable() {
        let mut s = CliStatus::offline();
        assert_eq!(s.format_short(), "[Offline]");
        s.core_running = true;
        assert_eq!(s.format_short(), "[Disconnected]");
        s.connected = true;
        s.tun_enabled = true;
        s.rx_speed_human = "1.2 MB/s".into();
        s.tx_speed_human = "340.0 KB/s".into();
        assert_eq!(
            s.format_short(),
            "[Connected] [TUN] ↓ 1.2 MB/s ↑ 340.0 KB/s"
        );
    }

    #[test]
    fn status_json_keeps_optional_fields_out_when_empty() {
        let s = CliStatus::offline();
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("profile_name"));
        assert!(json.contains("\"status\":\"offline\""));
    }
}
