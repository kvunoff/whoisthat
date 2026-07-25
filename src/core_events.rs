use std::time::Duration;

use tokio::sync::mpsc;

use crate::config;
use crate::core_client::CoreClient;
use crate::core_client::CoreEvent;
use crate::core_spawn::{find_core_binary, has_cap_net};
use crate::event_loop::AppEvent;
use crate::net_info::{fetch_public_ip, fetch_public_ipv6};
use crate::ui::App;

pub(crate) async fn handle_core_event(
    app: &mut App,
    client: &CoreClient,
    ev: CoreEvent,
    first_state: &mut bool,
    do_autoconnect: &mut bool,
    cfg: &mut config::AppConfig,
    ip_tx: &mpsc::UnboundedSender<AppEvent>,
) -> bool {
    match ev {
        CoreEvent::ApplicationState(s) => {
            let was_connected = s.connection_status.connection == "connected";
            let already_migrated = app.autoconnect_enabled;
            app.apply_state(s);
            if *first_state {
                *first_state = false;
                if !cfg.autoconnect_migrated
                    && cfg.autoconnect
                    && cfg.last_profile_id != 0
                    && !already_migrated
                {
                    let _ = client
                        .set_autoconnect(true, cfg.last_group_id, cfg.last_profile_id, "proxy")
                        .await;
                    cfg.autoconnect_migrated = true;
                    config::save_config(cfg);
                }
                if *do_autoconnect && !was_connected {
                    *do_autoconnect = false;
                    let gid = cfg.last_group_id;
                    let pid = cfg.last_profile_id;
                    if gid != 0 || pid != 0 {
                        let _ = client.connect(gid, pid).await;
                        app.msg("Autoconnecting...");
                    }
                }
                if app.autoconnect_enabled
                    && app.autostart_mode == "tun"
                    && app.is_connected()
                    && !app.tun_enabled
                {
                    let core_path = find_core_binary();
                    if !has_cap_net(std::path::Path::new(&core_path)) {
                        app.msg(format!(
                            "TUN autostart failed: core binary missing capabilities. Fix: sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep {}",
                            std::path::absolute(&core_path).unwrap_or_else(|_| core_path.into()).display()
                        ));
                    } else {
                        app.msg("TUN autostart failed. Check core.log for details.");
                    }
                }
            }
        }

        CoreEvent::StatusChanged(s) => {
            let was = app.is_connected();
            log::info!(
                "StatusChanged: connected={}, connected_at={}, profile={:?}",
                s.connection,
                s.connected_at,
                s.profile.as_ref().map(|p| (p.id, p.group_id))
            );
            app.connection_status = s;
            if was != app.is_connected() {
                app.clear_msg();
                let tx = ip_tx.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    if let Some(ip) = tokio::task::spawn_blocking(fetch_public_ip)
                        .await
                        .unwrap_or(None)
                    {
                        let _ = tx.send(AppEvent::PublicIp(ip));
                    }
                    if let Some(ip) = tokio::task::spawn_blocking(fetch_public_ipv6)
                        .await
                        .unwrap_or(None)
                    {
                        let _ = tx.send(AppEvent::PublicIpv6(ip));
                    }
                });
            }
            if app.is_connected() {
                if let Some(ref p) = app.connection_status.profile {
                    cfg.last_group_id = p.group_id;
                    cfg.last_profile_id = p.id;
                    config::save_config(cfg);
                    if app.autoconnect_enabled {
                        let _ = client
                            .set_autoconnect(true, p.group_id, p.id, &app.autostart_mode)
                            .await;
                    }
                }
            }
        }

        CoreEvent::ProfilesAdded(p) => {
            app.apply_profiles_added(p);
            app.popup = None;
            app.msg("Ok");
        }

        CoreEvent::ProfilesDeleted(d) => {
            app.apply_profiles_deleted(&d);
            app.popup = None;
            app.msg("Ok");
        }

        CoreEvent::ProfileUpdated(p) => {
            if p.test_result != -2 {
                app.clear_msg();
            }
            app.apply_profile_updated(&p);
        }

        CoreEvent::TunStatusChanged(e) => {
            app.tun_enabled = e;
            app.msg(if e { "Warning: TUN mode active" } else { "Ok" });
            let tx = ip_tx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if let Some(ip) = tokio::task::spawn_blocking(fetch_public_ip)
                    .await
                    .unwrap_or(None)
                {
                    let _ = tx.send(AppEvent::PublicIp(ip));
                }
                if let Some(ip) = tokio::task::spawn_blocking(fetch_public_ipv6)
                    .await
                    .unwrap_or(None)
                {
                    let _ = tx.send(AppEvent::PublicIpv6(ip));
                }
            });
        }

        CoreEvent::IsRootAnswer(root) => {
            if root {
                app.msg("Ok");
                let _ = client.enable_tun().await;
            } else {
                app.msg("Error: no TUN permission. Run: sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep /path/to/whoisthat-core");
            }
        }

        CoreEvent::Warning { content, .. } => {
            app.msg(format!("Warning: {}", content));
        }

        CoreEvent::Error(e) => {
            app.msg(format!("Error: {}", e));
        }

        CoreEvent::TrafficStats(ts) => {
            app.traffic_stats = ts;
        }

        CoreEvent::Disconnected => {
            app.msg("Error: Core disconnected and recovery failed. Press q to quit.");
        }

        CoreEvent::Reconnected => {
            log::info!("Core reconnected; re-syncing state");
            app.msg("Core reconnected. Re-syncing state...");
            let _ = client.get_application_state().await;
            let _ = client.get_routing().await;
        }
        CoreEvent::SubscriptionUpdated { group, profiles } => {
            app.apply_subscription_updated(group, profiles);
            app.msg("Subscription updated");
        }

        CoreEvent::GroupAdded(g) => {
            app.apply_group_added(g);
        }

        CoreEvent::GroupDeleted(id) => {
            app.apply_group_deleted(id);
            app.popup = None;
            app.msg("Group deleted");
        }

        CoreEvent::GroupUpdated(g) => {
            app.apply_group_updated(&g);
            app.msg("Group updated");
        }

        CoreEvent::RoutingUpdated(cfg) => {
            app.routing = cfg;
            let len = app.routing.rules.len();
            if app.routing_cursor >= len && len > 0 {
                app.routing_cursor = len - 1;
            }
        }
        CoreEvent::HwidUpdated(hw) => {
            app.hwid_info = Some(hw);
        }
        CoreEvent::TunNameUpdated(name) => {
            app.tun_name = name;
            cfg.tun_name = app.tun_name.clone();
            config::save_config(cfg);
        }
        CoreEvent::KillSwitchUpdated(enabled) => {
            app.kill_switch_enabled = enabled;
        }
        CoreEvent::SplitTunnelUpdated(mode) => {
            app.split_tunnel = if mode.is_empty() {
                "off".to_string()
            } else {
                mode
            };
        }
        CoreEvent::AutoconnectUpdated(info) => {
            app.autoconnect_enabled = info.enabled;
            app.autostart_mode = info.mode;
        }
        CoreEvent::TestProgress(p) => {
            if p.total == 0 || p.tested >= p.total {
                app.test_progress = None;
                if p.total > 0 {
                    app.msg(format!("Tested {} profiles", p.total));
                }
            } else {
                app.test_progress = Some(p);
            }
        }
        CoreEvent::TestConfigUpdated(c) => {
            app.test_config = c;
        }
    }
    false
}
