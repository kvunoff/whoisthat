use crate::config;
use crate::core_client::CoreClient;
use crate::logger::FileLogger;
use crate::ui::App;
use crate::ui::app::{ActiveTab, Focus, Popup, TreeNode};
use crate::ui::routing::RoutingPopup;

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
