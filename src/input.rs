use crossterm::event::{self, Event, KeyCode, KeyEventKind};

use tokio::sync::mpsc;

use crate::config;
use crate::core_client::protocol::SetHwidData;
use crate::core_client::CoreClient;
use crate::core_events::refresh_public_ip;
use crate::core_spawn::find_core_binary;
use crate::event_loop::AppEvent;
use crate::keymap::{resolve_action, AppAction, KeyContext};
use crate::logger::{configure_logger, FileLogger};
use crate::popups::{handle_popup_input, handle_routing_popup_input};
use crate::systemd::{setup_systemd_service, teardown_systemd_service};
use crate::testing::{build_test_list, persist_and_sync_test_config, run_test_batch};
use crate::text_edit::{edit_text_field, read_clipboard};
use crate::ui::app::{ActiveTab, Focus, ItemMoveAction, Popup, TreeNode};
use crate::ui::routing::{rule_to_form, RoutingPopup};
use crate::ui::settings::next_split_tunnel_mode;
use crate::ui::App;

pub(crate) async fn handle_input(
    app: &mut App,
    client: &CoreClient,
    ev: AppEvent,
    cfg: &mut config::AppConfig,
    logger: &'static FileLogger,
    ip_tx: &mpsc::UnboundedSender<AppEvent>,
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

                if app.popup.is_some() {
                    return handle_popup_input(app, client, key).await;
                }

                return handle_normal_input(app, client, key, cfg, logger, ip_tx).await;
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
    ip_tx: &mpsc::UnboundedSender<AppEvent>,
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

    let ctx = match app.tab {
        ActiveTab::Profiles => match app.focus {
            Focus::LeftPanel => KeyContext::ProfilesTree,
            Focus::RightPanel => KeyContext::ProfilesDetails,
            Focus::Popup => KeyContext::Global,
        },
        ActiveTab::Routing => KeyContext::Routing,
        ActiveTab::Settings => KeyContext::Settings,
        ActiveTab::Logs => KeyContext::Logs,
        ActiveTab::Traffic => KeyContext::Traffic,
    };

    let action = resolve_action(ctx, &key);
    match action {
        AppAction::Quit => {
            let _ = client.die().await;
            true
        }
        AppAction::Detach => true,
        AppAction::Help => {
            app.help_scroll = 0;
            app.popup = Some(Popup::Help);
            app.focus = Focus::Popup;
            false
        }
        AppAction::TabSwitcherModal => {
            app.popup = Some(Popup::TabSwitcher {
                cursor: app.tab_index(),
            });
            app.focus = Focus::Popup;
            false
        }
        AppAction::SwitchTab(idx) => {
            let target = App::tab_from_index(idx);
            app.tab = target;
            app.focus = Focus::LeftPanel;
            if target == ActiveTab::Routing {
                app.routing_popup = None;
                app.sync_routing_group_to_connected();
                let gid = app.routing_page_group_id();
                let _ = client.get_routing(gid).await;
            }
            false
        }
        AppAction::NextTab => {
            let target = app.next_tab();
            app.focus = Focus::LeftPanel;
            if target == ActiveTab::Routing {
                app.routing_popup = None;
                app.sync_routing_group_to_connected();
                let gid = app.routing_page_group_id();
                let _ = client.get_routing(gid).await;
            }
            false
        }
        AppAction::PrevTab => {
            let target = app.prev_tab();
            app.focus = Focus::LeftPanel;
            if target == ActiveTab::Routing {
                app.routing_popup = None;
                app.sync_routing_group_to_connected();
                let gid = app.routing_page_group_id();
                let _ = client.get_routing(gid).await;
            }
            false
        }
        AppAction::ToggleTun => {
            if app.tun_enabled {
                let _ = client.disable_tun().await;
            } else {
                let _ = client.is_root().await;
                app.msg("Checking root...");
            }
            false
        }
        AppAction::Disconnect => {
            let _ = client.disconnect().await;
            app.msg("Disconnecting...");
            false
        }
        AppAction::Dismiss => {
            if app.focus == Focus::RightPanel {
                app.focus = Focus::LeftPanel;
            } else if app.search_query.is_some() {
                app.search_query = None;
                app.search_input.clear();
                app.cursor = 0;
                app.clamp_cursor();
            }
            false
        }
        AppAction::ToggleFocus => {
            app.toggle_focus();
            false
        }
        AppAction::CursorDown => {
            match app.tab {
                ActiveTab::Profiles => match app.focus {
                    Focus::LeftPanel => app.cursor_down(),
                    Focus::RightPanel => {
                        app.details_scroll_down(app.details_line_count(), app.details_visible());
                    }
                    Focus::Popup => {}
                },
                ActiveTab::Routing => {
                    let len = app.active_routing_config().rules.len();
                    if len > 0 && app.routing_cursor + 1 < len {
                        app.routing_cursor += 1;
                    }
                }
                ActiveTab::Settings => app.settings_state.cursor_down(),
                ActiveTab::Logs => app.logs_state.scroll_down(),
                ActiveTab::Traffic => {}
            }
            false
        }
        AppAction::CursorUp => {
            match app.tab {
                ActiveTab::Profiles => match app.focus {
                    Focus::LeftPanel => app.cursor_up(),
                    Focus::RightPanel => app.details_scroll_up(),
                    Focus::Popup => {}
                },
                ActiveTab::Routing => {
                    app.routing_cursor = app.routing_cursor.saturating_sub(1);
                }
                ActiveTab::Settings => app.settings_state.cursor_up(),
                ActiveTab::Logs => app.logs_state.scroll_up(),
                ActiveTab::Traffic => {}
            }
            false
        }
        AppAction::CursorLeft => {
            if app.tab == ActiveTab::Profiles {
                match app.focus {
                    Focus::LeftPanel => {
                        if let Some(TreeNode::Group(gi)) = app.tree_node_at(app.cursor) {
                            if let Some(g) = app.groups.get(gi) {
                                app.collapse_group(g.group.id);
                            }
                        } else if let Some(parent_pos) = app.parent_group_cursor() {
                            app.cursor = parent_pos;
                        }
                    }
                    Focus::RightPanel => {
                        app.focus = Focus::LeftPanel;
                    }
                    Focus::Popup => {}
                }
            }
            false
        }
        AppAction::CursorRight => {
            if app.tab == ActiveTab::Profiles {
                match app.focus {
                    Focus::LeftPanel => {
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
                    Focus::RightPanel => {}
                    Focus::Popup => {}
                }
            }
            false
        }
        AppAction::ScrollTop => {
            match app.tab {
                ActiveTab::Profiles => match app.focus {
                    Focus::LeftPanel => app.cursor_top(),
                    Focus::RightPanel => app.details_scroll_top(),
                    Focus::Popup => {}
                },
                ActiveTab::Logs => app.logs_state.scroll_top(),
                _ => {}
            }
            false
        }
        AppAction::ScrollBottom => {
            match app.tab {
                ActiveTab::Profiles => match app.focus {
                    Focus::LeftPanel => app.cursor_bottom(),
                    Focus::RightPanel => {
                        app.details_scroll_bottom(app.details_line_count(), app.details_visible());
                    }
                    Focus::Popup => {}
                },
                ActiveTab::Logs => app.logs_state.scroll_bottom(),
                _ => {}
            }
            false
        }
        AppAction::PageDown => {
            match app.tab {
                ActiveTab::Profiles => match app.focus {
                    Focus::LeftPanel => app.cursor_page_down(10),
                    Focus::RightPanel => {
                        for _ in 0..10 {
                            app.details_scroll_down(
                                app.details_line_count(),
                                app.details_visible(),
                            );
                        }
                    }
                    Focus::Popup => {}
                },
                ActiveTab::Logs => app.logs_state.scroll_page_down(10),
                _ => {}
            }
            false
        }
        AppAction::PageUp => {
            match app.tab {
                ActiveTab::Profiles => match app.focus {
                    Focus::LeftPanel => app.cursor_page_up(10),
                    Focus::RightPanel => {
                        for _ in 0..10 {
                            app.details_scroll_up();
                        }
                    }
                    Focus::Popup => {}
                },
                ActiveTab::Logs => app.logs_state.scroll_page_up(10),
                _ => {}
            }
            false
        }
        AppAction::Connect => {
            if let Some(p) = app.selected_profile() {
                let _ = client.connect(p.group_id, p.id).await;
                app.msg("Connecting...");
            } else if let Some(TreeNode::Group(gi)) = app.tree_node_at(app.cursor) {
                if let Some(g) = app.groups.get(gi) {
                    app.toggle_group_collapsed(g.group.id);
                }
            }
            false
        }
        AppAction::ToggleGroup => {
            if let Some(TreeNode::Group(gi)) = app.tree_node_at(app.cursor) {
                if let Some(g) = app.groups.get(gi) {
                    app.toggle_group_collapsed(g.group.id);
                }
            }
            false
        }
        AppAction::TestFocused => {
            let list = build_test_list(app, true);
            run_test_batch(app, client, &list).await;
            false
        }
        AppAction::TestAll => {
            let list = build_test_list(app, false);
            run_test_batch(app, client, &list).await;
            false
        }
        AppAction::CancelTests => {
            let _ = client.cancel_tests().await;
            app.test_progress = None;
            app.msg("Cancelling in-flight tests...");
            false
        }
        AppAction::AddProfile => {
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
            false
        }
        AppAction::AddGroup => {
            app.popup = Some(Popup::AddGroup {
                url: String::new(),
                cursor: 0,
            });
            app.focus = Focus::Popup;
            false
        }
        AppAction::UpdateSubscription => {
            if let Some(g) = app.current_group() {
                if !g.group.subscription_url.is_empty() {
                    let _ = client.update_subscription(g.group.id).await;
                    app.msg("Updating subscription...");
                }
            }
            false
        }
        AppAction::EditItem => {
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
            false
        }
        AppAction::CopyUri => {
            if let Some(p) = app.selected_profile() {
                match arboard::Clipboard::new() {
                    Ok(mut cb) => match cb.set_text(&p.uri) {
                        Ok(()) => app.msg("Copied profile URI to clipboard"),
                        Err(e) => app.msg(format!("Failed to copy URI: {e}")),
                    },
                    Err(e) => app.msg(format!("Clipboard unavailable: {e}")),
                }
            }
            false
        }
        AppAction::DeleteItem => {
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
            false
        }
        AppAction::DeleteGroup => {
            if let Some(g) = app.current_group() {
                app.popup = Some(Popup::ConfirmDeleteGroup {
                    gid: g.group.id,
                    name: g.group.name.clone(),
                });
                app.focus = Focus::Popup;
            }
            false
        }
        AppAction::MoveItemDown => {
            if app.search_query.is_some() {
                app.msg("Cannot reorder profiles while search filter is active");
            } else if let Some(action) = app.move_item_down() {
                match action {
                    ItemMoveAction::ReorderProfiles {
                        group_id,
                        profile_ids,
                        profile_name,
                        ..
                    } => {
                        app.msg(format!("Moved \"{}\" down", profile_name));
                        let _ = client.reorder_profiles(group_id, profile_ids).await;
                    }
                    ItemMoveAction::ReorderGroups {
                        group_ids,
                        group_name,
                        ..
                    } => {
                        app.msg(format!("Moved group \"{}\" down", group_name));
                        let _ = client.reorder_groups(group_ids).await;
                    }
                }
            }
            false
        }
        AppAction::MoveItemUp => {
            if app.search_query.is_some() {
                app.msg("Cannot reorder profiles while search filter is active");
            } else if let Some(action) = app.move_item_up() {
                match action {
                    ItemMoveAction::ReorderProfiles {
                        group_id,
                        profile_ids,
                        profile_name,
                        ..
                    } => {
                        app.msg(format!("Moved \"{}\" up", profile_name));
                        let _ = client.reorder_profiles(group_id, profile_ids).await;
                    }
                    ItemMoveAction::ReorderGroups {
                        group_ids,
                        group_name,
                        ..
                    } => {
                        app.msg(format!("Moved group \"{}\" up", group_name));
                        let _ = client.reorder_groups(group_ids).await;
                    }
                }
            }
            false
        }
        AppAction::StartSearch => {
            app.search_mode = true;
            app.search_input.clear();
            app.search_query = Some(String::new());
            app.cursor = 0;
            false
        }
        AppAction::AddRoutingRule => {
            app.routing_popup = Some(RoutingPopup::Add {
                match_type: 0,
                value: String::new(),
                outbound: 0,
                cursor: 0,
                field: 0,
            });
            false
        }
        AppAction::EditRoutingRule => {
            if let Some(rule) = app.active_routing_config().rules.get(app.routing_cursor) {
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
            false
        }
        AppAction::DeleteRoutingRule => {
            if app
                .active_routing_config()
                .rules
                .get(app.routing_cursor)
                .is_some()
            {
                app.routing_popup = Some(RoutingPopup::ConfirmDelete {
                    index: app.routing_cursor,
                });
            }
            false
        }
        AppAction::ToggleRuleEnabled => {
            app.ensure_custom_routing();
            let idx = app.routing_cursor;
            if let Some(rule) = app.active_routing_config_mut().rules.get_mut(idx) {
                rule.enabled = !rule.enabled;
                let action = if rule.enabled { "enabled" } else { "disabled" };
                app.msg(format!("Rule {action}"));
                let gid = app.routing_page_group_id();
                let cfg = app.active_routing_config().clone();
                let _ = client.update_routing(gid, &cfg).await;
            }
            false
        }
        AppAction::OpenPresets => {
            app.routing_popup = Some(RoutingPopup::Presets { cursor: 0 });
            false
        }
        AppAction::OpenHappProfiles => {
            if app.routing_selected_group().is_some() {
                app.routing_popup = Some(RoutingPopup::HappProfiles { cursor: 0 });
            } else {
                app.msg("Select a subscription group to manage Happ profiles");
            }
            false
        }
        AppAction::CycleRoutingMode => {
            if let Some(group) = app.routing_selected_group() {
                let gid = group.group.id;
                let cur = match group.group.routing_mode.as_str() {
                    "manual" => "manual",
                    "subscription" => "subscription",
                    _ => "auto",
                };
                let next_mode = match cur {
                    "auto" => "manual",
                    "manual" => "subscription",
                    _ => "auto",
                };
                let _ = client.set_group_routing_mode(gid, next_mode).await;
                app.msg(format!("Group routing mode: {next_mode}"));
            } else {
                app.msg("Select a subscription group to change its routing mode");
            }
            false
        }
        AppAction::ToggleManualRoutingSource => {
            let gid = app.routing_page_group_id();
            if gid == 0 {
                app.msg("Global rules are the shared base; select a group to override them");
            } else {
                let next = if app.routing_source() == "custom" {
                    "inherit"
                } else {
                    "custom"
                };
                let _ = client.set_manual_routing_source(gid, next).await;
                let _ = client.get_routing(gid).await;
                let label = if next == "custom" {
                    "custom group rules"
                } else {
                    "global rules"
                };
                app.msg(format!("Manual routing source: {label}"));
            }
            false
        }
        AppAction::ImportHappLink => {
            let clip = read_clipboard();
            let text = clip.unwrap_or_default().trim().to_string();
            if text.is_empty() {
                app.msg("Clipboard is empty");
            } else {
                let link = if let Some(idx) = text.find("happ://routing/") {
                    let sub = &text[idx..];
                    sub.split_whitespace().next().unwrap_or(sub).to_string()
                } else if let Some(idx) = text.find("wit://") {
                    let sub = &text[idx..];
                    sub.split_whitespace().next().unwrap_or(sub).to_string()
                } else {
                    text
                };

                let gid = app.routing_page_group_id();
                if gid == 0 {
                    app.msg("Select a subscription group to import Happ routing");
                } else {
                    let _ = client.import_happ_routing(gid, &link).await;
                    app.msg("Importing Happ routing from clipboard...");
                }
            }
            false
        }
        AppAction::RoutingGroupPrev => {
            app.routing_group_prev();
            let gid = app.routing_page_group_id();
            let _ = client.get_routing(gid).await;
            false
        }
        AppAction::RoutingGroupNext => {
            app.routing_group_next();
            let gid = app.routing_page_group_id();
            let _ = client.get_routing(gid).await;
            false
        }
        AppAction::SettingsPrev => {
            if app.settings_state.cursor() == 3 {
                let prev_theme = crate::ui::theme::cycle_theme_prev();
                cfg.theme = prev_theme.to_string();
                config::save_config(cfg);
                app.msg(format!("Theme: {}", crate::ui::theme::current_theme().name));
            }
            false
        }
        AppAction::SettingsNext => {
            if app.settings_state.cursor() == 3 {
                let next_theme = crate::ui::theme::cycle_theme();
                cfg.theme = next_theme.to_string();
                config::save_config(cfg);
                app.msg(format!("Theme: {}", crate::ui::theme::current_theme().name));
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
                    config::save_config(cfg);
                    app.msg(format!("Theme: {}", crate::ui::theme::current_theme().name));
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
                    config::save_config(cfg);
                }
                5 => {
                    app.log_enabled = !app.log_enabled;
                    cfg.log_enabled = app.log_enabled;
                    config::save_config(cfg);
                    configure_logger(logger, cfg.log_enabled, &cfg.log_level);
                    app.msg(if app.log_enabled {
                        "Verbose TUI logging enabled"
                    } else {
                        "TUI logging: warnings and errors only"
                    });
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
                    config::save_config(cfg);
                    configure_logger(logger, cfg.log_enabled, &cfg.log_level);
                    if let Err(e) = client.set_log_level(&app.log_level).await {
                        log::warn!("failed to set core log level: {e}");
                        app.msg(format!("Core log level not applied: {e}"));
                    } else {
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
                    config::save_config(cfg);
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
                _ => {}
            }
            false
        }
        AppAction::LogsCycleFilter => {
            app.logs_state.cycle_filter();
            app.msg(format!("Log filter: {}", app.logs_state.filter.label()));
            false
        }
        AppAction::LogsCycleSource => {
            app.logs_state.cycle_source();
            app.msg(format!("Log source: {}", app.logs_state.source.label()));
            false
        }
        AppAction::None => false,
    }
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
                        let gid = app.routing_page_group_id();
                        let _ = client.get_routing(gid).await;
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
                            app.ensure_custom_routing();
                            let now_applied = crate::ui::routing::toggle_preset(
                                app.active_routing_config_mut(),
                                preset,
                            );
                            let action = if now_applied { "applied" } else { "removed" };
                            app.msg(format!("Preset {}: {}", action, preset.title));
                            let gid = app.routing_page_group_id();
                            let cfg = app.active_routing_config().clone();
                            let _ = client.update_routing(gid, &cfg).await;
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
                    let max = app.active_routing_config().rules.len().saturating_sub(1);
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
    // 1. Check click in Top Bar
    let top_bar = app.layout.top_bar;
    if row >= top_bar.y && row < top_bar.bottom() {
        let is_compact = app.layout.width_tier == crate::ui::layout::WidthTier::Compact;
        let total_actions_len = if is_compact { 28 } else { 62 };
        let start_x = top_bar.x + top_bar.width.saturating_sub(total_actions_len + 2);
        if col >= start_x {
            let rel_from_right = top_bar.right().saturating_sub(col);
            let (quit_limit, help_limit) = if is_compact { (8, 16) } else { (18, 33) };
            if rel_from_right <= quit_limit {
                return true;
            } else if rel_from_right <= help_limit {
                app.help_scroll = 0;
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

        // Check click on status line
        let top_x = top_bar.x;
        if col >= top_x + 2 && col <= top_x + 32 {
            if app.is_connected() {
                let _ = client.disconnect().await;
                app.msg("Disconnecting...");
            } else if let Some(p) = app.selected_profile() {
                let _ = client.connect(p.group_id, p.id).await;
                app.msg("Connecting...");
            }
            return false;
        }
        return false;
    }

    // 2. Check click in Main Area
    let main_area = app.layout.main_area;
    if row >= main_area.y && row < main_area.bottom() {
        match app.tab {
            ActiveTab::Profiles => {
                let tree_area = app.layout.tree_area;
                let details_area = app.layout.details_area;
                let in_tree = col >= tree_area.x
                    && col < tree_area.right()
                    && row >= tree_area.y
                    && row < tree_area.bottom();
                let in_details = details_area.width > 0
                    && col >= details_area.x
                    && col < details_area.right()
                    && row >= details_area.y
                    && row < details_area.bottom();

                if in_tree {
                    app.focus = Focus::LeftPanel;
                    let inner_x = tree_area.x + 1;
                    let inner_y = tree_area.y + 1;
                    let inner_w = tree_area.width.saturating_sub(2);
                    let inner_h = tree_area.height.saturating_sub(2);
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
                } else if in_details {
                    app.focus = Focus::RightPanel;
                }
            }
            ActiveTab::Settings => {
                app.focus = Focus::LeftPanel;
                let inner_y = main_area.y + 1;
                let inner_h = main_area.height.saturating_sub(2);
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
                let start_row = main_area.y + 2;
                if row >= start_row && row < main_area.bottom().saturating_sub(2) {
                    let rule_idx = (row - start_row) as usize;
                    if rule_idx < app.active_routing_config().rules.len() {
                        if app.routing_cursor == rule_idx {
                            app.ensure_custom_routing();
                            if let Some(rule) =
                                app.active_routing_config_mut().rules.get_mut(rule_idx)
                            {
                                rule.enabled = !rule.enabled;
                                let gid = app.routing_page_group_id();
                                let cfg = app.active_routing_config().clone();
                                let _ = client.update_routing(gid, &cfg).await;
                            }
                        } else {
                            app.routing_cursor = rule_idx;
                        }
                    }
                } else if row >= main_area.bottom().saturating_sub(2) && row < main_area.bottom() {
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
    let bot_area = app.layout.bottom_bar;
    if row >= bot_area.y && row < bot_area.bottom() {
        let bot_x = bot_area.x;
        if col >= bot_x + 12 && col <= bot_x + 35 {
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
