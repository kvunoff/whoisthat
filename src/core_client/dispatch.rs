use log::warn;
use tokio::sync::mpsc;

use super::connection::CoreConnection;
use super::protocol::*;

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

pub fn spawn_read_loop(mut conn: CoreConnection) -> mpsc::UnboundedReceiver<CoreEvent> {
    let (tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        loop {
            match conn.recv().await {
                Ok(msg) => {
                    let event = dispatch(msg);
                    if tx.send(event).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    warn!("Read error: {}", e);
                    let _ = tx.send(CoreEvent::Disconnected);
                    break;
                }
            }
        }
    });

    rx
}

fn dispatch(msg: TcpMessage) -> CoreEvent {
    match msg.msg.as_str() {
        "application-state" => try_dispatch!(msg, "application-state", ApplicationState, ApplicationState),
        "status-changed" => try_dispatch!(msg, "status-changed", ProxyStatus, StatusChanged),
        "profiles-added" => try_dispatch!(msg, "profiles-added", ProfilesAdded, |d| CoreEvent::ProfilesAdded(d.profiles)),
        "profiles-deleted" => try_dispatch!(msg, "profiles-deleted", ProfilesDeleted, |d| CoreEvent::ProfilesDeleted(d.deleted_profiles)),
        "profile-updated" => try_dispatch!(msg, "profile-updated", ProfileUpdated, |d| CoreEvent::ProfileUpdated(d.profile)),
        "group-added" => try_dispatch!(msg, "group-added", GroupAdded, |d| CoreEvent::GroupAdded(Group {
            id: d.id,
            name: d.name,
            subscription_url: d.subscription_url,
            last_id: 0,
            ..Default::default()
        })),
        "group-deleted" => try_dispatch!(msg, "group-deleted", GroupDeleted, |d| CoreEvent::GroupDeleted(d.id)),
        "group-updated" => try_dispatch!(msg, "group-updated", Group, GroupUpdated),
        "subscription-updated" => try_dispatch!(msg, "subscription-updated", SubscriptionUpdated, |d| CoreEvent::SubscriptionUpdated {
            group: d.group,
            profiles: d.profiles,
        }),
        "tun-status-changed" => try_dispatch!(msg, "tun-status-changed", TunStatus, |d| CoreEvent::TunStatusChanged(d.is_enabled)),
        "is-root-answer" => try_dispatch!(msg, "is-root-answer", IsRootAnswer, |d| CoreEvent::IsRootAnswer(d.is_root)),
        "warn" => try_dispatch!(msg, "warn", Warning, |d| CoreEvent::Warning {
            key: d.key,
            content: d.content,
        }),
        "error" => try_dispatch!(msg, "error", Warning, |d| CoreEvent::Error(d.content)),
        "traffic-stats" => try_dispatch!(msg, "traffic-stats", TrafficStats, TrafficStats),
        "routing-updated" => try_dispatch!(msg, "routing-updated", RoutingUpdated, |d| CoreEvent::RoutingUpdated(d.config)),
        other => {
            warn!("Unknown message type: {}", other);
            CoreEvent::Error(format!("Unknown message: {}", other))
        }
    }
}
