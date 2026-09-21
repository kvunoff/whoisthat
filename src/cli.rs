use std::time::Duration;
use serde::{Deserialize, Serialize};

use crate::config;
use crate::core_client::connection::CoreConnection;
use crate::core_client::protocol::*;
use crate::core_spawn::find_core_binary;
use crate::net_info;
use crate::systemd::{setup_systemd_service, systemd_is_enabled, teardown_systemd_service};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CliStatus {
    pub core_running: bool,
    pub connected: bool,
    pub status: String,
    pub mode: String,
    pub tun_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    pub rx_speed_bytes: i64,
    pub tx_speed_bytes: i64,
    pub rx_speed_human: String,
    pub tx_speed_human: String,
}

impl CliStatus {
    pub fn offline() -> Self {
        Self {
            core_running: false,
            connected: false,
            status: "offline".to_string(),
            mode: String::new(),
            tun_enabled: false,
            group_id: None,
            group_name: None,
            profile_id: None,
            profile_name: None,
            protocol: None,
            rx_speed_bytes: 0,
            tx_speed_bytes: 0,
            rx_speed_human: "0 B/s".to_string(),
            tx_speed_human: "0 B/s".to_string(),
        }
    }

    pub fn from_app_state(state: &ApplicationState) -> Self {
        let is_conn = state.connection_status.connection == "connected";
        let tun = state.tun_status;
        let mode = if is_conn {
            if tun {
                "tun".to_string()
            } else {
                "proxy".to_string()
            }
        } else {
            String::new()
        };

        let mut group_id = None;
        let mut group_name = None;
        let mut profile_id = None;
        let mut profile_name = None;
        let mut protocol = None;

        if let Some(ref prof) = state.connection_status.profile {
            profile_id = Some(prof.id);
            group_id = Some(prof.group_id);
            profile_name = if prof.name.is_empty() {
                None
            } else {
                Some(prof.name.clone())
            };
            protocol = if prof.protocol.is_empty() {
                None
            } else {
                Some(prof.protocol.clone())
            };

            for g in &state.groups {
                if g.group.id == prof.group_id {
                    group_name = Some(g.group.name.clone());
                    break;
                }
            }
        }

        Self {
            core_running: true,
            connected: is_conn,
            status: state.connection_status.connection.clone(),
            mode,
            tun_enabled: tun,
            group_id,
            group_name,
            profile_id,
            profile_name,
            protocol,
            rx_speed_bytes: 0,
            tx_speed_bytes: 0,
            rx_speed_human: "0 B/s".to_string(),
            tx_speed_human: "0 B/s".to_string(),
        }
    }

    pub fn update_traffic(&mut self, stats: &TrafficStats) {
        let rx = stats.proxy_down + stats.direct_down;
        let tx = stats.proxy_up + stats.direct_up;
        self.rx_speed_bytes = rx;
        self.tx_speed_bytes = tx;
        self.rx_speed_human = format_speed_bytes(rx);
        self.tx_speed_human = format_speed_bytes(tx);
    }

    pub fn update_tun_status(&mut self, enabled: bool) {
        self.tun_enabled = enabled;
        if self.connected {
            self.mode = if enabled { "tun" } else { "proxy" }.to_string();
        }
    }

    pub fn format_short(&self) -> String {
        if !self.core_running {
            "[Offline]".to_string()
        } else if !self.connected {
            "[Disconnected]".to_string()
        } else {
            let mode_tag = if self.tun_enabled { "TUN" } else { "Proxy" };
            format!(
                "[Connected] [{}] ↓ {} ↑ {}",
                mode_tag, self.rx_speed_human, self.tx_speed_human
            )
        }
    }

    pub fn format_pretty(&self) -> String {
        if !self.core_running {
            "WhoisThat: Core is offline".to_string()
        } else if !self.connected {
            "WhoisThat: Disconnected".to_string()
        } else {
            let mode_tag = if self.tun_enabled { "TUN" } else { "Proxy" };
            let name = self
                .profile_name
                .as_deref()
                .unwrap_or("connected");
            format!(
                "WhoisThat: [Connected] [{}] ({}) ↓ {} ↑ {}",
                mode_tag, name, self.rx_speed_human, self.tx_speed_human
            )
        }
    }
}

pub fn format_speed_bytes(bytes_per_sec: i64) -> String {
    let bytes = bytes_per_sec.max(0) as f64;
    if bytes < 1024.0 {
        format!("{bytes:.0} B/s")
    } else if bytes < 1024.0 * 1024.0 {
        format!("{:.1} KB/s", bytes / 1024.0)
    } else if bytes < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB/s", bytes / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB/s", bytes / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Checks if the first argument looks like a CLI subcommand or option flag.
pub fn is_cli_command(arg: &str) -> bool {
    if arg.starts_with('-') {
        return true;
    }
    matches!(
        arg,
        "status"
            | "connect"
            | "start"
            | "disconnect"
            | "stop"
            | "toggle"
            | "toggle-tun"
            | "mode-toggle"
            | "mode-tun"
            | "mode-proxy"
            | "systemd-on"
            | "systemd-off"
            | "systemd-toggle"
            | "systemd-status"
            | "killswitch-on"
            | "killswitch-off"
            | "killswitch-toggle"
            | "profiles"
            | "ip"
            | "version"
            | "help"
    )
}

pub async fn handle_cli(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let cmd = args.first().map(String::as_str).unwrap_or("help");
    match cmd {
        // Status & monitoring
        "status" | "--status" => handle_status(&args[1..]).await,

        // Connection control
        "start" | "connect" | "--start" | "-c" => handle_connect(&args[1..]).await,
        "stop" | "disconnect" | "--stop" | "-d" => handle_disconnect().await,
        "toggle" | "--toggle" | "-t" => handle_toggle().await,

        // Mode control (TUN vs Proxy)
        "mode-toggle" | "toggle-mode" | "toggle-tun" | "--mode-toggle" | "-mt" => {
            handle_toggle_tun().await
        }
        "mode-tun" | "--mode-tun" | "--tun" => handle_set_tun(true).await,
        "mode-proxy" | "--mode-proxy" | "--proxy" => handle_set_tun(false).await,

        // Systemd user service control
        "systemd-toggle" | "--systemd-toggle" | "-st" => handle_systemd_toggle().await,
        "systemd-on" | "systemd-enable" | "--systemd-on" | "--systemd-enable" => {
            handle_systemd_set(true).await
        }
        "systemd-off" | "systemd-disable" | "--systemd-off" | "--systemd-disable" => {
            handle_systemd_set(false).await
        }
        "systemd-status" | "--systemd-status" => handle_systemd_status().await,

        // Kill-switch control
        "killswitch-toggle" | "--killswitch-toggle" | "-kt" => handle_killswitch_toggle().await,
        "killswitch-on" | "--killswitch-on" => handle_killswitch_set(true).await,
        "killswitch-off" | "--killswitch-off" => handle_killswitch_set(false).await,

        // Profile list
        "profiles" | "--profiles" | "-p" => handle_profiles(&args[1..]).await,

        // Public IP
        "ip" | "--ip" => handle_ip().await,

        // Version & help
        "version" | "--version" | "-v" => {
            println!("whoisthat v{}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "-h" | "--help" | "help" => {
            print_help();
            Ok(())
        }
        unknown => {
            eprintln!("Unknown command or option: '{unknown}'\n");
            print_help();
            std::process::exit(1);
        }
    }
}

fn print_help() {
    println!("WhoisThat v{} — Linux CLI & desktop integration\n", env!("CARGO_PKG_VERSION"));
    println!("USAGE:");
    println!("    whoisthat [COMMAND / OPTION] [ARGS...]\n");
    println!("CONNECTION COMMANDS:");
    println!("    -t,  --toggle, toggle         Toggle VPN connection (connect / disconnect)");
    println!("    -c,  --start,  connect [P]    Connect to profile (by ID, group:id, or name)");
    println!("    -d,  --stop,   disconnect     Disconnect current VPN connection\n");
    println!("MODE COMMANDS (TUN vs Proxy):");
    println!("    -mt, --mode-toggle, toggle-tun Toggle between TUN and Proxy modes");
    println!("         --mode-tun,    --tun       Switch to full TUN mode (system-wide)");
    println!("         --mode-proxy,  --proxy     Switch to Proxy mode (SOCKS5/HTTP)\n");
    println!("SYSTEMD USER SERVICE:");
    println!("    -st, --systemd-toggle         Toggle systemd service (whoisthat-core.service)");
    println!("         --systemd-on,  --enable   Enable & start systemd user service");
    println!("         --systemd-off, --disable  Disable & stop systemd user service");
    println!("         --systemd-status          Print current service status (enabled/disabled)\n");
    println!("KILL-SWITCH COMMANDS:");
    println!("    -kt, --killswitch-toggle       Toggle kill-switch protection");
    println!("         --killswitch-on           Enable kill-switch");
    println!("         --killswitch-off          Disable kill-switch\n");
    println!("STATUS & MONITORING:");
    println!("    status [--short] [--json]     Show status snapshot or stream updates");
    println!("    -s,  --short                  Compact status: [Connected] [TUN] ↓ 1.2 MB/s ↑ 340 KB/s");
    println!("    -j,  --json                   Structured JSON output");
    println!("    -w,  --watch                  Stream live updates continuously per second\n");
    println!("INFORMATION & UTILITIES:");
    println!("    -p,  --profiles [--json]      List all subscription groups and profiles");
    println!("         --ip,      ip            Fetch and print public IPv4 & IPv6");
    println!("         run <app> [args...]      Launch app into split-tunnel cgroup slice");
    println!("    -v,  --version                Print version");
    println!("    -h,  --help                   Show this help message\n");
    println!("EXAMPLES:");
    println!("    whoisthat -t                     # Toggle VPN on/off");
    println!("    whoisthat -mt                    # Toggle TUN mode");
    println!("    whoisthat --start 2              # Connect to profile #2");
    println!("    whoisthat status --short         # For status bars (Waybar, Polybar, GNOME)");
    println!("    whoisthat status --watch --json  # For reactive UI widgets");
    println!("    whoisthat -st                    # Toggle autostart systemd service");
}

async fn handle_status(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let is_short = args.iter().any(|a| a == "--short" || a == "-s");
    let is_json = args.iter().any(|a| a == "--json" || a == "-j");
    let is_watch = args.iter().any(|a| a == "--watch" || a == "-w");

    let cfg = config::load_config();
    let endpoint = cfg.endpoint();

    let conn_res = CoreConnection::connect_endpoint(&endpoint).await;
    if conn_res.is_err() {
        let status = CliStatus::offline();
        if is_json {
            println!("{}", serde_json::to_string(&status)?);
        } else if is_short {
            println!("{}", status.format_short());
        } else {
            println!("{}", status.format_pretty());
        }
        return Ok(());
    }

    let conn = conn_res?;
    let (mut read_half, mut write_half) = conn.into_split();

    write_half
        .send("get-application-state", &GetApplicationStateData {})
        .await?;

    let state_msg = tokio::time::timeout(Duration::from_secs(3), async {
        while let Ok(msg) = read_half.recv().await {
            if msg.msg == "application-state" {
                if let Ok(state) = serde_json::from_value::<ApplicationState>(msg.data) {
                    return Ok(state);
                }
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "Failed to receive application state",
        ))
    })
    .await??;

    let mut current_status = CliStatus::from_app_state(&state_msg);

    if current_status.connected && !is_watch {
        // Wait up to 1.1s for first traffic-stats tick if connected
        let _ = tokio::time::timeout(Duration::from_millis(1100), async {
            while let Ok(msg) = read_half.recv().await {
                if msg.msg == "traffic-stats" {
                    if let Ok(stats) = serde_json::from_value::<TrafficStats>(msg.data) {
                        current_status.update_traffic(&stats);
                        break;
                    }
                }
            }
        })
        .await;
    }

    let print_line = |s: &CliStatus| {
        if is_json {
            if let Ok(json) = serde_json::to_string(s) {
                println!("{json}");
            }
        } else if is_short {
            println!("{}", s.format_short());
        } else {
            println!("{}", s.format_pretty());
        }
    };

    print_line(&current_status);

    if !is_watch {
        return Ok(());
    }

    // Streaming watch mode: emit updates as they arrive
    while let Ok(msg) = read_half.recv().await {
        match msg.msg.as_str() {
            "traffic-stats" => {
                if let Ok(stats) = serde_json::from_value::<TrafficStats>(msg.data) {
                    current_status.update_traffic(&stats);
                    print_line(&current_status);
                }
            }
            "status-changed" => {
                if let Ok(st) = serde_json::from_value::<ProxyStatus>(msg.data) {
                    current_status.connected = st.connection == "connected";
                    current_status.status = st.connection.clone();
                    if let Some(prof) = st.profile {
                        current_status.profile_id = Some(prof.id);
                        current_status.group_id = Some(prof.group_id);
                        current_status.profile_name = if prof.name.is_empty() {
                            None
                        } else {
                            Some(prof.name)
                        };
                        current_status.protocol = if prof.protocol.is_empty() {
                            None
                        } else {
                            Some(prof.protocol)
                        };
                    } else if !current_status.connected {
                        current_status.profile_id = None;
                        current_status.group_id = None;
                        current_status.profile_name = None;
                        current_status.protocol = None;
                        current_status.rx_speed_bytes = 0;
                        current_status.tx_speed_bytes = 0;
                        current_status.rx_speed_human = "0 B/s".into();
                        current_status.tx_speed_human = "0 B/s".into();
                    }
                    print_line(&current_status);
                }
            }
            "tun-status-changed" => {
                if let Ok(enabled) = serde_json::from_value::<bool>(msg.data) {
                    current_status.update_tun_status(enabled);
                    print_line(&current_status);
                }
            }
            "application-state" => {
                if let Ok(state) = serde_json::from_value::<ApplicationState>(msg.data) {
                    let rx = current_status.rx_speed_bytes;
                    let tx = current_status.tx_speed_bytes;
                    current_status = CliStatus::from_app_state(&state);
                    current_status.rx_speed_bytes = rx;
                    current_status.tx_speed_bytes = tx;
                    current_status.rx_speed_human = format_speed_bytes(rx);
                    current_status.tx_speed_human = format_speed_bytes(tx);
                    print_line(&current_status);
                }
            }
            _ => {}
        }
    }

    Ok(())
}

async fn handle_toggle() -> Result<(), Box<dyn std::error::Error>> {
    let mut cfg = config::load_config();
    let endpoint = cfg.endpoint();
    let conn = CoreConnection::connect_endpoint(&endpoint)
        .await
        .map_err(|e| format!("Failed to connect to core: {e}"))?;
    let (mut read_half, mut write_half) = conn.into_split();

    write_half
        .send("get-application-state", &GetApplicationStateData {})
        .await?;

    let state = tokio::time::timeout(Duration::from_secs(3), async {
        while let Ok(msg) = read_half.recv().await {
            if msg.msg == "application-state" {
                if let Ok(state) = serde_json::from_value::<ApplicationState>(msg.data) {
                    return Ok(state);
                }
            }
        }
        Err("Failed to receive state from core")
    })
    .await??;

    if state.connection_status.connection == "connected" {
        write_half.send("disconnect", &DisconnectData {}).await?;
        println!("Disconnected.");
        return Ok(());
    }

    // Connect to target: last configured profile, or first available
    let mut target_profile: Option<(i32, i32, String)> = None;

    if cfg.last_profile_id != 0 && cfg.last_group_id != 0 {
        for group in &state.groups {
            if group.group.id == cfg.last_group_id {
                for prof in &group.profiles {
                    if prof.id == cfg.last_profile_id {
                        target_profile = Some((group.group.id, prof.id, prof.name.clone()));
                        break;
                    }
                }
            }
        }
    }

    if target_profile.is_none() {
        for group in &state.groups {
            if let Some(prof) = group.profiles.first() {
                target_profile = Some((group.group.id, prof.id, prof.name.clone()));
                break;
            }
        }
    }

    let (g_id, p_id, p_name) = match target_profile {
        Some(t) => t,
        None => {
            eprintln!("Error: No profiles found to connect to. Import profiles first.");
            std::process::exit(1);
        }
    };

    write_half
        .send(
            "connect",
            &ConnectData {
                profile: ProfileID {
                    id: p_id,
                    group_id: g_id,
                },
            },
        )
        .await?;

    cfg.last_group_id = g_id;
    cfg.last_profile_id = p_id;
    config::save_config(&cfg);

    println!("Connecting to '{p_name}' (group: {g_id}, id: {p_id})...");
    Ok(())
}

async fn handle_connect(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut cfg = config::load_config();
    let target = args.first().map(|s| s.trim()).filter(|s| !s.is_empty());

    let endpoint = cfg.endpoint();
    let conn = CoreConnection::connect_endpoint(&endpoint)
        .await
        .map_err(|e| format!("Failed to connect to core: {e}"))?;
    let (mut read_half, mut write_half) = conn.into_split();

    write_half
        .send("get-application-state", &GetApplicationStateData {})
        .await?;

    let state = tokio::time::timeout(Duration::from_secs(3), async {
        while let Ok(msg) = read_half.recv().await {
            if msg.msg == "application-state" {
                if let Ok(state) = serde_json::from_value::<ApplicationState>(msg.data) {
                    return Ok(state);
                }
            }
        }
        Err("Failed to receive state from core")
    })
    .await??;

    let mut selected: Option<(i32, i32, String)> = None;

    if let Some(t) = target {
        if let Some((g_str, p_str)) = t.split_once(':') {
            if let (Ok(g), Ok(p)) = (g_str.parse::<i32>(), p_str.parse::<i32>()) {
                for group in &state.groups {
                    if group.group.id == g {
                        for prof in &group.profiles {
                            if prof.id == p {
                                selected = Some((g, p, prof.name.clone()));
                                break;
                            }
                        }
                    }
                }
            }
        } else if let Ok(p_id) = t.parse::<i32>() {
            for group in &state.groups {
                for prof in &group.profiles {
                    if prof.id == p_id {
                        selected = Some((group.group.id, prof.id, prof.name.clone()));
                        break;
                    }
                }
                if selected.is_some() {
                    break;
                }
            }
        }

        if selected.is_none() {
            let needle = t.to_lowercase();
            for group in &state.groups {
                for prof in &group.profiles {
                    if prof.name.to_lowercase().contains(&needle) {
                        selected = Some((group.group.id, prof.id, prof.name.clone()));
                        break;
                    }
                }
                if selected.is_some() {
                    break;
                }
            }
        }

        if selected.is_none() {
            eprintln!("Error: Profile '{t}' not found. Run 'whoisthat profiles' to list.");
            std::process::exit(1);
        }
    } else {
        // No target specified: use last saved or first available
        if cfg.last_profile_id != 0 && cfg.last_group_id != 0 {
            for group in &state.groups {
                if group.group.id == cfg.last_group_id {
                    for prof in &group.profiles {
                        if prof.id == cfg.last_profile_id {
                            selected = Some((group.group.id, prof.id, prof.name.clone()));
                            break;
                        }
                    }
                }
            }
        }
        if selected.is_none() {
            for group in &state.groups {
                if let Some(prof) = group.profiles.first() {
                    selected = Some((group.group.id, prof.id, prof.name.clone()));
                    break;
                }
            }
        }
    }

    let (g_id, p_id, p_name) = match selected {
        Some(s) => s,
        None => {
            eprintln!("Error: No profile available to connect.");
            std::process::exit(1);
        }
    };

    write_half
        .send(
            "connect",
            &ConnectData {
                profile: ProfileID {
                    id: p_id,
                    group_id: g_id,
                },
            },
        )
        .await?;

    cfg.last_group_id = g_id;
    cfg.last_profile_id = p_id;
    config::save_config(&cfg);

    println!("Connecting to '{p_name}' (group: {g_id}, id: {p_id})...");
    Ok(())
}

async fn handle_disconnect() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load_config();
    let endpoint = cfg.endpoint();
    let conn = CoreConnection::connect_endpoint(&endpoint)
        .await
        .map_err(|e| format!("Failed to connect to core: {e}"))?;
    let (_read_half, mut write_half) = conn.into_split();

    write_half.send("disconnect", &DisconnectData {}).await?;
    println!("Disconnected.");
    Ok(())
}

async fn handle_toggle_tun() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load_config();
    let endpoint = cfg.endpoint();
    let conn = CoreConnection::connect_endpoint(&endpoint)
        .await
        .map_err(|e| format!("Failed to connect to core: {e}"))?;
    let (mut read_half, mut write_half) = conn.into_split();

    write_half
        .send("get-application-state", &GetApplicationStateData {})
        .await?;

    let state = tokio::time::timeout(Duration::from_secs(3), async {
        while let Ok(msg) = read_half.recv().await {
            if msg.msg == "application-state" {
                if let Ok(state) = serde_json::from_value::<ApplicationState>(msg.data) {
                    return Ok(state);
                }
            }
        }
        Err("Failed to receive state from core")
    })
    .await??;

    if state.tun_status {
        write_half.send("disable-tun", &DisableTunData {}).await?;
        println!("Mode switched to: Proxy (TUN disabled).");
    } else {
        write_half.send("enable-tun", &EnableTunData {}).await?;
        println!("Mode switched to: TUN (system-wide VPN).");
    }

    Ok(())
}

async fn handle_set_tun(enable: bool) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load_config();
    let endpoint = cfg.endpoint();
    let conn = CoreConnection::connect_endpoint(&endpoint)
        .await
        .map_err(|e| format!("Failed to connect to core: {e}"))?;
    let (mut read_half, mut write_half) = conn.into_split();

    write_half
        .send("get-application-state", &GetApplicationStateData {})
        .await?;

    let state = tokio::time::timeout(Duration::from_secs(3), async {
        while let Ok(msg) = read_half.recv().await {
            if msg.msg == "application-state" {
                if let Ok(state) = serde_json::from_value::<ApplicationState>(msg.data) {
                    return Ok(state);
                }
            }
        }
        Err("Failed to receive state from core")
    })
    .await??;

    if enable {
        if state.tun_status {
            println!("TUN mode is already enabled.");
        } else {
            write_half.send("enable-tun", &EnableTunData {}).await?;
            println!("TUN mode enabled.");
        }
    } else if !state.tun_status {
        println!("Proxy mode is already active (TUN disabled).");
    } else {
        write_half.send("disable-tun", &DisableTunData {}).await?;
        println!("Proxy mode enabled (TUN disabled).");
    }

    Ok(())
}

async fn handle_systemd_status() -> Result<(), Box<dyn std::error::Error>> {
    let enabled = systemd_is_enabled();
    if enabled {
        println!("Systemd user service (whoisthat-core.service): ENABLED");
    } else {
        println!("Systemd user service (whoisthat-core.service): DISABLED");
    }
    Ok(())
}

async fn handle_systemd_set(enable: bool) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load_config();
    if enable {
        if systemd_is_enabled() {
            println!("Systemd user service is already enabled.");
            return Ok(());
        }
        let core_bin = find_core_binary();
        match setup_systemd_service(&core_bin, &cfg.log_level) {
            Ok(_) => println!("Systemd user service enabled (whoisthat-core.service)."),
            Err(e) => eprintln!("Error enabling systemd service: {e}"),
        }
    } else if !systemd_is_enabled() {
        println!("Systemd user service is already disabled.");
    } else {
        match teardown_systemd_service() {
            Ok(_) => println!("Systemd user service disabled."),
            Err(e) => eprintln!("Error disabling systemd service: {e}"),
        }
    }
    Ok(())
}

async fn handle_systemd_toggle() -> Result<(), Box<dyn std::error::Error>> {
    let is_on = systemd_is_enabled();
    handle_systemd_set(!is_on).await
}

async fn handle_killswitch_toggle() -> Result<(), Box<dyn std::error::Error>> {
    let mut cfg = config::load_config();
    let endpoint = cfg.endpoint();
    let conn = CoreConnection::connect_endpoint(&endpoint)
        .await
        .map_err(|e| format!("Failed to connect to core: {e}"))?;
    let (mut read_half, mut write_half) = conn.into_split();

    write_half
        .send("get-application-state", &GetApplicationStateData {})
        .await?;

    let state = tokio::time::timeout(Duration::from_secs(3), async {
        while let Ok(msg) = read_half.recv().await {
            if msg.msg == "application-state" {
                if let Ok(state) = serde_json::from_value::<ApplicationState>(msg.data) {
                    return Ok(state);
                }
            }
        }
        Err("Failed to receive state from core")
    })
    .await??;

    let new_val = !state.kill_switch;
    write_half
        .send("set-kill-switch", &SetKillSwitchData { enabled: new_val })
        .await?;

    cfg.kill_switch_enabled = new_val;
    config::save_config(&cfg);

    if new_val {
        println!("Kill-switch: ENABLED");
    } else {
        println!("Kill-switch: DISABLED");
    }
    Ok(())
}

async fn handle_killswitch_set(enable: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut cfg = config::load_config();
    let endpoint = cfg.endpoint();
    let conn = CoreConnection::connect_endpoint(&endpoint)
        .await
        .map_err(|e| format!("Failed to connect to core: {e}"))?;
    let (_read_half, mut write_half) = conn.into_split();

    write_half
        .send("set-kill-switch", &SetKillSwitchData { enabled: enable })
        .await?;

    cfg.kill_switch_enabled = enable;
    config::save_config(&cfg);

    if enable {
        println!("Kill-switch: ENABLED");
    } else {
        println!("Kill-switch: DISABLED");
    }
    Ok(())
}

async fn handle_ip() -> Result<(), Box<dyn std::error::Error>> {
    println!("Fetching public IP...");
    let ipv4 = tokio::task::spawn_blocking(net_info::fetch_public_ip)
        .await
        .unwrap_or(None);
    let ipv6 = tokio::task::spawn_blocking(net_info::fetch_public_ipv6)
        .await
        .unwrap_or(None);

    println!("IPv4: {}", ipv4.as_deref().unwrap_or("unavailable"));
    println!("IPv6: {}", ipv6.as_deref().unwrap_or("unavailable"));
    Ok(())
}

async fn handle_profiles(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let is_json = args.iter().any(|a| a == "--json" || a == "-j");

    let cfg = config::load_config();
    let endpoint = cfg.endpoint();
    let conn = CoreConnection::connect_endpoint(&endpoint)
        .await
        .map_err(|e| format!("Failed to connect to core: {e}"))?;
    let (mut read_half, mut write_half) = conn.into_split();

    write_half
        .send("get-application-state", &GetApplicationStateData {})
        .await?;

    let state = tokio::time::timeout(Duration::from_secs(3), async {
        while let Ok(msg) = read_half.recv().await {
            if msg.msg == "application-state" {
                if let Ok(state) = serde_json::from_value::<ApplicationState>(msg.data) {
                    return Ok(state);
                }
            }
        }
        Err("Failed to receive state from core")
    })
    .await??;

    if is_json {
        println!("{}", serde_json::to_string_pretty(&state.groups)?);
        return Ok(());
    }

    println!("Groups & Profiles:\n");
    for g in &state.groups {
        println!("Group #{}: {} ({} profiles)", g.group.id, g.group.name, g.profiles.len());
        for p in &g.profiles {
            let active = if let Some(ref cur) = state.connection_status.profile {
                if cur.id == p.id && cur.group_id == p.group_id && state.connection_status.connection == "connected" {
                    " [ACTIVE]"
                } else {
                    ""
                }
            } else {
                ""
            };
            println!("  [{}:{}] {} ({}){}", g.group.id, p.id, p.name, p.protocol, active);
        }
        println!();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_speed_bytes() {
        assert_eq!(format_speed_bytes(0), "0 B/s");
        assert_eq!(format_speed_bytes(512), "512 B/s");
        assert_eq!(format_speed_bytes(1024), "1.0 KB/s");
        assert_eq!(format_speed_bytes(1536), "1.5 KB/s");
        assert_eq!(format_speed_bytes(1048576), "1.0 MB/s");
        assert_eq!(format_speed_bytes(1048576 * 5 / 2), "2.5 MB/s");
        assert_eq!(format_speed_bytes(1073741824 * 3), "3.00 GB/s");
    }

    #[test]
    fn test_cli_status_formatting() {
        let mut s = CliStatus::offline();
        assert_eq!(s.format_short(), "[Offline]");

        s.core_running = true;
        s.connected = false;
        assert_eq!(s.format_short(), "[Disconnected]");

        s.connected = true;
        s.tun_enabled = true;
        s.rx_speed_human = "1.2 MB/s".to_string();
        s.tx_speed_human = "340.0 KB/s".to_string();
        assert_eq!(s.format_short(), "[Connected] [TUN] ↓ 1.2 MB/s ↑ 340.0 KB/s");

        s.tun_enabled = false;
        assert_eq!(s.format_short(), "[Connected] [Proxy] ↓ 1.2 MB/s ↑ 340.0 KB/s");
    }

    #[test]
    fn test_cli_status_json() {
        let s = CliStatus {
            core_running: true,
            connected: true,
            status: "connected".to_string(),
            mode: "tun".to_string(),
            tun_enabled: true,
            group_id: Some(1),
            group_name: Some("Sub1".to_string()),
            profile_id: Some(42),
            profile_name: Some("Server-1".to_string()),
            protocol: Some("vless".to_string()),
            rx_speed_bytes: 1000,
            tx_speed_bytes: 500,
            rx_speed_human: "1000 B/s".to_string(),
            tx_speed_human: "500 B/s".to_string(),
        };

        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"core_running\":true"));
        assert!(json.contains("\"connected\":true"));
        assert!(json.contains("\"mode\":\"tun\""));
        assert!(json.contains("\"profile_name\":\"Server-1\""));
    }

    #[test]
    fn test_is_cli_command() {
        assert!(is_cli_command("-t"));
        assert!(is_cli_command("--toggle"));
        assert!(is_cli_command("--start"));
        assert!(is_cli_command("--stop"));
        assert!(is_cli_command("-mt"));
        assert!(is_cli_command("--mode-tun"));
        assert!(is_cli_command("--mode-proxy"));
        assert!(is_cli_command("-st"));
        assert!(is_cli_command("--systemd-on"));
        assert!(is_cli_command("--systemd-off"));
        assert!(is_cli_command("-kt"));
        assert!(is_cli_command("--killswitch-on"));
        assert!(is_cli_command("--killswitch-off"));
        assert!(is_cli_command("status"));
        assert!(is_cli_command("toggle"));
        assert!(is_cli_command("profiles"));
        assert!(is_cli_command("ip"));
        assert!(is_cli_command("version"));
        assert!(!is_cli_command("arbitrary_unknown_arg"));
    }
}
