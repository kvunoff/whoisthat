use std::sync::Arc;
use tokio::sync::Mutex;

use super::connection::CoreConnection;
use super::protocol::*;

pub struct CoreClient {
    conn: Arc<Mutex<CoreConnection>>,
}

impl CoreClient {
    pub fn new(conn: CoreConnection) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    pub fn clone_ref(&self) -> Self {
        Self {
            conn: self.conn.clone(),
        }
    }

    pub async fn get_application_state(&self) -> std::io::Result<()> {
        self.conn
            .lock()
            .await
            .send("get-application-state", &GetApplicationStateData {})
            .await
    }

    pub async fn connect(&self, group_id: i32, profile_id: i32) -> std::io::Result<()> {
        self.conn
            .lock()
            .await
            .send(
                "connect",
                &ConnectData {
                    profile: ProfileID {
                        id: profile_id,
                        group_id,
                    },
                },
            )
            .await
    }

    pub async fn disconnect(&self) -> std::io::Result<()> {
        self.conn
            .lock()
            .await
            .send("disconnect", &DisconnectData {})
            .await
    }

    pub async fn add_profiles(&self, uris: &str, group_id: i32) -> std::io::Result<()> {
        self.conn
            .lock()
            .await
            .send(
                "add-profiles",
                &AddProfilesData {
                    uris: uris.to_string(),
                    group_id,
                },
            )
            .await
    }

    pub async fn delete_profiles(&self, profiles: &[ProfileID]) -> std::io::Result<()> {
        self.conn
            .lock()
            .await
            .send(
                "delete-profiles",
                &DeleteProfilesData {
                    profiles: profiles.to_vec(),
                },
            )
            .await
    }

    pub async fn test_profile(&self, group_id: i32, profile_id: i32, method: &str) -> std::io::Result<()> {
        self.conn
            .lock()
            .await
            .send(
                "test-profile",
                &TestProfileData {
                    profile: ProfileID {
                        id: profile_id,
                        group_id,
                    },
                    method: method.to_string(),
                },
            )
            .await
    }

    pub async fn enable_tun(&self) -> std::io::Result<()> {
        self.conn
            .lock()
            .await
            .send("enable-tun", &EnableTunData {})
            .await
    }

    pub async fn disable_tun(&self) -> std::io::Result<()> {
        self.conn
            .lock()
            .await
            .send("disable-tun", &DisableTunData {})
            .await
    }

    pub async fn set_tun_name(&self, name: &str) -> std::io::Result<()> {
        self.conn
            .lock()
            .await
            .send("set-tun-name", &SetTunNameData {
                tun_name: name.to_string(),
            })
            .await
    }

    pub async fn is_root(&self) -> std::io::Result<()> {
        self.conn
            .lock()
            .await
            .send("is-root", &IsRootData {})
            .await
    }

    pub async fn rename_profile(
        &self,
        group_id: i32,
        profile_id: i32,
        name: &str,
    ) -> std::io::Result<()> {
        self.conn
            .lock()
            .await
            .send(
                "update-profile",
                &UpdateProfileData {
                    profile: ProfileID {
                        id: profile_id,
                        group_id,
                    },
                    name: name.to_string(),
                },
            )
            .await
    }

    pub async fn die(&self) -> std::io::Result<()> {
        self.conn
            .lock()
            .await
            .send("die", &DieData {})
            .await
    }

    pub async fn update_subscription(&self, group_id: i32) -> std::io::Result<()> {
        self.conn
            .lock()
            .await
            .send(
                "update-subscription",
                &UpdateSubscriptionData { group_id },
            )
            .await
    }

    pub async fn delete_group(&self, id: i32) -> std::io::Result<()> {
        self.conn
            .lock()
            .await
            .send("delete-group", &DeleteGroupData { id })
            .await
    }

    pub async fn add_group(&self, name: &str, subscription_url: &str) -> std::io::Result<()> {
        self.conn
            .lock()
            .await
            .send(
                "add-group",
                &AddGroupData {
                    name: name.to_string(),
                    subscription_url: subscription_url.to_string(),
                },
            )
            .await
    }

    pub async fn update_group(
        &self,
        id: i32,
        name: &str,
        subscription_url: &str,
    ) -> std::io::Result<()> {
        self.conn
            .lock()
            .await
            .send(
                "update-group",
                &UpdateGroupData {
                    id,
                    name: name.to_string(),
                    subscription_url: subscription_url.to_string(),
                },
            )
            .await
    }

    pub async fn get_routing(&self) -> std::io::Result<()> {
        self.conn
            .lock()
            .await
            .send("get-routing", &GetRoutingData {})
            .await
    }

    pub async fn update_routing(&self, config: &RoutingConfig) -> std::io::Result<()> {
        self.conn
            .lock()
            .await
            .send(
                "update-routing",
                &UpdateRoutingData {
                    config: config.clone(),
                },
            )
            .await
    }

    pub async fn set_hwid(&self, data: &SetHwidData) -> std::io::Result<()> {
        self.conn
            .lock()
            .await
            .send("set-hwid", data)
            .await
    }

    pub async fn set_kill_switch(&self, enabled: bool) -> std::io::Result<()> {
        self.conn
            .lock()
            .await
            .send("set-kill-switch", &SetKillSwitchData { enabled })
            .await
    }
}
