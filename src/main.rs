mod config;
mod core_client;
mod ui;

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
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

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> io::Result<()> {
    init_logger();

    let mut cfg = config::load_config();

    let conn = match CoreConnection::connect(&cfg.core_host, cfg.core_tcp_port).await {
        Ok(c) => {
            log::info!("Connected to core");
            c
        }
        Err(e) => {
            log::info!("Core not found ({}), spawning...", e);
            spawn_core()?;
            tokio::time::sleep(Duration::from_millis(1200)).await;
            CoreConnection::connect(&cfg.core_host, cfg.core_tcp_port)
                .await
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::ConnectionRefused,
                        format!("Failed to connect after spawn: {}", e),
                    )
                })?
        }
    };

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
    )
    .await;

    disable_raw_mode()?;
    execute!(
        term.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    term.show_cursor()?;

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
) -> io::Result<()> {
    let mut first_state = true;
    loop {
        term.draw(|f| app.render(f))?;

        tokio::select! {
            Some(ev) = core_rx.recv() => {
                if handle_core_event(app, client, ev, &mut first_state, &mut do_autoconnect, cfg).await {
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
) -> bool {
    match ev {
        CoreEvent::ApplicationState(s) => {
            app.apply_state(s);
            if *first_state {
                *first_state = false;
                if *do_autoconnect {
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
            app.connection_status = s;
            if was != app.is_connected() {
                app.clear_msg();
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

        CoreEvent::Disconnected => {
            app.msg("Error: Core disconnected. Press q to quit.");
        }
        _ => {}
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
            KeyCode::Char(c) if c != 'q' => {
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
