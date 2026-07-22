use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_core_tcp_port")]
    pub core_tcp_port: u16,
    #[serde(default = "default_core_host")]
    pub core_host: String,
    #[serde(default)]
    pub autoconnect: bool,
    #[serde(default)]
    pub last_group_id: i32,
    #[serde(default)]
    pub last_profile_id: i32,
    #[serde(default)]
    pub core_version: String,
    #[serde(default = "default_true")]
    pub show_ip: bool,
    #[serde(default)]
    pub log_enabled: bool,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_test_method")]
    pub test_method: String,
    #[serde(default = "default_tun_name")]
    pub tun_name: String,
    #[serde(default)]
    pub kill_switch_enabled: bool,
    /// Explicit Unix domain socket path override. Empty = derive the default
    /// from XDG_RUNTIME_DIR (must match the core's AppConfig.SocketPath()).
    #[serde(default)]
    pub ipc_socket_path: String,
    /// Reach the core over legacy TCP instead of the Unix domain socket.
    /// Off by default; enable only for remote/advanced setups where the core
    /// runs with tcp-enabled.
    #[serde(default)]
    pub use_tcp: bool,
    #[serde(default)]
    pub autoconnect_migrated: bool,
    #[serde(default = "default_test_concurrency")]
    pub test_concurrency: i32,
    #[serde(default = "default_test_timeout_seconds")]
    pub test_timeout_seconds: i32,
    #[serde(default = "default_test_samples")]
    pub test_samples: i32,
    #[serde(default = "default_test_endpoint")]
    pub test_endpoint: String,
    #[serde(default = "default_true")]
    pub auto_test_on_subscribe: bool,
}

fn default_core_tcp_port() -> u16 {
    4897
}
fn default_core_host() -> String {
    "127.0.0.1".into()
}
fn default_true() -> bool {
    true
}
fn default_log_level() -> String {
    "warn".into()
}
fn default_test_method() -> String {
    "http-get".into()
}
fn default_tun_name() -> String {
    "whoisthattun".into()
}
fn default_test_concurrency() -> i32 {
    16
}
fn default_test_timeout_seconds() -> i32 {
    5
}
fn default_test_samples() -> i32 {
    3
}
fn default_test_endpoint() -> String {
    "https://cp.cloudflare.com/generate_204".into()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            core_tcp_port: default_core_tcp_port(),
            core_host: default_core_host(),
            autoconnect: false,
            last_group_id: 0,
            last_profile_id: 0,
            core_version: String::new(),
            show_ip: true,
            log_enabled: false,
            log_level: default_log_level(),
            test_method: default_test_method(),
            tun_name: default_tun_name(),
            kill_switch_enabled: false,
            ipc_socket_path: String::new(),
            use_tcp: false,
            autoconnect_migrated: false,
            test_concurrency: default_test_concurrency(),
            test_timeout_seconds: default_test_timeout_seconds(),
            test_samples: default_test_samples(),
            test_endpoint: default_test_endpoint(),
            auto_test_on_subscribe: true,
        }
    }
}

/// The default Unix domain socket path. Must stay in sync with the core's
/// AppConfig.DefaultSocketPath() in Go so the two agree without a handshake.
pub fn default_socket_path() -> String {
    let uid = unsafe { libc_getuid() };
    match std::env::var("XDG_RUNTIME_DIR") {
        Ok(dir) if !dir.is_empty() => format!("{}/whoisthat/core.sock", dir),
        _ => format!("/tmp/whoisthat-{}/core.sock", uid),
    }
}

// Minimal getuid without pulling in the libc crate — matches Go's os.Getuid().
extern "C" {
    #[link_name = "getuid"]
    fn libc_getuid() -> u32;
}

impl AppConfig {
    /// Resolve which endpoint the TUI should use to reach the core.
    pub fn endpoint(&self) -> crate::core_client::connection::Endpoint {
        use crate::core_client::connection::Endpoint;
        if self.use_tcp {
            Endpoint::Tcp {
                host: self.core_host.clone(),
                port: self.core_tcp_port,
            }
        } else if !self.ipc_socket_path.is_empty() {
            Endpoint::Unix(self.ipc_socket_path.clone())
        } else {
            Endpoint::Unix(default_socket_path())
        }
    }
}

pub fn config_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "whoisthat")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
            PathBuf::from(home).join(".config").join("whoisthat")
        })
}

pub fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "whoisthat")
        .map(|d| d.data_local_dir().to_path_buf())
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
            PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("whoisthat")
        })
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn load_config() -> AppConfig {
    let path = config_path();
    if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(content) => match toml::from_str(&content) {
                Ok(config) => return config,
                Err(e) => {
                    log::warn!("Failed to parse config: {}, using defaults", e);
                }
            },
            Err(e) => {
                log::warn!("Failed to read config: {}, using defaults", e);
            }
        }
    }
    let default = AppConfig::default();
    save_config(&default);
    default
}

pub fn save_config(cfg: &AppConfig) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(content) = toml::to_string_pretty(cfg) {
        let _ = std::fs::write(&path, content);
    }
}
