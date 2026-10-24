use crossterm::event::{self, KeyCode};

use crate::core_client::protocol::{ProfileID, SetHwidData};
use crate::core_client::CoreClient;
use crate::text_edit::edit_text_field;
use crate::ui::app::{ActiveTab, Focus, Popup};
use crate::ui::routing::{form_to_rule, toggle_preset, RoutingPopup, ROUTING_PRESETS};
use crate::ui::App;

pub(crate) async fn handle_popup_input(
    app: &mut App,
    client: &CoreClient,
    key: event::KeyEvent,
) -> bool {
    match app.popup.take() {
        Some(Popup::TabSwitcher { mut cursor }) => match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                app.popup = None;
                app.focus = Focus::LeftPanel;
            }
            KeyCode::Char('k') | KeyCode::Up | KeyCode::BackTab => {
                cursor = if cursor == 0 { 4 } else { cursor - 1 };
                app.popup = Some(Popup::TabSwitcher { cursor });
            }
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Tab => {
                cursor = (cursor + 1) % 5;
                app.popup = Some(Popup::TabSwitcher { cursor });
            }
            KeyCode::Enter => {
                let target = App::tab_from_index(cursor);
                app.tab = target;
                app.popup = None;
                app.focus = Focus::LeftPanel;
                if target == ActiveTab::Routing {
                    app.routing_popup = None;
                    let _ = client.get_routing().await;
                }
            }
            KeyCode::Char('1') => {
                app.tab = ActiveTab::Profiles;
                app.popup = None;
                app.focus = Focus::LeftPanel;
            }
            KeyCode::Char('2') | KeyCode::Char('r') => {
                app.tab = ActiveTab::Routing;
                app.popup = None;
                app.focus = Focus::LeftPanel;
                app.routing_popup = None;
                let _ = client.get_routing().await;
            }
            KeyCode::Char('3') | KeyCode::Char('m') => {
                app.tab = ActiveTab::Traffic;
                app.popup = None;
                app.focus = Focus::LeftPanel;
            }
            KeyCode::Char('4') | KeyCode::Char('l') => {
                app.tab = ActiveTab::Logs;
                app.popup = None;
                app.focus = Focus::LeftPanel;
            }
            KeyCode::Char('5') | KeyCode::Char('s') => {
                app.tab = ActiveTab::Settings;
                app.popup = None;
                app.focus = Focus::LeftPanel;
            }
            _ => {
                app.popup = Some(Popup::TabSwitcher { cursor });
            }
        },
        Some(Popup::Help) => match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                app.help_scroll = app.help_scroll.saturating_add(1);
                app.popup = Some(Popup::Help);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                app.help_scroll = app.help_scroll.saturating_sub(1);
                app.popup = Some(Popup::Help);
            }
            _ => {
                app.popup = None;
                app.focus = Focus::LeftPanel;
            }
        },
        Some(Popup::Import {
            mut input,
            mut cursor,
        }) => match key.code {
            KeyCode::Esc => {
                app.popup = None;
                app.focus = Focus::LeftPanel;
            }
            KeyCode::Enter => {
                let uris = input.trim().to_string();
                if !uris.is_empty() {
                    let gid = app.current_group().map(|g| g.group.id).unwrap_or(0);
                    let _ = client.add_profiles(&uris, gid).await;
                    app.msg("Importing...");
                    app.focus = Focus::LeftPanel;
                } else {
                    app.popup = None;
                    app.focus = Focus::LeftPanel;
                }
            }
            _ => {
                edit_text_field(&mut input, &mut cursor, key);
                app.popup = Some(Popup::Import { input, cursor });
            }
        },
        Some(Popup::EditUserAgent {
            mut input,
            mut cursor,
        }) => match key.code {
            KeyCode::Esc => {
                app.popup = None;
                app.focus = Focus::LeftPanel;
            }
            KeyCode::Enter => {
                let ua = input.trim().to_string();
                if !ua.is_empty() {
                    let _ = client
                        .set_hwid(&SetHwidData {
                            user_agent: Some(ua),
                            ..Default::default()
                        })
                        .await;
                }
                app.popup = None;
                app.focus = Focus::LeftPanel;
            }
            _ => {
                edit_text_field(&mut input, &mut cursor, key);
                app.popup = Some(Popup::EditUserAgent { input, cursor });
            }
        },
        Some(Popup::EditTunName {
            mut input,
            mut cursor,
        }) => match key.code {
            KeyCode::Esc => {
                app.popup = None;
                app.focus = Focus::LeftPanel;
            }
            KeyCode::Enter => {
                let name = input.trim().to_string();
                if !name.is_empty() {
                    app.tun_name = name.clone();
                    let _ = client.set_tun_name(&name).await;
                }
                app.popup = None;
                app.focus = Focus::LeftPanel;
            }
            _ => {
                edit_text_field(&mut input, &mut cursor, key);
                app.popup = Some(Popup::EditTunName { input, cursor });
            }
        },
        Some(Popup::EditProfileName {
            mut input,
            mut cursor,
            group_id,
            profile_id,
        }) => match key.code {
            KeyCode::Esc => {
                app.popup = None;
                app.focus = Focus::LeftPanel;
            }
            KeyCode::Enter => {
                let name = input.trim().to_string();
                if !name.is_empty() {
                    let _ = client.rename_profile(group_id, profile_id, &name).await;
                    app.msg("Renaming...");
                }
                app.popup = None;
                app.focus = Focus::LeftPanel;
            }
            _ => {
                edit_text_field(&mut input, &mut cursor, key);
                app.popup = Some(Popup::EditProfileName {
                    input,
                    cursor,
                    group_id,
                    profile_id,
                });
            }
        },
        Some(Popup::ConfirmDelete { gid, pid, .. }) => match key.code {
            KeyCode::Enter => {
                let _ = client
                    .delete_profiles(&[ProfileID {
                        id: pid,
                        group_id: gid,
                    }])
                    .await;
                app.msg("Deleting...");
                app.focus = Focus::LeftPanel;
            }
            KeyCode::Esc => {
                app.focus = Focus::LeftPanel;
            }
            _ => {
                app.popup = Some(Popup::ConfirmDelete {
                    gid,
                    pid,
                    name: String::new(),
                });
                return false;
            }
        },
        Some(Popup::ConfirmDeleteGroup { gid, .. }) => match key.code {
            KeyCode::Enter => {
                let _ = client.delete_group(gid).await;
                app.msg("Deleting...");
                app.focus = Focus::LeftPanel;
            }
            KeyCode::Esc => {
                app.focus = Focus::LeftPanel;
            }
            _ => {
                app.popup = Some(Popup::ConfirmDeleteGroup {
                    gid,
                    name: String::new(),
                });
                return false;
            }
        },
        Some(Popup::EditSubscription {
            mut name,
            mut url,
            group_id,
            mut cursor,
            mut field,
        }) => {
            let consumed =
                handle_two_field_popup(&mut name, &mut url, &mut cursor, &mut field, key);
            if consumed {
                let has_sub = !url.trim().is_empty();
                let _ = client.update_group(group_id, &name, &url).await;
                if has_sub {
                    app.msg("Updating group and subscription...");
                } else {
                    app.msg("Updating group...");
                }
                app.focus = Focus::LeftPanel;
            } else if key.code == KeyCode::Esc {
                app.popup = None;
                app.focus = Focus::LeftPanel;
            } else {
                app.popup = Some(Popup::EditSubscription {
                    name,
                    url,
                    group_id,
                    cursor,
                    field,
                });
            }
        }
        Some(Popup::AddGroup {
            mut name,
            mut url,
            mut cursor,
            mut field,
        }) => {
            let consumed =
                handle_two_field_popup(&mut name, &mut url, &mut cursor, &mut field, key);
            if consumed {
                let has_sub = !url.trim().is_empty();
                let _ = client.add_group(&name, &url).await;
                if has_sub {
                    app.msg("Adding group and updating subscription...");
                } else {
                    app.msg("Adding group...");
                }
                app.focus = Focus::LeftPanel;
            } else if key.code == KeyCode::Esc {
                app.popup = None;
                app.focus = Focus::LeftPanel;
            } else {
                app.popup = Some(Popup::AddGroup {
                    name,
                    url,
                    cursor,
                    field,
                });
            }
        }
        None => {}
    }
    false
}

pub(crate) async fn handle_routing_popup_input(
    app: &mut App,
    client: &CoreClient,
    key: event::KeyEvent,
) {
    match app.routing_popup.take() {
        Some(RoutingPopup::ConfirmDelete { index }) => match key.code {
            KeyCode::Enter => {
                app.routing.rules.remove(index);
                if app.routing_cursor >= app.routing.rules.len() && app.routing_cursor > 0 {
                    app.routing_cursor -= 1;
                }
                app.msg("Rule deleted");
                let _ = client.update_routing(&app.routing).await;
            }
            KeyCode::Esc => {}
            _ => {
                app.routing_popup = Some(RoutingPopup::ConfirmDelete { index });
            }
        },
        Some(RoutingPopup::Add {
            mut match_type,
            mut value,
            mut outbound,
            mut cursor,
            mut field,
        }) => {
            match handle_routing_form(
                app,
                &mut match_type,
                &mut value,
                &mut outbound,
                &mut cursor,
                &mut field,
                key,
            ) {
                FormResult::Save => {
                    let rule = form_to_rule(match_type, &value, outbound);
                    app.routing.rules.push(rule);
                    app.msg("Rule added");
                    let _ = client.update_routing(&app.routing).await;
                }
                FormResult::Cancel => {
                    app.routing_popup = None;
                }
                FormResult::Continue => {
                    app.routing_popup = Some(RoutingPopup::Add {
                        match_type,
                        value,
                        outbound,
                        cursor,
                        field,
                    });
                }
            }
        }
        Some(RoutingPopup::Edit {
            index,
            mut match_type,
            mut value,
            mut outbound,
            mut cursor,
            mut field,
        }) => {
            match handle_routing_form(
                app,
                &mut match_type,
                &mut value,
                &mut outbound,
                &mut cursor,
                &mut field,
                key,
            ) {
                FormResult::Save => {
                    let rule = form_to_rule(match_type, &value, outbound);
                    if index < app.routing.rules.len() {
                        app.routing.rules[index] = rule;
                        app.msg("Rule saved");
                        let _ = client.update_routing(&app.routing).await;
                    }
                }
                FormResult::Cancel => {
                    app.routing_popup = None;
                }
                FormResult::Continue => {
                    app.routing_popup = Some(RoutingPopup::Edit {
                        index,
                        match_type,
                        value,
                        outbound,
                        cursor,
                        field,
                    });
                }
            }
        }
        Some(RoutingPopup::Presets { mut cursor }) => match key.code {
            KeyCode::Esc => {}
            KeyCode::Char('j') | KeyCode::Down => {
                let max = ROUTING_PRESETS.len().saturating_sub(1);
                if cursor < max {
                    cursor += 1;
                }
                app.routing_popup = Some(RoutingPopup::Presets { cursor });
            }
            KeyCode::Char('k') | KeyCode::Up => {
                cursor = cursor.saturating_sub(1);
                app.routing_popup = Some(RoutingPopup::Presets { cursor });
            }
            KeyCode::Home => {
                app.routing_popup = Some(RoutingPopup::Presets { cursor: 0 });
            }
            KeyCode::End => {
                let max = ROUTING_PRESETS.len().saturating_sub(1);
                app.routing_popup = Some(RoutingPopup::Presets { cursor: max });
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(preset) = ROUTING_PRESETS.get(cursor) {
                    let now_applied = toggle_preset(&mut app.routing, preset);
                    let action = if now_applied { "applied" } else { "removed" };
                    app.msg(format!("Preset {}: {}", action, preset.title));
                    let _ = client.update_routing(&app.routing).await;
                }
                app.routing_popup = Some(RoutingPopup::Presets { cursor });
            }
            _ => {
                app.routing_popup = Some(RoutingPopup::Presets { cursor });
            }
        },
        Some(RoutingPopup::HappProfiles { mut cursor }) => match key.code {
            KeyCode::Esc => {}
            KeyCode::Char('j') | KeyCode::Down => {
                let count = app
                    .routing_selected_group()
                    .map(|g| g.routing_profiles.len())
                    .unwrap_or(0);
                if count > 0 && cursor + 1 < count {
                    cursor += 1;
                }
                app.routing_popup = Some(RoutingPopup::HappProfiles { cursor });
            }
            KeyCode::Char('k') | KeyCode::Up => {
                cursor = cursor.saturating_sub(1);
                app.routing_popup = Some(RoutingPopup::HappProfiles { cursor });
            }
            KeyCode::Home => {
                app.routing_popup = Some(RoutingPopup::HappProfiles { cursor: 0 });
            }
            KeyCode::End => {
                let count = app
                    .routing_selected_group()
                    .map(|g| g.routing_profiles.len())
                    .unwrap_or(0);
                let max = count.saturating_sub(1);
                app.routing_popup = Some(RoutingPopup::HappProfiles { cursor: max });
            }
            KeyCode::Enter => {
                if let Some(group) = app.routing_selected_group() {
                    let gid = group.group.id;
                    if let Some(target_p) = group.routing_profiles.get(cursor) {
                        let pid = target_p.id.clone();
                        let name = target_p.name.clone();
                        let _ = client.set_active_routing_profile(gid, &pid).await;
                        app.msg(format!("Activating Happ routing: {name}"));
                    }
                }
                app.routing_popup = Some(RoutingPopup::HappProfiles { cursor });
            }
            KeyCode::Char('o') | KeyCode::Char('O') => {
                if let Some(group) = app.routing_selected_group() {
                    let gid = group.group.id;
                    let _ = client.set_active_routing_profile(gid, "off").await;
                    app.msg("Happ routing deactivated (off)");
                }
                app.routing_popup = Some(RoutingPopup::HappProfiles { cursor });
            }
            KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Char('x') | KeyCode::Delete => {
                let to_delete = app.routing_selected_group().and_then(|group| {
                    group.routing_profiles.get(cursor).map(|target_p| {
                        (
                            group.group.id,
                            target_p.id.clone(),
                            target_p.name.clone(),
                            group.routing_profiles.len().saturating_sub(1),
                        )
                    })
                });
                if let Some((gid, pid, name, count)) = to_delete {
                    let _ = client.delete_routing_profile(gid, &pid).await;
                    app.msg(format!("Deleted Happ routing: {name}"));
                    if cursor >= count && cursor > 0 {
                        cursor -= 1;
                    }
                }
                app.routing_popup = Some(RoutingPopup::HappProfiles { cursor });
            }
            KeyCode::Char('m') | KeyCode::Char('M') => {
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
                }
                app.routing_popup = Some(RoutingPopup::HappProfiles { cursor });
            }
            _ => {
                app.routing_popup = Some(RoutingPopup::HappProfiles { cursor });
            }
        },
        None => {}
    }
}

fn handle_two_field_popup(
    field0: &mut String,
    field1: &mut String,
    cursor: &mut usize,
    field: &mut usize,
    key: event::KeyEvent,
) -> bool {
    match key.code {
        KeyCode::Tab | KeyCode::BackTab => {
            *field = if *field == 0 { 1 } else { 0 };
            *cursor = if *field == 0 {
                field0.chars().count()
            } else {
                field1.chars().count()
            };
            false
        }
        KeyCode::Enter => {
            if *field == 0 {
                *field = 1;
                *cursor = field1.chars().count();
                false
            } else {
                true
            }
        }
        _ => {
            let target = if *field == 0 { field0 } else { field1 };
            edit_text_field(target, cursor, key);
            false
        }
    }
}

pub(crate) enum FormResult {
    Save,
    Cancel,
    Continue,
}

fn handle_routing_form(
    _app: &mut App,
    match_type: &mut usize,
    value: &mut String,
    outbound: &mut usize,
    cursor: &mut usize,
    field: &mut usize,
    key: event::KeyEvent,
) -> FormResult {
    match key.code {
        KeyCode::Esc => FormResult::Cancel,
        KeyCode::Tab => {
            *field = (*field + 1) % 3;
            *cursor = if *field == 1 {
                value.chars().count()
            } else {
                0
            };
            FormResult::Continue
        }
        KeyCode::BackTab => {
            *field = if *field == 0 { 2 } else { *field - 1 };
            *cursor = if *field == 1 {
                value.chars().count()
            } else {
                0
            };
            FormResult::Continue
        }
        KeyCode::Enter => {
            if *field < 2 {
                *field += 1;
                *cursor = if *field == 1 {
                    value.chars().count()
                } else {
                    0
                };
                FormResult::Continue
            } else {
                FormResult::Save
            }
        }
        KeyCode::Left => {
            if *field == 0 {
                *match_type = if *match_type == 0 { 5 } else { *match_type - 1 };
            } else if *field == 1 {
                edit_text_field(value, cursor, key);
            } else if *field == 2 {
                *outbound = if *outbound == 0 { 2 } else { *outbound - 1 };
            }
            FormResult::Continue
        }
        KeyCode::Right => {
            if *field == 0 {
                *match_type = (*match_type + 1) % 6;
            } else if *field == 1 {
                edit_text_field(value, cursor, key);
            } else if *field == 2 {
                *outbound = (*outbound + 1) % 3;
            }
            FormResult::Continue
        }
        _ if *field == 1 => {
            edit_text_field(value, cursor, key);
            FormResult::Continue
        }
        _ => FormResult::Continue,
    }
}
