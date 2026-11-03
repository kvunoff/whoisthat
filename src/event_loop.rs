use crossterm::event::Event;
use ratatui::backend::Backend;
use tokio::sync::mpsc;

use crate::config;
use crate::core_client::{CoreClient, CoreEvent};
use crate::core_events::handle_core_event;
use crate::input::handle_input;
use crate::logger::FileLogger;
use crate::ui::App;

pub(crate) enum AppEvent {
    Input(Event),
    Tick,
    PublicIp(String),
    PublicIpv6(String),
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_loop<B: Backend<Error = std::io::Error>>(
    term: &mut ratatui::Terminal<B>,
    app: &mut App,
    client: &CoreClient,
    core_rx: &mut mpsc::UnboundedReceiver<CoreEvent>,
    input_rx: &mut mpsc::UnboundedReceiver<AppEvent>,
    mut do_autoconnect: bool,
    cfg: &mut config::AppConfig,
    ip_tx: mpsc::UnboundedSender<AppEvent>,
    logger: &'static FileLogger,
) -> std::io::Result<()> {
    let mut first_state = true;
    loop {
        request_selected_profile_details(app, client).await;
        term.draw(|f| app.render(f))?;

        tokio::select! {
            Some(ev) = core_rx.recv() => {
                if handle_core_event(app, client, ev, &mut first_state, &mut do_autoconnect, cfg, &ip_tx).await {
                    break;
                }
            }
            Some(ev) = input_rx.recv() => {
                if handle_input(app, client, ev, cfg, logger, &ip_tx).await {
                    break;
                }
            }
            else => break,
        }
    }
    Ok(())
}

/// Managed JSON profiles carry their stream settings in a separate core-side
/// config. Fetch safe display metadata once per selected profile.
async fn request_selected_profile_details(app: &mut App, client: &CoreClient) {
    if let Some((group_id, profile_id)) = app.profile_details_request()
        && client
            .get_profile_details(group_id, profile_id)
            .await
            .is_ok()
    {
        app.mark_profile_details_pending((group_id, profile_id));
    }
}
