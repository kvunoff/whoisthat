use crossterm::event::{self, Event, KeyCode, KeyEventKind};

use crate::config;
use crate::core_client::protocol::SetHwidData;
use crate::core_client::CoreClient;
use crate::core_spawn::find_core_binary;
use crate::event_loop::AppEvent;
use crate::logger::{configure_logger, FileLogger};
use crate::popups::{handle_popup_input, handle_routing_popup_input};
use crate::systemd::{setup_systemd_service, teardown_systemd_service};
use crate::testing::{build_test_list, persist_and_sync_test_config, run_test_batch};
use crate::text_edit::{edit_text_field, read_clipboard};
use crate::ui::app::{ActiveTab, Focus, Popup, TreeNode};
use crate::ui::routing::{rule_to_form, RoutingPopup};
use crate::ui::settings::next_split_tunnel_mode;
use crate::ui::App;

pub(crate) async fn handle_input(
    app: &mut App,
    client: &CoreClient,
    ev: AppEvent,
    cfg: &mut config::AppConfig,
    logger: &'static FileLogger,
) -> bool {
    match ev {
        AppEvent::Tick => {
            app.logs_state.poll();
            if !app.is_connected() {
                app.traffic_history.push_zero();
            }
        }
        AppEvent::PublicIp(ip) => {
            if app.show_ip {
                app.public_ip = ip;
            }
        }
        AppEvent::PublicIpv6(ip) => {
            if app.show_ip {
                app.public_ipv6 = ip;
            }
        }
        AppEvent::Input(input) => match input {
            Event::Key(key) => {
                if key.kind == KeyEventKind::Release {
                    return false;
                }

                if key.code == KeyCode::Char('q')
                    && app.popup.is_none()
                    && app.routing_popup.is_none()
                    && !app.search_mode
                    && !matches!(key.modifiers, crossterm::event::KeyModifiers::CONTROL)
                {
                    return true;
                }

                if (key.code == KeyCode::Char('Q')
                    || (key.code == KeyCode::Char('c')
                        && key.modifiers == crossterm::event::KeyModifiers::CONTROL))
                    && app.popup.is_none()
                    && app.routing_popup.is_none()
                    && !app.search_mode
                {
                    let _ = client.die().await;
                    return true;
                }

                if app.popup.is_some() {
                    return handle_popup_input(app, client, key).await;
                }

                return handle_normal_input(app, client, key, cfg, logger).await;
            }
            Event::Mouse(mouse) => {
                return handle_mouse_input(app, client, mouse, cfg, logger).await;
            }
            _ => {}
        },
    }
    false
}

async fn handle_normal_input(
    app: &mut App,
    client: &CoreClient,
    key: event::KeyEvent,
    cfg: &mut config::AppConfig,
    logger: &'static FileLogger,
) -> bool {
    app.clear_msg();

    if app.routing_popup.is_some() {
        handle_routing_popup_input(app, client, key).await;
        return false;
    }

    if app.search_mode && app.focus == Focus::LeftPanel && app.tab == ActiveTab::Profiles {
        match key.code {
            KeyCode::Esc => {
                app.search_mode = false;
                app.search_query = None;
                app.search_input.clear();
                app.cursor = 0;
                app.clamp_cursor();
            }
            KeyCode::Enter => {
                app.search_mode = false;
                if app.search_input.is_empty() {
                    app.search_query = None;
                    app.cursor = 0;
                    app.clamp_cursor();
                }
            }
            KeyCode::Down => app.cursor_down(),
            KeyCode::Up => app.cursor_up(),
            KeyCode::Char(_) | KeyCode::Backspace => {
                let mut cur = app.search_input.chars().count();
                edit_text_field(&mut app.search_input, &mut cur, key);
                app.search_query = Some(app.search_input.clone());
                app.cursor = 0;
            }
            _ => {}
        }
        return false;
    }

    match key.code {
        KeyCode::Tab | KeyCode::BackTab => {
            app.popup = Some(Popup::TabSwitcher {
                cursor: app.tab_index(),
            });
            app.focus = Focus::Popup;
            return false;
        }
        KeyCode::Char('a') => {
            if app.tab == ActiveTab::Routing {
                app.routing_popup = Some(RoutingPopup::Add {
                    match_type: 0,
                    value: String::new(),
                    outbound: 0,
                    cursor: 0,
                    field: 0,
                });
                return false;
            }
            let clip = read_clipboard();
            if let Some(text) = clip {
                if !text.is_empty() {
                    let gid = app.current_group().map(|g| g.group.id).unwrap_or(0);
                    let _ = client.add_profiles(&text, gid).await;
                    app.msg("Importing from clipboard...");
                    return false;
                }
            }
            app.popup = Some(Popup::Import {
                input: String::new(),
                cursor: 0,
            });
            app.focus = Focus::Popup;
            return false;
        }
        KeyCode::Char('l') => {
            app.tab = ActiveTab::Logs;
            app.focus = Focus::LeftPanel;
            return false;
        }
        KeyCode::Char('r') => {
            app.routing_popup = None;
            app.tab = ActiveTab::Routing;
            app.focus = Focus::LeftPanel;
            let _ = client.get_routing().await;
            return false;
        }
        KeyCode::Char('m') => {
            app.tab = ActiveTab::Traffic;
            app.focus = Focus::LeftPanel;
            return false;
        }
        KeyCode::Char('s') => {
            app.tab = ActiveTab::Settings;
            app.focus = Focus::LeftPanel;
            return false;
        }
        KeyCode::Char('v') => {
            if app.tun_enabled {
                let _ = client.disable_tun().await;
            } else {
                let _ = client.is_root().await;
                app.msg("Checking root...");
            }
            return false;
        }
        KeyCode::Char('1') | KeyCode::Esc => {
            app.tab = ActiveTab::Profiles;
            return false;
        }
        KeyCode::Char('2') => {
            app.routing_popup = None;
            app.tab = ActiveTab::Routing;
            app.focus = Focus::LeftPanel;
            let _ = client.get_routing().await;
            return false;
        }
        KeyCode::Char('3') => {
            app.tab = ActiveTab::Logs;
            app.focus = Focus::LeftPanel;
            return false;
        }
        KeyCode::Char('4') => {
            app.tab = ActiveTab::Settings;
            app.focus = Focus::LeftPanel;
            return false;
        }
        KeyCode::Char('5') => {
            app.tab = ActiveTab::Traffic;
            app.focus = Focus::LeftPanel;
            return false;
        }
        KeyCode::Char('h') | KeyCode::Char('?') => {
            app.help_scroll = 0;
            app.popup = Some(Popup::Help);
            app.focus = Focus::Popup;
            return false;
        }
        _ => {}
    }

    if app.tab == ActiveTab::Routing {
        match key.code {
            KeyCode::Char('p') | KeyCode::Char('P') => {
                app.routing_popup = Some(RoutingPopup::Presets { cursor: 0 });
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let len = app.routing.rules.len();
                if len > 0 && app.routing_cursor + 1 < len {
                    app.routing_cursor += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                app.routing_cursor = app.routing_cursor.saturating_sub(1);
            }
            KeyCode::Char('e') | KeyCode::Enter => {
                if let Some(rule) = app.routing.rules.get(app.routing_cursor) {
                    let (mt, val, ob) = rule_to_form(rule);
                    let cursor = val.chars().count();
                    app.routing_popup = Some(RoutingPopup::Edit {
                        index: app.routing_cursor,
                        match_type: mt,
                        value: val,
                        outbound: ob,
                        cursor,
                        field: 0,
                    });
                }
            }
            KeyCode::Char('x') | KeyCode::Delete => {
                if app.routing.rules.get(app.routing_cursor).is_some() {
                    app.routing_popup = Some(RoutingPopup::ConfirmDelete {
                        index: app.routing_cursor,
                    });
                }
            }
            KeyCode::Char(' ') => {
                if let Some(rule) = app.routing.rules.get_mut(app.routing_cursor) {
                    rule.enabled = !rule.enabled;
                    let action = if rule.enabled { "enabled" } else { "disabled" };
                    app.msg(format!("Rule {action}"));
                    let _ = client.update_routing(&app.routing).await;
                }
            }
            _ => {}
        }
        return false;
    }

    if app.tab == ActiveTab::Settings {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.settings_state.cursor_down(),
            KeyCode::Char('k') | KeyCode::Up => app.settings_state.cursor_up(),
            KeyCode::Char(' ') | KeyCode::Enter => match app.settings_state.cursor() {
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
                    if app.systemd_enabled {
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
                        let log_level = cfg.log_level.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            setup_systemd_service(&core_path, &log_level)
                        })
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
                    app.show_ip = !app.show_ip;
                    cfg.show_ip = app.show_ip;
                    if !app.show_ip {
                        app.public_ip = String::new();
                        app.public_ipv6 = String::new();
                    }
                    config::save_config(cfg);
                }
                4 => {
                    app.log_enabled = !app.log_enabled;
                    cfg.log_enabled = app.log_enabled;
                    config::save_config(cfg);
                    configure_logger(logger, cfg.log_enabled, &cfg.log_level);
                }
                5 => {
                    let levels = ["error", "warn", "info", "debug", "trace"];
                    let current = levels
                        .iter()
                        .position(|l| *l == app.log_level.as_str())
                        .unwrap_or(1);
                    let next = (current + 1) % levels.len();
                    app.log_level = levels[next].to_string();
                    cfg.log_level = app.log_level.clone();
                    config::save_config(cfg);
                    configure_logger(logger, cfg.log_enabled, &cfg.log_level);
                }
                6 => {
                    app.popup = Some(Popup::EditTunName {
                        input: app.tun_name.clone(),
                        cursor: app.tun_name.chars().count(),
                    });
                    app.focus = Focus::Popup;
                }
                7 => {
                    app.kill_switch_enabled = !app.kill_switch_enabled;
                    let _ = client.set_kill_switch(app.kill_switch_enabled).await;
                    cfg.kill_switch_enabled = app.kill_switch_enabled;
                    config::save_config(cfg);
                }
                8 => {
                    let next = next_split_tunnel_mode(&app.split_tunnel);
                    app.split_tunnel = next.to_string();
                    let _ = client.set_split_tunnel(next).await;
                }
                9 => {
                    let methods = ["tcp", "http-get", "http-head"];
                    let current = methods
                        .iter()
                        .position(|m| *m == app.test_method.as_str())
                        .unwrap_or(1);
                    let next = (current + 1) % methods.len();
                    app.test_method = methods[next].to_string();
                    cfg.test_method = app.test_method.clone();
                    config::save_config(cfg);
                }
                10 => {
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
                11 => {
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
                12 => {
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
                13 => {
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
                14 => {
                    app.test_config.auto_test_on_subscribe =
                        !app.test_config.auto_test_on_subscribe;
                    cfg.auto_test_on_subscribe = app.test_config.auto_test_on_subscribe;
                    persist_and_sync_test_config(app, cfg, client).await;
                }
                15 => {
                    if let Some(ref hw) = app.hwid_info {
                        let _ = client
                            .set_hwid(&SetHwidData {
                                enabled: Some(!hw.enabled),
                                ..Default::default()
                            })
                            .await;
                    }
                }
                16 => {}
                17 => {
                    let _ = client
                        .set_hwid(&SetHwidData {
                            reset: true,
                            ..Default::default()
                        })
                        .await;
                }
                18 => {
                    if let Some(ref hw) = app.hwid_info {
                        app.popup = Some(Popup::EditUserAgent {
                            input: hw.user_agent.clone(),
                            cursor: hw.user_agent.chars().count(),
                        });
                        app.focus = Focus::Popup;
                    }
                }
                _ => {}
            },
            _ => {}
        }
        return false;
    }

    if app.tab == ActiveTab::Logs {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.logs_state.scroll_down(),
            KeyCode::Char('k') | KeyCode::Up => app.logs_state.scroll_up(),
            KeyCode::Char('g') => app.logs_state.scroll_top(),
            KeyCode::Char('G') => app.logs_state.scroll_bottom(),
            KeyCode::Char('f') | KeyCode::Char('F') => {
                app.logs_state.cycle_filter();
                app.msg(format!("Log filter: {}", app.logs_state.filter.label()));
            }
            _ => {}
        }
        return false;
    }

    if app.tab == ActiveTab::Traffic {
        if key.code == KeyCode::Char('d') {
            let _ = client.disconnect().await;
            app.msg("Disconnecting...");
        }
        return false;
    }

    match app.focus {
        Focus::LeftPanel => match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.cursor_down(),
            KeyCode::Char('k') | KeyCode::Up => app.cursor_up(),
            KeyCode::Char('g') => app.cursor_top(),
            KeyCode::Char('G') => app.cursor_bottom(),
            KeyCode::Char('/') => {
                app.search_mode = true;
                app.search_input.clear();
                app.search_query = Some(String::new());
                app.cursor = 0;
                return false;
            }
            KeyCode::Esc => {
                if app.search_query.is_some() {
                    app.search_query = None;
                    app.search_input.clear();
                    app.cursor = 0;
                    app.clamp_cursor();
                    return false;
                }
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if let Some(TreeNode::Group(gi)) = app.tree_node_at(app.cursor) {
                    if let Some(g) = app.groups.get(gi) {
                        app.collapse_group(g.group.id);
                    }
                } else if let Some(parent_pos) = app.parent_group_cursor() {
                    app.cursor = parent_pos;
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if let Some(TreeNode::Group(gi)) = app.tree_node_at(app.cursor) {
                    if let Some(g) = app.groups.get(gi) {
                        if app.is_group_collapsed(g.group.id) {
                            app.expand_group(g.group.id);
                        } else if !g.profiles.is_empty() {
                            app.cursor_down();
                        }
                    }
                } else if app.selected_profile().is_some() {
                    app.focus = Focus::RightPanel;
                }
            }
            KeyCode::Char(' ') => {
                if let Some(TreeNode::Group(gi)) = app.tree_node_at(app.cursor) {
                    if let Some(g) = app.groups.get(gi) {
                        app.toggle_group_collapsed(g.group.id);
                    }
                }
            }
            KeyCode::Char('c') => {
                if let Some(p) = app.selected_profile() {
                    let _ = client.connect(p.group_id, p.id).await;
                    app.msg("Connecting...");
                }
            }
            KeyCode::Enter => {
                if let Some(p) = app.selected_profile() {
                    let _ = client.connect(p.group_id, p.id).await;
                    app.msg("Connecting...");
                } else if let Some(TreeNode::Group(gi)) = app.tree_node_at(app.cursor) {
                    if let Some(g) = app.groups.get(gi) {
                        app.toggle_group_collapsed(g.group.id);
                    }
                }
            }
            KeyCode::Char('d') => {
                let _ = client.disconnect().await;
                app.msg("Disconnecting...");
            }
            KeyCode::Char('u') => {
                if let Some(g) = app.current_group() {
                    if !g.group.subscription_url.is_empty() {
                        let _ = client.update_subscription(g.group.id).await;
                        app.msg("Updating subscription...");
                    }
                }
            }
            KeyCode::Char('e') => {
                if app.on_group() {
                    if let Some(g) = app.current_group() {
                        app.popup = Some(Popup::EditSubscription {
                            name: g.group.name.clone(),
                            url: g.group.subscription_url.clone(),
                            group_id: g.group.id,
                            cursor: g.group.subscription_url.chars().count(),
                            field: 1,
                        });
                        app.focus = Focus::Popup;
                    }
                } else if let Some(p) = app.selected_profile() {
                    app.popup = Some(Popup::EditProfileName {
                        input: p.name.clone(),
                        cursor: p.name.chars().count(),
                        group_id: p.group_id,
                        profile_id: p.id,
                    });
                    app.focus = Focus::Popup;
                }
            }
            KeyCode::Char('U') => {
                let default_name = format!("Group {}", app.groups.len() + 1);
                app.popup = Some(Popup::AddGroup {
                    name: default_name,
                    url: String::new(),
                    cursor: 0,
                    field: 0,
                });
                app.focus = Focus::Popup;
            }
            KeyCode::Char('t') => {
                let list = build_test_list(app, false);
                run_test_batch(app, client, &list).await;
            }
            KeyCode::Char('T') => {
                let list = build_test_list(app, true);
                run_test_batch(app, client, &list).await;
            }
            KeyCode::Char('C') => {
                let _ = client.cancel_tests().await;
                app.test_progress = None;
                app.msg("Cancelling in-flight tests...");
            }
            KeyCode::Char('y') => {
                if let Some(p) = app.selected_profile() {
                    match arboard::Clipboard::new() {
                        Ok(mut cb) => match cb.set_text(&p.uri) {
                            Ok(()) => app.msg("Copied profile URI to clipboard"),
                            Err(e) => app.msg(format!("Failed to copy URI: {e}")),
                        },
                        Err(e) => app.msg(format!("Clipboard unavailable: {e}")),
                    }
                }
            }
            KeyCode::Char('x') => {
                if let Some(p) = app.selected_profile() {
                    let name = if p.name.is_empty() {
                        if p.address.is_empty() {
                            "Unknown".to_string()
                        } else {
                            p.address.clone()
                        }
                    } else {
                        p.name.clone()
                    };
                    app.popup = Some(Popup::ConfirmDelete {
                        gid: p.group_id,
                        pid: p.id,
                        name,
                    });
                    app.focus = Focus::Popup;
                }
            }
            KeyCode::Char('X') => {
                if let Some(g) = app.current_group() {
                    app.popup = Some(Popup::ConfirmDeleteGroup {
                        gid: g.group.id,
                        name: g.group.name.clone(),
                    });
                    app.focus = Focus::Popup;
                }
            }
            _ => {}
        },
        Focus::RightPanel => match key.code {
            KeyCode::Char('h') | KeyCode::Left => {
                app.focus = Focus::LeftPanel;
            }
            KeyCode::Char('c') | KeyCode::Enter => {
                if let Some(p) = app.selected_profile() {
                    let _ = client.connect(p.group_id, p.id).await;
                    app.msg("Connecting...");
                }
            }
            KeyCode::Char('d') => {
                let _ = client.disconnect().await;
                app.msg("Disconnecting...");
            }
            KeyCode::Char('j') | KeyCode::Down => {
                app.details_scroll_down(app.details_line_count(), app.details_visible());
            }
            KeyCode::Char('k') | KeyCode::Up => {
                app.details_scroll_up();
            }
            KeyCode::Char('g') => {
                app.details_scroll_top();
            }
            KeyCode::Char('G') => {
                app.details_scroll_bottom(app.details_line_count(), app.details_visible());
            }
            KeyCode::Char('t') => {
                let list = build_test_list(app, false);
                run_test_batch(app, client, &list).await;
            }
            KeyCode::Char('T') => {
                let list = build_test_list(app, true);
                run_test_batch(app, client, &list).await;
            }
            KeyCode::Char('y') => {
                if let Some(p) = app.selected_profile() {
                    match arboard::Clipboard::new() {
                        Ok(mut cb) => match cb.set_text(&p.uri) {
                            Ok(()) => app.msg("Copied profile URI to clipboard"),
                            Err(e) => app.msg(format!("Failed to copy URI: {e}")),
                        },
                        Err(e) => app.msg(format!("Clipboard unavailable: {e}")),
                    }
                }
            }
            _ => {}
        },
        Focus::Popup => {}
    }

    false
}

pub(crate) async fn handle_mouse_input(
    app: &mut App,
    client: &CoreClient,
    mouse: crossterm::event::MouseEvent,
    cfg: &mut config::AppConfig,
    logger: &'static FileLogger,
) -> bool {
    use crossterm::event::{MouseButton, MouseEventKind};

    if let Some(Popup::TabSwitcher { cursor: _ }) = app.popup {
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            let pa = crate::ui::app::centered_rect_fixed(56, 9, app.last_area);
            if mouse.column >= pa.x
                && mouse.column < pa.x + pa.width
                && mouse.row >= pa.y
                && mouse.row < pa.y + pa.height
            {
                let inner_y = pa.y + 1;
                if mouse.row >= inner_y && mouse.row < inner_y + 5 {
                    let clicked_tab_idx = (mouse.row - inner_y) as usize;
                    let target = App::tab_from_index(clicked_tab_idx);
                    app.tab = target;
                    app.popup = None;
                    app.focus = Focus::LeftPanel;
                    if target == ActiveTab::Routing {
                        app.routing_popup = None;
                        let _ = client.get_routing().await;
                    }
                    return false;
                }
            } else {
                app.popup = None;
                app.focus = Focus::LeftPanel;
                return false;
            }
        }
        return false;
    }

    if let Some(RoutingPopup::Presets { cursor }) = app.routing_popup {
        match mouse.kind {
            MouseEventKind::ScrollDown => {
                let max = crate::ui::routing::ROUTING_PRESETS.len().saturating_sub(1);
                if cursor < max {
                    app.routing_popup = Some(RoutingPopup::Presets { cursor: cursor + 1 });
                }
                return false;
            }
            MouseEventKind::ScrollUp => {
                app.routing_popup = Some(RoutingPopup::Presets {
                    cursor: cursor.saturating_sub(1),
                });
                return false;
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let popup_h = 22.min(app.last_area.height.saturating_sub(2));
                let popup_w = 84.min(app.last_area.width.saturating_sub(4));
                let start_y = (app.last_area.height.saturating_sub(popup_h)) / 2 + app.last_area.y;
                let start_x = (app.last_area.width.saturating_sub(popup_w)) / 2 + app.last_area.x;
                if mouse.column >= start_x
                    && mouse.column < start_x + popup_w
                    && mouse.row >= start_y
                    && mouse.row < start_y + popup_h
                {
                    let inner_y = start_y + 1;
                    let list_h = popup_h.saturating_sub(3);
                    if mouse.row >= inner_y && mouse.row < inner_y + list_h {
                        let visible_items = (list_h as usize) / 2;
                        let scroll_item = if visible_items > 0 && cursor >= visible_items {
                            cursor - visible_items + 1
                        } else {
                            0
                        };
                        let rel_row = (mouse.row - inner_y) as usize;
                        let clicked_item = scroll_item + rel_row / 2;
                        if clicked_item < crate::ui::routing::ROUTING_PRESETS.len() {
                            let preset = &crate::ui::routing::ROUTING_PRESETS[clicked_item];
                            let now_applied =
                                crate::ui::routing::toggle_preset(&mut app.routing, preset);
                            let action = if now_applied { "applied" } else { "removed" };
                            app.msg(format!("Preset {}: {}", action, preset.title));
                            let _ = client.update_routing(&app.routing).await;
                            app.routing_popup = Some(RoutingPopup::Presets {
                                cursor: clicked_item,
                            });
                        }
                    }
                } else {
                    app.routing_popup = None;
                }
                return false;
            }
            _ => return false,
        }
    }

    if app.popup.is_some() || app.routing_popup.is_some() {
        return false;
    }

    match mouse.kind {
        MouseEventKind::ScrollDown => {
            match app.tab {
                ActiveTab::Profiles => {
                    app.cursor_down();
                }
                ActiveTab::Logs => {
                    app.logs_state.scroll_down();
                }
                ActiveTab::Settings => {
                    app.settings_state.cursor_down();
                }
                ActiveTab::Routing => {
                    let max = app.routing.rules.len().saturating_sub(1);
                    if app.routing_cursor < max {
                        app.routing_cursor += 1;
                    }
                }
                ActiveTab::Traffic => {}
            }
            false
        }
        MouseEventKind::ScrollUp => {
            match app.tab {
                ActiveTab::Profiles => {
                    app.cursor_up();
                }
                ActiveTab::Logs => {
                    app.logs_state.scroll_up();
                }
                ActiveTab::Settings => {
                    app.settings_state.cursor_up();
                }
                ActiveTab::Routing => {
                    if app.routing_cursor > 0 {
                        app.routing_cursor -= 1;
                    }
                }
                ActiveTab::Traffic => {}
            }
            false
        }
        MouseEventKind::Down(MouseButton::Left) => {
            handle_mouse_left_click(app, client, mouse.column, mouse.row, cfg, logger).await
        }
        _ => false,
    }
}

async fn handle_mouse_left_click(
    app: &mut App,
    client: &CoreClient,
    col: u16,
    row: u16,
    _cfg: &mut config::AppConfig,
    _logger: &'static FileLogger,
) -> bool {
    let top_y = app.last_area.y;
    let top_h = 4;

    // 1. Check click in Top Bar
    if row >= top_y && row < top_y + top_h {
        let total_actions_len = 62;
        let start_x = app.last_area.x + app.last_area.width.saturating_sub(total_actions_len + 2);
        if col >= start_x {
            let rel_from_right = (app.last_area.x + app.last_area.width).saturating_sub(col);
            if rel_from_right <= 18 {
                return true;
            } else if rel_from_right <= 33 {
                app.popup = Some(Popup::Help);
                app.focus = Focus::Popup;
                return false;
            } else {
                app.popup = Some(Popup::TabSwitcher {
                    cursor: app.tab_index(),
                });
                app.focus = Focus::Popup;
                return false;
            }
        }

        // Check row 1 (status line: " WhoisThat │ ● Connected ...")
        if row == top_y + 1 {
            let top_x = app.last_area.x;
            if col >= top_x + 12 && col <= top_x + 32 {
                if app.is_connected() {
                    let _ = client.disconnect().await;
                    app.msg("Disconnecting...");
                } else if let Some(p) = app.selected_profile() {
                    let _ = client.connect(p.group_id, p.id).await;
                    app.msg("Connecting...");
                }
                return false;
            }
        }
        return false;
    }

    // 2. Check click in Main Area
    let main_y = top_y + 4;
    let main_h = app.last_area.height.saturating_sub(7);
    if row >= main_y && row < main_y + main_h {
        match app.tab {
            ActiveTab::Profiles => {
                let main_w = app.last_area.width;
                let tree_w = (main_w * 55) / 100;
                let tree_x = app.last_area.x;
                if col < tree_x + tree_w {
                    app.focus = Focus::LeftPanel;
                    let inner_x = tree_x + 1;
                    let inner_y = main_y + 1;
                    let inner_w = tree_w.saturating_sub(2);
                    let inner_h = main_h.saturating_sub(2);
                    if col >= inner_x
                        && col < inner_x + inner_w
                        && row >= inner_y
                        && row < inner_y + inner_h
                    {
                        let row_offset = (row - inner_y) as usize;
                        let target_idx = app.tree_scroll + row_offset;
                        if target_idx < app.tree_len() {
                            if app.cursor == target_idx {
                                // Clicked on already selected item
                                if let Some(TreeNode::Group(gi)) = app.tree_node_at(target_idx) {
                                    if let Some(g) = app.groups.get(gi) {
                                        app.toggle_group_collapsed(g.group.id);
                                    }
                                } else if let Some(p) = app.selected_profile() {
                                    let _ = client.connect(p.group_id, p.id).await;
                                    app.msg("Connecting...");
                                }
                            } else {
                                app.cursor = target_idx;
                            }
                        }
                    }
                } else {
                    app.focus = Focus::RightPanel;
                }
            }
            ActiveTab::Settings => {
                app.focus = Focus::LeftPanel;
                let inner_y = main_y + 1;
                let inner_h = main_h.saturating_sub(2);
                if row >= inner_y && row < inner_y + inner_h {
                    let row_offset = (row - inner_y) as usize;
                    let target_flat = app.settings_state.scroll + row_offset;
                    let layout = crate::ui::settings::settings_layout();
                    if let Some(crate::ui::settings::SettingsRow::Item { .. }) =
                        layout.get(target_flat)
                    {
                        let item_idx = layout[..=target_flat]
                            .iter()
                            .filter(|r| matches!(r, crate::ui::settings::SettingsRow::Item { .. }))
                            .count()
                            .saturating_sub(1);
                        app.settings_state.item_cursor = item_idx;
                    }
                }
            }
            ActiveTab::Routing => {
                app.focus = Focus::LeftPanel;
                let is_hy2 = app.is_connected_hy2();
                let start_row = if is_hy2 { main_y + 3 } else { main_y + 2 };
                if row >= start_row && row < main_y + main_h.saturating_sub(2) {
                    let rule_idx = (row - start_row) as usize;
                    if rule_idx < app.routing.rules.len() {
                        if app.routing_cursor == rule_idx {
                            if let Some(rule) = app.routing.rules.get_mut(rule_idx) {
                                rule.enabled = !rule.enabled;
                                let _ = client.update_routing(&app.routing).await;
                            }
                        } else {
                            app.routing_cursor = rule_idx;
                        }
                    }
                } else if row >= main_y + main_h.saturating_sub(2) && row < main_y + main_h {
                    app.routing_popup = Some(RoutingPopup::Presets { cursor: 0 });
                }
            }
            ActiveTab::Logs => {
                app.focus = Focus::LeftPanel;
            }
            ActiveTab::Traffic => {}
        }
        return false;
    }

    // 3. Check click in Bottom Bar
    let bot_y = app.last_area.bottom().saturating_sub(3);
    if row >= bot_y {
        let bot_x = app.last_area.x;
        if col >= bot_x + 22 && col <= bot_x + 35 {
            if app.tun_enabled {
                let _ = client.disable_tun().await;
            } else {
                let _ = client.is_root().await;
                app.msg("Checking root...");
            }
        }
        return false;
    }

    false
}
