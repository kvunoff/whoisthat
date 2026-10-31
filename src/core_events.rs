use std::time::Duration;

use tokio::sync::mpsc;

use crate::config;
use crate::core_client::CoreClient;
use crate::core_client::CoreEvent;
use crate::core_spawn::{find_core_binary, has_cap_net};
use crate::event_loop::AppEvent;
use crate::net_info::{fetch_public_ip, fetch_public_ipv6};
use crate::ui::App;

/// Fetch the public IPv4/IPv6 pair once and push it into the UI event stream.
/// Skipped entirely while "Show IP" is off: the old code queried ipify even
/// when the address was never rendered.
pub(crate) fn refresh_public_ip(tx: mpsc::UnboundedSender<AppEvent>, delay: Duration) {
    tokio::spawn(async move {
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
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
            let config_state = s.config_status.state.clone();
            let db_state = s.db_status.state.clone();
            let db_day = s.db_status.snapshot_at.clone();
            if !s.core_version.is_empty() && cfg.core_version != s.core_version {
                cfg.core_version = s.core_version.clone();
                config::save_config(cfg);
            }
            app.apply_state(*s);
            // Do not push recovered/daemon values into the offline cache while
            // the core's own settings file is damaged: that would make the
            // loss permanent. The user restores first, then the cache follows.
            let config_usable = config_state != "corrupt" && config_state != "orphaned";
            if !app.recovery_prompted {
                let needs_config = config_state == "corrupt" || config_state == "orphaned";
                let needs_db = !db_state.is_empty() && db_state != "ok";
                if needs_config || needs_db {
                    app.recovery_prompted = true;
                    app.popup = Some(crate::ui::app::Popup::Recovery {
                        config: needs_config,
                        db: needs_db,
                        day: db_day,
                    });
                }
            }
            // Mirror daemon-authoritative settings into the offline cache.
            let mut cfg_changed = false;
            if config_usable {
                if cfg.tun_name != app.tun_name {
                    cfg.tun_name = app.tun_name.clone();
                    cfg_changed = true;
                }
                if cfg.kill_switch_enabled != app.kill_switch_enabled {
                    cfg.kill_switch_enabled = app.kill_switch_enabled;
                    cfg_changed = true;
                }
                if cfg.test_concurrency != app.test_config.concurrency {
                    cfg.test_concurrency = app.test_config.concurrency;
                    cfg_changed = true;
                }
                if cfg.test_timeout_seconds != app.test_config.timeout_seconds {
                    cfg.test_timeout_seconds = app.test_config.timeout_seconds;
                    cfg_changed = true;
                }
                if cfg.test_samples != app.test_config.samples_per_test {
                    cfg.test_samples = app.test_config.samples_per_test;
                    cfg_changed = true;
                }
                if cfg.test_endpoint != app.test_config.test_endpoint {
                    cfg.test_endpoint = app.test_config.test_endpoint.clone();
                    cfg_changed = true;
                }
                if cfg.auto_test_on_subscribe != app.test_config.auto_test_on_subscribe {
                    cfg.auto_test_on_subscribe = app.test_config.auto_test_on_subscribe;
                    cfg_changed = true;
                }
                if cfg_changed {
                    config::save_config(cfg);
                }
            }
            if *first_state {
                *first_state = false;
                if config_usable
                    && !cfg.autoconnect_migrated
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
            app.connection_status = *s;
            if was != app.is_connected() {
                app.clear_msg();
                if app.show_ip {
                    refresh_public_ip(ip_tx.clone(), Duration::from_secs(1));
                }
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
            if app.show_ip {
                refresh_public_ip(ip_tx.clone(), Duration::from_secs(1));
            }
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
            log::warn!("core warning: {content}");
            app.msg(format!("Warning: {}", content));
        }

        CoreEvent::Error(e) => {
            log::error!("core error: {e}");
            app.msg(format!("Error: {}", e));
        }

        CoreEvent::TrafficStats(ts) => {
            app.traffic_history.push(&ts);
            app.traffic_stats = ts;
        }

        CoreEvent::Disconnected => {
            log::error!("core disconnected and recovery failed");
            app.msg("Error: Core disconnected and recovery failed. Press q to quit.");
        }

        CoreEvent::Reconnected => {
            log::info!("Core reconnected; re-syncing state");
            app.msg("Core reconnected. Re-syncing state...");
            // A respawned core starts with the level from WHOISTHAT_LOG_LEVEL;
            // re-apply the current setting so a runtime change survives.
            let _ = client.set_log_level(&cfg.log_level).await;
            let _ = client.get_application_state().await;
            let _ = client.get_routing(0).await;
        }
        CoreEvent::SubscriptionUpdated {
            group,
            profiles,
            routing_profiles,
        } => {
            app.apply_subscription_updated(group, profiles, routing_profiles);
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

        CoreEvent::ProfilesReordered {
            group_id,
            profile_ids,
        } => {
            app.apply_profiles_reordered(group_id, &profile_ids);
        }

        CoreEvent::GroupsReordered { group_ids } => {
            app.apply_groups_reordered(&group_ids);
        }

        CoreEvent::ProfileMoved {
            old_profile,
            new_profile,
        } => {
            app.apply_profile_moved(old_profile, new_profile);
            app.popup = None;
        }

        CoreEvent::RoutingUpdated {
            group_id,
            config,
            source,
            custom,
        } => {
            app.apply_routing_updated(group_id, config, source, custom);
            if app.is_connected() && app.show_ip {
                refresh_public_ip(ip_tx.clone(), Duration::from_millis(600));
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
            // The core may reject the change (e.g. include-mode split tunnel);
            // keep config.toml in sync with the authoritative value.
            if cfg.kill_switch_enabled != enabled {
                cfg.kill_switch_enabled = enabled;
                config::save_config(cfg);
            }
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
        CoreEvent::XrayProgress(p) => {
            let ver = if p.version.is_empty() {
                "core"
            } else {
                &p.version
            };
            if p.status == "downloading" {
                app.msg(format!("Downloading Xray {} ({}%)...", ver, p.progress));
            } else if p.status == "ready" {
                app.msg(format!("Xray {} ready", ver));
            } else if p.status == "error" && !p.error.is_empty() {
                app.msg(format!("Xray download error: {}", p.error));
            }

            if let Some(ref mut st) = app.xray_status {
                st.status = p.status;
                st.version = p.version;
                st.progress = p.progress;
                st.bytes_downloaded = p.bytes_downloaded;
                st.total_bytes = p.total_bytes;
                st.error = p.error;
            } else {
                app.xray_status = Some(crate::core_client::protocol::XrayStatusInfo {
                    status: p.status,
                    version: p.version,
                    progress: p.progress,
                    bytes_downloaded: p.bytes_downloaded,
                    total_bytes: p.total_bytes,
                    error: p.error,
                    ..Default::default()
                });
            }
        }
    }
    false
}
