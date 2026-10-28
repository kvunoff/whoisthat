use std::process::Command;
use std::time::Duration;

use crate::config;
use crate::core_client::connection::CoreConnection;
use crate::core_client::protocol::*;
use crate::core_spawn;

/// The core sends preflight warnings (missing xray) right after connecting, so
/// a `warn` frame is only a reply to *this* command when its key matches.
fn is_warn_key(msg: &TcpMessage, key: &str) -> bool {
    msg.msg == "warn"
        && serde_json::from_value::<Warning>(msg.data.clone())
            .map(|w| w.key == key)
            .unwrap_or(false)
}

fn warn_content(msg: &TcpMessage) -> String {
    serde_json::from_value::<Warning>(msg.data.clone())
        .map(|w| w.content)
        .unwrap_or_else(|_| "core reported a warning".to_string())
}

async fn recv_state(
    read_half: &mut crate::core_client::connection::CoreReadHalf,
) -> Result<ApplicationState, Box<dyn std::error::Error>> {
    let msg = read_half
        .recv_until(Duration::from_secs(3), |m| m.msg == "application-state")
        .await?;
    Ok(serde_json::from_value(msg.data)?)
}

/// Represents an action requested via a `wit://` deep link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeepLinkAction {
    /// Add a subscription URL and auto-update it.
    AddSubscription { name: String, url: String },
    /// Import one or more profile URIs (e.g. vless://, ss://, etc.).
    AddProfile {
        uri: String,
        group_name: Option<String>,
    },
    /// Connect to VPN.
    Connect,
    /// Disconnect from VPN.
    Disconnect,
    /// Toggle VPN connection.
    Toggle,
    /// Import Happ routing link (happ://routing/add/..., onadd/..., off).
    ImportHappRouting { link: String, group: Option<String> },
}

/// Percent-decodes a URL-encoded UTF-8 string.
pub fn url_decode(s: &str) -> String {
    let mut out_bytes = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex_str = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(decoded) = u8::from_str_radix(hex_str, 16) {
                out_bytes.push(decoded);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out_bytes.push(b' ');
        } else {
            out_bytes.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out_bytes).to_string()
}

/// Helper to derive a friendly group name from a subscription URL (e.g. "example.com").
fn derive_name_from_url(url: &str) -> String {
    let trimmed = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let host = trimmed
        .split(['/', '?', '#', ':'])
        .next()
        .unwrap_or("Subscription");
    if host.is_empty() {
        "Subscription".to_string()
    } else {
        host.to_string()
    }
}

/// Parses a `wit://` deep link into a `DeepLinkAction`.
///
/// Supported formats:
/// 1. Subscription query format:
///    `wit://sub?url=https%3A%2F%2F...&name=MySub`
///    `wit://add-sub?url=...&name=...`
///    `wit://install-sub?url=...&name=...`
///    `wit://subscription?url=...&name=...`
///
/// 2. Direct subscription URL:
///    `wit://sub/https://example.com/sub`
///    `wit://https://example.com/sub`
///    `wit://http://example.com/sub`
///
/// 3. Direct profile URIs:
///    `wit://vless://...`
///    `wit://vmess://...`
///    `wit://trojan://...`
///    `wit://ss://...`
///    `wit://hysteria2://...`
///    `wit://hy2://...`
///
/// 4. Profile query format:
///    `wit://import?uri=vless%3A%2F%2F...&group=MyGroup`
///    `wit://add?uri=...`
///
/// 5. Commands:
///    `wit://connect`, `wit://disconnect`, `wit://toggle`
pub fn parse_deeplink(raw: &str) -> Result<DeepLinkAction, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("Empty deep link".to_string());
    }

    // Direct Happ routing link
    let raw_lower = raw.to_lowercase();
    if raw_lower.starts_with("happ://routing/") || raw_lower.starts_with("happ:routing/") {
        return Ok(DeepLinkAction::ImportHappRouting {
            link: raw.to_string(),
            group: None,
        });
    }

    // Strip "wit://" or "wit:"
    let after_scheme = if let Some(stripped) = raw.strip_prefix("wit://") {
        stripped
    } else if let Some(stripped) = raw.strip_prefix("wit:") {
        stripped
    } else {
        return Err(format!(
            "Invalid scheme in deep link (expected wit:// or happ://routing/...): {raw}"
        ));
    };

    let without_leading_slashes = after_scheme.trim_start_matches('/');

    // Check embedded happ:// routing link in wit://
    let without_lower = without_leading_slashes.to_lowercase();
    if without_lower.starts_with("happ://routing/") || without_lower.starts_with("happ:routing/") {
        return Ok(DeepLinkAction::ImportHappRouting {
            link: without_leading_slashes.to_string(),
            group: None,
        });
    }

    // Check direct profile schemes
    for proto in &[
        "vless://",
        "vmess://",
        "trojan://",
        "ss://",
        "socks://",
        "socks5://",
        "hysteria2://",
        "hy2://",
    ] {
        if without_leading_slashes.starts_with(proto) {
            return Ok(DeepLinkAction::AddProfile {
                uri: without_leading_slashes.to_string(),
                group_name: None,
            });
        }
    }

    // Check direct http/https subscription URLs
    if without_leading_slashes.starts_with("https://")
        || without_leading_slashes.starts_with("http://")
    {
        let url = without_leading_slashes.to_string();
        let name = derive_name_from_url(&url);
        return Ok(DeepLinkAction::AddSubscription { name, url });
    }

    // Check prefix commands like "sub/https://..."
    if let Some(rest) = without_leading_slashes
        .strip_prefix("sub/")
        .or_else(|| without_leading_slashes.strip_prefix("subscription/"))
        .or_else(|| without_leading_slashes.strip_prefix("add-sub/"))
    {
        let url = if rest.starts_with("https://") || rest.starts_with("http://") {
            rest.to_string()
        } else if rest.starts_with("https%3A%2F%2F") || rest.starts_with("http%3A%2F%2F") {
            url_decode(rest)
        } else {
            format!("https://{rest}")
        };
        let name = derive_name_from_url(&url);
        return Ok(DeepLinkAction::AddSubscription { name, url });
    }

    // Split action and query string
    let (action_part, query_str) = match without_leading_slashes.split_once('?') {
        Some((a, q)) => (a.trim_end_matches('/'), q),
        None => (without_leading_slashes.trim_end_matches('/'), ""),
    };

    // Parse query params
    let mut query_params: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for pair in query_str.split('&').filter(|s| !s.is_empty()) {
        if let Some((k, v)) = pair.split_once('=') {
            query_params.insert(k.to_lowercase(), url_decode(v));
        }
    }

    match action_part {
        "connect" => Ok(DeepLinkAction::Connect),
        "disconnect" => Ok(DeepLinkAction::Disconnect),
        "toggle" => Ok(DeepLinkAction::Toggle),

        "routing" | "happ" | "happ-routing" | "import-routing" => {
            let link = query_params
                .get("link")
                .or_else(|| query_params.get("url"))
                .or_else(|| query_params.get("uri"))
                .cloned()
                .ok_or_else(|| "Missing 'link' parameter in routing deep link".to_string())?;
            let group = query_params.get("group").cloned();
            Ok(DeepLinkAction::ImportHappRouting { link, group })
        }

        "sub" | "subscription" | "add-sub" | "install-sub" | "import-sub" => {
            let url = query_params
                .get("url")
                .or_else(|| query_params.get("link"))
                .or_else(|| query_params.get("sub"))
                .cloned()
                .ok_or_else(|| "Missing 'url' parameter in subscription deep link".to_string())?;

            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Err(format!(
                    "Subscription URL must use http:// or https://: {url}"
                ));
            }

            let name = query_params
                .get("name")
                .or_else(|| query_params.get("title"))
                .cloned()
                .unwrap_or_else(|| derive_name_from_url(&url));

            Ok(DeepLinkAction::AddSubscription { name, url })
        }

        "import" | "add" | "profile" | "node" => {
            let uri = query_params
                .get("uri")
                .or_else(|| query_params.get("link"))
                .or_else(|| query_params.get("url"))
                .cloned()
                .ok_or_else(|| "Missing 'uri' parameter in profile deep link".to_string())?;

            let group_name = query_params.get("group").cloned();
            Ok(DeepLinkAction::AddProfile { uri, group_name })
        }

        _ => {
            // Fallback: If query contains link, check if it's happ routing
            if let Some(link) = query_params.get("link") {
                let link_lower = link.to_lowercase();
                if link_lower.starts_with("happ://routing/")
                    || link_lower.starts_with("happ:routing/")
                {
                    let group = query_params.get("group").cloned();
                    return Ok(DeepLinkAction::ImportHappRouting {
                        link: link.clone(),
                        group,
                    });
                }
            }

            // Fallback: If query contains url, treat as subscription
            if let Some(url) = query_params.get("url").or_else(|| query_params.get("sub")) {
                let name = query_params
                    .get("name")
                    .cloned()
                    .unwrap_or_else(|| derive_name_from_url(url));
                return Ok(DeepLinkAction::AddSubscription {
                    name,
                    url: url.clone(),
                });
            }

            // Fallback: If query contains uri, treat as profile
            if let Some(uri) = query_params.get("uri") {
                let group_name = query_params.get("group").cloned();
                return Ok(DeepLinkAction::AddProfile {
                    uri: uri.clone(),
                    group_name,
                });
            }

            Err(format!("Unknown deep link action: '{action_part}'"))
        }
    }
}

/// Sends a desktop notification using notify-send if available.
pub fn notify_desktop(title: &str, message: &str) {
    let _ = Command::new("notify-send")
        .arg("-a")
        .arg("WhoisThat")
        .arg("-i")
        .arg("preferences-system-network")
        .arg(title)
        .arg(message)
        .spawn();
}

/// Connects to a running whoisthat-core or spawns one if offline.
async fn ensure_core_connected(
    cfg: &config::AppConfig,
) -> Result<CoreConnection, Box<dyn std::error::Error>> {
    let endpoint = cfg.endpoint();
    if let Ok(conn) = CoreConnection::connect_endpoint(&endpoint).await {
        return Ok(conn);
    }

    // Offline: spawn core daemon
    // No pkexec here: deeplinks are launched by the browser, without a TTY.
    core_spawn::spawn_core(&cfg.log_level)?;

    let mut retries = 0;
    while retries < 30 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Ok(conn) = CoreConnection::connect_endpoint(&endpoint).await {
            return Ok(conn);
        }
        retries += 1;
    }

    Err("Timed out waiting for whoisthat-core to start".into())
}

/// Executes the given deep link action against whoisthat-core.
pub async fn run_deeplink_action(action: DeepLinkAction) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load_config();

    match action {
        DeepLinkAction::AddSubscription { name, url } => {
            // The URL carries provider tokens: never echo it to stdout/desktop.
            println!("Adding subscription '{name}'...");
            let conn = ensure_core_connected(&cfg).await?;
            let (mut read_half, mut write_half) = conn.into_split();

            write_half
                .send(
                    "add-group",
                    &AddGroupData {
                        name: name.clone(),
                        subscription_url: url.clone(),
                    },
                )
                .await?;

            // Wait for response from core:
            // 1. "group-added" confirms creation
            // 2. "subscription-updated" confirms profiles were fetched
            let mut group_id: Option<i32> = None;
            let mut profiles_count: Option<usize> = None;
            let mut warning_msg: Option<String> = None;

            let timeout = Duration::from_secs(12);
            let start = tokio::time::Instant::now();

            while start.elapsed() < timeout {
                let remaining = timeout.saturating_sub(start.elapsed());
                let msg = match tokio::time::timeout(remaining, read_half.recv()).await {
                    Ok(Ok(m)) => m,
                    _ => break,
                };

                match msg.msg.as_str() {
                    "group-added" => {
                        if let Ok(g) = serde_json::from_value::<GroupAdded>(msg.data) {
                            group_id = Some(g.id);
                        }
                    }
                    "subscription-updated" => {
                        if let Ok(sub) = serde_json::from_value::<SubscriptionUpdated>(msg.data) {
                            if group_id.is_none() || group_id == Some(sub.group_id) {
                                profiles_count = Some(sub.profiles.len());
                                break;
                            }
                        }
                    }
                    "warn" => {
                        if let Ok(w) = serde_json::from_value::<Warning>(msg.data) {
                            if w.key.contains("subscription") || w.key.contains("add-group") {
                                warning_msg = Some(w.content);
                                break;
                            }
                        }
                    }
                    _ => {}
                }
            }

            if let Some(count) = profiles_count {
                let msg = format!("Subscription '{name}' added ({count} profiles imported)");
                println!("✓ {msg}");
                notify_desktop("WhoisThat: Subscription Added", &msg);
            } else if let Some(warn) = warning_msg {
                let msg = format!("Subscription '{name}' added, but update reported: {warn}");
                println!("⚠ {msg}");
                notify_desktop("WhoisThat: Warning", &msg);
            } else if group_id.is_some() {
                let msg = format!("Subscription '{name}' added (updating in background...)");
                println!("✓ {msg}");
                notify_desktop("WhoisThat: Subscription Added", &msg);
            } else {
                let msg = format!("Request sent to core to add subscription '{name}'");
                println!("✓ {msg}");
                notify_desktop("WhoisThat", &msg);
            }
        }

        DeepLinkAction::AddProfile { uri, group_name } => {
            println!("Importing profile URI...");
            let conn = ensure_core_connected(&cfg).await?;
            let (mut read_half, mut write_half) = conn.into_split();

            // Fetch state to find target group
            write_half
                .send("get-application-state", &GetApplicationStateData {})
                .await?;

            let mut target_gid = 1;
            let groups = recv_state(&mut read_half).await?.groups;

            if let Some(ref desired_name) = group_name {
                if let Some(found) = groups
                    .iter()
                    .find(|g| g.group.name.eq_ignore_ascii_case(desired_name))
                {
                    target_gid = found.group.id;
                } else {
                    // Create group
                    write_half
                        .send(
                            "add-group",
                            &AddGroupData {
                                name: desired_name.clone(),
                                subscription_url: String::new(),
                            },
                        )
                        .await?;
                    let msg = read_half
                        .recv_until(Duration::from_secs(3), |m| {
                            m.msg == "group-added" || is_warn_key(m, "add-group-failed")
                        })
                        .await?;
                    if msg.msg == "group-added" {
                        target_gid = serde_json::from_value::<GroupAdded>(msg.data)?.id;
                    } else {
                        return Err(warn_content(&msg).into());
                    }
                }
            } else if let Some(first) = groups.first() {
                target_gid = first.group.id;
            }

            write_half
                .send(
                    "add-profiles",
                    &AddProfilesData {
                        uris: uri,
                        group_id: target_gid,
                    },
                )
                .await?;

            let msg = read_half
                .recv_until(Duration::from_secs(3), |m| m.msg == "profiles-added")
                .await?;
            let added_count = serde_json::from_value::<ProfilesAdded>(msg.data)?
                .profiles
                .len();

            let msg = format!("Imported {added_count} profile(s) into group #{target_gid}");
            println!("✓ {msg}");
            notify_desktop("WhoisThat: Profile Imported", &msg);
        }

        DeepLinkAction::Connect => {
            println!("Connecting via deep link...");
            crate::cli::handle_connect(&[]).await?;
            notify_desktop("WhoisThat", "Connecting to VPN...");
        }

        DeepLinkAction::Disconnect => {
            println!("Disconnecting via deep link...");
            crate::cli::handle_disconnect().await?;
            notify_desktop("WhoisThat", "Disconnected from VPN.");
        }

        DeepLinkAction::Toggle => {
            println!("Toggling connection via deep link...");
            crate::cli::handle_toggle().await?;
        }

        DeepLinkAction::ImportHappRouting { link, group } => {
            println!("Importing Happ routing link...");
            let conn = ensure_core_connected(&cfg).await?;
            let (mut read_half, mut write_half) = conn.into_split();

            // Fetch state to discover available groups
            write_half
                .send("get-application-state", &GetApplicationStateData {})
                .await?;

            let groups = recv_state(&mut read_half).await?.groups;

            if groups.is_empty() {
                return Err("No groups found in core. Please add a group first.".into());
            }

            let (target_gid, target_name) = match group {
                Some(ref desired) => {
                    // Try parsing as numeric group ID first
                    let found = if let Ok(id) = desired.parse::<i32>() {
                        groups.iter().find(|g| g.group.id == id)
                    } else {
                        None
                    };
                    let found = found.or_else(|| {
                        groups
                            .iter()
                            .find(|g| g.group.name.eq_ignore_ascii_case(desired))
                    });

                    if let Some(g) = found {
                        (g.group.id, g.group.name.clone())
                    } else {
                        let group_list: Vec<String> = groups
                            .iter()
                            .map(|g| format!("  - [{}] {}", g.group.id, g.group.name))
                            .collect();
                        return Err(format!(
                            "Group '{desired}' not found. Available groups:\n{}",
                            group_list.join("\n")
                        )
                        .into());
                    }
                }
                None => {
                    if groups.len() == 1 {
                        (groups[0].group.id, groups[0].group.name.clone())
                    } else {
                        let group_list: Vec<String> = groups
                            .iter()
                            .map(|g| format!("  - [{}] {}", g.group.id, g.group.name))
                            .collect();
                        return Err(format!(
                            "Multiple groups exist. Please specify target group with --group <id|name>:\n{}",
                            group_list.join("\n")
                        )
                        .into());
                    }
                }
            };

            write_half
                .send(
                    "import-happ-routing",
                    &ImportHappRoutingData {
                        group_id: target_gid,
                        link: link.clone(),
                    },
                )
                .await?;

            let timeout = Duration::from_secs(10);
            let start = tokio::time::Instant::now();
            let mut success = false;
            let mut warn_msg: Option<String> = None;
            let mut profile_name: Option<String> = None;

            while start.elapsed() < timeout {
                let remaining = timeout.saturating_sub(start.elapsed());
                let msg = match tokio::time::timeout(remaining, read_half.recv()).await {
                    Ok(Ok(m)) => m,
                    _ => break,
                };

                match msg.msg.as_str() {
                    "subscription-updated" => {
                        if let Ok(sub) = serde_json::from_value::<SubscriptionUpdated>(msg.data) {
                            if sub.group_id == target_gid {
                                if let Some(active) =
                                    sub.routing_profiles.iter().find(|rp| rp.active)
                                {
                                    profile_name = Some(active.name.clone());
                                }
                                success = true;
                                break;
                            }
                        }
                    }
                    "group-updated" => {
                        if let Ok(g) = serde_json::from_value::<Group>(msg.data) {
                            if g.id == target_gid {
                                success = true;
                                break;
                            }
                        }
                    }
                    "warn" => {
                        if let Ok(w) = serde_json::from_value::<Warning>(msg.data) {
                            if w.key.contains("happ") || w.key.contains("routing") {
                                warn_msg = Some(w.content);
                                break;
                            }
                        }
                    }
                    _ => {}
                }
            }

            if let Some(warn) = warn_msg {
                let msg = format!("Failed to import Happ routing: {warn}");
                eprintln!("⚠ {msg}");
                notify_desktop("WhoisThat: Routing Import Failed", &msg);
                return Err(msg.into());
            } else if success {
                let active_info = if let Some(pn) = profile_name {
                    format!(" (active: {pn})")
                } else {
                    String::new()
                };
                let msg = format!(
                    "Happ routing imported into group '{target_name}' (#{target_gid}){active_info}"
                );
                println!("✓ {msg}");
                notify_desktop("WhoisThat: Routing Imported", &msg);
            } else {
                let msg = format!("Import command sent for group '{target_name}' (#{target_gid})");
                println!("✓ {msg}");
                notify_desktop("WhoisThat: Routing Import Sent", &msg);
            }
        }
    }

    Ok(())
}

/// Parses and executes a deep link URL string.
pub async fn run_deeplink(url_or_link: &str) -> Result<(), Box<dyn std::error::Error>> {
    let action = parse_deeplink(url_or_link).map_err(|e| format!("Invalid deep link: {e}"))?;
    run_deeplink_action(action).await
}

/// Registers the `x-scheme-handler/wit` desktop integration in ~/.local/share/applications/
pub fn register_desktop_handler() -> Result<(), Box<dyn std::error::Error>> {
    let home = std::env::var("HOME").map_err(|_| "HOME environment variable not set")?;
    let apps_dir = std::path::Path::new(&home).join(".local/share/applications");
    std::fs::create_dir_all(&apps_dir)?;

    let desktop_file = apps_dir.join("whoisthat.desktop");
    let current_exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "whoisthat".to_string());

    let content = format!(
        "[Desktop Entry]\n\
         Name=WhoisThat\n\
         GenericName=VPN Client\n\
         Comment=Terminal-based VPN client with Xray-core backend\n\
         Exec={current_exe} handle-url %u\n\
         Icon=preferences-system-network\n\
         Terminal=false\n\
         Type=Application\n\
         Categories=Network;VPN;ConsoleOnly;\n\
         MimeType=x-scheme-handler/wit;\n\
         Keywords=vpn;proxy;xray;vless;vmess;hysteria;\n"
    );

    std::fs::write(&desktop_file, content)?;
    println!("✓ Written desktop entry: {}", desktop_file.display());

    // Register with xdg-mime
    let status = Command::new("xdg-mime")
        .args(["default", "whoisthat.desktop", "x-scheme-handler/wit"])
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("✓ Registered whoisthat.desktop as default handler for x-scheme-handler/wit");
        }
        _ => {
            println!("⚠ Note: 'xdg-mime default whoisthat.desktop x-scheme-handler/wit' returned non-zero (xdg-utils might not be installed)");
        }
    }

    // Try update-desktop-database
    let _ = Command::new("update-desktop-database")
        .arg(&apps_dir)
        .status();

    Ok(())
}

/// Silently ensures the `x-scheme-handler/wit` desktop integration is registered in the background.
/// Only writes and calls xdg-mime if the desktop entry is missing, points to a stale binary,
/// or is not registered as the default scheme handler.
pub fn ensure_desktop_registered() {
    std::thread::spawn(|| {
        let home = match std::env::var("HOME") {
            Ok(h) => h,
            Err(_) => return,
        };
        let apps_dir = std::path::Path::new(&home).join(".local/share/applications");
        let desktop_file = apps_dir.join("whoisthat.desktop");

        let current_exe = std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "whoisthat".to_string());

        // Check if desktop file already exists with current binary
        let needs_write = match std::fs::read_to_string(&desktop_file) {
            Ok(content) => {
                !content.contains(&format!("Exec={current_exe} handle-url %u"))
                    || !content.contains("MimeType=x-scheme-handler/wit;")
            }
            Err(_) => true,
        };

        if needs_write {
            let _ = std::fs::create_dir_all(&apps_dir);
            let content = format!(
                "[Desktop Entry]\n\
                 Name=WhoisThat\n\
                 GenericName=VPN Client\n\
                 Comment=Terminal-based VPN client with Xray-core backend\n\
                 Exec={current_exe} handle-url %u\n\
                 Icon=preferences-system-network\n\
                 Terminal=false\n\
                 Type=Application\n\
                 Categories=Network;VPN;ConsoleOnly;\n\
                 MimeType=x-scheme-handler/wit;\n\
                 Keywords=vpn;proxy;xray;vless;vmess;hysteria;\n"
            );
            let _ = std::fs::write(&desktop_file, content);
            log::info!(
                "Wrote desktop integration entry to {}",
                desktop_file.display()
            );
        }

        // Verify if default handler is registered
        let registered = Command::new("xdg-mime")
            .args(["query", "default", "x-scheme-handler/wit"])
            .output()
            .map(|out| String::from_utf8_lossy(&out.stdout).trim() == "whoisthat.desktop")
            .unwrap_or(false);

        if !registered || needs_write {
            let _ = Command::new("xdg-mime")
                .args(["default", "whoisthat.desktop", "x-scheme-handler/wit"])
                .status();
            let _ = Command::new("update-desktop-database")
                .arg(&apps_dir)
                .status();
            log::info!("Registered whoisthat.desktop as default x-scheme-handler/wit handler");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_subscription_query() {
        let link = "wit://sub?url=https%3A%2F%2Fexample.com%2Fsub%2Ftoken&name=My%20VPN";
        let action = parse_deeplink(link).expect("must parse");
        assert_eq!(
            action,
            DeepLinkAction::AddSubscription {
                name: "My VPN".to_string(),
                url: "https://example.com/sub/token".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_install_sub() {
        let link = "wit://install-sub?url=https://sub.provider.org/clash.yaml";
        let action = parse_deeplink(link).expect("must parse");
        assert_eq!(
            action,
            DeepLinkAction::AddSubscription {
                name: "sub.provider.org".to_string(),
                url: "https://sub.provider.org/clash.yaml".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_direct_https_url() {
        let link = "wit://https://example.com/api/v1/client/subscribe?token=123";
        let action = parse_deeplink(link).expect("must parse");
        assert_eq!(
            action,
            DeepLinkAction::AddSubscription {
                name: "example.com".to_string(),
                url: "https://example.com/api/v1/client/subscribe?token=123".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_sub_path_url() {
        let link = "wit://sub/https://server.net/sub";
        let action = parse_deeplink(link).expect("must parse");
        assert_eq!(
            action,
            DeepLinkAction::AddSubscription {
                name: "server.net".to_string(),
                url: "https://server.net/sub".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_direct_vless_profile() {
        let link =
            "wit://vless://11111111-2222-3333-4444-555555555555@example.com:443?type=tcp#Node1";
        let action = parse_deeplink(link).expect("must parse");
        assert_eq!(
            action,
            DeepLinkAction::AddProfile {
                uri: "vless://11111111-2222-3333-4444-555555555555@example.com:443?type=tcp#Node1"
                    .to_string(),
                group_name: None,
            }
        );
    }

    #[test]
    fn test_parse_import_profile_with_group() {
        let link = "wit://import?uri=ss%3A%2F%2FY2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzQDEuMi4zLjQ6NDQz#Node&group=Special";
        let action = parse_deeplink(link).expect("must parse");
        assert_eq!(
            action,
            DeepLinkAction::AddProfile {
                uri: "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzQDEuMi4zLjQ6NDQz#Node".to_string(),
                group_name: Some("Special".to_string()),
            }
        );
    }

    #[test]
    fn test_parse_commands() {
        assert_eq!(
            parse_deeplink("wit://connect").unwrap(),
            DeepLinkAction::Connect
        );
        assert_eq!(
            parse_deeplink("wit://disconnect").unwrap(),
            DeepLinkAction::Disconnect
        );
        assert_eq!(
            parse_deeplink("wit://toggle").unwrap(),
            DeepLinkAction::Toggle
        );
    }

    #[test]
    fn test_parse_invalid() {
        assert!(parse_deeplink("").is_err());
        assert!(parse_deeplink("http://example.com").is_err());
        assert!(parse_deeplink("wit://unknown-action").is_err());
    }

    #[test]
    fn test_parse_happ_routing_direct() {
        let link = "happ://routing/add/eyJNYW1lIjoiVGVzdCJ9";
        let action = parse_deeplink(link).expect("must parse direct happ link");
        assert_eq!(
            action,
            DeepLinkAction::ImportHappRouting {
                link: link.to_string(),
                group: None,
            }
        );

        let off_link = "happ://routing/off";
        let off_action = parse_deeplink(off_link).expect("must parse off link");
        assert_eq!(
            off_action,
            DeepLinkAction::ImportHappRouting {
                link: off_link.to_string(),
                group: None,
            }
        );
    }

    #[test]
    fn test_parse_happ_routing_with_wit_scheme() {
        let link = "wit://happ://routing/onadd/eyJNYW1lIjoiT25BZGQifQ==";
        let action = parse_deeplink(link).expect("must parse embedded happ link");
        assert_eq!(
            action,
            DeepLinkAction::ImportHappRouting {
                link: "happ://routing/onadd/eyJNYW1lIjoiT25BZGQifQ==".to_string(),
                group: None,
            }
        );
    }

    #[test]
    fn test_parse_happ_routing_query() {
        let link = "wit://routing?link=happ%3A%2F%2Frouting%2Fadd%2Fabc&group=Work";
        let action = parse_deeplink(link).expect("must parse routing query");
        assert_eq!(
            action,
            DeepLinkAction::ImportHappRouting {
                link: "happ://routing/add/abc".to_string(),
                group: Some("Work".to_string()),
            }
        );
    }
}
