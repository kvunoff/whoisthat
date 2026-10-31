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
    /// Last seen whoisthat-core version (from `core_version` in application-state).
    /// This is the real daemon version (core-vX.Y.Z), NOT the TUI version.
    /// Legacy configs may hold a TUI version here (pre-split); it is
    /// overwritten on the next successful handshake.
    #[serde(default)]
    pub core_version: String,
    #[serde(default = "default_true")]
    pub show_ip: bool,
    #[serde(default = "default_theme")]
    pub theme: String,
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
fn default_theme() -> String {
    "graphite-dark".into()
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
            theme: default_theme(),
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

/// Shared secret for the opt-in TCP transport. Must stay in sync with
/// `AppConfig.IPCTokenPath()` in the Go core.
pub fn ipc_token_path() -> String {
    let uid = unsafe { libc_getuid() };
    match std::env::var("XDG_RUNTIME_DIR") {
        Ok(dir) if !dir.is_empty() => format!("{}/whoisthat/ipc.token", dir),
        _ => format!("/tmp/whoisthat-{}/ipc.token", uid),
    }
}

/// Effective user for log paths. Matches Go's `utils.RealUserUid()`: when the
/// process was started through sudo/pkexec, the log file belongs to the real
/// user, not root.
pub(crate) fn uid() -> u32 {
    for key in ["SUDO_UID", "PKEXEC_UID"] {
        if let Ok(value) = std::env::var(key) {
            if let Ok(uid) = value.trim().parse::<u32>() {
                return uid;
            }
        }
    }
    unsafe { libc_getuid() }
}

/// Fallback location when the XDG directory is not writable. A per-uid
/// directory keeps the file out of a shared predictable /tmp path.
pub fn fallback_log_path(name: &str) -> PathBuf {
    std::env::temp_dir()
        .join(format!("whoisthat-{}", uid()))
        .join(name)
}

/// Candidates for the core log: the normal config dir first, then the
/// read-only fallback (must match core/core/main.go).
pub fn core_log_paths() -> Vec<PathBuf> {
    vec![config_dir().join("core.log"), fallback_log_path("core.log")]
}

pub fn tui_log_paths() -> Vec<PathBuf> {
    vec![data_dir().join("tui.log"), fallback_log_path("tui.log")]
}

/// First existing candidate, otherwise the primary path (for a "not found"
/// message).
pub fn first_existing(paths: &[PathBuf]) -> PathBuf {
    paths
        .iter()
        .find(|p| p.exists())
        .cloned()
        .unwrap_or_else(|| paths[0].clone())
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
    let existed = path.exists();
    if existed {
        match std::fs::read_to_string(&path) {
            Ok(content) => match toml::from_str(&content) {
                Ok(config) => return config,
                Err(e) => {
                    log::warn!(
                        "Failed to parse config: {}, backing up and using defaults",
                        e
                    );
                    let bak = path.with_file_name("config.toml.bak");
                    let _ = std::fs::copy(&path, &bak);
                }
            },
            Err(e) => {
                log::warn!("Failed to read config: {}, using defaults", e);
            }
        }
    }
    let default = AppConfig::default();
    if !existed {
        save_config(&default);
    }
    default
}

pub fn save_config(cfg: &AppConfig) {
    let Ok(content) = toml::to_string_pretty(cfg) else {
        return;
    };
    // shortcut: save failures are logged, not propagated; UI callers are sync.
    if let Err(e) = write_atomic(&config_path(), content.as_bytes()) {
        log::warn!("failed to save config: {e}");
    }
}

/// Write via a temp file in the same directory and rename it into place, so a
/// crash or a full disk cannot leave a truncated config.toml behind.
fn write_atomic(path: &std::path::Path, content: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    std::fs::create_dir_all(dir)?;

    let tmp = dir.join(format!(
        ".{}.tmp.{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));

    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        file.write_all(content)?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)?;
        if let Ok(dir) = std::fs::File::open(dir) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Whether the daemon will autostart a TUN session. Only then does the TUI
/// need capabilities before the user explicitly asks for TUN.
pub fn core_autoconnect_tun() -> bool {
    let path = config_dir().join("config.json");
    std::fs::read_to_string(path)
        .map(|text| parse_autoconnect_tun(&text))
        .unwrap_or(false)
}

fn parse_autoconnect_tun(text: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return false;
    };
    value
        .get("autoconnect-enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        && value.get("autoconnect-mode").and_then(|v| v.as_str()) == Some("tun")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("whoisthat-config-{}-{}", std::process::id(), name));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parse_autoconnect_tun_requires_enabled_tun_mode() {
        assert!(parse_autoconnect_tun(
            r#"{"autoconnect-enabled": true, "autoconnect-mode": "tun"}"#
        ));
        assert!(!parse_autoconnect_tun(
            r#"{"autoconnect-enabled": true, "autoconnect-mode": "proxy"}"#
        ));
        assert!(!parse_autoconnect_tun(
            r#"{"autoconnect-enabled": false, "autoconnect-mode": "tun"}"#
        ));
        assert!(!parse_autoconnect_tun("not json"));
    }

    #[test]
    fn write_atomic_replaces_content_with_owner_only_permissions() {
        let dir = tmp_dir("atomic");
        let path = dir.join("config.toml");

        write_atomic(&path, b"version = 1\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "version = 1\n");
        assert_eq!(
            std::fs::metadata(&path).unwrap().mode() & 0o777,
            0o600,
            "config must not stay world-readable"
        );

        write_atomic(&path, b"version = 2\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "version = 2\n");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_atomic_reports_errors_and_leaves_no_temp_file() {
        let dir = tmp_dir("atomic-error");
        let blocker = dir.join("blocker");
        std::fs::write(&blocker, b"not a directory").unwrap();

        assert!(write_atomic(&blocker.join("config.toml"), b"x").is_err());
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".tmp."))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {leftovers:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
