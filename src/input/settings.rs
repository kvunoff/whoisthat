use tokio::sync::mpsc;

use crate::config;
use crate::core_client::CoreClient;
use crate::core_client::protocol::SetHwidData;
use crate::core_events::refresh_public_ip;
use crate::core_spawn::find_core_binary;
use crate::event_loop::AppEvent;
use crate::keymap::AppAction;
use crate::logger::{FileLogger, configure_logger};
use crate::systemd::{setup_systemd_service, teardown_systemd_service};
use crate::testing::persist_and_sync_test_config;
use crate::ui::App;
use crate::ui::app::{Focus, Popup};
use crate::ui::settings::next_split_tunnel_mode;

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

pub(super) async fn handle(
    action: AppAction,
    app: &mut App,
    client: &CoreClient,
    cfg: &mut config::AppConfig,
    logger: &'static FileLogger,
    ip_tx: &mpsc::UnboundedSender<AppEvent>,
) -> Option<bool> {
    Some(match action {
        AppAction::SettingsPrev => {
            if app.settings_state.cursor() == 3 {
                let prev_theme = crate::ui::theme::cycle_theme_prev();
                cfg.theme = prev_theme.to_string();
                if save(app, cfg) {
                    app.msg(format!("Theme: {}", crate::ui::theme::current_theme().name));
                }
            }
            false
        }
        AppAction::SettingsNext => {
            if app.settings_state.cursor() == 3 {
                let next_theme = crate::ui::theme::cycle_theme();
                cfg.theme = next_theme.to_string();
                if save(app, cfg) {
                    app.msg(format!("Theme: {}", crate::ui::theme::current_theme().name));
                }
            }
            false
        }
        AppAction::SettingsActivate => {
            match app.settings_state.cursor() {
                0 => {
                    let new_val = !app.autoconnect_enabled;
                    let _ = client
                        .set_autoconnect(
                            new_val,
                            cfg.last_group_id,
                            cfg.last_profile_id,
                            &app.autostart_mode,
                        )
                        .await;
                }
                1 => {
                    let modes = ["proxy", "tun"];
                    let current = modes
                        .iter()
                        .position(|m| *m == app.autostart_mode.as_str())
                        .unwrap_or(0);
                    let next = (current + 1) % modes.len();
                    let new_mode = modes[next].to_string();
                    let _ = client
                        .set_autoconnect(
                            app.autoconnect_enabled,
                            cfg.last_group_id,
                            cfg.last_profile_id,
                            &new_mode,
                        )
                        .await;
                }
                2 => {
                    if !app.systemd_available {
                        app.msg("Systemd autostart unavailable: systemd not detected");
                    } else if app.systemd_enabled {
                        app.msg("Disabling systemd autostart...");
                        let result = tokio::task::spawn_blocking(teardown_systemd_service).await;
                        match result {
                            Ok(Ok(())) => {
                                app.systemd_enabled = false;
                                app.msg("Systemd autostart disabled");
                            }
                            Ok(Err(e)) => app.msg(format!("Error: {}", e)),
                            Err(e) => app.msg(format!("Error: {}", e)),
                        }
                    } else {
                        app.msg("Enabling systemd autostart...");
                        let core_path = find_core_binary();
                        let result =
                            tokio::task::spawn_blocking(move || setup_systemd_service(&core_path))
                                .await;
                        match result {
                            Ok(Ok(())) => {
                                app.systemd_enabled = true;
                                app.msg("Systemd autostart enabled");
                            }
                            Ok(Err(e)) => app.msg(format!("Error: {}", e)),
                            Err(e) => app.msg(format!("Error: {}", e)),
                        }
                    }
                }
                3 => {
                    let next_theme = crate::ui::theme::cycle_theme();
                    cfg.theme = next_theme.to_string();
                    if save(app, cfg) {
                        app.msg(format!("Theme: {}", crate::ui::theme::current_theme().name));
                    }
                }
                4 => {
                    app.show_ip = !app.show_ip;
                    cfg.show_ip = app.show_ip;
                    if !app.show_ip {
                        app.public_ip = String::new();
                        app.public_ipv6 = String::new();
                    } else {
                        // The periodic loop only exists when Show IP was on at
                        // startup; fetch once immediately after enabling it.
                        refresh_public_ip(ip_tx.clone(), std::time::Duration::ZERO);
                    }
                    save(app, cfg);
                }
                5 => {
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
                6 => {
                    let levels = ["error", "warn", "info", "debug", "trace"];
                    let current = levels
                        .iter()
                        .position(|l| *l == app.log_level.as_str())
                        .unwrap_or(1);
                    let next = (current + 1) % levels.len();
                    app.log_level = levels[next].to_string();
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
                7 => {
                    app.popup = Some(Popup::EditTunName {
                        input: app.tun_name.clone(),
                        cursor: app.tun_name.chars().count(),
                    });
                    app.focus = Focus::Popup;
                }
                8 => {
                    // Wait for `kill-switch-updated` before changing what the
                    // UI shows: an unconfirmed optimistic toggle is what made
                    // settings look "reset" after a restart.
                    if let Err(e) = client.set_kill_switch(!app.kill_switch_enabled).await {
                        app.msg(format!("Error: {e}"));
                    }
                }
                9 => {
                    let next = next_split_tunnel_mode(&app.split_tunnel);
                    if let Err(e) = client.set_split_tunnel(next).await {
                        app.msg(format!("Error: {e}"));
                    }
                }
                10 => {
                    let methods = ["tcp", "http-get", "http-head"];
                    let current = methods
                        .iter()
                        .position(|m| *m == app.test_method.as_str())
                        .unwrap_or(1);
                    let next = (current + 1) % methods.len();
                    app.test_method = methods[next].to_string();
                    cfg.test_method = app.test_method.clone();
                    save(app, cfg);
                }
                11 => {
                    let opts = [1, 3, 5, 10];
                    let cur = opts
                        .iter()
                        .position(|&v| v == app.test_config.samples_per_test)
                        .unwrap_or(1);
                    let next = opts[(cur + 1) % opts.len()];
                    app.test_config.samples_per_test = next;
                    cfg.test_samples = next;
                    persist_and_sync_test_config(app, cfg, client).await;
                }
                12 => {
                    let opts = [4, 8, 16, 32, 64];
                    let cur = opts
                        .iter()
                        .position(|&v| v == app.test_config.concurrency)
                        .unwrap_or(2);
                    let next = opts[(cur + 1) % opts.len()];
                    app.test_config.concurrency = next;
                    cfg.test_concurrency = next;
                    persist_and_sync_test_config(app, cfg, client).await;
                }
                13 => {
                    let opts = [3, 5, 10, 15];
                    let cur = opts
                        .iter()
                        .position(|&v| v == app.test_config.timeout_seconds)
                        .unwrap_or(1);
                    let next = opts[(cur + 1) % opts.len()];
                    app.test_config.timeout_seconds = next;
                    cfg.test_timeout_seconds = next;
                    persist_and_sync_test_config(app, cfg, client).await;
                }
                14 => {
                    let opts = [
                        ("cloudflare", "https://cp.cloudflare.com/generate_204"),
                        ("gstatic", "https://www.gstatic.com/generate_204"),
                        ("bing", "https://www.bing.com/"),
                    ];
                    let cur = opts
                        .iter()
                        .position(|(_, url)| *url == app.test_config.test_endpoint)
                        .unwrap_or(0);
                    let next = opts[(cur + 1) % opts.len()];
                    app.test_config.test_endpoint = next.1.to_string();
                    cfg.test_endpoint = next.1.to_string();
                    persist_and_sync_test_config(app, cfg, client).await;
                }
                15 => {
                    app.test_config.auto_test_on_subscribe =
                        !app.test_config.auto_test_on_subscribe;
                    cfg.auto_test_on_subscribe = app.test_config.auto_test_on_subscribe;
                    persist_and_sync_test_config(app, cfg, client).await;
                }
                16 => {
                    if let Some(ref hw) = app.hwid_info {
                        let _ = client
                            .set_hwid(&SetHwidData {
                                enabled: Some(!hw.enabled),
                                ..Default::default()
                            })
                            .await;
                    }
                }
                17 => {}
                18 => {
                    let _ = client
                        .set_hwid(&SetHwidData {
                            reset: true,
                            ..Default::default()
                        })
                        .await;
                }
                19 => {
                    if let Some(ref hw) = app.hwid_info {
                        app.popup = Some(Popup::EditUserAgent {
                            input: hw.user_agent.clone(),
                            cursor: hw.user_agent.chars().count(),
                        });
                        app.focus = Focus::Popup;
                    }
                }
                20 => {
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
                _ => {}
            }
            false
        }
        _ => return None,
    })
}
