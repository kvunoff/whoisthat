use std::fs;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::config::{self, AppConfig};
use crate::core_client::connection::{CoreConnection, Endpoint};
use crate::core_client::protocol::*;
use crate::core_spawn::find_core_binary;
use crate::doctor::types::{CheckCategory, CheckItem, DoctorReport};

pub async fn run_doctor() -> DoctorReport {
    let cfg = config::load_config();

    let cat_binaries = check_binaries();
    let cat_capabilities = check_capabilities();
    let cat_polkit = check_polkit();
    let cat_ipc = check_ipc_and_daemon(&cfg).await;
    let cat_config = check_config_and_storage(&cfg);

    let mut report = DoctorReport::new(vec![
        cat_binaries,
        cat_capabilities,
        cat_polkit,
        cat_ipc,
        cat_config,
    ]);
    if let Some((cv, proto)) = crate::cli::fetch_core_version_ipc().await {
        report.core_version = Some(cv);
        report.core_protocol_version = Some(proto);
    } else {
        // Offline: never execute the core binary (old binaries ignore
        // --version and boot a daemon). Use last-seen version from config.
        let last_seen = cfg.core_version.clone();
        if !last_seen.is_empty() {
            report.core_version = Some(last_seen);
        }
    }
    report
}

fn check_binaries() -> CheckCategory {
    let mut items = Vec::new();

    // 1. whoisthat (client)
    let client_path = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "whoisthat".to_string());
    let version = env!("CARGO_PKG_VERSION");
    items.push(CheckItem::pass(
        "whoisthat",
        format!("{} (v{})", client_path, version),
    ));

    // 2. whoisthat-core (daemon)
    // NOTE: existence check only — never execute the binary here, old
    // binaries ignore --version and boot a daemon instead.
    let core_bin = find_core_binary();
    let core_path = Path::new(&core_bin);
    if core_path.exists() {
        items.push(CheckItem::pass("whoisthat-core", core_bin.clone()));
    } else {
        items.push(CheckItem::fail(
            "whoisthat-core",
            "daemon binary not found in PATH or standard directories",
            Some("sudo install -Dm755 core/core/whoisthat-core /usr/bin/whoisthat-core".into()),
        ));
    }

    // 3. xray (Xray-core)
    let xray_info = probe_xray();
    match xray_info {
        Some((path, ver)) => {
            let msg = format!("{} ({})", path, ver);
            if ver.contains("26.9.9") {
                items.push(CheckItem::pass("xray-core", msg));
            } else {
                items.push(CheckItem::warn(
                    "xray-core",
                    format!("{} (recommended pinned version: v26.9.9)", msg),
                    Some("WhoisThat will auto-download pinned v26.9.9 into ~/.local/share/whoisthat/runtimes/xray/".into()),
                ));
            }
        }
        None => {
            items.push(CheckItem::warn(
                "xray-core",
                "xray binary not found in PATH or managed runtimes",
                Some(
                    "Install 'xray' or run WhoisThat to trigger automatic download of v26.9.9"
                        .into(),
                ),
            ));
        }
    }

    // 5. System CLI tools
    let mut missing_tools = Vec::new();
    let mut found_tools = Vec::new();
    for tool in ["ip", "getcap", "setcap", "pkexec"] {
        if find_binary(tool, &[]).is_some() {
            found_tools.push(tool);
        } else {
            missing_tools.push(tool);
        }
    }

    if missing_tools.is_empty() {
        items.push(CheckItem::pass(
            "system tools",
            format!("{} available", found_tools.join(", ")),
        ));
    } else {
        items.push(CheckItem::warn(
            "system tools",
            format!("missing: {}", missing_tools.join(", ")),
            Some("Install iproute2, libcap, and polkit packages".into()),
        ));
    }

    CheckCategory::new("Binaries & Runtimes", items)
}

fn check_capabilities() -> CheckCategory {
    let mut items = Vec::new();

    // 1. whoisthat-core capabilities
    let core_bin = find_core_binary();
    let core_path = Path::new(&core_bin);
    if core_path.exists() {
        let caps_output = Command::new("getcap")
            .arg(&core_bin)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        let has_net_admin = caps_output.contains("cap_net_admin");
        let has_net_raw = caps_output.contains("cap_net_raw");
        let has_setpcap = caps_output.contains("cap_setpcap");

        if has_net_admin && has_net_raw && has_setpcap {
            let caps_str = caps_output.split_whitespace().last().unwrap_or("verified");
            items.push(CheckItem::pass(
                "whoisthat-core caps",
                format!("{} ({})", core_bin, caps_str),
            ));
        } else {
            let fix = format!(
                "sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep {}",
                core_bin
            );
            items.push(CheckItem::fail(
                "whoisthat-core caps",
                "missing required network capabilities for rootless TUN mode",
                Some(fix),
            ));
        }
    } else {
        items.push(CheckItem::fail(
            "whoisthat-core caps",
            "cannot check capabilities because whoisthat-core binary was not found",
            None,
        ));
    }

    // 2. /dev/net/tun
    let tun_dev = Path::new("/dev/net/tun");
    if tun_dev.exists() {
        match fs::metadata(tun_dev) {
            Ok(meta) => {
                let is_char = meta.file_type().is_char_device();
                let mode = meta.permissions().mode() & 0o777;
                if is_char {
                    items.push(CheckItem::pass(
                        "/dev/net/tun",
                        format!("available (char device, mode {:04o})", mode),
                    ));
                } else {
                    items.push(CheckItem::fail(
                        "/dev/net/tun",
                        "exists but is not a character device",
                        Some("Ensure the 'tun' kernel module is loaded: sudo modprobe tun".into()),
                    ));
                }
            }
            Err(e) => {
                items.push(CheckItem::fail(
                    "/dev/net/tun",
                    format!("cannot read metadata: {}", e),
                    None,
                ));
            }
        }
    } else {
        items.push(CheckItem::fail(
            "/dev/net/tun",
            "virtual device /dev/net/tun not found",
            Some("Load tun kernel module: sudo modprobe tun".into()),
        ));
    }

    // 3. Firewall backend
    let has_nft = Command::new("nft").arg("--version").output().is_ok();
    let has_iptables = Command::new("iptables").arg("--version").output().is_ok();

    if has_nft {
        items.push(CheckItem::pass(
            "firewall backend",
            "nftables active (recommended)",
        ));
    } else if has_iptables {
        items.push(CheckItem::pass("firewall backend", "iptables legacy mode"));
    } else {
        items.push(CheckItem::fail(
            "firewall backend",
            "neither nftables nor iptables found — TUN routing & killswitch will fail",
            Some("Install 'nftables' or 'iptables' from package manager".into()),
        ));
    }

    // 4. cgroup v2
    let cgroup_controllers = Path::new("/sys/fs/cgroup/cgroup.controllers");
    if cgroup_controllers.exists() {
        items.push(CheckItem::pass(
            "cgroup v2",
            "available — split-tunneling supported",
        ));
    } else {
        items.push(CheckItem::warn(
            "cgroup v2",
            "/sys/fs/cgroup/cgroup.controllers not found — system may use legacy cgroup v1",
            Some("Split-tunneling ('whoisthat run') requires unified cgroup v2 hierarchy".into()),
        ));
    }

    CheckCategory::new("Linux Capabilities & Kernel", items)
}

fn check_polkit() -> CheckCategory {
    let mut items = Vec::new();

    // 1. Polkit daemon
    let polkit_active = Command::new("systemctl")
        .arg("is-active")
        .arg("polkit.service")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    if polkit_active {
        items.push(CheckItem::pass("polkit daemon", "active (polkit.service)"));
    } else {
        items.push(CheckItem::warn(
            "polkit daemon",
            "polkit.service is not active",
            Some("Start polkit daemon: sudo systemctl start polkit".into()),
        ));
    }

    // 2. pkexec
    if let Some(path) = find_binary("pkexec", &["/usr/bin/pkexec"]) {
        items.push(CheckItem::pass("pkexec", path));
    } else {
        items.push(CheckItem::warn(
            "pkexec",
            "pkexec binary not found",
            Some("Install 'polkit' package to allow graphical/prompt capability setup".into()),
        ));
    }

    // 3. Systemd linger
    let user = std::env::var("USER").unwrap_or_default();
    if !user.is_empty() {
        let linger_res = Command::new("loginctl")
            .arg("show-user")
            .arg(&user)
            .arg("--property=Linger")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("Linger=yes"))
            .unwrap_or(false);

        if linger_res {
            items.push(CheckItem::pass(
                "systemd linger",
                format!("enabled for user '{}'", user),
            ));
        } else {
            items.push(CheckItem::info(
                "systemd linger",
                format!("disabled for user '{}' (enable with: loginctl enable-linger {} to start VPN on boot)", user, user),
            ));
        }
    }

    CheckCategory::new("Polkit & Permissions", items)
}

async fn check_ipc_and_daemon(cfg: &AppConfig) -> CheckCategory {
    let mut items = Vec::new();

    // 1. XDG_RUNTIME_DIR
    match std::env::var("XDG_RUNTIME_DIR") {
        Ok(dir) => {
            let p = Path::new(&dir);
            if p.exists() {
                items.push(CheckItem::pass("XDG_RUNTIME_DIR", dir));
            } else {
                items.push(CheckItem::warn(
                    "XDG_RUNTIME_DIR",
                    format!("directory does not exist: {}", dir),
                    None,
                ));
            }
        }
        Err(_) => {
            items.push(CheckItem::warn(
                "XDG_RUNTIME_DIR",
                "env variable not set, using fallback /tmp/whoisthat-<uid>",
                None,
            ));
        }
    }

    // 2. IPC Socket
    let endpoint = cfg.endpoint();
    match &endpoint {
        Endpoint::Unix(sock_path) => {
            let p = Path::new(sock_path);
            if p.exists() {
                if let Ok(meta) = fs::metadata(p) {
                    let is_sock = meta.file_type().is_socket();
                    let mode = meta.permissions().mode() & 0o777;
                    if is_sock {
                        if mode == 0o600 {
                            items.push(CheckItem::pass(
                                "IPC socket",
                                format!("{} (mode 0600, secure)", sock_path),
                            ));
                        } else {
                            items.push(CheckItem::warn(
                                "IPC socket",
                                format!("{} (mode {:04o}, expected 0600)", sock_path, mode),
                                Some(format!("chmod 0600 {}", sock_path)),
                            ));
                        }
                    } else {
                        items.push(CheckItem::fail(
                            "IPC socket",
                            format!("{} exists but is not a unix domain socket", sock_path),
                            Some(format!("rm -f {}", sock_path)),
                        ));
                    }
                } else {
                    items.push(CheckItem::pass("IPC socket", sock_path.clone()));
                }
            } else {
                items.push(CheckItem::info(
                    "IPC socket",
                    format!("{} (not created — daemon is offline)", sock_path),
                ));
            }
        }
        Endpoint::Tcp { host, port } => {
            items.push(CheckItem::info(
                "IPC transport",
                format!("TCP mode configured on {}:{}", host, port),
            ));
        }
    }

    // 3. Live daemon connection
    let conn_attempt = tokio::time::timeout(
        Duration::from_millis(1500),
        CoreConnection::connect_endpoint(&endpoint),
    )
    .await;

    match conn_attempt {
        Ok(Ok(conn)) => {
            let (mut read_half, mut write_half) = conn.into_split();
            let query_res = async {
                write_half
                    .send("get-application-state", &GetApplicationStateData {})
                    .await?;
                let state = tokio::time::timeout(Duration::from_millis(2000), async {
                    while let Ok(msg) = read_half.recv().await {
                        if msg.msg == "application-state" {
                            if let Ok(state) = serde_json::from_value::<ApplicationState>(msg.data)
                            {
                                return Ok(state);
                            }
                        }
                    }
                    Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "state message not received",
                    ))
                })
                .await??;
                Ok::<ApplicationState, std::io::Error>(state)
            }
            .await;

            match query_res {
                Ok(state) => {
                    let conn_status = &state.connection_status.connection;
                    let tun_str = if state.tun_status {
                        "TUN active"
                    } else {
                        "Proxy mode"
                    };
                    let prof_str = state
                        .connection_status
                        .profile
                        .as_ref()
                        .map(|p| format!("profile #{} ({})", p.id, p.name))
                        .unwrap_or_else(|| "none".to_string());

                    items.push(CheckItem::pass(
                        "daemon connection",
                        format!(
                            "active & responsive (VPN: {}, {}, profile: {})",
                            conn_status, tun_str, prof_str
                        ),
                    ));
                }
                Err(e) => {
                    items.push(CheckItem::warn(
                        "daemon connection",
                        format!("connected but state query failed: {}", e),
                        None,
                    ));
                }
            }
        }
        _ => {
            items.push(CheckItem::info(
                "daemon connection",
                "core is not running (start with 'whoisthat' or 'systemctl --user start whoisthat-core')",
            ));
        }
    }

    // 4. Systemd user service
    let s_active = Command::new("systemctl")
        .arg("--user")
        .arg("is-active")
        .arg("whoisthat-core.service")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let s_enabled = Command::new("systemctl")
        .arg("--user")
        .arg("is-enabled")
        .arg("whoisthat-core.service")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    items.push(CheckItem::info(
        "systemd user service",
        format!(
            "whoisthat-core.service (status: {}, autostart: {})",
            s_active, s_enabled
        ),
    ));

    CheckCategory::new("IPC & Daemon", items)
}

fn check_config_and_storage(cfg: &AppConfig) -> CheckCategory {
    let mut items = Vec::new();

    // 1. TUI config
    let tui_cfg_path = config::config_path();
    if tui_cfg_path.exists() {
        items.push(CheckItem::pass(
            "TUI config",
            format!(
                "{} (theme: {}, test: {})",
                tui_cfg_path.display(),
                cfg.theme,
                cfg.test_method
            ),
        ));
    } else {
        items.push(CheckItem::info(
            "TUI config",
            format!(
                "{} (using defaults, created on first save)",
                tui_cfg_path.display()
            ),
        ));
    }

    // 2. Core config
    let core_cfg_path = config::config_dir().join("config.json");
    if core_cfg_path.exists() {
        match fs::read_to_string(&core_cfg_path) {
            Ok(json_str) => match serde_json::from_str::<serde_json::Value>(&json_str) {
                Ok(val) => {
                    let socks = val.get("socks-port").and_then(|v| v.as_i64()).unwrap_or(0);
                    let http = val.get("http-port").and_then(|v| v.as_i64()).unwrap_or(0);
                    items.push(CheckItem::pass(
                        "core config",
                        format!(
                            "{} (SOCKS: {}, HTTP: {})",
                            core_cfg_path.display(),
                            socks,
                            http
                        ),
                    ));
                }
                Err(e) => {
                    items.push(CheckItem::warn(
                        "core config",
                        format!("invalid JSON: {}", e),
                        Some("Core will recreate default config on startup".into()),
                    ));
                }
            },
            Err(e) => {
                items.push(CheckItem::warn(
                    "core config",
                    format!("cannot read {}: {}", core_cfg_path.display(), e),
                    None,
                ));
            }
        }
    } else {
        items.push(CheckItem::info(
            "core config",
            format!(
                "{} (auto-generated on core startup)",
                core_cfg_path.display()
            ),
        ));
    }

    // 3. Encrypted DB directory
    let db_dir = config::data_dir().join("db");
    if db_dir.exists() {
        let key_file = db_dir.join(".key");
        if key_file.exists() {
            let key_size = fs::metadata(&key_file).map(|m| m.len()).unwrap_or(0);
            if key_size == 32 {
                items.push(CheckItem::pass(
                    "encrypted db",
                    format!("{} (AES-256-GCM key present)", db_dir.display()),
                ));
            } else {
                items.push(CheckItem::warn(
                    "encrypted db",
                    format!("key file size {} bytes (expected 32 bytes)", key_size),
                    None,
                ));
            }
        } else {
            items.push(CheckItem::warn(
                "encrypted db",
                format!("{} exists but .key file is missing", db_dir.display()),
                Some("Core will generate a new AES key on startup".into()),
            ));
        }
    } else {
        items.push(CheckItem::info(
            "encrypted db",
            format!(
                "{} (will be created on first profile/group add)",
                db_dir.display()
            ),
        ));
    }

    // 4. Geo assets (geoip.dat, geosite.dat)
    let (geoip, geosite) = find_geo_assets();
    match (geoip, geosite) {
        (Some(p_ip), Some(p_site)) => {
            let size_ip = fs::metadata(&p_ip).map(|m| m.len()).unwrap_or(0);
            let size_site = fs::metadata(&p_site).map(|m| m.len()).unwrap_or(0);
            let mb_ip = size_ip as f64 / 1_048_576.0;
            let mb_site = size_site as f64 / 1_048_576.0;

            if size_ip >= 10 * 1024 * 1024 {
                items.push(CheckItem::pass(
                    "geo assets",
                    format!(
                        "geoip.dat ({:.1} MB), geosite.dat ({:.1} MB)",
                        mb_ip, mb_site
                    ),
                ));
            } else {
                items.push(CheckItem::warn(
                    "geo assets",
                    format!(
                        "geoip.dat ({:.1} MB is under 10MB) — may trigger re-download",
                        mb_ip
                    ),
                    None,
                ));
            }
        }
        _ => {
            items.push(CheckItem::info(
                "geo assets",
                "not downloaded yet (auto-downloaded from v2fly on first xray startup)",
            ));
        }
    }

    CheckCategory::new("Configuration & Storage", items)
}

fn find_binary(name: &str, candidates: &[&str]) -> Option<String> {
    if let Ok(output) = Command::new("which").arg(name).output() {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() && Path::new(&s).exists() {
                return Some(s);
            }
        }
    }

    for c in candidates {
        let p = Path::new(c);
        if p.exists() && p.is_file() {
            return Some(c.to_string());
        }
    }

    None
}

fn probe_xray() -> Option<(String, String)> {
    let mut candidates = Vec::new();

    // Check managed runtime directories
    let data_dir = config::data_dir();
    let runtime_dir = data_dir.join("runtimes").join("xray");
    if runtime_dir.exists() {
        if let Ok(entries) = fs::read_dir(&runtime_dir) {
            let mut dirs: Vec<_> = entries.flatten().map(|e| e.path()).collect();
            dirs.sort_by(|a, b| b.cmp(a)); // descending order so v26.9.9 precedes v26.3.27
            for dir in dirs {
                let bin = dir.join("xray");
                if bin.exists() {
                    candidates.push(bin.to_string_lossy().to_string());
                }
            }
        }
    }

    // Check system binaries
    if let Some(sys) = find_binary("xray", &["/usr/bin/xray", "/usr/local/bin/xray"]) {
        candidates.push(sys);
    }

    let mut first_working = None;

    for bin in candidates {
        if let Ok(output) = Command::new(&bin).arg("version").output() {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout);
                let mut found_ver = "version unknown".to_string();
                for line in text.lines() {
                    if line.starts_with("Xray ") {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 2 {
                            found_ver = parts[1].to_string();
                            break;
                        }
                    }
                }

                if found_ver.contains("26.9.9") {
                    return Some((bin, found_ver));
                }

                if first_working.is_none() {
                    first_working = Some((bin, found_ver));
                }
            }
        }
    }

    first_working
}

fn find_geo_assets() -> (Option<PathBuf>, Option<PathBuf>) {
    let mut search_dirs = Vec::new();

    search_dirs.push(config::config_dir().join("geo"));

    let data_dir = config::data_dir();
    let runtime_dir = data_dir.join("runtimes").join("xray");
    if let Ok(entries) = fs::read_dir(&runtime_dir) {
        for entry in entries.flatten() {
            search_dirs.push(entry.path());
        }
    }

    search_dirs.push(PathBuf::from("/usr/share/xray"));
    search_dirs.push(PathBuf::from("/usr/local/share/xray"));

    for dir in search_dirs {
        let ip = dir.join("geoip.dat");
        let site = dir.join("geosite.dat");
        if ip.exists() && site.exists() {
            return (Some(ip), Some(site));
        }
    }

    (None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doctor_report_counts() {
        let cat1 = CheckCategory::new(
            "Test Cat",
            vec![
                CheckItem::pass("item1", "ok"),
                CheckItem::warn("item2", "warning", Some("fix me".into())),
                CheckItem::fail("item3", "error", Some("critical fix".into())),
                CheckItem::info("item4", "info notice"),
            ],
        );

        let report = DoctorReport::new(vec![cat1]);
        assert_eq!(report.summary.passed, 1);
        assert_eq!(report.summary.warnings, 1);
        assert_eq!(report.summary.failed, 1);
        assert_eq!(report.summary.info, 1);
        assert!(!report.summary.is_healthy);
    }

    #[test]
    fn test_doctor_report_json_serialization() {
        let cat = CheckCategory::new("Sample", vec![CheckItem::pass("test", "working properly")]);
        let report = DoctorReport::new(vec![cat]);
        let json = serde_json::to_string(&report).expect("must serialize");
        assert!(json.contains("\"passed\":1"));
        assert!(json.contains("\"is_healthy\":true"));
        assert!(json.contains("\"test\""));
    }
}
