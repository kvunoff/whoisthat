use crossterm::event::{self, KeyCode};

use crate::core_client::CoreClient;
use crate::text_edit::edit_text_field;
use crate::ui::App;
use crate::ui::routing::{ROUTING_PRESETS, RoutingPopup, form_to_rule, toggle_preset};

pub(crate) async fn handle_routing_popup_input(
    app: &mut App,
    client: &CoreClient,
    key: event::KeyEvent,
) {
    match app.routing_popup.take() {
        Some(RoutingPopup::ConfirmDelete { index }) => match key.code {
            KeyCode::Enter => {
                app.ensure_custom_routing();
                app.active_routing_config_mut().rules.remove(index);
                if app.routing_cursor >= app.active_routing_config().rules.len()
                    && app.routing_cursor > 0
                {
                    app.routing_cursor -= 1;
                }
                app.msg("Rule deleted");
                let gid = app.routing_page_group_id();
                let cfg = app.active_routing_config().clone();
                let _ = client.update_routing(gid, &cfg).await;
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
                    app.ensure_custom_routing();
                    app.active_routing_config_mut().rules.push(rule);
                    app.msg("Rule added");
                    let gid = app.routing_page_group_id();
                    let cfg = app.active_routing_config().clone();
                    let _ = client.update_routing(gid, &cfg).await;
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
                    app.ensure_custom_routing();
                    if index < app.active_routing_config().rules.len() {
                        app.active_routing_config_mut().rules[index] = rule;
                        app.msg("Rule saved");
                        let gid = app.routing_page_group_id();
                        let cfg = app.active_routing_config().clone();
                        let _ = client.update_routing(gid, &cfg).await;
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
                    app.ensure_custom_routing();
                    let now_applied = toggle_preset(app.active_routing_config_mut(), preset);
                    let action = if now_applied { "applied" } else { "removed" };
                    app.msg(format!("Preset {}: {}", action, preset.title));
                    let gid = app.routing_page_group_id();
                    let cfg = app.active_routing_config().clone();
                    let _ = client.update_routing(gid, &cfg).await;
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

pub(crate) fn manual_interval_secs(input: &str) -> i64 {
    let hours = input.trim().parse::<u64>().unwrap_or(0).min(8760);
    (hours * 3600) as i64
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
#[cfg(test)]
mod tests {
    use super::manual_interval_secs;

    #[test]
    fn manual_interval_parses_hours() {
        assert_eq!(manual_interval_secs("24"), 24 * 3600);
        assert_eq!(manual_interval_secs(""), 0);
        assert_eq!(manual_interval_secs("0"), 0);
        assert_eq!(manual_interval_secs("abc"), 0);
        assert_eq!(manual_interval_secs("99999"), 8760 * 3600);
    }
}
