use std::time::Duration;

use crate::config;
use crate::core_client::connection::CoreConnection;
use crate::core_client::protocol::*;
use crate::core_spawn;

use super::DeepLinkAction;
use super::desktop::notify_desktop;
use super::parse::parse_deeplink;

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
                        if let Ok(sub) = serde_json::from_value::<SubscriptionUpdated>(msg.data)
                            && (group_id.is_none() || group_id == Some(sub.group_id))
                        {
                            profiles_count = Some(sub.profiles.len());
                            break;
                        }
                    }
                    "warn" => {
                        if let Ok(w) = serde_json::from_value::<Warning>(msg.data)
                            && (w.key.contains("subscription") || w.key.contains("add-group"))
                        {
                            warning_msg = Some(w.content);
                            break;
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
            crate::cli::connect_default().await?;
            notify_desktop("WhoisThat", "Connecting to VPN...");
        }

        DeepLinkAction::Disconnect => {
            println!("Disconnecting via deep link...");
            crate::cli::disconnect_default().await?;
            notify_desktop("WhoisThat", "Disconnected from VPN.");
        }

        DeepLinkAction::Toggle => {
            println!("Toggling connection via deep link...");
            crate::cli::toggle_default().await?;
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
                        if let Ok(sub) = serde_json::from_value::<SubscriptionUpdated>(msg.data)
                            && sub.group_id == target_gid
                        {
                            if let Some(active) = sub.routing_profiles.iter().find(|rp| rp.active) {
                                profile_name = Some(active.name.clone());
                            }
                            success = true;
                            break;
                        }
                    }
                    "group-updated" => {
                        if let Ok(g) = serde_json::from_value::<Group>(msg.data)
                            && g.id == target_gid
                        {
                            success = true;
                            break;
                        }
                    }
                    "warn" => {
                        if let Ok(w) = serde_json::from_value::<Warning>(msg.data)
                            && (w.key.contains("happ") || w.key.contains("routing"))
                        {
                            warn_msg = Some(w.content);
                            break;
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
