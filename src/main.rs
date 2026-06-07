mod config;
mod core_client;
mod ui;

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::os::unix::process::CommandExt;
use std::process::Command;
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
use ui::app::{ActiveTab, Focus, Popup};
use ui::App;

// ---------------------------------------------------------------------------
// File-only logger
// ---------------------------------------------------------------------------
struct FileLogger {
    file: Mutex<File>,
}

impl Log for FileLogger {
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }
    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let ts = chrono::Local::now().format("%H:%M:%S");
            let _ = writeln!(
                self.file.lock().unwrap(),
                "{} {:5} {}",
                ts,
                record.level(),
                record.args()
            );
        }
    }
    fn flush(&self) {
        let _ = self.file.lock().unwrap().flush();
    }
}

fn init_logger() {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("whoisthat.log")
        .or_else(|_| {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/whoisthat.log")
        })
        .expect("failed to open log file");

    let logger: &'static dyn Log = Box::leak(Box::new(FileLogger {
        file: Mutex::new(file),
    }));
    log::set_logger(logger)
        .map(|()| log::set_max_level(LevelFilter::Info))
        .ok();
}

// ---------------------------------------------------------------------------
// Spawn core
// ---------------------------------------------------------------------------

enum AppEvent {
    Input(Event),
    Tick,
    PublicIp(String),
}

fn spawn_core() -> io::Result<()> {
    let bin = find_core_binary();
    Command::new(&bin)
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

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> io::Result<()> {
    init_logger();

    let mut cfg = config::load_config();
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
        spawn_core()?;
        tokio::time::sleep(Duration::from_millis(1200)).await;
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

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut term = ratatui::Terminal::new(backend)?;

    let mut app = App::new(cfg.autoconnect);

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
                if handle_input(app, client, ev, cfg).await {
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
            });
        }

        CoreEvent::IsRootAnswer(root) => {
            if root {
                app.msg("Ok");
                let _ = client.enable_tun().await;
            } else {
                app.msg("Error: need root for TUN. Restart with sudo.");
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
) -> bool {
    match ev {
        AppEvent::Tick => {
            app.logs_state.poll();
        }
        AppEvent::PublicIp(ip) => {
            app.public_ip = ip;
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

                return handle_normal_input(app, client, key, cfg).await;
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
        Some(Popup::Help) => {
            app.popup = None;
            app.focus = Focus::LeftPanel;
        }
        Some(Popup::Import { input, cursor }) => match key.code {
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
            KeyCode::Char(c) => {
                if c == 'v'
                    && matches!(key.modifiers, crossterm::event::KeyModifiers::CONTROL)
                {
                    if let Some(clip) = read_clipboard() {
                        app.popup = Some(Popup::Import {
                            input: clip,
                            cursor: 0,
                        });
                    } else {
                        app.popup = Some(Popup::Import { input, cursor });
                    }
                } else {
                    let mut s = input;
                    let mut cur = cursor;
                    if cur <= s.len() {
                        s.insert(cur, c);
                    } else {
                        s.push(c);
                    }
                    cur += 1;
                    app.popup = Some(Popup::Import {
                        input: s,
                        cursor: cur,
                    });
                }
            }
            KeyCode::Backspace => {
                let mut cur = cursor;
                if cur > 0 && !input.is_empty() {
                    let mut s = input;
                    s.remove(cur - 1);
                    cur -= 1;
                    app.popup = Some(Popup::Import {
                        input: s,
                        cursor: cur,
                    });
                } else {
                    app.popup = Some(Popup::Import { input, cursor });
                }
            }
            KeyCode::Delete => {
                if cursor < input.len() {
                    let mut s = input;
                    s.remove(cursor);
                    app.popup = Some(Popup::Import { input: s, cursor });
                } else {
                    app.popup = Some(Popup::Import { input, cursor });
                }
            }
            KeyCode::Left => {
                let cur = if cursor > 0 { cursor - 1 } else { 0 };
                app.popup = Some(Popup::Import {
                    input,
                    cursor: cur,
                });
            }
            KeyCode::Right => {
                let cur = if cursor < input.len() {
                    cursor + 1
                } else {
                    cursor
                };
                app.popup = Some(Popup::Import {
                    input,
                    cursor: cur,
                });
            }
            KeyCode::Home => {
                app.popup = Some(Popup::Import { input, cursor: 0 });
            }
            KeyCode::End => {
                let len = input.len();
                app.popup = Some(Popup::Import {
                    input,
                    cursor: len,
                });
            }
            _ => {
                app.popup = Some(Popup::Import { input, cursor });
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
        Some(Popup::EditSubscription { mut name, mut url, group_id, mut cursor, mut field }) => match key.code {
            KeyCode::Esc => {
                app.popup = None;
                app.focus = Focus::LeftPanel;
            }
            KeyCode::Tab => {
                field = if field == 0 { 1 } else { 0 };
                cursor = if field == 0 { name.len() } else { url.len() };
                app.popup = Some(Popup::EditSubscription { name, url, group_id, cursor, field });
            }
            KeyCode::Enter => {
                if field == 0 {
                    field = 1;
                    cursor = url.len();
                    app.popup = Some(Popup::EditSubscription { name, url, group_id, cursor, field });
                } else {
                    let _ = client.update_group(group_id, &name, &url).await;
                    app.msg("Updating group...");
                    app.focus = Focus::LeftPanel;
                }
            }
            KeyCode::Char(c) => {
                if c == 'v'
                    && matches!(key.modifiers, crossterm::event::KeyModifiers::CONTROL)
                {
                    if let Some(clip) = read_clipboard() {
                        if field == 0 { name = clip; cursor = name.len(); }
                        else { url = clip; cursor = url.len(); }
                    }
                } else {
                    if field == 0 {
                        if cursor <= name.len() { name.insert(cursor, c); } else { name.push(c); }
                    } else {
                        if cursor <= url.len() { url.insert(cursor, c); } else { url.push(c); }
                    }
                    cursor += 1;
                }
                app.popup = Some(Popup::EditSubscription { name, url, group_id, cursor, field });
            }
            KeyCode::Backspace => {
                if field == 0 && cursor > 0 && !name.is_empty() {
                    name.remove(cursor - 1);
                    cursor -= 1;
                } else if field == 1 && cursor > 0 && !url.is_empty() {
                    url.remove(cursor - 1);
                    cursor -= 1;
                }
                app.popup = Some(Popup::EditSubscription { name, url, group_id, cursor, field });
            }
            KeyCode::Delete => {
                if field == 0 && cursor < name.len() {
                    name.remove(cursor);
                } else if field == 1 && cursor < url.len() {
                    url.remove(cursor);
                }
                app.popup = Some(Popup::EditSubscription { name, url, group_id, cursor, field });
            }
            KeyCode::Left => {
                cursor = if cursor > 0 { cursor - 1 } else { 0 };
                app.popup = Some(Popup::EditSubscription { name, url, group_id, cursor, field });
            }
            KeyCode::Right => {
                let max = if field == 0 { name.len() } else { url.len() };
                if cursor < max { cursor += 1; }
                app.popup = Some(Popup::EditSubscription { name, url, group_id, cursor, field });
            }
            KeyCode::Home => {
                app.popup = Some(Popup::EditSubscription { name, url, group_id, cursor: 0, field });
            }
            KeyCode::End => {
                cursor = if field == 0 { name.len() } else { url.len() };
                app.popup = Some(Popup::EditSubscription { name, url, group_id, cursor, field });
            }
            _ => {
                app.popup = Some(Popup::EditSubscription { name, url, group_id, cursor, field });
            }
        },
        Some(Popup::AddGroup { mut name, mut url, mut cursor, mut field }) => match key.code {
            KeyCode::Esc => {
                app.popup = None;
                app.focus = Focus::LeftPanel;
            }
            KeyCode::Tab => {
                field = if field == 0 { 1 } else { 0 };
                cursor = if field == 0 { name.len() } else { url.len() };
                app.popup = Some(Popup::AddGroup { name, url, cursor, field });
            }
            KeyCode::Enter => {
                if field == 0 {
                    field = 1;
                    cursor = url.len();
                    app.popup = Some(Popup::AddGroup { name, url, cursor, field });
                } else {
                    let _ = client.add_group(&name, &url).await;
                    app.msg("Adding group...");
                    app.focus = Focus::LeftPanel;
                }
            }
            KeyCode::Char(c) => {
                if c == 'v'
                    && matches!(key.modifiers, crossterm::event::KeyModifiers::CONTROL)
                {
                    if let Some(clip) = read_clipboard() {
                        if field == 0 { name = clip; cursor = name.len(); }
                        else { url = clip; cursor = url.len(); }
                    }
                } else {
                    if field == 0 {
                        if cursor <= name.len() { name.insert(cursor, c); } else { name.push(c); }
                    } else {
                        if cursor <= url.len() { url.insert(cursor, c); } else { url.push(c); }
                    }
                    cursor += 1;
                }
                app.popup = Some(Popup::AddGroup { name, url, cursor, field });
            }
            KeyCode::Backspace => {
                if field == 0 && cursor > 0 && !name.is_empty() {
                    name.remove(cursor - 1);
                    cursor -= 1;
                } else if field == 1 && cursor > 0 && !url.is_empty() {
                    url.remove(cursor - 1);
                    cursor -= 1;
                }
                app.popup = Some(Popup::AddGroup { name, url, cursor, field });
            }
            KeyCode::Delete => {
                if field == 0 && cursor < name.len() {
                    name.remove(cursor);
                } else if field == 1 && cursor < url.len() {
                    url.remove(cursor);
                }
                app.popup = Some(Popup::AddGroup { name, url, cursor, field });
            }
            KeyCode::Left => {
                cursor = if cursor > 0 { cursor - 1 } else { 0 };
                app.popup = Some(Popup::AddGroup { name, url, cursor, field });
            }
            KeyCode::Right => {
                let max = if field == 0 { name.len() } else { url.len() };
                if cursor < max { cursor += 1; }
                app.popup = Some(Popup::AddGroup { name, url, cursor, field });
            }
            KeyCode::Home => {
                app.popup = Some(Popup::AddGroup { name, url, cursor: 0, field });
            }
            KeyCode::End => {
                cursor = if field == 0 { name.len() } else { url.len() };
                app.popup = Some(Popup::AddGroup { name, url, cursor, field });
            }
            _ => {
                app.popup = Some(Popup::AddGroup { name, url, cursor, field });
            }
        },
        None => {}
    }
    false
}

async fn handle_normal_input(
    app: &mut App,
    client: &CoreClient,
    key: event::KeyEvent,
    cfg: &mut config::AppConfig,
) -> bool {
    app.clear_msg();

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
            app.popup = Some(Popup::Help);
            app.focus = Focus::Popup;
            return false;
        }
        _ => {}
    }

    // Settings tab keys
    if app.tab == ActiveTab::Settings {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.settings_state.cursor_down(),
            KeyCode::Char('k') | KeyCode::Up => app.settings_state.cursor_up(),
            KeyCode::Char(' ') | KeyCode::Enter => {
                match app.settings_state.cursor {
                    0 => {
                        app.autoconnect = !app.autoconnect;
                        cfg.autoconnect = app.autoconnect;
                        config::save_config(cfg);
                    }
                    _ => {}
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
                if let Some(p) = app.selected_profile() {
                    let _ = client.test_profile(p.group_id, p.id).await;
                    app.msg("Testing...");
                }
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
            _ => {}
        },
        Focus::Popup => {}
    }

    false
}

fn read_clipboard() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}
