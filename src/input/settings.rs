use tokio::sync::mpsc;

use crate::antidpi::{default_packet_for_type, next_noise_type, next_packets};
use crate::autostart::{self, Backend};
use crate::config;
use crate::core_client::CoreClient;
use crate::core_client::protocol::{AntiDPIConfig, SetHwidData};
use crate::core_events::refresh_public_ip;
use crate::event_loop::AppEvent;
use crate::keymap::AppAction;
use crate::logger::{FileLogger, configure_logger};
use crate::settings::registry::{self, GroupId, Kind, SettingDef, SettingsId, Source};
use crate::testing::persist_and_sync_test_config;
use crate::ui::App;
use crate::ui::app::{ActiveTab, Focus, Popup};
use crate::ui::settings::{SettingsTreeRow, next_split_tunnel_mode};

async fn send_anti_dpi(app: &mut App, client: &CoreClient, cfg: AntiDPIConfig) {
    if let Err(e) = client.set_anti_dpi(&cfg).await {
        app.msg(format!("Error: {e}"));
    }
}

fn anti_dpi_unsupported(app: &mut App) {
    app.msg("Running core does not support anti-DPI; restart it with the new binary");
}

/// Persist `cfg` and tell the user when it could not be written.
fn save(app: &mut App, cfg: &config::AppConfig) -> bool {
    match config::try_save_config(cfg) {
        Ok(()) => true,
        Err(e) => {
            app.msg(format!("Config not saved: {e}"));
            false
        }
    }
}

fn current(app: &App) -> &'static SettingDef {
    let compact = app.last_area.width < 60;
    registry::by_id(app.settings_state.current_id(compact))
}

fn unsupported_optional(app: &mut App, def: &SettingDef) -> bool {
    if def.source != Source::CoreOptional {
        return false;
    }
    let missing = match def.group {
        GroupId::AntiDpiMasks => app.anti_dpi.is_none(),
        _ => app.network_settings.is_none(),
    };
    if !missing {
        return false;
    }
    if def.group == GroupId::AntiDpiMasks {
        anti_dpi_unsupported(app);
    } else {
        app.msg("Running core does not support network settings; restart it with the new binary");
    }
    true
}

pub(super) async fn handle(
    action: AppAction,
    app: &mut App,
    client: &CoreClient,
    cfg: &mut config::AppConfig,
    logger: &'static FileLogger,
    ip_tx: &mpsc::UnboundedSender<AppEvent>,
) -> Option<bool> {
    Some(match action {
        AppAction::StartSearch => {
            if app.tab != ActiveTab::Settings {
                return None;
            }
            app.cancel_search();
            app.search_mode = true;
            app.search_query = Some(String::new());
            app.settings_state.search_cursor = 0;
            false
        }
        AppAction::SettingsPrev => {
            let compact = app.last_area.width < 60;
            if !compact && app.focus == Focus::LeftPanel {
                app.settings_state.collapse_or_expand(false);
            } else if current(app).id == SettingsId::Theme {
                let prev_theme = crate::ui::theme::cycle_theme_prev();
                cfg.theme = prev_theme.to_string();
                if save(app, cfg) {
                    app.msg(format!("Theme: {}", crate::ui::theme::current_theme().name));
                }
            }
            false
        }
        AppAction::SettingsNext => {
            let compact = app.last_area.width < 60;
            if !compact && app.focus == Focus::LeftPanel {
                app.settings_state.collapse_or_expand(true);
            } else if current(app).id == SettingsId::Theme {
                let next_theme = crate::ui::theme::cycle_theme();
                cfg.theme = next_theme.to_string();
                if save(app, cfg) {
                    app.msg(format!("Theme: {}", crate::ui::theme::current_theme().name));
                }
            }
            false
        }
        AppAction::SettingsActivate => {
            let compact = app.last_area.width < 60;
            if !compact && app.focus == Focus::LeftPanel {
                let row = app
                    .settings_state
                    .tree_rows()
                    .get(app.settings_state.tree_cursor)
                    .copied();
                app.settings_state.activate_tree();
                if matches!(row, Some(SettingsTreeRow::Group(..))) {
                    app.focus = Focus::RightPanel;
                }
                return Some(false);
            }
            let def = current(app);
            match def.kind {
                Kind::Toggle => apply_toggle(app, cfg, def, client, logger, ip_tx).await,
                Kind::Cycle => apply_cycle(app, cfg, def, client, logger).await,
                Kind::Editable => apply_editable(app, def),
                Kind::Action => apply_action(app, def, client).await,
                Kind::Display => {}
            }
            false
        }
        _ => return None,
    })
}

async fn apply_toggle(
    app: &mut App,
    cfg: &mut config::AppConfig,
    def: &SettingDef,
    client: &CoreClient,
    logger: &'static FileLogger,
    ip_tx: &mpsc::UnboundedSender<AppEvent>,
) {
    if unsupported_optional(app, def) {
        return;
    }
    match def.id {
        SettingsId::Autoconnect => {
            let new_val = !app.autoconnect_enabled;
            let _ = client
                .set_autoconnect(
                    new_val,
                    cfg.last_group_id,
                    cfg.last_profile_id,
                    &app.autoconnect_mode,
                )
                .await;
        }
        SettingsId::ShowIp => {
            app.show_ip = !app.show_ip;
            cfg.show_ip = app.show_ip;
            if !app.show_ip {
                app.public_ip = String::new();
                app.public_ipv6 = String::new();
            } else {
                // The periodic loop only exists when Show IP was on at startup;
                // fetch once immediately after enabling it.
                refresh_public_ip(ip_tx.clone(), std::time::Duration::ZERO);
            }
            save(app, cfg);
        }
        SettingsId::VerboseTuiLog => {
            app.log_enabled = !app.log_enabled;
            cfg.log_enabled = app.log_enabled;
            configure_logger(logger, cfg.log_enabled, &cfg.log_level);
            if save(app, cfg) {
                app.msg(if app.log_enabled {
                    "Verbose TUI logging enabled"
                } else {
                    "TUI logging: warnings and errors only"
                });
            }
        }
        SettingsId::KillSwitch => {
            // Wait for `kill-switch-updated` before changing what the UI shows:
            // an unconfirmed optimistic toggle made settings look "reset".
            if let Err(e) = client.set_kill_switch(!app.kill_switch_enabled).await {
                app.msg(format!("Error: {e}"));
            }
        }
        SettingsId::AutoTestOnRefresh => {
            app.test_config.auto_test_on_subscribe = !app.test_config.auto_test_on_subscribe;
            cfg.auto_test_on_subscribe = app.test_config.auto_test_on_subscribe;
            persist_and_sync_test_config(app, cfg, client).await;
        }
        SettingsId::HwidEnabled => {
            if let Some(ref hw) = app.hwid_info {
                let _ = client
                    .set_hwid(&SetHwidData {
                        enabled: Some(!hw.enabled),
                        ..Default::default()
                    })
                    .await;
            }
        }
        SettingsId::CheckUpdates => {
            app.check_updates = !app.check_updates;
            cfg.check_updates = app.check_updates;
            if save(app, cfg) {
                app.msg(if app.check_updates {
                    "Update check enabled"
                } else {
                    "Update check disabled"
                });
            }
        }
        SettingsId::FragmentEnabled => {
            if let Some(mut next) = app.anti_dpi.clone() {
                next.fragment.enabled = !next.fragment.enabled;
                send_anti_dpi(app, client, next).await;
            }
        }
        SettingsId::NoiseEnabled => {
            if let Some(mut next) = app.anti_dpi.clone() {
                next.noise.enabled = !next.noise.enabled;
                send_anti_dpi(app, client, next).await;
            }
        }
        _ => unreachable!("non-toggle id in apply_toggle"),
    }
}

async fn apply_cycle(
    app: &mut App,
    cfg: &mut config::AppConfig,
    def: &SettingDef,
    client: &CoreClient,
    logger: &'static FileLogger,
) {
    if unsupported_optional(app, def) {
        return;
    }
    match def.id {
        SettingsId::AutoconnectMode => {
            let modes = ["proxy", "tun"];
            let current = modes
                .iter()
                .position(|m| *m == app.autoconnect_mode.as_str())
                .unwrap_or(0);
            let new_mode = modes[(current + 1) % modes.len()].to_string();
            let _ = client
                .set_autoconnect(
                    app.autoconnect_enabled,
                    cfg.last_group_id,
                    cfg.last_profile_id,
                    &new_mode,
                )
                .await;
        }
        SettingsId::Theme => {
            let next_theme = crate::ui::theme::cycle_theme();
            cfg.theme = next_theme.to_string();
            if save(app, cfg) {
                app.msg(format!("Theme: {}", crate::ui::theme::current_theme().name));
            }
        }
        SettingsId::LogLevel => {
            let levels = ["error", "warn", "info", "debug", "trace"];
            let current = levels
                .iter()
                .position(|l| *l == app.log_level.as_str())
                .unwrap_or(1);
            app.log_level = levels[(current + 1) % levels.len()].to_string();
            cfg.log_level = app.log_level.clone();
            let saved = save(app, cfg);
            configure_logger(logger, cfg.log_enabled, &cfg.log_level);
            if let Err(e) = client.set_log_level(&app.log_level).await {
                log::warn!("failed to set core log level: {e}");
                if saved {
                    app.msg(format!("Core log level not applied: {e}"));
                }
            } else if saved {
                app.msg(format!("Log level: {}", app.log_level));
            }
        }
        SettingsId::SplitTunnel => {
            let next = next_split_tunnel_mode(&app.split_tunnel);
            if let Err(e) = client.set_split_tunnel(next).await {
                app.msg(format!("Error: {e}"));
            }
        }
        SettingsId::TestMethod => {
            let methods = ["tcp", "http-get", "http-head"];
            let current = methods
                .iter()
                .position(|m| *m == app.test_method.as_str())
                .unwrap_or(1);
            app.test_method = methods[(current + 1) % methods.len()].to_string();
            cfg.test_method = app.test_method.clone();
            save(app, cfg);
        }
        SettingsId::TestSamples => {
            let opts = [1, 3, 5, 10];
            let cur = opts
                .iter()
                .position(|&v| v == app.test_config.samples_per_test)
                .unwrap_or(1);
            app.test_config.samples_per_test = opts[(cur + 1) % opts.len()];
            cfg.test_samples = app.test_config.samples_per_test;
            persist_and_sync_test_config(app, cfg, client).await;
        }
        SettingsId::TestConcurrency => {
            let opts = [4, 8, 16, 32, 64];
            let cur = opts
                .iter()
                .position(|&v| v == app.test_config.concurrency)
                .unwrap_or(2);
            app.test_config.concurrency = opts[(cur + 1) % opts.len()];
            cfg.test_concurrency = app.test_config.concurrency;
            persist_and_sync_test_config(app, cfg, client).await;
        }
        SettingsId::TestTimeout => {
            let opts = [3, 5, 10, 15];
            let cur = opts
                .iter()
                .position(|&v| v == app.test_config.timeout_seconds)
                .unwrap_or(1);
            app.test_config.timeout_seconds = opts[(cur + 1) % opts.len()];
            cfg.test_timeout_seconds = app.test_config.timeout_seconds;
            persist_and_sync_test_config(app, cfg, client).await;
        }
        SettingsId::TestEndpoint => {
            let opts = [
                ("cloudflare", "https://cp.cloudflare.com/generate_204"),
                ("gstatic", "https://www.gstatic.com/generate_204"),
                ("bing", "https://www.bing.com/"),
            ];
            let cur = opts
                .iter()
                .position(|(_, url)| *url == app.test_config.test_endpoint)
                .unwrap_or(0);
            app.test_config.test_endpoint = opts[(cur + 1) % opts.len()].1.to_string();
            cfg.test_endpoint = app.test_config.test_endpoint.clone();
            persist_and_sync_test_config(app, cfg, client).await;
        }
        SettingsId::FragmentPackets => {
            if let Some(mut next) = app.anti_dpi.clone() {
                next.fragment.packets = next_packets(&next.fragment.packets).to_string();
                send_anti_dpi(app, client, next).await;
            }
        }
        SettingsId::NoiseType => {
            if let Some(mut next) = app.anti_dpi.clone() {
                let noise_type = next_noise_type(&next.noise.noise_type).to_string();
                next.noise.packet = default_packet_for_type(&noise_type).to_string();
                next.noise.noise_type = noise_type;
                send_anti_dpi(app, client, next).await;
            }
        }
        _ => unreachable!("non-cycle id in apply_cycle"),
    }
}

fn apply_editable(app: &mut App, def: &SettingDef) {
    if unsupported_optional(app, def) {
        return;
    }
    let input = match def.id {
        SettingsId::TunName => app.tun_name.clone(),
        SettingsId::UserAgent => app
            .hwid_info
            .as_ref()
            .map(|h| h.user_agent.clone())
            .unwrap_or_default(),
        SettingsId::FragmentLength => app
            .anti_dpi
            .as_ref()
            .map(|a| a.fragment.length.clone())
            .unwrap_or_default(),
        SettingsId::FragmentInterval => app
            .anti_dpi
            .as_ref()
            .map(|a| a.fragment.interval.clone())
            .unwrap_or_default(),
        SettingsId::NoisePacket => app
            .anti_dpi
            .as_ref()
            .map(|a| a.noise.packet.clone())
            .unwrap_or_default(),
        SettingsId::NoiseDelay => app
            .anti_dpi
            .as_ref()
            .map(|a| a.noise.delay.clone())
            .unwrap_or_default(),
        SettingsId::TunMtu => app
            .network_settings
            .as_ref()
            .map(|n| n.tun_mtu.to_string())
            .unwrap_or_default(),
        SettingsId::DnsServers => app
            .network_settings
            .as_ref()
            .map(|n| n.dns_servers.join(", "))
            .unwrap_or_default(),
        _ => unreachable!("non-editable id in apply_editable"),
    };
    let cursor = input.chars().count();
    app.popup = Some(Popup::EditSetting {
        id: def.id,
        input,
        cursor,
    });
    app.focus = Focus::Popup;
}

async fn apply_action(app: &mut App, def: &SettingDef, client: &CoreClient) {
    match def.id {
        SettingsId::Autostart => {
            if !app.autostart_available {
                app.msg("Autostart unavailable: no backend detected");
                return;
            }
            let current = autostart::status();
            let mut order: Vec<Option<Backend>> = vec![None];
            order.extend(current.available.iter().copied().map(Some));
            let active = if current.enabled {
                current.backend
            } else {
                None
            };
            let index = order
                .iter()
                .position(|candidate| *candidate == active)
                .unwrap_or(0);
            let next = order[(index + 1) % order.len()];
            let result = tokio::task::spawn_blocking(move || match next {
                None => autostart::disable(),
                Some(backend) => autostart::enable(backend),
            })
            .await;
            match result {
                Ok(Ok(())) => {
                    let status = autostart::status();
                    app.autostart_backend = status.backend.map(|b| b.name().to_string());
                    app.autostart_enabled = status.enabled;
                    app.autostart_available = !status.available.is_empty();
                    app.msg(match next {
                        None => "Autostart disabled".to_string(),
                        Some(backend) => format!("Autostart: {}", backend.name()),
                    });
                }
                Ok(Err(e)) => app.msg(format!("Error: {e}")),
                Err(e) => app.msg(format!("Error: {e}")),
            }
        }
        SettingsId::ResetHwid => {
            let _ = client
                .set_hwid(&SetHwidData {
                    reset: true,
                    ..Default::default()
                })
                .await;
        }
        _ => unreachable!("non-action id in apply_action"),
    }
}
