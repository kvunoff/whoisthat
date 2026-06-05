use log::{error, warn};
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
    SubscriptionUpdated {
        group_id: i32,
        profiles: Vec<Profile>,
    },
    TunStatusChanged(bool),
    IsRootAnswer(bool),
    Warning {
        key: String,
        content: String,
    },
    Error(String),
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
                    error!("Read error: {}", e);
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
            match serde_json::from_value::<ApplicationState>(msg.data) {
                Ok(data) => CoreEvent::ApplicationState(data),
                Err(e) => {
                    error!("Invalid application-state: {}", e);
                    CoreEvent::Error("Invalid application-state".into())
                }
            }
        }
        "status-changed" => match serde_json::from_value::<ProxyStatus>(msg.data) {
            Ok(data) => CoreEvent::StatusChanged(data),
            Err(e) => {
                error!("Invalid status-changed: {}", e);
                CoreEvent::Error("Invalid status-changed".into())
            }
        },
        "profiles-added" => match serde_json::from_value::<ProfilesAdded>(msg.data) {
            Ok(data) => CoreEvent::ProfilesAdded(data.profiles),
            Err(e) => {
                error!("Invalid profiles-added: {}", e);
                CoreEvent::Error("Invalid profiles-added".into())
            }
        },
        "profiles-deleted" => match serde_json::from_value::<ProfilesDeleted>(msg.data) {
            Ok(data) => CoreEvent::ProfilesDeleted(data.deleted_profiles),
            Err(e) => {
                error!("Invalid profiles-deleted: {}", e);
                CoreEvent::Error("Invalid profiles-deleted".into())
            }
        },
        "profile-updated" => match serde_json::from_value::<ProfileUpdated>(msg.data) {
            Ok(data) => CoreEvent::ProfileUpdated(data.profile),
            Err(e) => {
                error!("Invalid profile-updated: {}", e);
                CoreEvent::Error("Invalid profile-updated".into())
            }
        },
        "group-added" => match serde_json::from_value::<GroupAdded>(msg.data) {
            Ok(data) => CoreEvent::GroupAdded(Group {
                id: data.id,
                name: data.name,
                subscription_url: data.subscription_url,
                last_id: 0,
            }),
            Err(e) => {
                error!("Invalid group-added: {}", e);
                CoreEvent::Error("Invalid group-added".into())
            }
        },
        "group-deleted" => match serde_json::from_value::<GroupDeleted>(msg.data) {
            Ok(data) => CoreEvent::GroupDeleted(data.id),
            Err(e) => {
                error!("Invalid group-deleted: {}", e);
                CoreEvent::Error("Invalid group-deleted".into())
            }
        },
        "subscription-updated" => {
            match serde_json::from_value::<SubscriptionUpdated>(msg.data) {
                Ok(data) => CoreEvent::SubscriptionUpdated {
                    group_id: data.group_id,
                    profiles: data.profiles,
                },
                Err(e) => {
                    error!("Invalid subscription-updated: {}", e);
                    CoreEvent::Error("Invalid subscription-updated".into())
                }
            }
        }
        "tun-status-changed" => match serde_json::from_value::<TunStatus>(msg.data) {
            Ok(data) => CoreEvent::TunStatusChanged(data.is_enabled),
            Err(e) => {
                error!("Invalid tun-status-changed: {}", e);
                CoreEvent::Error("Invalid tun-status-changed".into())
            }
        },
        "is-root-answer" => match serde_json::from_value::<IsRootAnswer>(msg.data) {
            Ok(data) => CoreEvent::IsRootAnswer(data.is_root),
            Err(e) => {
                error!("Invalid is-root-answer: {}", e);
                CoreEvent::Error("Invalid is-root-answer".into())
            }
        },
        "warn" => match serde_json::from_value::<Warning>(msg.data) {
            Ok(data) => CoreEvent::Warning {
                key: data.key,
                content: data.content,
            },
            Err(e) => {
                error!("Invalid warn: {}", e);
                CoreEvent::Error("Invalid warn".into())
            }
        },
        "error" => match serde_json::from_value::<Warning>(msg.data) {
            Ok(data) => CoreEvent::Error(data.content),
            Err(e) => {
                error!("Invalid error: {}", e);
                CoreEvent::Error("Invalid error".into())
            }
        },
        other => {
            warn!("Unknown message type: {}", other);
            CoreEvent::Error(format!("Unknown message: {}", other))
        }
    }
}
