use crate::core_client::CoreClient;
use crate::keymap::AppAction;
use crate::text_edit::read_clipboard;
use crate::ui::App;
use crate::ui::routing::{RoutingPopup, rule_to_form};
pub(super) async fn handle(action: AppAction, app: &mut App, client: &CoreClient) -> Option<bool> {
    Some(match action {
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
            if app
                .routing_selected_group()
                .map(|g| g.group.subscription_format == "json")
                .unwrap_or(false)
            {
                app.msg("Provider routing is immutable for JSON subscriptions");
            } else if app.routing_selected_group().is_some() {
                app.routing_popup = Some(RoutingPopup::HappProfiles { cursor: 0 });
            } else {
                app.msg("Select a subscription group to manage Happ profiles");
            }
            false
        }
        AppAction::CycleRoutingMode => {
            if app
                .routing_selected_group()
                .map(|g| g.group.subscription_format == "json")
                .unwrap_or(false)
            {
                app.msg("Routing mode is provider-managed for JSON subscriptions");
            } else if let Some(group) = app.routing_selected_group() {
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
                let is_json = app
                    .routing_selected_group()
                    .map(|g| g.group.subscription_format == "json")
                    .unwrap_or(false);
                if gid == 0 {
                    app.msg("Select a subscription group to import Happ routing");
                } else if is_json {
                    app.msg("Provider routing is immutable for JSON subscriptions");
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
        _ => return None,
    })
}
