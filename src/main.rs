mod config;
mod core_client;
mod core_events;
mod core_spawn;
mod event_loop;
mod input;
mod launcher;
mod logger;
mod net_info;
mod popups;
mod systemd;
mod testing;
mod text_edit;
mod ui;

use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use io::Write;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc;

use core_client::protocol::{DieData, TestConfig};
use core_client::{CoreClient, CoreConnection};
use event_loop::{run_loop, AppEvent};
use logger::{configure_logger, init_logger};
use net_info::check_sudo_env;
use systemd::systemd_is_enabled;
use ui::App;

#[tokio::main]
async fn main() -> io::Result<()> {
    let argv: Vec<String> = std::env::args().collect();
    if argv.get(1).map(String::as_str) == Some("run") {
        let code = launcher::run_in_split_slice(&argv[2..]);
        std::process::exit(code);
    }

    let logger = init_logger();

    let mut cfg = config::load_config();
    configure_logger(logger, cfg.log_enabled, &cfg.log_level);
    let current_version = env!("CARGO_PKG_VERSION").to_string();

    let endpoint = cfg.endpoint();
    log::info!("Core IPC endpoint: {}", endpoint.describe());

    let mut core_alive = false;
    if let Ok(mut conn) = CoreConnection::connect_endpoint(&endpoint).await {
        if cfg.core_version != current_version {
            log::info!(
                "Core version mismatch (cfg='{}' current='{current_version}'), restarting",
                cfg.core_version
            );
            let _ = conn.send("die", &DieData {}).await;
            drop(conn);
            tokio::time::sleep(Duration::from_millis(500)).await;
        } else {
            log::info!("Reattaching to existing core v{current_version}");
            core_alive = true;
        }
    }

    if !core_alive {
        log::info!("Spawning fresh core v{current_version}");
        core_spawn::ensure_core_caps(&core_spawn::find_core_binary());
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
        cfg.core_version = current_version;
        config::save_config(&cfg);
    }
    let conn = CoreConnection::connect_endpoint(&endpoint)
        .await
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::ConnectionRefused,
                format!("Failed to connect to core: {}", e),
            )
        })?;

    let client = CoreClient::new(conn);

    let read_conn = CoreConnection::connect_endpoint(&endpoint).await?;
    let mut core_rx = core_client::spawn_read_loop(
        read_conn,
        client.clone_ref(),
        endpoint.clone(),
        cfg.log_level.clone(),
    );
    client.get_application_state().await?;
    let _ = client.get_routing().await;

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

    if let Some(warning) = check_sudo_env() {
        app.msg(warning);
        eprintln!("whoisthat: {}", warning);
    }

    let (input_tx, mut input_rx) = mpsc::unbounded_channel();
    let input_tx2 = input_tx.clone();
    tokio::spawn(async move {
        loop {
            if let Ok(true) = event::poll(Duration::from_millis(100)) {
                if let Ok(ev) = event::read() {
                    if input_tx2.send(AppEvent::Input(ev)).is_err() {
                        break;
                    }
                }
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

    {
        let ip_tx = input_tx.clone();
        tokio::spawn(async move {
            loop {
                if let Some(ip) = tokio::task::spawn_blocking(net_info::fetch_public_ip)
                    .await
                    .unwrap_or(None)
                {
                    let _ = ip_tx.send(AppEvent::PublicIp(ip));
                }
                if let Some(ip) = tokio::task::spawn_blocking(net_info::fetch_public_ipv6)
                    .await
                    .unwrap_or(None)
                {
                    let _ = ip_tx.send(AppEvent::PublicIpv6(ip));
                }
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        })
    };

    {
        let ip_tx = input_tx.clone();
        tokio::spawn(async move {
            if let Some(ip) = tokio::task::spawn_blocking(net_info::fetch_public_ip)
                .await
                .unwrap_or(None)
            {
                let _ = ip_tx.send(AppEvent::PublicIp(ip));
            }
            if let Some(ip) = tokio::task::spawn_blocking(net_info::fetch_public_ipv6)
                .await
                .unwrap_or(None)
            {
                let _ = ip_tx.send(AppEvent::PublicIpv6(ip));
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
    let _ = writeln!(stdout, "--- Press Enter ---");
    let _ = stdout.flush();

    config::save_config(&cfg);

    res
}
