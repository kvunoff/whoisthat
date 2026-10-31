use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpMessage {
    pub msg: String,
    pub data: serde_json::Value,
    /// Optional request-correlation id (forward-compatible; the current TUI
    /// does not require it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct RoutingProfileInfo {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub last_error: String,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub rules_count: usize,
    #[serde(default)]
    pub global_proxy: bool,
    #[serde(default)]
    pub domain_strategy: String,
    #[serde(default)]
    pub fake_dns_enabled: bool,
    #[serde(default)]
    pub remote_dns: String,
    #[serde(default)]
    pub domestic_dns: String,
    #[serde(default)]
    pub last_updated: i64,
    #[serde(default)]
    pub geo_updated_at: i64,
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
    #[serde(default)]
    pub profile_order: Vec<i32>,
    #[serde(default)]
    pub routing_mode: String,
    /// "inherit" (default, use the global manual rules) or "custom".
    #[serde(default)]
    pub manual_routing_mode: String,
    #[serde(default)]
    pub active_routing_profile_id: String,
    #[serde(default)]
    pub pending_active_routing_profile_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GroupWithProfiles {
    pub group: Group,
    pub profiles: Vec<Profile>,
    #[serde(default)]
    pub routing_profiles: Vec<RoutingProfileInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyStatus {
    #[serde(default)]
    pub connection: String,
    #[serde(default)]
    pub profile: Option<Profile>,
    #[serde(default)]
    pub connected_at: i64,
    /// Set when the connection dropped without an explicit disconnect.
    #[serde(default)]
    pub reason: String,
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
    #[serde(rename = "xray_status", default)]
    pub xray_status: Option<XrayStatusInfo>,
    #[serde(rename = "core_version", default)]
    pub core_version: String,
    #[serde(rename = "protocol_version", default)]
    pub protocol_version: u32,
    /// Daemon-authoritative TUN name. Mirrored into config.toml for offline
    /// startup, but the value from this message always wins.
    #[serde(rename = "tun_name", default)]
    pub tun_name: String,
    /// Daemon-authoritative latency-test settings.
    #[serde(rename = "test_config", default)]
    pub test_config: TestConfig,
    /// How config.json was read; drives the one-click settings restore.
    #[serde(rename = "config_status", default)]
    pub config_status: ConfigStatus,
    /// Encrypted database state; drives the one-click profiles restore.
    #[serde(rename = "db_status", default)]
    pub db_status: DbStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ConfigStatus {
    /// "ok" | "missing" | "corrupt" | "repaired"
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DbStatus {
    /// "ok" | "read-only"
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub error: String,
    #[serde(rename = "snapshot_at", default)]
    pub snapshot_at: String,
}

/// IPC contract version expected by this TUI. The value is mirrored in
/// `core/core/lib/version/protocol.txt` (embedded by both sides); the test
/// below fails if they drift.
/// A running core is reused while versions match; on mismatch it is restarted.
pub const EXPECTED_CORE_PROTOCOL_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct XrayStatusInfo {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub target_version: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub progress: i32,
    #[serde(default)]
    pub bytes_downloaded: i64,
    #[serde(default)]
    pub total_bytes: i64,
    #[serde(default)]
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct XrayProgressNotification {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub progress: i32,
    #[serde(default)]
    pub bytes_downloaded: i64,
    #[serde(default)]
    pub total_bytes: i64,
    #[serde(default)]
    pub error: String,
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
pub struct MoveProfileData {
    pub profile_id: i32,
    pub from_group_id: i32,
    pub to_group_id: i32,
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
pub struct ReorderProfilesData {
    pub group_id: i32,
    pub profile_ids: Vec<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorderGroupsData {
    pub group_ids: Vec<i32>,
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
    #[serde(default)]
    pub routing_profiles: Vec<RoutingProfileInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilesReordered {
    pub group_id: i32,
    pub profile_ids: Vec<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupsReordered {
    pub group_ids: Vec<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMoved {
    pub old_profile: ProfileID,
    pub new_profile: Profile,
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
    pub network: String,
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
    /// Happ DNS section, kept opaque so CLI read-modify-write does not drop it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns: Option<serde_json::Value>,
}

fn default_ipifnonmatch() -> String {
    "IPIfNonMatch".into()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GetRoutingData {
    #[serde(default)]
    pub group_id: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRoutingData {
    #[serde(default)]
    pub group_id: i32,
    pub config: RoutingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingUpdated {
    #[serde(default)]
    pub group_id: i32,
    pub config: RoutingConfig,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub custom: Option<RoutingConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetManualRoutingSourceData {
    pub group_id: i32,
    pub source: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportHappRoutingData {
    pub group_id: i32,
    pub link: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetGroupRoutingModeData {
    pub group_id: i32,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetActiveRoutingProfileData {
    pub group_id: i32,
    pub profile_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteRoutingProfileData {
    pub group_id: i32,
    pub profile_id: String,
}

#[cfg(test)]
mod tests {
    use super::RoutingProfileInfo;

    #[test]
    fn routing_profile_info_decodes_dns_fields() {
        let json = r#"{
            "id": "p1",
            "name": "DNS Profile",
            "state": "ready",
            "active": true,
            "rules_count": 3,
            "global_proxy": true,
            "domain_strategy": "AsIs",
            "fake_dns_enabled": true,
            "remote_dns": "DoH cloudflare-dns.com",
            "domestic_dns": "DoU 8.8.8.8"
        }"#;

        let info: RoutingProfileInfo =
            serde_json::from_str(json).expect("decode routing profile info");
        assert!(info.fake_dns_enabled);
        assert_eq!(info.remote_dns, "DoH cloudflare-dns.com");
        assert_eq!(info.domestic_dns, "DoU 8.8.8.8");
    }

    #[test]
    fn routing_profile_info_decodes_old_payload_without_dns_fields() {
        let json = r#"{"id":"p2","name":"Old","state":"ready","active":false,"rules_count":1}"#;

        let info: RoutingProfileInfo =
            serde_json::from_str(json).expect("decode old routing profile info");
        assert!(!info.fake_dns_enabled);
        assert!(info.remote_dns.is_empty());
        assert!(info.domestic_dns.is_empty());
    }

    #[test]
    fn protocol_version_matches_shared_file() {
        let shared: u32 = include_str!("../../core/core/lib/version/protocol.txt")
            .trim()
            .parse()
            .expect("protocol.txt must contain a single integer");
        assert_eq!(
            super::EXPECTED_CORE_PROTOCOL_VERSION,
            shared,
            "Rust and Go protocol versions drifted; update core/core/lib/version/protocol.txt"
        );
    }
}
