use crossterm::event::{self, KeyCode};

use crate::config;
use crate::core_client::CoreClient;
use crate::core_client::protocol::{ProfileID, SetHwidData};
use crate::logfile::ClearAction;
use crate::logger::FileLogger;
use crate::text_edit::edit_text_field;
use crate::ui::App;
use crate::ui::app::{ActiveTab, Focus, Popup};
use crate::ui::logs::LogSource;

use super::editors;

pub(crate) async fn handle_popup_input(
    app: &mut App,
    client: &CoreClient,
    key: event::KeyEvent,
    cfg: &mut crate::config::AppConfig,
    logger: &'static FileLogger,
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
                    app.sync_routing_group_to_connected();
                    let gid = app.routing_page_group_id();
                    let _ = client.get_routing(gid).await;
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
                app.sync_routing_group_to_connected();
                let gid = app.routing_page_group_id();
                let _ = client.get_routing(gid).await;
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
        Some(Popup::Recovery { config, db, day }) => match key.code {
            KeyCode::Enter => {
                if config {
                    let _ = client.repair_config().await;
                }
                if db {
                    let _ = client.restore_db().await;
                }
                app.msg("Restoring...");
                app.focus = Focus::LeftPanel;
            }
            KeyCode::Esc => {
                app.focus = Focus::LeftPanel;
            }
            _ => {
                app.popup = Some(Popup::Recovery { config, db, day });
                return false;
            }
        },
        Some(Popup::Update { info }) => match key.code {
            KeyCode::Enter | KeyCode::Char('u') => {
                if let Some(command) = &info.command {
                    let copied = arboard::Clipboard::new()
                        .and_then(|mut clipboard| clipboard.set_text(command.clone()));
                    match copied {
                        Ok(()) => app.msg("Update command copied"),
                        Err(_) => app.msg("Could not copy update command"),
                    }
                } else {
                    app.msg("No update command for this install");
                }
                app.focus = Focus::LeftPanel;
            }
            KeyCode::Char('s') => {
                cfg.skipped_version = info.latest.clone();
                match config::try_save_config(cfg) {
                    Ok(()) => {
                        app.msg(format!("Version {} skipped", info.latest));
                        app.focus = Focus::LeftPanel;
                    }
                    Err(e) => {
                        // Keep the popup: otherwise the user thinks the skip
                        // stuck and it silently comes back after a restart.
                        app.msg(format!("Skip not saved: {e}"));
                        app.popup = Some(Popup::Update { info });
                    }
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                app.focus = Focus::LeftPanel;
            }
            _ => {
                app.popup = Some(Popup::Update { info });
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
                    // Do not apply optimistically: the daemon is the source of
                    // truth and may reject the name. The UI follows the
                    // `tun-name-updated` broadcast instead.
                    if let Err(e) = client.set_tun_name(&name).await {
                        app.msg(format!("Error: {e}"));
                    }
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
        Some(Popup::ConfirmClearLogs { source }) => match key.code {
            KeyCode::Enter => {
                let message = match source {
                    LogSource::Tui => match logger.clear() {
                        Ok(_) => "TUI log cleared".to_string(),
                        Err(e) => format!("Clear failed: {e}"),
                    },
                    LogSource::Core => {
                        let mut results =
                            crate::logfile::clear_core(&crate::config::core_log_paths());
                        results.extend(crate::logfile::clear_crash(&crate::config::data_dir()));
                        let failed = results
                            .iter()
                            .filter(|item| item.action == ClearAction::Failed)
                            .count();
                        if failed == 0 {
                            "Core logs cleared".to_string()
                        } else {
                            format!("Clear failed for {failed} file(s) (permissions?)")
                        }
                    }
                };
                app.logs_state.reload();
                app.msg(message);
                app.focus = Focus::LeftPanel;
            }
            KeyCode::Esc => {
                app.focus = Focus::LeftPanel;
            }
            _ => {
                app.popup = Some(Popup::ConfirmClearLogs { source });
            }
        },
        Some(Popup::EditSubscription {
            name,
            url,
            group_id,
            cursor,
            field,
            auto_update,
            interval_hours,
            provider_interval_secs,
        }) => {
            editors::edit_subscription(
                app,
                client,
                key,
                name,
                url,
                group_id,
                cursor,
                field,
                auto_update,
                interval_hours,
                provider_interval_secs,
            )
            .await;
        }
        Some(Popup::AddGroup { url, cursor }) => {
            editors::add_group(app, client, key, url, cursor).await;
        }
        None => {}
    }
    false
}
