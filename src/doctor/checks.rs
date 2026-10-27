use std::fs;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
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
    let cat_dns = check_dns_and_routing(&cfg);
    let cat_config = check_config_and_storage(&cfg);

    let mut report = DoctorReport::new(vec![
        cat_binaries,
        cat_capabilities,
        cat_polkit,
        cat_ipc,
        cat_dns,
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

const DNS_SINK4: &str = "198.18.0.53";
const DNS_SINK6: &str = "fd00::53";
const FAKE_POOL4: &str = "198.19.0.0/16";
const FAKE_POOL6: &str = "fc00::/18";

#[derive(Default)]
struct CoreRuntimeConfig {
    split_mode: String,
    kill_switch: bool,
    tun_name: String,
}

enum DnsRules {
    Happ { text: String, ipv6: bool },
    Legacy,
    None,
    PermissionDenied,
    Unavailable,
}

fn check_dns_and_routing(cfg: &AppConfig) -> CheckCategory {
    let mut items = Vec::new();
    let core_cfg = read_core_runtime_config();
    let tun_name = if core_cfg.tun_name.is_empty() {
        cfg.tun_name.clone()
    } else {
        core_cfg.tun_name.clone()
    };

    let tun_up = Command::new("ip")
        .args(["link", "show", "dev", &tun_name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    // The sink must never be assigned to an interface: kernel local delivery
    // would bypass the TUN inbound and Xray would never see the query.
    match Command::new("ip").args(["-o", "addr", "show"]).output() {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            if sink_assigned(&text) {
                items.push(CheckItem::fail(
                    "DNS sink address",
                    format!("{} or {} is assigned to an interface", DNS_SINK4, DNS_SINK6),
                    Some(
                        "Remove the address from the interface; the sink must stay unassigned"
                            .into(),
                    ),
                ));
            } else {
                items.push(CheckItem::pass(
                    "DNS sink address",
                    format!(
                        "{} / {} are not assigned to any interface",
                        DNS_SINK4, DNS_SINK6
                    ),
                ));
            }
        }
        _ => items.push(CheckItem::info(
            "DNS sink address",
            "could not inspect interface addresses",
        )),
    }

    if tun_up {
        match Command::new("ip")
            .args(["-o", "addr", "show", "dev", &tun_name])
            .output()
        {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout);
                match fake_pool_overlap(&text) {
                    Some(pool) => items.push(CheckItem::fail(
                        "FakeDNS pools",
                        format!("TUN address overlaps the {} fake pool", pool),
                        Some("Change the TUN address or the fake pool constants".into()),
                    )),
                    None => items.push(CheckItem::pass(
                        "FakeDNS pools",
                        format!(
                            "TUN address does not overlap {} / {}",
                            FAKE_POOL4, FAKE_POOL6
                        ),
                    )),
                }
            }
            _ => items.push(CheckItem::info(
                "FakeDNS pools",
                format!("could not inspect addresses of {}", tun_name),
            )),
        }
    } else {
        items.push(CheckItem::info(
            "FakeDNS pools",
            "TUN is inactive — pool overlap check skipped",
        ));
    }

    let dns_rules = probe_dns_rules();
    match &dns_rules {
        DnsRules::Happ { text, ipv6 } => {
            items.push(CheckItem::pass(
                "DNS hijack rules",
                format!("Happ sink ({} / {})", DNS_SINK4, DNS_SINK6),
            ));
            if !has_mark_return(text) {
                items.push(CheckItem::fail(
                    "DNS hijack safety returns",
                    "missing mark-1 return; Xray direct DNS or split-excluded apps may loop",
                    Some("Restart TUN so the scoped hijack is reinstalled".into()),
                ));
            } else if !has_sink_return(text) {
                items.push(CheckItem::fail(
                    "DNS hijack safety returns",
                    "missing sink return rule",
                    Some("Restart TUN so the scoped hijack is reinstalled".into()),
                ));
            } else {
                items.push(CheckItem::pass(
                    "DNS hijack safety returns",
                    "mark 1 and sink returns are present",
                ));
            }
            if text.to_lowercase().contains("masquerade") {
                items.push(CheckItem::warn(
                    "DNS hijack mode",
                    "Happ sink rules also contain masquerade (legacy leftovers?)",
                    Some("Restart TUN to rebuild a clean whoisthat table".into()),
                ));
            }
            if !ipv6 {
                items.push(CheckItem::warn(
                    "IPv6 DNS hijack",
                    "IPv6 hijack table is missing",
                    Some("Enable IPv6 in the kernel or restart TUN".into()),
                ));
            } else {
                items.push(CheckItem::pass(
                    "IPv6 DNS hijack",
                    format!("sink {} is installed", DNS_SINK6),
                ));
            }
        }
        DnsRules::Legacy => {
            items.push(CheckItem::pass(
                "DNS hijack rules",
                "legacy resolver DNAT (manual routing or Happ profile without DNS)",
            ));
        }
        DnsRules::None => {
            if !tun_up {
                items.push(CheckItem::info(
                    "DNS hijack rules",
                    "TUN is inactive — no hijack expected",
                ));
            } else if core_cfg.split_mode == "include" {
                items.push(CheckItem::info(
                    "DNS hijack rules",
                    "split include — hijack intentionally disabled",
                ));
            } else {
                items.push(CheckItem::warn(
                    "DNS hijack rules",
                    "TUN is active but no DNS hijack table was found",
                    Some(
                        "Restart TUN; if it persists, check core.log and firewall permissions"
                            .into(),
                    ),
                ));
            }
        }
        DnsRules::PermissionDenied => items.push(CheckItem::info(
            "DNS hijack rules",
            "cannot inspect firewall state without root (try: sudo nft list table ip whoisthat)",
        )),
        DnsRules::Unavailable => items.push(CheckItem::info(
            "DNS hijack rules",
            "neither nftables nor iptables is available for inspection",
        )),
    }

    let happ_mode = matches!(dns_rules, DnsRules::Happ { .. });
    if find_binary(
        "resolvectl",
        &["/usr/bin/resolvectl", "/usr/local/bin/resolvectl"],
    )
    .is_none()
    {
        items.push(CheckItem::info(
            "systemd-resolved",
            "resolvectl not installed — system resolver is left untouched",
        ));
    } else if !tun_up {
        items.push(CheckItem::info(
            "systemd-resolved",
            "TUN is inactive — resolvectl is not configured",
        ));
    } else if core_cfg.split_mode == "include" {
        items.push(CheckItem::info(
            "systemd-resolved",
            "split include — resolvectl is left untouched",
        ));
    } else {
        match Command::new("resolvectl").args(["dns", &tun_name]).output() {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout);
                if happ_mode && !text.contains(DNS_SINK4) {
                    items.push(CheckItem::warn(
                        "systemd-resolved",
                        format!("{} DNS is not pointed at the Happ sink", tun_name),
                        Some(format!("resolvectl dns {} {}", tun_name, DNS_SINK4)),
                    ));
                } else {
                    items.push(CheckItem::pass(
                        "systemd-resolved",
                        "system resolver configuration is consistent with the active DNS mode",
                    ));
                }
            }
            _ => items.push(CheckItem::warn(
                "systemd-resolved",
                "resolvectl is installed but systemd-resolved is not reachable",
                Some(
                    "Start systemd-resolved or ignore this warning — kernel DNAT still works"
                        .into(),
                ),
            )),
        }
    }

    if !core_cfg.kill_switch {
        items.push(CheckItem::info(
            "kill-switch DNS allowlist",
            "kill-switch is disabled",
        ));
    } else if !tun_up {
        items.push(CheckItem::info(
            "kill-switch DNS allowlist",
            "kill-switch is enabled, TUN is inactive",
        ));
    } else {
        match Command::new("nft")
            .args(["list", "table", "inet", "whoisthat_ks"])
            .output()
        {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout);
                let mut missing = Vec::new();
                if !text.contains(&tun_name) {
                    missing.push("TUN interface");
                }
                if !text.contains(DNS_SINK4) {
                    missing.push("DNS sink");
                }
                if !text.contains("mark 0x00000001") && !text.contains("mark 1") {
                    missing.push("mark 1");
                }
                if missing.is_empty() {
                    items.push(CheckItem::pass(
                        "kill-switch DNS allowlist",
                        "TUN, DNS sink, and mark 1 are allowed",
                    ));
                } else {
                    items.push(CheckItem::warn(
                        "kill-switch DNS allowlist",
                        format!("missing allows: {}", missing.join(", ")),
                        Some("Re-apply the kill-switch so the whitelist is regenerated".into()),
                    ));
                }
            }
            Ok(out) if permission_denied(&out.stderr) => items.push(CheckItem::info(
                "kill-switch DNS allowlist",
                "cannot inspect the kill-switch table without root (try: sudo nft list table inet whoisthat_ks)",
            )),
            _ => items.push(CheckItem::warn(
                "kill-switch DNS allowlist",
                "kill-switch is enabled but its nftables table was not found",
                Some("Restart the kill-switch or check core.log".into()),
            )),
        }
    }

    CheckCategory::new("DNS & Routing", items)
}

fn read_core_runtime_config() -> CoreRuntimeConfig {
    let path = config::config_dir().join("config.json");
    let Ok(text) = fs::read_to_string(path) else {
        return CoreRuntimeConfig::default();
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) else {
        return CoreRuntimeConfig::default();
    };
    CoreRuntimeConfig {
        split_mode: val
            .get("split-tunnel")
            .and_then(|v| v.get("mode"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        kill_switch: val
            .get("kill-switch-enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        tun_name: val
            .get("tun-name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    }
}

fn probe_dns_rules() -> DnsRules {
    if find_binary("nft", &["/usr/bin/nft", "/usr/local/bin/nft"]).is_some() {
        let mut text = String::new();
        let mut any = false;
        let mut ipv6 = false;
        let mut denied = false;
        for (family, is_v6) in [("ip", false), ("ip6", true)] {
            match Command::new("nft")
                .args(["list", "table", family, "whoisthat"])
                .output()
            {
                Ok(out) if out.status.success() => {
                    any = true;
                    ipv6 |= is_v6;
                    text.push_str(&String::from_utf8_lossy(&out.stdout));
                }
                Ok(out) if permission_denied(&out.stderr) => denied = true,
                _ => {}
            }
        }
        if any {
            return match dns_mode_from_rules(&text) {
                Some("happ") => DnsRules::Happ { text, ipv6 },
                Some("legacy") => DnsRules::Legacy,
                _ => DnsRules::None,
            };
        }
        if denied {
            return DnsRules::PermissionDenied;
        }
        return DnsRules::None;
    }

    let mut text = String::new();
    let mut any = false;
    let mut ipv6 = false;
    let mut denied = false;
    for (cmd, is_v6) in [("iptables", false), ("ip6tables", true)] {
        if find_binary(cmd, &[]).is_none() {
            continue;
        }
        match Command::new(cmd).args(["-t", "nat", "-S"]).output() {
            Ok(out) if out.status.success() => {
                any = true;
                ipv6 |= is_v6;
                text.push_str(&String::from_utf8_lossy(&out.stdout));
            }
            Ok(out) if permission_denied(&out.stderr) => denied = true,
            _ => {}
        }
    }
    if any {
        return match dns_mode_from_rules(&text) {
            Some("happ") => DnsRules::Happ { text, ipv6 },
            Some("legacy") => DnsRules::Legacy,
            _ => DnsRules::None,
        };
    }
    if denied {
        return DnsRules::PermissionDenied;
    }
    DnsRules::Unavailable
}

fn dns_mode_from_rules(text: &str) -> Option<&'static str> {
    if text.contains(DNS_SINK4) || text.contains(DNS_SINK6) {
        Some("happ")
    } else if text.contains("dnat to") || text.contains("DNAT --to-destination") {
        Some("legacy")
    } else {
        None
    }
}

fn has_mark_return(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("meta mark 0x00000001 return")
        || lower.contains("mark 1 return")
        || lower.contains("--mark 0x1 -j return")
        || lower.contains("--mark 1 -j return")
}

fn has_sink_return(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains(&format!("daddr {} return", DNS_SINK4))
        || lower.contains(&format!("daddr {} return", DNS_SINK6))
        || lower.contains(&format!("-d {}/32 -j return", DNS_SINK4))
        || lower.contains(&format!("-d {}/128 -j return", DNS_SINK6))
        || lower.contains(&format!("-d {} -j return", DNS_SINK4))
        || lower.contains(&format!("-d {} -j return", DNS_SINK6))
}

fn sink_assigned(ip_output: &str) -> bool {
    ip_output.lines().any(|line| {
        line.contains(&format!("inet {}/", DNS_SINK4))
            || line.contains(&format!("inet6 {}/", DNS_SINK6))
    })
}

fn fake_pool_overlap(ip_output: &str) -> Option<&'static str> {
    let pool4 = Ipv4Addr::new(198, 19, 0, 0);
    let pool6 = Ipv6Addr::from_str("fc00::").ok()?;

    for line in ip_output.lines() {
        let mut tokens = line.split_whitespace();
        while let Some(kind) = tokens.next() {
            if kind != "inet" && kind != "inet6" {
                continue;
            }
            let Some(cidr) = tokens.next() else {
                continue;
            };
            let Some((addr, _prefix)) = cidr.split_once('/') else {
                continue;
            };
            if kind == "inet" {
                if let Ok(ip) = Ipv4Addr::from_str(addr) {
                    if ipv4_in_cidr(ip, pool4, 16) {
                        return Some(FAKE_POOL4);
                    }
                }
            } else if let Ok(ip) = Ipv6Addr::from_str(addr) {
                if ipv6_in_cidr(ip, pool6, 18) {
                    return Some(FAKE_POOL6);
                }
            }
        }
    }
    None
}

fn ipv4_in_cidr(ip: Ipv4Addr, network: Ipv4Addr, prefix: u32) -> bool {
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    (u32::from(ip) & mask) == (u32::from(network) & mask)
}

fn ipv6_in_cidr(ip: Ipv6Addr, network: Ipv6Addr, prefix: u32) -> bool {
    let mask = if prefix == 0 {
        0
    } else {
        u128::MAX << (128 - prefix)
    };
    (u128::from(ip) & mask) == (u128::from(network) & mask)
}

fn permission_denied(stderr: &[u8]) -> bool {
    let text = String::from_utf8_lossy(stderr).to_lowercase();
    text.contains("operation not permitted")
        || text.contains("permission denied")
        || text.contains("must be root")
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

#[cfg(test)]
mod dns_checks_tests {
    use super::*;

    #[test]
    fn classifies_dns_hijack_mode() {
        assert_eq!(
            dns_mode_from_rules("udp dport 53 dnat to 198.18.0.53:53"),
            Some("happ")
        );
        assert_eq!(
            dns_mode_from_rules("udp dport 53 dnat to 1.1.1.1:53"),
            Some("legacy")
        );
        assert_eq!(dns_mode_from_rules("table ip whoisthat {}"), None);
    }

    #[test]
    fn detects_sink_assignment() {
        assert!(sink_assigned(
            "2: wlan0    inet 198.18.0.53/32 scope global wlan0"
        ));
        assert!(sink_assigned("2: wlan0    inet6 fd00::53/128 scope global"));
        assert!(!sink_assigned(
            "2: wlan0    inet 192.168.3.3/24 scope global wlan0"
        ));
        assert!(!sink_assigned(
            "2: wlan0    inet 198.18.0.1/32 scope global wlan0"
        ));
    }

    #[test]
    fn detects_fake_pool_overlap() {
        assert_eq!(
            fake_pool_overlap("1: tun inet 198.18.0.1/32 scope global tun"),
            None
        );
        assert_eq!(
            fake_pool_overlap("1: tun inet 198.19.7.7/32 scope global tun"),
            Some(FAKE_POOL4)
        );
        assert_eq!(
            fake_pool_overlap("1: tun inet6 fc00::1/128 scope global tun"),
            Some(FAKE_POOL6)
        );
        assert_eq!(
            fake_pool_overlap("1: tun inet6 fd00::1/128 scope global tun"),
            None
        );
    }

    #[test]
    fn detects_happ_safety_returns() {
        let nft = "meta mark 0x00000001 return\nip daddr 198.18.0.53 return";
        assert!(has_mark_return(nft));
        assert!(has_sink_return(nft));

        let iptables = "-A WHOISTHAT_DNS_OUT -m mark --mark 0x1 -j RETURN";
        assert!(has_mark_return(iptables));
        assert!(!has_sink_return(iptables));

        let iptables_sink = "-A WHOISTHAT_DNS_OUT -d 198.18.0.53/32 -j RETURN";
        assert!(has_sink_return(iptables_sink));
    }
}
