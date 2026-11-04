use crossterm::event::{self, KeyCode, KeyModifiers};

use crate::core_client::CoreClient;
use crate::text_edit::edit_text_field;
use crate::ui::App;
use crate::ui::app::{Focus, Popup};

use super::routing::manual_interval_secs;

#[allow(clippy::too_many_arguments)]
pub(super) async fn edit_subscription(
    app: &mut App,
    client: &CoreClient,
    key: event::KeyEvent,
    mut name: String,
    mut url: String,
    group_id: i32,
    mut cursor: usize,
    mut field: usize,
    mut auto_update: bool,
    mut interval_hours: String,
    provider_interval_secs: i64,
) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('r') {
        let _ = client
            .update_group(
                group_id,
                "",
                &url,
                Some(auto_update),
                Some(manual_interval_secs(&interval_hours)),
            )
            .await;
        app.msg("Resetting group name to provider default...");
        app.popup = None;
        app.focus = Focus::LeftPanel;
        return;
    }
    let mut save = false;
    match key.code {
        KeyCode::Esc => {
            app.popup = None;
            app.focus = Focus::LeftPanel;
            return;
        }
        KeyCode::Tab | KeyCode::BackTab => {
            field = if key.code == KeyCode::BackTab {
                (field + 3) % 4
            } else {
                (field + 1) % 4
            };
            cursor = match field {
                0 => name.chars().count(),
                1 => url.chars().count(),
                3 => interval_hours.chars().count(),
                _ => 0,
            };
        }
        KeyCode::Enter => {
            if field < 3 {
                field += 1;
                cursor = match field {
                    0 => name.chars().count(),
                    1 => url.chars().count(),
                    3 => interval_hours.chars().count(),
                    _ => 0,
                };
            } else {
                save = true;
            }
        }
        KeyCode::Char(' ') if field == 2 => {
            auto_update = !auto_update;
        }
        _ if field == 2 => {}
        _ if field == 3 => {
            // Interval is a whole number of hours; ignore other input.
            if let KeyCode::Char(c) = key.code {
                if c.is_ascii_digit() {
                    edit_text_field(&mut interval_hours, &mut cursor, key);
                }
            } else {
                edit_text_field(&mut interval_hours, &mut cursor, key);
            }
        }
        _ => {
            let target = if field == 0 { &mut name } else { &mut url };
            edit_text_field(target, &mut cursor, key);
        }
    }
    if save {
        let has_sub = !url.trim().is_empty();
        let _ = client
            .update_group(
                group_id,
                &name,
                &url,
                Some(auto_update),
                Some(manual_interval_secs(&interval_hours)),
            )
            .await;
        if has_sub {
            app.msg("Updating group and subscription...");
        } else {
            app.msg("Updating group...");
        }
        app.focus = Focus::LeftPanel;
    } else {
        app.popup = Some(Popup::EditSubscription {
            name,
            url,
            group_id,
            cursor,
            field,
            auto_update,
            interval_hours,
            provider_interval_secs,
        });
    }
}

pub(super) async fn add_group(
    app: &mut App,
    client: &CoreClient,
    key: event::KeyEvent,
    mut url: String,
    mut cursor: usize,
) {
    match key.code {
        KeyCode::Esc => {
            app.popup = None;
            app.focus = Focus::LeftPanel;
        }
        KeyCode::Enter => {
            let url = url.trim().to_string();
            let _ = client.add_group("", &url).await;
            if url.is_empty() {
                app.msg("Adding group...");
            } else {
                app.msg("Adding group and updating subscription...");
            }
            app.focus = Focus::LeftPanel;
        }
        _ => {
            edit_text_field(&mut url, &mut cursor, key);
            app.popup = Some(Popup::AddGroup { url, cursor });
        }
    }
}
