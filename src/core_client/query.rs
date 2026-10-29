use std::time::Duration;

use crate::config;

use super::connection::{CoreConnection, Endpoint};
use super::protocol::{ApplicationState, GetApplicationStateData};

/// Query a running core over IPC for its version. `None` when offline.
///
/// The core binary is deliberately never executed for this: pre-split binaries
/// ignore `--version` and boot a daemon instead.
pub async fn fetch_core_version_ipc() -> Option<(String, u32)> {
    let cfg = config::load_config();
    fetch_core_version_at(&cfg.endpoint()).await
}

pub async fn fetch_core_version_at(endpoint: &Endpoint) -> Option<(String, u32)> {
    let conn = CoreConnection::connect_endpoint(endpoint).await.ok()?;
    let (mut read_half, mut write_half) = conn.into_split();
    write_half
        .send("get-application-state", &GetApplicationStateData {})
        .await
        .ok()?;
    // Preflight warnings may arrive before the state reply; wait for the right frame.
    let msg = read_half
        .recv_until(Duration::from_secs(2), |m| m.msg == "application-state")
        .await
        .ok()?;
    let state: ApplicationState = serde_json::from_value(msg.data).ok()?;
    if state.core_version.is_empty() {
        return None;
    }
    Some((state.core_version, state.protocol_version))
}
