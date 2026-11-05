use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use tokio::sync::mpsc;

use crate::config;
use crate::core_client::CoreClient;
use crate::event_loop::AppEvent;
use crate::keymap::{AppAction, KeyContext, resolve_action};
use crate::logger::FileLogger;
use crate::popups::{handle_popup_input, handle_routing_popup_input};
use crate::text_edit::edit_text_field;
use crate::ui::App;
use crate::ui::app::{ActiveTab, Focus, Popup};

mod mouse;
mod navigation;
mod profiles;
mod routing;
mod settings;

use mouse::handle_mouse_input;

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
        AppEvent::UpdateAvailable(info) => {
            if cfg.latest_version != info.latest {
                cfg.latest_version = info.latest.clone();
                if let Err(e) = config::try_save_config(cfg) {
                    log::warn!("failed to save latest version: {e}");
                }
            }
            app.pending_update = Some(info);
        }
        AppEvent::Input(input) => match input {
            Event::Key(key) => {
                if key.kind == KeyEventKind::Release {
                    return false;
                }

                if app.popup.is_some() {
                    return handle_popup_input(app, client, key, logger).await;
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

    if app.tab == ActiveTab::Logs && app.logs_state.search_active {
        match key.code {
            KeyCode::Esc => app.logs_state.clear_search(),
            KeyCode::Enter => app.logs_state.finish_search(),
            KeyCode::Down => app.logs_state.scroll_down(),
            KeyCode::Up => app.logs_state.scroll_up(),
            KeyCode::Char(_) | KeyCode::Backspace => {
                let mut cursor = app.logs_state.search_input.chars().count();
                edit_text_field(&mut app.logs_state.search_input, &mut cursor, key);
                app.logs_state.search_changed();
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
    if let Some(result) = navigation::handle(action, app, client).await {
        return result;
    }
    if let Some(result) = profiles::handle(action, app, client).await {
        return result;
    }
    if let Some(result) = routing::handle(action, app, client).await {
        return result;
    }
    if let Some(result) = settings::handle(action, app, client, cfg, logger, ip_tx).await {
        return result;
    }
    match action {
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
        AppAction::LogsStartSearch => {
            app.logs_state.start_search();
            false
        }
        AppAction::LogsClear => {
            app.popup = Some(Popup::ConfirmClearLogs {
                source: app.logs_state.source,
            });
            app.focus = Focus::Popup;
            false
        }
        AppAction::None => false,
        _ => false,
    }
}
