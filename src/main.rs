mod config;
mod core_client;
mod ui;

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use log::{LevelFilter, Log, Metadata, Record};
use ratatui::backend::{Backend, CrosstermBackend};
use tokio::sync::mpsc;

use core_client::{CoreClient, CoreConnection, CoreEvent};
use core_client::protocol::DieData;
use core_client::protocol::SetHwidData;
use ui::app::{ActiveTab, Focus, Popup};
use ui::routing::{form_to_rule, RoutingPopup};
use ui::App;

// ---------------------------------------------------------------------------
// File-only logger (disabled by default, enabled via config)
// ---------------------------------------------------------------------------
struct FileLogger {
    file: Mutex<File>,
    enabled: AtomicBool,
    level: Mutex<LevelFilter>,
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.enabled.load(Ordering::Relaxed)
            && metadata.level() <= *self.level.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let ts = chrono::Local::now().format("%H:%M:%S");
            let _ = writeln!(
                self.file.lock().unwrap_or_else(|e| e.into_inner()),
                "{} {:5} {}",
                ts,
                record.level(),
                record.args()
            );
        }
    }
    fn flush(&self) {
        let _ = self.file.lock().unwrap_or_else(|e| e.into_inner()).flush();
    }
}

fn init_logger() -> &'static FileLogger {
    let log_dir = config::data_dir();
    let _ = std::fs::create_dir_all(&log_dir);
    let log_path = log_dir.join("tui.log");

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .or_else(|_| {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/whoisthat-tui.log")
        })
        .unwrap_or_else(|_| {
            // Final fallback: /dev/null so Mutex<File> is always valid
            OpenOptions::new()
                .create(true)
                .append(true)
                .open("/dev/null")
                .expect("failed to open /dev/null")
        });

    let logger: &'static FileLogger = Box::leak(Box::new(FileLogger {
        file: Mutex::new(file),
        enabled: AtomicBool::new(false),
        level: Mutex::new(LevelFilter::Warn),
    }));
    log::set_logger(logger).ok();
    log::set_max_level(LevelFilter::Trace);
    logger
}

fn configure_logger(logger: &FileLogger, enabled: bool, level: &str) {
    let lf = match level.to_lowercase().as_str() {
        "error" => LevelFilter::Error,
        "warn" => LevelFilter::Warn,
        "info" => LevelFilter::Info,
        "debug" => LevelFilter::Debug,
        "trace" => LevelFilter::Trace,
        _ => LevelFilter::Warn,
    };
    *logger.level.lock().unwrap_or_else(|e| e.into_inner()) = lf;
    logger.enabled.store(enabled, Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// Spawn core
// ---------------------------------------------------------------------------

enum AppEvent {
    Input(Event),
    Tick,
    PublicIp(String),
    PublicIpv6(String),
}

fn spawn_core(log_level: &str) -> io::Result<()> {
    let bin = find_core_binary();
    Command::new(&bin)
        .env("WHOISTHAT_LOG_LEVEL", log_level)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()?;
    log::info!("Spawned whoisthat-core from {}", bin);
    Ok(())
}

fn find_core_binary() -> String {
    let candidates = [
        "whoisthat-core",
        "./whoisthat-core",
        "./core/core/whoisthat-core",
        "./bin/whoisthat-core",
        "/usr/bin/whoisthat-core",
        "/usr/local/bin/whoisthat-core",
    ];
    for c in candidates {
        if std::path::Path::new(c).exists() {
            return c.to_string();
        }
    }
    "whoisthat-core".to_string()
}

fn has_cap_net(path: &std::path::Path) -> bool {
    if let Ok(output) = Command::new("getcap").arg(path).output() {
        String::from_utf8_lossy(&output.stdout).contains("cap_net_admin")
    } else {
        false
    }
}

fn ensure_core_caps(core_path: &str) {
    let core = std::path::absolute(core_path).unwrap_or_else(|_| core_path.into());

    if has_cap_net(&core) {
        return;
    }

    eprintln!("whoisthat: TUN mode needs network capabilities on core binary.");
    eprint!("Set up via pkexec? [Y/n] ");
    io::stderr().flush().ok();

    let caps = "cap_net_admin,cap_net_raw,cap_setpcap=+ep";
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        eprintln!("\nRun manually: sudo setcap {} {}", caps, core.display());
        return;
    }
    let answer = answer.trim().to_lowercase();
    if !answer.is_empty() && answer != "y" && answer != "yes" {
        eprintln!("Run manually: sudo setcap {} {}", caps, core.display());
        return;
    }

    let s = Command::new("pkexec")
        .arg("setcap")
        .arg(caps)
        .arg(core.as_os_str())
        .status();

    match s {
        Ok(s) if s.success() => eprintln!("whoisthat: capabilities set. TUN mode ready."),
        _ => eprintln!("whoisthat: failed. Run: sudo setcap {} {}", caps, core.display()),
    }
}

fn fetch_public_ip() -> Option<String> {
    let addr = "api.ipify.org:80"
        .to_socket_addrs()
        .ok()?
        .find(|a| a.is_ipv4())?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    stream.write_all(b"GET / HTTP/1.0\r\nHost: api.ipify.org\r\nConnection: close\r\n\r\n").ok()?;
    let mut buf = String::new();
    stream.read_to_string(&mut buf).ok()?;
    let body = buf.split("\r\n\r\n").nth(1)?;
    let ip = body.trim();
    if ip.is_empty() { None } else { Some(ip.to_string()) }
}

fn fetch_public_ipv6() -> Option<String> {
    let addr = "api6.ipify.org:80"
        .to_socket_addrs()
        .ok()?
        .next()?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    stream.write_all(b"GET / HTTP/1.0\r\nHost: api6.ipify.org\r\nConnection: close\r\n\r\n").ok()?;
    let mut buf = String::new();
    stream.read_to_string(&mut buf).ok()?;
    let body = buf.split("\r\n\r\n").nth(1)?;
    let ip = body.trim();
    if ip.is_empty() { None } else { Some(ip.to_string()) }
}

fn check_sudo_env() -> Option<&'static str> {
    if std::env::var("SUDO_UID").is_ok() && std::env::var("HOME").unwrap_or_default() == "/root" {
        Some("Restart with: sudo -E whoisthat")
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> io::Result<()> {
    let logger = init_logger();

    let mut cfg = config::load_config();
    configure_logger(logger, cfg.log_enabled, &cfg.log_level);
    let current_version = env!("CARGO_PKG_VERSION").to_string();

    // Check if core is running and version matches
    // Reattach if same version, kill+respawn if different, spawn if not running
    let mut core_alive = false;
    if let Ok(mut conn) = CoreConnection::connect(&cfg.core_host, cfg.core_tcp_port).await {
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
        ensure_core_caps(&find_core_binary());
        spawn_core(&cfg.log_level)?;
        // Wait for core to start listening (retry with backoff)
        let addr = format!("{}:{}", cfg.core_host, cfg.core_tcp_port);
        let mut retries = 0u32;
        loop {
            if tokio::net::TcpStream::connect(&addr).await.is_ok() {
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
    let conn = CoreConnection::connect(&cfg.core_host, cfg.core_tcp_port)
        .await
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::ConnectionRefused,
                format!("Failed to connect to core: {}", e),
            )
        })?;

    let client = CoreClient::new(conn);

    let read_conn = CoreConnection::connect(&cfg.core_host, cfg.core_tcp_port).await?;
    let mut core_rx = core_client::spawn_read_loop(read_conn);
    client.get_application_state().await?;
    let _ = client.get_routing().await;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut term = ratatui::Terminal::new(backend)?;

    let mut app = App::new(cfg.autoconnect, cfg.show_ip, cfg.log_enabled, cfg.log_level.clone(), cfg.test_method.clone());

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

    // Periodic public IP fetch (every 30 s)
    {
        let ip_tx = input_tx.clone();
        tokio::spawn(async move {
            loop {
                if let Some(ip) = tokio::task::spawn_blocking(fetch_public_ip).await.unwrap_or(None) {
                    let _ = ip_tx.send(AppEvent::PublicIp(ip));
                }
                if let Some(ip) = tokio::task::spawn_blocking(fetch_public_ipv6).await.unwrap_or(None) {
                    let _ = ip_tx.send(AppEvent::PublicIpv6(ip));
                }
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        })
    };

    // Initial IP fetch (non-blocking, result arrives later)
    {
        let ip_tx = input_tx.clone();
        tokio::spawn(async move {
            if let Some(ip) = tokio::task::spawn_blocking(fetch_public_ip).await.unwrap_or(None) {
                let _ = ip_tx.send(AppEvent::PublicIp(ip));
            }
            if let Some(ip) = tokio::task::spawn_blocking(fetch_public_ipv6).await.unwrap_or(None) {
                let _ = ip_tx.send(AppEvent::PublicIpv6(ip));
            }
        });
    }

    // Autoconnect after first app state arrives
    let do_autoconnect = cfg.autoconnect && cfg.last_profile_id != 0;

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

// ---------------------------------------------------------------------------
// Event loop
// ---------------------------------------------------------------------------

async fn run_loop<B: Backend>(
    term: &mut ratatui::Terminal<B>,
    app: &mut App,
    client: &CoreClient,
    core_rx: &mut mpsc::UnboundedReceiver<CoreEvent>,
    input_rx: &mut mpsc::UnboundedReceiver<AppEvent>,
    mut do_autoconnect: bool,
    cfg: &mut config::AppConfig,
    ip_tx: mpsc::UnboundedSender<AppEvent>,
    logger: &'static FileLogger,
) -> io::Result<()> {
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

// ---------------------------------------------------------------------------
// Core events
// ---------------------------------------------------------------------------

async fn handle_core_event(
    app: &mut App,
    client: &CoreClient,
    ev: CoreEvent,
    first_state: &mut bool,
    do_autoconnect: &mut bool,
    cfg: &mut config::AppConfig,
    ip_tx: &mpsc::UnboundedSender<AppEvent>,
) -> bool {
    match ev {
        CoreEvent::ApplicationState(s) => {
            let was_connected = s.connection_status.connection == "connected";
            app.apply_state(s);
            if *first_state {
                *first_state = false;
                if *do_autoconnect && !was_connected {
                    *do_autoconnect = false;
                    let gid = cfg.last_group_id;
                    let pid = cfg.last_profile_id;
                    if gid != 0 || pid != 0 {
                        let _ = client.connect(gid, pid).await;
                        app.msg("Autoconnecting...");
                    }
                }
            }
        }

        CoreEvent::StatusChanged(s) => {
            let was = app.is_connected();
            log::info!("StatusChanged: connected={}, connected_at={}, profile={:?}", 
                s.connection, s.connected_at, s.profile.as_ref().map(|p| (p.id, p.group_id)));
            app.connection_status = s;
            if was != app.is_connected() {
                app.clear_msg();
                let tx = ip_tx.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    if let Some(ip) = tokio::task::spawn_blocking(fetch_public_ip).await.unwrap_or(None) {
                        let _ = tx.send(AppEvent::PublicIp(ip));
                    }
                    if let Some(ip) = tokio::task::spawn_blocking(fetch_public_ipv6).await.unwrap_or(None) {
                        let _ = tx.send(AppEvent::PublicIpv6(ip));
                    }
                });
            }
            // Save last connected profile
            if app.is_connected() {
                if let Some(ref p) = app.connection_status.profile {
                    cfg.last_group_id = p.group_id;
                    cfg.last_profile_id = p.id;
                    config::save_config(cfg);
                }
            }
        }

        CoreEvent::ProfilesAdded(p) => {
            app.apply_profiles_added(p);
            app.popup = None;
            app.msg("Ok");
        }

        CoreEvent::ProfilesDeleted(d) => {
            app.apply_profiles_deleted(&d);
            app.popup = None;
            app.msg("Ok");
        }

        CoreEvent::ProfileUpdated(p) => {
            if p.test_result != -2 {
                app.clear_msg();
            }
            app.apply_profile_updated(&p);
        }

        CoreEvent::TunStatusChanged(e) => {
            app.tun_enabled = e;
            app.msg(if e {
                "Warning: TUN mode active"
            } else {
                "Ok"
            });
            let tx = ip_tx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if let Some(ip) = tokio::task::spawn_blocking(fetch_public_ip).await.unwrap_or(None) {
                    let _ = tx.send(AppEvent::PublicIp(ip));
                }
                if let Some(ip) = tokio::task::spawn_blocking(fetch_public_ipv6).await.unwrap_or(None) {
                    let _ = tx.send(AppEvent::PublicIpv6(ip));
                }
            });
        }

        CoreEvent::IsRootAnswer(root) => {
            if root {
                app.msg("Ok");
                let _ = client.enable_tun().await;
            } else {
                app.msg("Error: no TUN permission. Run: sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep /path/to/whoisthat-core");
            }
        }

        CoreEvent::Warning { content, .. } => {
            app.msg(format!("Warning: {}", content));
        }

        CoreEvent::Error(e) => {
            app.msg(format!("Error: {}", e));
        }

        CoreEvent::TrafficStats(ts) => {
            app.traffic_stats = ts;
        }

        CoreEvent::Disconnected => {
            app.msg("Error: Core disconnected. Press q to quit.");
        }
        CoreEvent::SubscriptionUpdated { group, profiles } => {
            app.apply_subscription_updated(group, profiles);
            app.msg("Subscription updated");
        }

        CoreEvent::GroupAdded(g) => {
            app.apply_group_added(g);
        }

        CoreEvent::GroupDeleted(id) => {
            app.apply_group_deleted(id);
            app.popup = None;
            app.msg("Group deleted");
        }

        CoreEvent::GroupUpdated(g) => {
            app.apply_group_updated(&g);
            app.msg("Group updated");
        }

        CoreEvent::RoutingUpdated(cfg) => {
            app.routing = cfg;
            let len = app.routing.rules.len();
            if app.routing_cursor >= len && len > 0 {
                app.routing_cursor = len - 1;
            }
        }
        CoreEvent::HwidUpdated(hw) => {
            app.hwid_info = Some(hw);
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

async fn handle_input(
    app: &mut App,
    client: &CoreClient,
    ev: AppEvent,
    cfg: &mut config::AppConfig,
    logger: &'static FileLogger,
) -> bool {
    match ev {
        AppEvent::Tick => {
            app.logs_state.poll();
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
        AppEvent::Input(input) => match input {
            Event::Key(key) => {
                if key.kind == KeyEventKind::Release {
                    return false;
                }

                if key.code == KeyCode::Char('q')
                    && app.popup.is_none()
                    && !matches!(key.modifiers, crossterm::event::KeyModifiers::CONTROL)
                {
                    return true;
                }

                if (key.code == KeyCode::Char('Q')
                    || (key.code == KeyCode::Char('c')
                        && key.modifiers == crossterm::event::KeyModifiers::CONTROL))
                    && app.popup.is_none()
                {
                    let _ = client.die().await;
                    return true;
                }

                if app.popup.is_some() {
                    return handle_popup_input(app, client, key).await;
                }

                return handle_normal_input(app, client, key, cfg, logger).await;
            }
            Event::Mouse(mouse) => {
                // Mouse not used for navigation in current version
                let _ = mouse;
            }
            _ => {}
        },
    }
    false
}

async fn handle_popup_input(
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
        Some(Popup::Import { mut input, mut cursor }) => match key.code {
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
        Some(Popup::EditUserAgent { mut input, mut cursor }) => match key.code {
            KeyCode::Esc => {
                app.popup = None;
                app.focus = Focus::LeftPanel;
            }
            KeyCode::Enter => {
                let ua = input.trim().to_string();
                if !ua.is_empty() {
                    let _ = client.set_hwid(&SetHwidData {
                        user_agent: Some(ua),
                        ..Default::default()
                    }).await;
                }
                app.popup = None;
                app.focus = Focus::LeftPanel;
            }
            _ => {
                edit_text_field(&mut input, &mut cursor, key);
                app.popup = Some(Popup::EditUserAgent { input, cursor });
            }
        },
        Some(Popup::ConfirmDelete { gid, pid, .. }) => match key.code {
            KeyCode::Enter => {
                let _ = client
                    .delete_profiles(&[core_client::protocol::ProfileID {
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
        Some(Popup::EditSubscription { mut name, mut url, group_id, mut cursor, mut field }) => {
            let consumed = handle_two_field_popup(
                &mut name, &mut url, &mut cursor, &mut field, key,
            );
            if consumed {
                let _ = client.update_group(group_id, &name, &url).await;
                app.msg("Updating group...");
                app.focus = Focus::LeftPanel;
            } else if key.code == KeyCode::Esc {
                app.popup = None;
                app.focus = Focus::LeftPanel;
            } else {
                app.popup = Some(Popup::EditSubscription { name, url, group_id, cursor, field });
            }
        }
        Some(Popup::AddGroup { mut name, mut url, mut cursor, mut field }) => {
            let consumed = handle_two_field_popup(
                &mut name, &mut url, &mut cursor, &mut field, key,
            );
            if consumed {
                let _ = client.add_group(&name, &url).await;
                app.msg("Adding group...");
                app.focus = Focus::LeftPanel;
            } else if key.code == KeyCode::Esc {
                app.popup = None;
                app.focus = Focus::LeftPanel;
            } else {
                app.popup = Some(Popup::AddGroup { name, url, cursor, field });
            }
        }
        None => {}
    }
    false
}

async fn handle_routing_popup_input(
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
        Some(RoutingPopup::Add { mut match_type, mut value, mut outbound, mut cursor, mut field }) => {
            let save = handle_routing_form(app, &mut match_type, &mut value, &mut outbound, &mut cursor, &mut field, key);
            if save {
                let rule = form_to_rule(match_type, &value, outbound);
                app.routing.rules.push(rule);
                let _ = client.update_routing(&app.routing).await;
            } else {
                app.routing_popup = Some(RoutingPopup::Add { match_type, value, outbound, cursor, field });
            }
        }
        Some(RoutingPopup::Edit { index, mut match_type, mut value, mut outbound, mut cursor, mut field }) => {
            let save = handle_routing_form(app, &mut match_type, &mut value, &mut outbound, &mut cursor, &mut field, key);
            if save {
                let rule = form_to_rule(match_type, &value, outbound);
                app.routing.rules[index] = rule;
                let _ = client.update_routing(&app.routing).await;
            } else {
                app.routing_popup = Some(RoutingPopup::Edit { index, match_type, value, outbound, cursor, field });
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
            *cursor = if *field == 0 { field0.len() } else { field1.len() };
            false
        }
        KeyCode::Enter => {
            if *field == 0 {
                *field = 1;
                *cursor = field1.len();
                false
            } else {
                true // save
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
    // We use repop only to reconstruct the popup; the actual save is in the caller.
    match key.code {
        KeyCode::Esc => { return false; }
        KeyCode::Tab => {
            *field = (*field + 1) % 3;
            *cursor = if *field == 1 { value.len() } else { 0 };
        }
        KeyCode::Enter => {
            if *field < 2 {
                *field += 1;
                *cursor = if *field == 1 { value.len() } else { 0 };
            } else {
                return true; // save
            }
        }
        KeyCode::Char(c) => {
            if *field == 0 {
                *match_type = (*match_type + 1) % 6;
            } else if *field == 2 {
                *outbound = (*outbound + 1) % 3;
            } else {
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
        KeyCode::Left => {
            if *field == 1 {
                *cursor = if *cursor > 0 { *cursor - 1 } else { 0 };
            }
        }
        KeyCode::Right => {
            if *field == 1 && *cursor < value.len() {
                *cursor += 1;
            }
        }
        KeyCode::Home => {
            if *field == 1 { *cursor = 0; }
        }
        KeyCode::End => {
            if *field == 1 { *cursor = value.len(); }
        }
        _ => {}
    }
    false
}

fn build_test_list(app: &ui::App, focused_only: bool) -> Vec<(i32, i32)> {
    let mut list = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut add_profile = |gid: i32, pid: i32| {
        if seen.insert((gid, pid)) {
            list.push((gid, pid));
        }
    };

    if focused_only {
        if app.on_group() {
            if let Some(g) = app.current_group() {
                for p in &g.profiles {
                    add_profile(g.group.id, p.id);
                }
            }
        } else if let Some(p) = app.selected_profile() {
            add_profile(p.group_id, p.id);
        }
        return list;
    }

    // Scan all: start with cursor item, then everything top-to-bottom
    if let Some(p) = app.selected_profile() {
        add_profile(p.group_id, p.id);
    } else if let Some(g) = app.current_group() {
        for p in &g.profiles {
            add_profile(g.group.id, p.id);
        }
    }

    for g in &app.groups {
        for p in &g.profiles {
            add_profile(g.group.id, p.id);
        }
    }

    list
}

async fn handle_normal_input(
    app: &mut App,
    client: &CoreClient,
    key: event::KeyEvent,
    cfg: &mut config::AppConfig,
    logger: &'static FileLogger,
) -> bool {
    app.clear_msg();

    // Routing tab keys (before global handlers so 'a' etc. are intercepted)
    if app.tab == ActiveTab::Routing {
        if app.routing_popup.is_some() {
            handle_routing_popup_input(app, client, key).await;
            return false;
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                let len = app.routing.rules.len();
                if len > 0 && app.routing_cursor + 1 < len {
                    app.routing_cursor += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                app.routing_cursor = app.routing_cursor.saturating_sub(1);
            }
            KeyCode::Char('a') => {
                app.routing_popup = Some(RoutingPopup::Add {
                    match_type: 0,
                    value: String::new(),
                    outbound: 0,
                    cursor: 0,
                    field: 0,
                });
                return false;
            }
            KeyCode::Char('e') => {
                if let Some(rule) = app.routing.rules.get(app.routing_cursor) {
                    let (mt, val, ob) = ui::routing::rule_to_form(rule);
                    let cursor = val.len();
                    app.routing_popup = Some(RoutingPopup::Edit {
                        index: app.routing_cursor,
                        match_type: mt,
                        value: val,
                        outbound: ob,
                        cursor,
                        field: 0,
                    });
                }
                return false;
            }
            KeyCode::Char('x') => {
                if app.routing.rules.get(app.routing_cursor).is_some() {
                    app.routing_popup = Some(RoutingPopup::ConfirmDelete {
                        index: app.routing_cursor,
                    });
                }
                return false;
            }
            KeyCode::Char(' ') => {
                if let Some(rule) = app.routing.rules.get_mut(app.routing_cursor) {
                    rule.enabled = !rule.enabled;
                    let _ = client.update_routing(&app.routing).await;
                }
                return false;
            }
            _ => {}
        }
    }

    // Global / tab-bar keys
    match key.code {
        KeyCode::Char('a') => {
            let clip = read_clipboard();
            if let Some(text) = clip {
                if !text.is_empty() {
                    let gid = app.current_group().map(|g| g.group.id).unwrap_or(0);
                    let _ = client.add_profiles(&text, gid).await;
                    app.msg("Importing from clipboard...");
                    return false;
                }
            }
            app.popup = Some(Popup::Import {
                input: String::new(),
                cursor: 0,
            });
            app.focus = Focus::Popup;
            return false;
        }
        KeyCode::Char('l') => {
            app.tab = ActiveTab::Logs;
            app.focus = Focus::LeftPanel;
            return false;
        }
        KeyCode::Char('r') => {
            app.routing_popup = None;
            app.tab = ActiveTab::Routing;
            app.focus = Focus::LeftPanel;
            let _ = client.get_routing().await;
            return false;
        }
        KeyCode::Char('s') => {
            app.tab = ActiveTab::Settings;
            app.focus = Focus::LeftPanel;
            return false;
        }
        KeyCode::Char('v') => {
            if app.tun_enabled {
                let _ = client.disable_tun().await;
            } else {
                let _ = client.is_root().await;
                app.msg("Checking root...");
            }
            return false;
        }
        KeyCode::Char('1') | KeyCode::Esc => {
            app.tab = ActiveTab::Profiles;
            return false;
        }
        KeyCode::Char('h') | KeyCode::Char('?') => {
            app.help_scroll = 0;
            app.popup = Some(Popup::Help);
            app.focus = Focus::Popup;
            return false;
        }
        _ => {}
    }

    // Settings tab keys
    if app.tab == ActiveTab::Settings {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.settings_state.cursor_down(8),
            KeyCode::Char('k') | KeyCode::Up => app.settings_state.cursor_up(),
            KeyCode::Char(' ') | KeyCode::Enter => {
                match app.settings_state.cursor() {
                    0 => {
                        app.autoconnect = !app.autoconnect;
                        cfg.autoconnect = app.autoconnect;
                        config::save_config(cfg);
                    }
                    1 => {
                        app.show_ip = !app.show_ip;
                        cfg.show_ip = app.show_ip;
                        if !app.show_ip {
                            app.public_ip = String::new();
                            app.public_ipv6 = String::new();
                        }
                        config::save_config(cfg);
                    }
                    2 => {
                        app.log_enabled = !app.log_enabled;
                        cfg.log_enabled = app.log_enabled;
                        config::save_config(cfg);
                        configure_logger(logger, cfg.log_enabled, &cfg.log_level);
                    }
                    5 => {
                        if let Some(ref hw) = app.hwid_info {
                            let _ = client.set_hwid(&SetHwidData {
                                enabled: Some(!hw.enabled),
                                ..Default::default()
                            }).await;
                        }
                    }
                    7 => {
                        let _ = client.set_hwid(&SetHwidData {
                            reset: true,
                            ..Default::default()
                        }).await;
                    }
                    8 => {
                        if let Some(ref hw) = app.hwid_info {
                            app.popup = Some(Popup::EditUserAgent {
                                input: hw.user_agent.clone(),
                                cursor: hw.user_agent.len(),
                            });
                            app.focus = Focus::Popup;
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if app.settings_state.cursor() == 3 {
                    let levels = ["error", "warn", "info", "debug", "trace"];
                    let current = levels.iter().position(|l| *l == app.log_level.as_str()).unwrap_or(1);
                    let next = (current + 1) % levels.len();
                    app.log_level = levels[next].to_string();
                    cfg.log_level = app.log_level.clone();
                    config::save_config(cfg);
                    configure_logger(logger, cfg.log_enabled, &cfg.log_level);
                }
                if app.settings_state.cursor() == 4 {
                    let methods = ["tcp", "http-get", "http-head"];
                    let current = methods.iter().position(|m| *m == app.test_method.as_str()).unwrap_or(1);
                    let next = (current + 1) % methods.len();
                    app.test_method = methods[next].to_string();
                    cfg.test_method = app.test_method.clone();
                    config::save_config(cfg);
                }
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if app.settings_state.cursor() == 3 {
                    let levels = ["error", "warn", "info", "debug", "trace"];
                    let current = levels.iter().position(|l| *l == app.log_level.as_str()).unwrap_or(1);
                    let prev = if current == 0 { levels.len() - 1 } else { current - 1 };
                    app.log_level = levels[prev].to_string();
                    cfg.log_level = app.log_level.clone();
                    config::save_config(cfg);
                    configure_logger(logger, cfg.log_enabled, &cfg.log_level);
                }
                if app.settings_state.cursor() == 4 {
                    let methods = ["tcp", "http-get", "http-head"];
                    let current = methods.iter().position(|m| *m == app.test_method.as_str()).unwrap_or(1);
                    let prev = if current == 0 { methods.len() - 1 } else { current - 1 };
                    app.test_method = methods[prev].to_string();
                    cfg.test_method = app.test_method.clone();
                    config::save_config(cfg);
                }
            }
            _ => {}
        }
        return false;
    }

    // Logs tab keys
    if app.tab == ActiveTab::Logs {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.logs_state.scroll_down(),
            KeyCode::Char('k') | KeyCode::Up => app.logs_state.scroll_up(),
            KeyCode::Char('g') => app.logs_state.scroll_top(),
            KeyCode::Char('G') => app.logs_state.scroll_bottom(),
            _ => {}
        }
        return false;
    }

    // Focus switching
    if key.code == KeyCode::Tab {
        app.focus = match app.focus {
            Focus::LeftPanel => Focus::RightPanel,
            Focus::RightPanel => Focus::LeftPanel,
            Focus::Popup => Focus::LeftPanel,
        };
        return false;
    }

    match app.focus {
        Focus::LeftPanel => match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.cursor_down(),
            KeyCode::Char('k') | KeyCode::Up => app.cursor_up(),
            KeyCode::Char('g') => app.cursor_top(),
            KeyCode::Char('G') => app.cursor_bottom(),
            KeyCode::Char('c') | KeyCode::Enter => {
                if let Some(p) = app.selected_profile() {
                    let _ = client.connect(p.group_id, p.id).await;
                    app.msg("Connecting...");
                }
            }
            KeyCode::Char('d') => {
                let _ = client.disconnect().await;
                app.msg("Disconnecting...");
            }
            KeyCode::Char('u') => {
                if let Some(g) = app.current_group() {
                    if !g.group.subscription_url.is_empty() {
                        let _ = client.update_subscription(g.group.id).await;
                        app.msg("Updating subscription...");
                    }
                }
            }
            KeyCode::Char('e') => {
                if let Some(g) = app.current_group() {
                    app.popup = Some(Popup::EditSubscription {
                        name: g.group.name.clone(),
                        url: g.group.subscription_url.clone(),
                        group_id: g.group.id,
                        cursor: g.group.subscription_url.len(),
                        field: 1,
                    });
                    app.focus = Focus::Popup;
                }
            }
            KeyCode::Char('U') => {
                let default_name = format!("Group {}", app.groups.len() + 1);
                app.popup = Some(Popup::AddGroup {
                    name: default_name,
                    url: String::new(),
                    cursor: 0,
                    field: 0,
                });
                app.focus = Focus::Popup;
            }
            KeyCode::Char('t') => {
                let method = app.test_method.clone();
                let list = build_test_list(app, false);
                for (gid, pid) in &list {
                    let _ = client.test_profile(*gid, *pid, &method).await;
                }
                app.msg(format!("Testing {} profiles...", list.len()));
            }
            KeyCode::Char('T') => {
                let method = app.test_method.clone();
                let list = build_test_list(app, true);
                for (gid, pid) in &list {
                    let _ = client.test_profile(*gid, *pid, &method).await;
                }
                app.msg(format!("Testing {} profiles...", list.len()));
            }
            KeyCode::Char('x') => {
                if let Some(p) = app.selected_profile() {
                    let name = if p.name.is_empty() {
                        if p.address.is_empty() {
                            "Unknown".to_string()
                        } else {
                            p.address.clone()
                        }
                    } else {
                        p.name.clone()
                    };
                    app.popup = Some(Popup::ConfirmDelete {
                        gid: p.group_id,
                        pid: p.id,
                        name,
                    });
                    app.focus = Focus::Popup;
                }
            }
            KeyCode::Char('X') => {
                if let Some(g) = app.current_group() {
                    app.popup = Some(Popup::ConfirmDeleteGroup {
                        gid: g.group.id,
                        name: g.group.name.clone(),
                    });
                    app.focus = Focus::Popup;
                }
            }
            _ => {}
        },
        Focus::RightPanel => match key.code {
            KeyCode::Char('c') | KeyCode::Enter => {
                if let Some(p) = app.selected_profile() {
                    let _ = client.connect(p.group_id, p.id).await;
                    app.msg("Connecting...");
                }
            }
            KeyCode::Char('d') => {
                let _ = client.disconnect().await;
                app.msg("Disconnecting...");
            }
            KeyCode::Char('j') | KeyCode::Down => app.cursor_down(),
            KeyCode::Char('k') | KeyCode::Up => app.cursor_up(),
            KeyCode::Char('t') => {
                let method = app.test_method.clone();
                let list = build_test_list(app, false);
                for (gid, pid) in &list {
                    let _ = client.test_profile(*gid, *pid, &method).await;
                }
                app.msg(format!("Testing {} profiles...", list.len()));
            }
            KeyCode::Char('T') => {
                let method = app.test_method.clone();
                let list = build_test_list(app, true);
                for (gid, pid) in &list {
                    let _ = client.test_profile(*gid, *pid, &method).await;
                }
                app.msg(format!("Testing {} profiles...", list.len()));
            }
            _ => {}
        },
        Focus::Popup => {}
    }

    false
}

fn read_clipboard() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

fn edit_text_field(s: &mut String, cursor: &mut usize, key: event::KeyEvent) -> bool {
    match key.code {
        KeyCode::Char(c) if c == 'v'
            && matches!(key.modifiers, crossterm::event::KeyModifiers::CONTROL) =>
        {
            if let Some(clip) = read_clipboard() {
                *s = clip;
                *cursor = s.len();
            }
            false
        }
        KeyCode::Char(c) => {
            if *cursor <= s.len() {
                s.insert(*cursor, c);
            } else {
                s.push(c);
            }
            *cursor += 1;
            false
        }
        KeyCode::Backspace => {
            if *cursor > 0 && !s.is_empty() {
                s.remove(*cursor - 1);
                *cursor -= 1;
            }
            false
        }
        KeyCode::Delete => {
            if *cursor < s.len() {
                s.remove(*cursor);
            }
            false
        }
        KeyCode::Left => {
            *cursor = cursor.saturating_sub(1);
            false
        }
        KeyCode::Right => {
            if *cursor < s.len() {
                *cursor += 1;
            }
            false
        }
        KeyCode::Home => {
            *cursor = 0;
            false
        }
        KeyCode::End => {
            *cursor = s.len();
            false
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn key_ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    // --- edit_text_field ---

    #[test]
    fn insert_char_at_end() {
        let mut s = String::from("ab");
        let mut c = 2usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Char('c')));
        assert_eq!(s, "abc");
        assert_eq!(c, 3);
    }

    #[test]
    fn insert_char_at_start() {
        let mut s = String::from("bc");
        let mut c = 0usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Char('a')));
        assert_eq!(s, "abc");
        assert_eq!(c, 1);
    }

    #[test]
    fn insert_char_in_middle() {
        let mut s = String::from("ac");
        let mut c = 1usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Char('b')));
        assert_eq!(s, "abc");
        assert_eq!(c, 2);
    }

    #[test]
    fn backspace_removes_char() {
        let mut s = String::from("abc");
        let mut c = 3usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Backspace));
        assert_eq!(s, "ab");
        assert_eq!(c, 2);
    }

    #[test]
    fn backspace_at_start_is_noop() {
        let mut s = String::from("abc");
        let mut c = 0usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Backspace));
        assert_eq!(s, "abc");
        assert_eq!(c, 0);
    }

    #[test]
    fn backspace_in_middle() {
        let mut s = String::from("abc");
        let mut c = 2usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Backspace));
        assert_eq!(s, "ac");
        assert_eq!(c, 1);
    }

    #[test]
    fn delete_removes_char_at_cursor() {
        let mut s = String::from("abc");
        let mut c = 1usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Delete));
        assert_eq!(s, "ac");
        assert_eq!(c, 1);
    }

    #[test]
    fn delete_at_end_is_noop() {
        let mut s = String::from("abc");
        let mut c = 3usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Delete));
        assert_eq!(s, "abc");
        assert_eq!(c, 3);
    }

    #[test]
    fn left_moves_cursor() {
        let mut s = String::from("abc");
        let mut c = 2usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Left));
        assert_eq!(c, 1);
        assert_eq!(s, "abc");
    }

    #[test]
    fn left_at_start_is_noop() {
        let mut s = String::from("abc");
        let mut c = 0usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Left));
        assert_eq!(c, 0);
    }

    #[test]
    fn right_moves_cursor() {
        let mut s = String::from("abc");
        let mut c = 1usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Right));
        assert_eq!(c, 2);
    }

    #[test]
    fn right_at_end_is_noop() {
        let mut s = String::from("abc");
        let mut c = 3usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Right));
        assert_eq!(c, 3);
    }

    #[test]
    fn home_moves_to_start() {
        let mut s = String::from("abc");
        let mut c = 3usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::Home));
        assert_eq!(c, 0);
    }

    #[test]
    fn end_moves_to_end() {
        let mut s = String::from("abc");
        let mut c = 0usize;
        edit_text_field(&mut s, &mut c, key(KeyCode::End));
        assert_eq!(c, 3);
    }

    #[test]
    fn ctrl_v_is_not_insert() {
        // Ctrl+V tries clipboard; without clipboard it's a no-op — just check no panic
        let mut s = String::from("abc");
        let mut c = 3usize;
        edit_text_field(&mut s, &mut c, key_ctrl(KeyCode::Char('v')));
        // s may or may not change depending on clipboard; just assert no panic and cursor <= len
        assert!(c <= s.len());
    }
}
