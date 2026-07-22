use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpMessage {
    pub msg: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Profile {
    pub id: i32,
    pub group_id: i32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub protocol: String,
    #[serde(default)]
    pub uri: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub host: String,
    #[serde(rename = "test-result", default)]
    pub test_result: i32,
    #[serde(rename = "tested_at", default)]
    pub tested_at: i64,
    #[serde(rename = "loss-pct", default)]
    pub loss_pct: i32,
    #[serde(rename = "jitter-ms", default)]
    pub jitter_ms: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileID {
    pub id: i32,
    pub group_id: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Group {
    pub id: i32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub subscription_url: String,
    #[serde(default)]
    pub last_id: i32,
    #[serde(default)]
    pub sub_last_updated: i64,
    #[serde(default)]
    pub sub_expires: i64,
    #[serde(default)]
    pub sub_upload: i64,
    #[serde(default)]
    pub sub_download: i64,
    #[serde(default)]
    pub sub_total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupWithProfiles {
    pub group: Group,
    pub profiles: Vec<Profile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyStatus {
    #[serde(default)]
    pub connection: String,
    #[serde(default)]
    pub profile: Option<Profile>,
    #[serde(default)]
    pub connected_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationState {
    pub groups: Vec<GroupWithProfiles>,
    #[serde(rename = "connection-status")]
    pub connection_status: ProxyStatus,
    #[serde(rename = "tun-status", default)]
    pub tun_status: bool,
    #[serde(rename = "hwid_info", default)]
    pub hwid_info: Option<HwidData>,
    #[serde(rename = "kill_switch", default)]
    pub kill_switch: bool,
    #[serde(rename = "split_tunnel", default)]
    pub split_tunnel: String,
    #[serde(default)]
    pub autoconnect: AutoconnectInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Warning {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub content: String,
}

// --- Request types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetApplicationStateData {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectData {
    pub profile: ProfileID,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisconnectData {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddProfilesData {
    pub uris: String,
    pub group_id: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteProfilesData {
    pub profiles: Vec<ProfileID>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddGroupData {
    pub name: String,
    pub subscription_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteGroupData {
    pub id: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSubscriptionData {
    pub group_id: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestProfileData {
    pub profile: ProfileID,
    pub method: String,
    #[serde(default)]
    pub sample_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestGroupData {
    pub group_id: i32,
    pub method: String,
    #[serde(default)]
    pub sample_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelTestsData {}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestConfig {
    pub concurrency: i32,
    pub timeout_seconds: i32,
    pub samples_per_test: i32,
    pub test_endpoint: String,
    pub auto_test_on_subscribe: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetTestConfigData {
    pub config: TestConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfigUpdated {
    pub config: TestConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestProgress {
    #[serde(default)]
    pub group_id: i32,
    pub tested: i32,
    pub total: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnableTunData {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisableTunData {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetTunNameData {
    pub tun_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsRootData {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProfileData {
    #[serde(rename = "Profile")]
    pub profile: ProfileID,
    #[serde(rename = "Name")]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateGroupData {
    pub id: i32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub subscription_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DieData {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HwidData {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub hwid: String,
    #[serde(default)]
    pub user_agent: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub kernel: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SetHwidData {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub user_agent: Option<String>,
    #[serde(default)]
    pub reset: bool,
}

// --- Notification/response types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilesAdded {
    pub profiles: Vec<Profile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilesDeleted {
    #[serde(rename = "deleted-profiles")]
    pub deleted_profiles: Vec<ProfileID>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileUpdated {
    pub profile: Profile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupAdded {
    pub id: i32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub subscription_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupDeleted {
    pub id: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionUpdated {
    pub group_id: i32,
    pub group: Group,
    pub profiles: Vec<Profile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunStatus {
    pub is_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsRootAnswer {
    #[serde(rename = "IsRoot")]
    pub is_root: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrafficStats {
    #[serde(default)]
    pub proxy_up: i64,
    #[serde(default)]
    pub proxy_down: i64,
    #[serde(default)]
    pub direct_up: i64,
    #[serde(default)]
    pub direct_down: i64,
}

// routing types
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoutingRule {
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub ip: String,
    #[serde(default)]
    pub protocol: String,
    #[serde(default)]
    pub port: String,
    #[serde(default)]
    pub outbound_tag: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoutingConfig {
    #[serde(default = "default_ipifnonmatch")]
    pub domain_strategy: String,
    #[serde(default)]
    pub rules: Vec<RoutingRule>,
}

fn default_ipifnonmatch() -> String {
    "IPIfNonMatch".into()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRoutingData {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRoutingData {
    pub config: RoutingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingUpdated {
    pub config: RoutingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetKillSwitchData {
    pub enabled: bool,
}

/// Split-tunnel mode: "off", "exclude" (split apps bypass tunnel), or
/// "include" (only split apps use the tunnel).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetSplitTunnelData {
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetAutoconnectData {
    pub enabled: bool,
    pub group_id: i32,
    pub profile_id: i32,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutoconnectInfo {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_autoconnect_mode")]
    pub mode: String,
}

fn default_autoconnect_mode() -> String {
    "proxy".into()
}
