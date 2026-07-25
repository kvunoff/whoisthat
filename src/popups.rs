use crossterm::event::{self, KeyCode};

use crate::core_client::protocol::{ProfileID, SetHwidData};
use crate::core_client::CoreClient;
use crate::text_edit::{edit_text_field, read_clipboard};
use crate::ui::app::{Focus, Popup};
use crate::ui::routing::{form_to_rule, RoutingPopup};
use crate::ui::App;

pub(crate) async fn handle_popup_input(
    app: &mut App,
    client: &CoreClient,
    key: event::KeyEvent,
) -> bool {
    match app.popup.take() {
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
                let _ = client.update_group(group_id, &name, &url).await;
                app.msg("Updating group...");
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
                let _ = client.add_group(&name, &url).await;
                app.msg("Adding group...");
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
            let save = handle_routing_form(
                app,
                &mut match_type,
                &mut value,
                &mut outbound,
                &mut cursor,
                &mut field,
                key,
            );
            if save {
                let rule = form_to_rule(match_type, &value, outbound);
                app.routing.rules.push(rule);
                let _ = client.update_routing(&app.routing).await;
            } else {
                app.routing_popup = Some(RoutingPopup::Add {
                    match_type,
                    value,
                    outbound,
                    cursor,
                    field,
                });
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
            let save = handle_routing_form(
                app,
                &mut match_type,
                &mut value,
                &mut outbound,
                &mut cursor,
                &mut field,
                key,
            );
            if save {
                let rule = form_to_rule(match_type, &value, outbound);
                app.routing.rules[index] = rule;
                let _ = client.update_routing(&app.routing).await;
            } else {
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
        KeyCode::Tab => {
            *field = if *field == 0 { 1 } else { 0 };
            *cursor = if *field == 0 {
                field0.len()
            } else {
                field1.len()
            };
            false
        }
        KeyCode::Enter => {
            if *field == 0 {
                *field = 1;
                *cursor = field1.len();
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

fn handle_routing_form(
    _app: &mut App,
    match_type: &mut usize,
    value: &mut String,
    outbound: &mut usize,
    cursor: &mut usize,
    field: &mut usize,
    key: event::KeyEvent,
) -> bool {
    match key.code {
        KeyCode::Esc => {
            return false;
        }
        KeyCode::Tab => {
            *field = (*field + 1) % 3;
            *cursor = if *field == 1 { value.len() } else { 0 };
        }
        KeyCode::Enter => {
            if *field < 2 {
                *field += 1;
                *cursor = if *field == 1 { value.len() } else { 0 };
            } else {
                return true;
            }
        }
        KeyCode::Left => {
            if *field == 0 {
                *match_type = if *match_type == 0 { 5 } else { *match_type - 1 };
            } else if *field == 1 {
                *cursor = cursor.saturating_sub(1);
            } else if *field == 2 {
                *outbound = if *outbound == 0 { 2 } else { *outbound - 1 };
            }
        }
        KeyCode::Right => {
            if *field == 0 {
                *match_type = (*match_type + 1) % 6;
            } else if *field == 1 && *cursor < value.len() {
                *cursor += 1;
            } else if *field == 2 {
                *outbound = (*outbound + 1) % 3;
            }
        }
        KeyCode::Char(c) => {
            if *field == 1 {
                if c == 'v' && matches!(key.modifiers, crossterm::event::KeyModifiers::CONTROL) {
                    if let Some(clip) = read_clipboard() {
                        *value = clip;
                        *cursor = value.len();
                    }
                } else {
                    if *cursor <= value.len() {
                        value.insert(*cursor, c);
                    } else {
                        value.push(c);
                    }
                    *cursor += 1;
                }
            }
        }
        KeyCode::Backspace => {
            if *field == 1 && *cursor > 0 && !value.is_empty() {
                value.remove(*cursor - 1);
                *cursor -= 1;
            }
        }
        KeyCode::Delete => {
            if *field == 1 && *cursor < value.len() {
                value.remove(*cursor);
            }
        }
        KeyCode::Home => {
            if *field == 1 {
                *cursor = 0;
            }
        }
        KeyCode::End => {
            if *field == 1 {
                *cursor = value.len();
            }
        }
        _ => {}
    }
    false
}
