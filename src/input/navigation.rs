use crate::core_client::CoreClient;
use crate::keymap::AppAction;
use crate::ui::App;
use crate::ui::app::{ActiveTab, Focus, Popup, TreeNode};
pub(super) async fn handle(action: AppAction, app: &mut App, client: &CoreClient) -> Option<bool> {
    Some(match action {
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
        _ => return None,
    })
}
