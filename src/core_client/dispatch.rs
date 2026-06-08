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
        "application-state" => {
            serde_json::from_value::<ApplicationState>(msg.data)
                .map(CoreEvent::ApplicationState)
                .unwrap_or_else(|e| {
                    warn!("Invalid application-state: {}", e);
                    CoreEvent::Error("Invalid application-state".into())
                })
        }
        "status-changed" => {
            serde_json::from_value::<ProxyStatus>(msg.data)
                .map(CoreEvent::StatusChanged)
                .unwrap_or_else(|e| {
                    warn!("Invalid status-changed: {}", e);
                    CoreEvent::Error("Invalid status-changed".into())
                })
        }
        "profiles-added" => {
            serde_json::from_value::<ProfilesAdded>(msg.data)
                .map(|d| CoreEvent::ProfilesAdded(d.profiles))
                .unwrap_or_else(|e| {
                    warn!("Invalid profiles-added: {}", e);
                    CoreEvent::Error("Invalid profiles-added".into())
                })
        }
        "profiles-deleted" => {
            serde_json::from_value::<ProfilesDeleted>(msg.data)
                .map(|d| CoreEvent::ProfilesDeleted(d.deleted_profiles))
                .unwrap_or_else(|e| {
                    warn!("Invalid profiles-deleted: {}", e);
                    CoreEvent::Error("Invalid profiles-deleted".into())
                })
        }
        "profile-updated" => {
            serde_json::from_value::<ProfileUpdated>(msg.data)
                .map(|d| CoreEvent::ProfileUpdated(d.profile))
                .unwrap_or_else(|e| {
                    warn!("Invalid profile-updated: {}", e);
                    CoreEvent::Error("Invalid profile-updated".into())
                })
        }
        "group-added" => {
            serde_json::from_value::<GroupAdded>(msg.data)
                .map(|d| CoreEvent::GroupAdded(Group {
                    id: d.id,
                    name: d.name,
                    subscription_url: d.subscription_url,
                    last_id: 0,
                    ..Default::default()
                }))
                .unwrap_or_else(|e| {
                    warn!("Invalid group-added: {}", e);
                    CoreEvent::Error("Invalid group-added".into())
                })
        }
        "group-deleted" => {
            serde_json::from_value::<GroupDeleted>(msg.data)
                .map(|d| CoreEvent::GroupDeleted(d.id))
                .unwrap_or_else(|e| {
                    warn!("Invalid group-deleted: {}", e);
                    CoreEvent::Error("Invalid group-deleted".into())
                })
        }
        "group-updated" => {
            serde_json::from_value::<Group>(msg.data)
                .map(CoreEvent::GroupUpdated)
                .unwrap_or_else(|e| {
                    warn!("Invalid group-updated: {}", e);
                    CoreEvent::Error("Invalid group-updated".into())
                })
        }
        "subscription-updated" => {
            serde_json::from_value::<SubscriptionUpdated>(msg.data)
                .map(|d| CoreEvent::SubscriptionUpdated {
                    group: d.group,
                    profiles: d.profiles,
                })
                .unwrap_or_else(|e| {
                    warn!("Invalid subscription-updated: {}", e);
                    CoreEvent::Error("Invalid subscription-updated".into())
                })
        }
        "tun-status-changed" => {
            serde_json::from_value::<TunStatus>(msg.data)
                .map(|d| CoreEvent::TunStatusChanged(d.is_enabled))
                .unwrap_or_else(|e| {
                    warn!("Invalid tun-status-changed: {}", e);
                    CoreEvent::Error("Invalid tun-status-changed".into())
                })
        }
        "is-root-answer" => {
            serde_json::from_value::<IsRootAnswer>(msg.data)
                .map(|d| CoreEvent::IsRootAnswer(d.is_root))
                .unwrap_or_else(|e| {
                    warn!("Invalid is-root-answer: {}", e);
                    CoreEvent::Error("Invalid is-root-answer".into())
                })
        }
        "warn" => {
            serde_json::from_value::<Warning>(msg.data)
                .map(|d| CoreEvent::Warning {
                    key: d.key,
                    content: d.content,
                })
                .unwrap_or_else(|e| {
                    warn!("Invalid warn: {}", e);
                    CoreEvent::Error("Invalid warn".into())
                })
        }
        "error" => {
            serde_json::from_value::<Warning>(msg.data)
                .map(|d| CoreEvent::Error(d.content))
                .unwrap_or_else(|e| {
                    warn!("Invalid error: {}", e);
                    CoreEvent::Error("Invalid error".into())
                })
        }
        "traffic-stats" => {
            serde_json::from_value::<TrafficStats>(msg.data)
                .map(CoreEvent::TrafficStats)
                .unwrap_or_else(|e| {
                    warn!("Invalid traffic-stats: {}", e);
                    CoreEvent::Error("Invalid traffic-stats".into())
                })
        }
        "routing-updated" => {
            serde_json::from_value::<RoutingUpdated>(msg.data)
                .map(|d| CoreEvent::RoutingUpdated(d.config))
                .unwrap_or_else(|e| {
                    warn!("Invalid routing-updated: {}", e);
                    CoreEvent::Error("Invalid routing-updated".into())
                })
        }
        other => {
            warn!("Unknown message type: {}", other);
            CoreEvent::Error(format!("Unknown message: {}", other))
        }
    }
}
