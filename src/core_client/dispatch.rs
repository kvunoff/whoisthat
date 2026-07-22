use log::warn;
use std::time::Duration;
use tokio::sync::mpsc;

use super::connection::{CoreConnection, Endpoint};
use super::protocol::*;
use super::CoreClient;

#[derive(Debug, Clone)]
pub enum CoreEvent {
    ApplicationState(ApplicationState),
    StatusChanged(ProxyStatus),
    ProfilesAdded(Vec<Profile>),
    ProfilesDeleted(Vec<ProfileID>),
    ProfileUpdated(Profile),
    GroupAdded(Group),
    GroupDeleted(i32),
    GroupUpdated(Group),
    SubscriptionUpdated {
        group: Group,
        profiles: Vec<Profile>,
    },
    TunStatusChanged(bool),
    IsRootAnswer(bool),
    Warning {
        key: String,
        content: String,
    },
    Error(String),
    TrafficStats(TrafficStats),
    RoutingUpdated(RoutingConfig),
    HwidUpdated(HwidData),
    TunNameUpdated(String),
    KillSwitchUpdated(bool),
    SplitTunnelUpdated(String),
    AutoconnectUpdated(AutoconnectInfo),
    TestProgress(TestProgress),
    TestConfigUpdated(TestConfig),
    Reconnected,
    Disconnected,
}

macro_rules! try_dispatch {
    ($msg:expr, $name:expr, $ty:ty, |$d:ident| $map:expr) => {
        serde_json::from_value::<$ty>(($msg).data)
            .map(|$d| $map)
            .unwrap_or_else(|e| {
                warn!(concat!("Invalid ", $name, ": {}"), e);
                CoreEvent::Error(concat!("Invalid ", $name).into())
            })
    };
    ($msg:expr, $name:expr, $ty:ty, $var:ident) => {
        try_dispatch!($msg, $name, $ty, |d| CoreEvent::$var(d))
    };
}

/// Spawn a supervised read loop with automatic reconnect/respawn on disconnect.
///
/// On read error this attempts to recover by:
/// 1. Trying to reconnect to the still-running core (5 attempts × 500 ms).
/// 2. If that fails, spawning a fresh core and waiting up to 30 s for it to listen.
/// 3. On success: replaces both the read and the command connections inline
///    and emits `CoreEvent::Reconnected` so the UI can re-sync state.
/// 4. On failure: emits `CoreEvent::Disconnected` and terminates (terminal).
pub fn spawn_read_loop(
    mut conn: CoreConnection,
    client: CoreClient,
    endpoint: Endpoint,
    log_level: String,
) -> mpsc::UnboundedReceiver<CoreEvent> {
    let (tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        loop {
            // Inner read loop — exit on read error.
            loop {
                match conn.recv().await {
                    Ok(msg) => {
                        let event = dispatch(msg);
                        if tx.send(event).is_err() {
                            return;
                        }
                    }
                    Err(e) => {
                        warn!("Read error: {}", e);
                        break;
                    }
                }
            }

            // Recovery phase.
            let _ = tx.send(CoreEvent::Warning {
                key: "core-disconnect".into(),
                content: "Core disconnected. Reconnecting...".into(),
            });
            log::warn!("Read loop disconnected; attempting recovery");

            let mut recovered = false;

            // (1) Try reconnecting to a still-alive core.
            for attempt in 1..=5 {
                tokio::time::sleep(Duration::from_millis(500)).await;
                let Ok(read_conn) = CoreConnection::connect_endpoint(&endpoint).await else {
                    log::info!("reconnect attempt {attempt}/5 failed (core connect)");
                    continue;
                };
                let Ok(cmd_conn) = CoreConnection::connect_endpoint(&endpoint).await else {
                    log::warn!("reconnect attempt {attempt}/5: read conn OK, cmd conn failed");
                    continue;
                };
                client.replace_conn(cmd_conn).await;
                conn = read_conn;
                recovered = true;
                break;
            }

            // (2) Respawn core from scratch if reconnect failed.
            if !recovered {
                log::info!("Reconnect failed; spawning fresh core");
                if let Err(e) = spawn_core(&log_level) {
                    log::error!("Failed to respawn core: {e}");
                } else {
                    for _ in 0..30 {
                        if CoreConnection::connect_endpoint(&endpoint).await.is_ok() {
                            recovered = true;
                            break;
                        }
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                    if recovered {
                        match (
                            CoreConnection::connect_endpoint(&endpoint).await,
                            CoreConnection::connect_endpoint(&endpoint).await,
                        ) {
                            (Ok(read_conn), Ok(cmd_conn)) => {
                                client.replace_conn(cmd_conn).await;
                                conn = read_conn;
                            }
                            _ => recovered = false,
                        }
                    }
                }
            }

            if recovered {
                log::info!("Recovery succeeded; resuming read loop");
                if tx.send(CoreEvent::Reconnected).is_err() {
                    return;
                }
                continue;
            }

            log::error!("Recovery failed; giving up");
            let _ = tx.send(CoreEvent::Disconnected);
            return;
        }
    });

    rx
}

fn spawn_core(log_level: &str) -> std::io::Result<()> {
    let bin = find_core_binary();
    let mut cmd = std::process::Command::new(&bin);
    cmd.env("WHOISTHAT_LOG_LEVEL", log_level)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // Run in a separate process group so we don't get killed if the TUI dies.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn()?;
    log::info!("Spawned whoisthat-core from {}", bin);
    Ok(())
}

fn find_core_binary() -> String {
    let candidates = [
        "whoisthat-core",
        "./whoisthat-core",
        "./core/core/whoisthat-core",
        "./bin/whoisthat-core",
        "/usr/bin/whoisthat-core",
        "/usr/local/bin/whoisthat-core",
    ];
    for c in candidates {
        if std::path::Path::new(c).exists() {
            return c.to_string();
        }
    }
    "whoisthat-core".to_string()
}

pub(crate) fn dispatch(msg: TcpMessage) -> CoreEvent {
    match msg.msg.as_str() {
        "application-state" => {
            try_dispatch!(msg, "application-state", ApplicationState, ApplicationState)
        }
        "status-changed" => try_dispatch!(msg, "status-changed", ProxyStatus, StatusChanged),
        "profiles-added" => try_dispatch!(msg, "profiles-added", ProfilesAdded, |d| {
            CoreEvent::ProfilesAdded(d.profiles)
        }),
        "profiles-deleted" => try_dispatch!(msg, "profiles-deleted", ProfilesDeleted, |d| {
            CoreEvent::ProfilesDeleted(d.deleted_profiles)
        }),
        "profile-updated" => try_dispatch!(msg, "profile-updated", ProfileUpdated, |d| {
            CoreEvent::ProfileUpdated(d.profile)
        }),
        "group-added" => try_dispatch!(msg, "group-added", GroupAdded, |d| CoreEvent::GroupAdded(
            Group {
                id: d.id,
                name: d.name,
                subscription_url: d.subscription_url,
                last_id: 0,
                ..Default::default()
            }
        )),
        "group-deleted" => try_dispatch!(msg, "group-deleted", GroupDeleted, |d| {
            CoreEvent::GroupDeleted(d.id)
        }),
        "group-updated" => try_dispatch!(msg, "group-updated", Group, GroupUpdated),
        "subscription-updated" => {
            try_dispatch!(msg, "subscription-updated", SubscriptionUpdated, |d| {
                CoreEvent::SubscriptionUpdated {
                    group: d.group,
                    profiles: d.profiles,
                }
            })
        }
        "tun-status-changed" => try_dispatch!(msg, "tun-status-changed", TunStatus, |d| {
            CoreEvent::TunStatusChanged(d.is_enabled)
        }),
        "is-root-answer" => try_dispatch!(msg, "is-root-answer", IsRootAnswer, |d| {
            CoreEvent::IsRootAnswer(d.is_root)
        }),
        "warn" => try_dispatch!(msg, "warn", Warning, |d| CoreEvent::Warning {
            key: d.key,
            content: d.content,
        }),
        "error" => try_dispatch!(msg, "error", Warning, |d| CoreEvent::Error(d.content)),
        "traffic-stats" => try_dispatch!(msg, "traffic-stats", TrafficStats, TrafficStats),
        "routing-updated" => try_dispatch!(msg, "routing-updated", RoutingUpdated, |d| {
            CoreEvent::RoutingUpdated(d.config)
        }),
        "hwid-updated" => try_dispatch!(msg, "hwid-updated", HwidData, HwidUpdated),
        "tun-name-updated" => try_dispatch!(msg, "tun-name-updated", SetTunNameData, |d| {
            CoreEvent::TunNameUpdated(d.tun_name)
        }),
        "kill-switch-updated" => {
            try_dispatch!(msg, "kill-switch-updated", SetKillSwitchData, |d| {
                CoreEvent::KillSwitchUpdated(d.enabled)
            })
        }
        "split-tunnel-updated" => {
            try_dispatch!(msg, "split-tunnel-updated", SetSplitTunnelData, |d| {
                CoreEvent::SplitTunnelUpdated(d.mode)
            })
        }
        "autoconnect-updated" => try_dispatch!(
            msg,
            "autoconnect-updated",
            AutoconnectInfo,
            AutoconnectUpdated
        ),
        "test-progress" => {
            try_dispatch!(msg, "test-progress", TestProgress, TestProgress)
        }
        "test-config-updated" => {
            try_dispatch!(msg, "test-config-updated", TestConfigUpdated, |d| {
                CoreEvent::TestConfigUpdated(d.config)
            })
        }
        other => {
            warn!("Unknown message type: {}", other);
            CoreEvent::Error(format!("Unknown message: {}", other))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_msg(msg: &str, data: serde_json::Value) -> TcpMessage {
        TcpMessage {
            msg: msg.to_string(),
            data,
        }
    }

    #[test]
    fn test_dispatch_application_state() {
        let event = dispatch(make_msg(
            "application-state",
            json!({
                "groups": [],
                "connection-status": { "connection": "disconnected", "connected_at": 0 },
                "tun-status": false
            }),
        ));
        assert!(matches!(event, CoreEvent::ApplicationState(_)));
    }

    #[test]
    fn test_dispatch_status_changed() {
        let event = dispatch(make_msg(
            "status-changed",
            json!({
                "connection": "connected",
                "connected_at": 1234567890
            }),
        ));
        assert!(matches!(event, CoreEvent::StatusChanged(_)));
    }

    #[test]
    fn test_dispatch_profiles_added() {
        let event = dispatch(make_msg(
            "profiles-added",
            json!({
                "profiles": [{ "id": 1, "group_id": 2 }]
            }),
        ));
        if let CoreEvent::ProfilesAdded(profiles) = event {
            assert_eq!(profiles.len(), 1);
            assert_eq!(profiles[0].id, 1);
        } else {
            panic!("expected ProfilesAdded");
        }
    }

    #[test]
    fn test_dispatch_profiles_deleted() {
        let event = dispatch(make_msg(
            "profiles-deleted",
            json!({
                "deleted-profiles": [{ "id": 3, "group_id": 1 }]
            }),
        ));
        if let CoreEvent::ProfilesDeleted(ids) = event {
            assert_eq!(ids.len(), 1);
            assert_eq!(ids[0].id, 3);
        } else {
            panic!("expected ProfilesDeleted");
        }
    }

    #[test]
    fn test_dispatch_profile_updated() {
        let event = dispatch(make_msg(
            "profile-updated",
            json!({
                "profile": { "id": 5, "group_id": 2 }
            }),
        ));
        if let CoreEvent::ProfileUpdated(p) = event {
            assert_eq!(p.id, 5);
        } else {
            panic!("expected ProfileUpdated");
        }
    }

    #[test]
    fn test_dispatch_group_added() {
        let event = dispatch(make_msg(
            "group-added",
            json!({
                "id": 10, "name": "MyGroup", "subscription_url": ""
            }),
        ));
        if let CoreEvent::GroupAdded(g) = event {
            assert_eq!(g.id, 10);
            assert_eq!(g.name, "MyGroup");
            assert_eq!(g.last_id, 0);
        } else {
            panic!("expected GroupAdded");
        }
    }

    #[test]
    fn test_dispatch_group_deleted() {
        let event = dispatch(make_msg("group-deleted", json!({ "id": 7 })));
        assert!(matches!(event, CoreEvent::GroupDeleted(7)));
    }

    #[test]
    fn test_dispatch_group_updated() {
        let event = dispatch(make_msg(
            "group-updated",
            json!({ "id": 3, "name": "Updated" }),
        ));
        if let CoreEvent::GroupUpdated(g) = event {
            assert_eq!(g.id, 3);
        } else {
            panic!("expected GroupUpdated");
        }
    }

    #[test]
    fn test_dispatch_subscription_updated() {
        let event = dispatch(make_msg(
            "subscription-updated",
            json!({
                "group_id": 1,
                "group": { "id": 1, "name": "Sub" },
                "profiles": [{ "id": 1, "group_id": 1 }]
            }),
        ));
        if let CoreEvent::SubscriptionUpdated { group, profiles } = event {
            assert_eq!(group.id, 1);
            assert_eq!(profiles.len(), 1);
        } else {
            panic!("expected SubscriptionUpdated");
        }
    }

    #[test]
    fn test_dispatch_tun_status_changed() {
        let event = dispatch(make_msg(
            "tun-status-changed",
            json!({ "is_enabled": true }),
        ));
        assert!(matches!(event, CoreEvent::TunStatusChanged(true)));
    }

    #[test]
    fn test_dispatch_is_root_answer() {
        let event = dispatch(make_msg("is-root-answer", json!({ "IsRoot": false })));
        assert!(matches!(event, CoreEvent::IsRootAnswer(false)));
    }

    #[test]
    fn test_dispatch_warn() {
        let event = dispatch(make_msg("warn", json!({ "key": "k1", "content": "msg" })));
        if let CoreEvent::Warning { key, content } = event {
            assert_eq!(key, "k1");
            assert_eq!(content, "msg");
        } else {
            panic!("expected Warning");
        }
    }

    #[test]
    fn test_dispatch_error() {
        let event = dispatch(make_msg("error", json!({ "key": "", "content": "boom" })));
        if let CoreEvent::Error(msg) = event {
            assert_eq!(msg, "boom");
        } else {
            panic!("expected Error");
        }
    }

    #[test]
    fn test_dispatch_traffic_stats() {
        let event = dispatch(make_msg(
            "traffic-stats",
            json!({
                "proxy_up": 100, "proxy_down": 200, "direct_up": 0, "direct_down": 0
            }),
        ));
        if let CoreEvent::TrafficStats(s) = event {
            assert_eq!(s.proxy_up, 100);
            assert_eq!(s.proxy_down, 200);
        } else {
            panic!("expected TrafficStats");
        }
    }

    #[test]
    fn test_dispatch_routing_updated() {
        let event = dispatch(make_msg(
            "routing-updated",
            json!({
                "config": { "domain_strategy": "IPIfNonMatch", "rules": [] }
            }),
        ));
        if let CoreEvent::RoutingUpdated(cfg) = event {
            assert_eq!(cfg.domain_strategy, "IPIfNonMatch");
        } else {
            panic!("expected RoutingUpdated");
        }
    }

    #[test]
    fn test_dispatch_hwid_updated() {
        let event = dispatch(make_msg(
            "hwid-updated",
            json!({
                "enabled": true, "hwid": "abc123", "user_agent": "", "platform": "", "kernel": "", "model": ""
            }),
        ));
        if let CoreEvent::HwidUpdated(h) = event {
            assert!(h.enabled);
            assert_eq!(h.hwid, "abc123");
        } else {
            panic!("expected HwidUpdated");
        }
    }

    #[test]
    fn test_dispatch_unknown_message() {
        let event = dispatch(make_msg("totally-unknown", json!({})));
        assert!(matches!(event, CoreEvent::Error(_)));
        if let CoreEvent::Error(msg) = event {
            assert!(msg.contains("totally-unknown"));
        }
    }

    #[test]
    fn test_dispatch_invalid_json_for_known_type() {
        // "groups" field is required for application-state but missing
        let event = dispatch(make_msg("application-state", json!({ "garbage": true })));
        assert!(matches!(event, CoreEvent::Error(_)));
    }

    #[test]
    fn test_dispatch_tun_name_updated() {
        let event = dispatch(make_msg(
            "tun-name-updated",
            json!({ "tun_name": "mytun0" }),
        ));
        if let CoreEvent::TunNameUpdated(name) = event {
            assert_eq!(name, "mytun0");
        } else {
            panic!("expected TunNameUpdated");
        }
    }

    #[test]
    fn test_dispatch_autoconnect_updated() {
        let event = dispatch(make_msg(
            "autoconnect-updated",
            json!({
                "enabled": true, "mode": "tun"
            }),
        ));
        if let CoreEvent::AutoconnectUpdated(info) = event {
            assert!(info.enabled);
            assert_eq!(info.mode, "tun");
        } else {
            panic!("expected AutoconnectUpdated");
        }
    }

    #[test]
    fn test_dispatch_test_progress() {
        let event = dispatch(make_msg(
            "test-progress",
            json!({ "group_id": 5, "tested": 12, "total": 30 }),
        ));
        if let CoreEvent::TestProgress(p) = event {
            assert_eq!(p.group_id, 5);
            assert_eq!(p.tested, 12);
            assert_eq!(p.total, 30);
        } else {
            panic!("expected TestProgress");
        }
    }

    #[test]
    fn test_dispatch_test_config_updated() {
        let event = dispatch(make_msg(
            "test-config-updated",
            json!({
                "config": {
                    "concurrency": 16,
                    "timeout_seconds": 5,
                    "samples_per_test": 3,
                    "test_endpoint": "https://example.com/204",
                    "auto_test_on_subscribe": true
                }
            }),
        ));
        if let CoreEvent::TestConfigUpdated(c) = event {
            assert_eq!(c.concurrency, 16);
            assert_eq!(c.samples_per_test, 3);
            assert!(c.auto_test_on_subscribe);
        } else {
            panic!("expected TestConfigUpdated");
        }
    }

    #[test]
    fn test_dispatch_profile_updated_rich_fields() {
        let event = dispatch(make_msg(
            "profile-updated",
            json!({
                "profile": {
                    "id": 5, "group_id": 2,
                    "test-result": 120,
                    "tested_at": 1700000000,
                    "loss-pct": 0,
                    "jitter-ms": 8
                }
            }),
        ));
        if let CoreEvent::ProfileUpdated(p) = event {
            assert_eq!(p.test_result, 120);
            assert_eq!(p.tested_at, 1700000000);
            assert_eq!(p.jitter_ms, 8);
            assert_eq!(p.loss_pct, 0);
        } else {
            panic!("expected ProfileUpdated");
        }
    }
}
