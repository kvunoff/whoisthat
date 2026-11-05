mod cli;
mod config;
mod core_client;
mod core_events;
mod core_spawn;
mod deeplink;
mod doctor;
mod event_loop;
mod input;
mod keymap;
mod launcher;
mod logfile;
mod logfmt;
mod logger;
mod net_info;
mod popups;
mod systemd;
mod terminal;
mod testing;
mod text_edit;
mod ui;
mod update_check;

use std::io::{self, IsTerminal};
use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use io::Write;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc;

use crate::core_events::refresh_public_ip;
use core_client::protocol::{
    ApplicationState, DieData, EXPECTED_CORE_PROTOCOL_VERSION, GetApplicationStateData, TestConfig,
};
use core_client::{CoreClient, CoreConnection};
use event_loop::{AppEvent, run_loop};
use logger::{configure_logger, env_log_level, filter_name, init_logger, install_panic_hook};
use net_info::check_sudo_env;
use systemd::systemd_is_enabled;
use ui::App;

/// A running core can be reused only when its wire contract matches ours.
fn core_protocol_matches(protocol_version: u32) -> bool {
    protocol_version == EXPECTED_CORE_PROTOCOL_VERSION && protocol_version != 0
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let argv: Vec<String> = std::env::args().collect();

    if let Some(cmd) = argv.get(1).map(String::as_str)
        && (cmd.starts_with("wit://") || cmd.starts_with("wit:"))
    {
        match deeplink::run_deeplink(cmd).await {
            Ok(_) => std::process::exit(0),
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
    }

    match cli::run(&argv[1..]).await {
        cli::Outcome::Tui => {}
        cli::Outcome::Exit(code) => std::process::exit(code),
    }

    let logger = init_logger();
    install_panic_hook(logger);

    let mut cfg = config::load_config();
    // Desktop entries start the TUI without a tty: re-exec it inside a terminal.
    if !io::stdin().is_terminal() && !io::stdout().is_terminal() {
        terminal::relaunch(&cfg.terminal);
    }
    // A deliberate env override turns file logging on for this session and is
    // not written back, so it also drives the core spawn/IPC level below.
    let env_override = env_log_level();
    if let Some((level, _)) = env_override {
        cfg.log_enabled = true;
        cfg.log_level = filter_name(level).to_string();
    }
    if !ui::theme::set_theme(&cfg.theme) {
        let fallback = ui::theme::current_theme().id.to_string();
        log::warn!(
            "Configured theme '{}' not recognized, falling back to '{}'",
            cfg.theme,
            fallback
        );
        cfg.theme = fallback;
        config::save_config(&cfg);
    }
    configure_logger(logger, cfg.log_enabled, &cfg.log_level);
    let tui_version = env!("CARGO_PKG_VERSION").to_string();
    log::info!(
        "tui v{} started (verbose log: {}, level: {})",
        tui_version,
        cfg.log_enabled,
        cfg.log_level
    );
    if let Some((_, var)) = env_override {
        log::info!("log level overridden by {var} (session only)");
    }

    // Auto-register wit:// desktop scheme handler in background
    deeplink::ensure_desktop_registered();

    let endpoint = cfg.endpoint();
    log::info!("Core IPC endpoint: {}", endpoint.describe());

    let mut core_alive = false;
    match query_core_state(&endpoint).await {
        Some(state) => {
            if core_protocol_matches(state.protocol_version) {
                log::info!(
                    "Reattaching to core v{} (protocol {}), tui v{}",
                    state.core_version,
                    state.protocol_version,
                    tui_version
                );
                if !state.core_version.is_empty() && cfg.core_version != state.core_version {
                    cfg.core_version = state.core_version;
                    config::save_config(&cfg);
                }
                core_alive = true;
            } else {
                log::info!(
                    "Core protocol mismatch (got {}, expected {}), restarting core",
                    state.protocol_version,
                    EXPECTED_CORE_PROTOCOL_VERSION
                );
                if let Ok(mut conn) = CoreConnection::connect_endpoint(&endpoint).await {
                    let _ = conn.send("die", &DieData {}).await;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
        None => {
            log::info!("No reachable core, will spawn fresh");
        }
    }

    if !core_alive {
        log::info!("Spawning fresh core for tui v{tui_version}");
        if std::io::stdin().is_terminal() && config::core_autoconnect_tun() {
            core_spawn::ensure_core_caps(&core_spawn::find_core_binary());
        }
        core_spawn::spawn_core(&cfg.log_level)?;
        let mut retries = 0u32;
        loop {
            if CoreConnection::connect_endpoint(&endpoint).await.is_ok() {
                break;
            }
            retries += 1;
            if retries > 30 {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Core failed to start within 30s",
                ));
            }
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
        if let Some(state) = query_core_state(&endpoint).await
            && !state.core_version.is_empty()
        {
            cfg.core_version = state.core_version;
            config::save_config(&cfg);
        }
    }
    let (read_half, write_half) = CoreConnection::connect_split(&endpoint)
        .await
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::ConnectionRefused,
                format!("Failed to connect to core: {}", e),
            )
        })?;

    let client = CoreClient::new(write_half);
    // Best effort: an older core ignores the unknown message, and the level
    // still comes from WHOISTHAT_LOG_LEVEL at spawn time.
    let _ = client.set_log_level(&cfg.log_level).await;

    let mut core_rx = core_client::spawn_read_loop(
        read_half,
        client.clone_ref(),
        endpoint.clone(),
        cfg.log_level.clone(),
    );
    client.get_application_state().await?;
    let _ = client.get_routing(0).await;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut term = ratatui::Terminal::new(backend)?;

    let mut app = App::new(
        cfg.show_ip,
        cfg.log_enabled,
        cfg.log_level.clone(),
        cfg.test_method.clone(),
        cfg.tun_name.clone(),
        cfg.kill_switch_enabled,
        TestConfig {
            concurrency: cfg.test_concurrency,
            timeout_seconds: cfg.test_timeout_seconds,
            samples_per_test: cfg.test_samples,
            test_endpoint: cfg.test_endpoint.clone(),
            auto_test_on_subscribe: cfg.auto_test_on_subscribe,
        },
    );
    app.systemd_enabled = systemd_is_enabled();
    app.check_updates = cfg.check_updates;

    if let Some(warning) = check_sudo_env() {
        app.msg(warning);
        eprintln!("whoisthat: {}", warning);
    }

    let (input_tx, mut input_rx) = mpsc::unbounded_channel();
    let input_tx2 = input_tx.clone();
    tokio::spawn(async move {
        loop {
            if let Ok(true) = event::poll(Duration::from_millis(100))
                && let Ok(ev) = event::read()
                && input_tx2.send(AppEvent::Input(ev)).is_err()
            {
                break;
            }
        }
    });

    let _tick = {
        let tx = input_tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if tx.send(AppEvent::Tick).is_err() {
                    break;
                }
            }
        })
    };

    // Check for a newer release in the background. A cached version is shown
    // on every launch; the network is queried at most once a day.
    let now = update_check::now_unix();
    let exe = std::env::current_exe().unwrap_or_default();
    let dev_build = update_check::is_dev_build(&exe);
    let no_check_env = std::env::var("WHOISTHAT_NO_UPDATE_CHECK").as_deref() == Ok("1");
    let test_mode_env = std::env::var("WHOISTHAT_TEST_MODE").as_deref() == Ok("1");
    let allowed = cfg.check_updates && !dev_build && !no_check_env && !test_mode_env;
    let due =
        allowed && update_check::should_check_cheap(true, now, cfg.last_update_check, false, false);
    if due {
        cfg.last_update_check = now;
        // Persist before the request so a crash cannot trigger a check storm.
        if let Err(e) = config::try_save_config(&cfg) {
            log::warn!("failed to save update check time: {e}");
        }
        update_check::spawn(
            true,
            cfg.latest_version.clone(),
            cfg.skipped_version.clone(),
            input_tx.clone(),
        );
    } else if allowed
        && !cfg.latest_version.is_empty()
        && update_check::should_prompt(&cfg.latest_version, &tui_version, &cfg.skipped_version)
    {
        update_check::spawn(
            false,
            cfg.latest_version.clone(),
            cfg.skipped_version.clone(),
            input_tx.clone(),
        );
    }

    if cfg.show_ip {
        let ip_tx = input_tx.clone();
        tokio::spawn(async move {
            loop {
                refresh_public_ip(ip_tx.clone(), Duration::ZERO);
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        });
    }

    let do_autoconnect = cfg.autoconnect && cfg.last_profile_id != 0 && !cfg.autoconnect_migrated;

    let res = run_loop(
        &mut term,
        &mut app,
        &client,
        &mut core_rx,
        &mut input_rx,
        do_autoconnect,
        &mut cfg,
        input_tx.clone(),
        logger,
    )
    .await;

    if let Err(e) = disable_raw_mode() {
        log::error!("disable_raw_mode: {e}");
    }
    if let Err(e) = execute!(
        term.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    ) {
        log::error!("LeaveAlternateScreen: {e}");
    }
    if let Err(e) = term.show_cursor() {
        log::error!("show_cursor: {e}");
    }
    drop(term);

    let mut stdout = io::stdout();
    let _ = stdout.flush();

    config::save_config(&cfg);

    match res {
        Ok(_) => std::process::exit(0),
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}

/// Query a running core for its full application-state (one-shot).
/// Returns None when no core is reachable or the handshake times out.
/// Used at startup to check `protocol_version` before deciding to reattach.
async fn query_core_state(
    endpoint: &core_client::connection::Endpoint,
) -> Option<ApplicationState> {
    let (mut read_half, mut write_half) = CoreConnection::connect_split(endpoint).await.ok()?;
    write_half
        .send("get-application-state", &GetApplicationStateData {})
        .await
        .ok()?;
    let msg = read_half
        .recv_until(Duration::from_secs(2), |m| m.msg == "application-state")
        .await
        .ok()?;
    serde_json::from_value(msg.data).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reattaches_only_on_a_matching_protocol() {
        assert!(core_protocol_matches(EXPECTED_CORE_PROTOCOL_VERSION));
        assert!(!core_protocol_matches(EXPECTED_CORE_PROTOCOL_VERSION + 1));
        assert!(!core_protocol_matches(EXPECTED_CORE_PROTOCOL_VERSION - 1));
        // An old core that predates the field reports 0 and must be replaced.
        assert!(!core_protocol_matches(0));
    }
}
