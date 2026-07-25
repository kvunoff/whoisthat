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

pub(crate) async fn run_loop<B: Backend>(
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
        term.draw(|f| app.render(f))?;

        tokio::select! {
            Some(ev) = core_rx.recv() => {
                if handle_core_event(app, client, ev, &mut first_state, &mut do_autoconnect, cfg, &ip_tx).await {
                    break;
                }
            }
            Some(ev) = input_rx.recv() => {
                if handle_input(app, client, ev, cfg, logger).await {
                    break;
                }
            }
            else => break,
        }
    }
    Ok(())
}
