use crate::core_client::CoreClient;
use crate::keymap::AppAction;
use crate::testing::{build_test_list, run_test_batch};
use crate::text_edit::read_clipboard;
use crate::ui::App;
use crate::ui::app::{ActiveTab, Focus, ItemMoveAction, Popup, TreeNode};
use crate::ui::routing::RoutingPopup;
pub(super) async fn handle(action: AppAction, app: &mut App, client: &CoreClient) -> Option<bool> {
    Some(match action {
        AppAction::Connect => {
            if let Some(p) = app.selected_profile() {
                let _ = client.connect(p.group_id, p.id).await;
                app.msg("Connecting...");
            } else if let Some(TreeNode::Group(gi)) = app.tree_node_at(app.cursor)
                && let Some(g) = app.groups.get(gi)
            {
                app.toggle_group_collapsed(g.group.id);
            }
            false
        }
        AppAction::ToggleGroup => {
            if let Some(TreeNode::Group(gi)) = app.tree_node_at(app.cursor)
                && let Some(g) = app.groups.get(gi)
            {
                app.toggle_group_collapsed(g.group.id);
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
                return Some(false);
            }
            let clip = read_clipboard();
            if let Some(text) = clip
                && !text.is_empty()
            {
                let gid = app.current_group().map(|g| g.group.id).unwrap_or(0);
                let _ = client.add_profiles(&text, gid).await;
                app.msg("Importing from clipboard...");
                return Some(false);
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
            if let Some(g) = app.current_group()
                && !g.group.subscription_url.is_empty()
            {
                let _ = client.update_subscription(g.group.id).await;
                app.msg("Updating subscription...");
            }
            false
        }
        AppAction::EditItem => {
            if app.on_group() {
                if let Some(g) = app.current_group() {
                    let override_hours = if g.group.sub_update_interval_override > 0 {
                        (g.group.sub_update_interval_override / 3600).to_string()
                    } else {
                        String::new()
                    };
                    app.popup = Some(Popup::EditSubscription {
                        name: g.group.name.clone(),
                        url: g.group.subscription_url.clone(),
                        group_id: g.group.id,
                        cursor: g.group.subscription_url.chars().count(),
                        field: 1,
                        auto_update: !g.group.sub_auto_update_disabled,
                        interval_hours: override_hours,
                        provider_interval_secs: g.group.sub_update_interval,
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
        _ => return None,
    })
}
