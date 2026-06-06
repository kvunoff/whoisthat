use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::config;
use crate::core_client::protocol::*;

use super::logs::{render_logs, LogsState};
use super::settings::{render_settings, SettingsState};
use super::theme::*;
use super::uri;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActiveTab {
    Profiles,
    Logs,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    LeftPanel,
    RightPanel,
    Popup,
}

#[derive(Debug)]
pub enum Popup {
    Import { input: String, cursor: usize },
    ConfirmDelete { gid: i32, pid: i32, name: String },
    Help,
}

pub struct App {
    pub groups: Vec<GroupWithProfiles>,
    pub connection_status: ProxyStatus,
    pub tun_enabled: bool,
    pub last_msg: Option<String>,
    pub autoconnect: bool,
    pub public_ip: String,

    pub tab: ActiveTab,
    pub focus: Focus,
    pub selected_group_idx: usize,
    pub cursor: usize,
    pub scroll: usize,
    pub popup: Option<Popup>,

    pub settings_state: SettingsState,
    pub logs_state: LogsState,
    pub traffic_stats: TrafficStats,
}

impl App {
    pub fn new(autoconnect: bool) -> Self {
        Self {
            groups: Vec::new(),
            connection_status: ProxyStatus {
                connection: "disconnected".into(),
                profile: None,
                connected_at: 0,
            },
            tun_enabled: false,
            last_msg: None,
            autoconnect,
            public_ip: String::new(),
            tab: ActiveTab::Profiles,
            focus: Focus::LeftPanel,
            selected_group_idx: 0,
            cursor: 0,
            scroll: 0,
            popup: None,
            settings_state: SettingsState::new(),
            logs_state: LogsState::new(
                config::config_dir()
                    .join("core.log")
                    .to_str()
                    .unwrap_or("core.log"),
            ),
            traffic_stats: TrafficStats::default(),
        }
    }

    pub fn current_group(&self) -> Option<&GroupWithProfiles> {
        self.groups.get(self.selected_group_idx)
    }

    pub fn profiles(&self) -> &[Profile] {
        self.current_group()
            .map(|g| g.profiles.as_slice())
            .unwrap_or(&[])
    }

    pub fn selected_profile(&self) -> Option<&Profile> {
        self.profiles().get(self.cursor)
    }

    pub fn profile_count(&self) -> usize {
        self.profiles().len()
    }

    pub fn is_connected(&self) -> bool {
        self.connection_status.connection == "connected"
    }

    pub fn connected_id(&self) -> Option<(i32, i32)> {
        self.connection_status
            .profile
            .as_ref()
            .map(|p| (p.group_id, p.id))
    }

    pub fn cursor_down(&mut self) {
        let len = self.profile_count();
        if len == 0 || self.cursor + 1 >= len {
            return;
        }
        self.cursor += 1;
    }

    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn cursor_top(&mut self) {
        self.cursor = 0;
    }

    pub fn cursor_bottom(&mut self) {
        let len = self.profile_count();
        if len > 0 {
            self.cursor = len - 1;
        }
    }

    pub fn clear_msg(&mut self) {
        self.last_msg = None;
    }

    pub fn msg(&mut self, text: impl Into<String>) {
        self.last_msg = Some(text.into());
    }

    pub fn apply_state(&mut self, state: ApplicationState) {
        self.groups = state.groups;
        self.connection_status = state.connection_status;
        self.tun_enabled = state.tun_status;
        let len = self.profile_count();
        if self.cursor >= len && len > 0 {
            self.cursor = len - 1;
        }
    }

    pub fn apply_profiles_added(&mut self, profiles: Vec<Profile>) {
        for p in profiles {
            if let Some(g) = self.groups.iter_mut().find(|g| g.group.id == p.group_id) {
                g.profiles.push(p);
            }
        }
    }

    pub fn apply_profiles_deleted(&mut self, deleted: &[ProfileID]) {
        for d in deleted {
            if let Some(g) = self.groups.iter_mut().find(|g| g.group.id == d.group_id) {
                g.profiles.retain(|p| p.id != d.id);
            }
        }
        let len = self.profile_count();
        if self.cursor >= len && len > 0 {
            self.cursor = len - 1;
        }
    }

    pub fn apply_profile_updated(&mut self, p: &Profile) {
        if let Some(g) = self.groups.iter_mut().find(|g| g.group.id == p.group_id) {
            if let Some(e) = g.profiles.iter_mut().find(|x| x.id == p.id) {
                *e = p.clone();
            }
        }
    }
}

// ===================== RENDER =====================

impl App {
    pub fn render(&self, f: &mut Frame) {
        let area = f.area();
        f.render_widget(Block::default().style(s_bg()), area);

        let v = Layout::vertical([
            Constraint::Length(4), // top bar (tabs in bottom border + stats)
            Constraint::Min(0),    // main area
            Constraint::Length(3), // bottom bar
        ])
        .split(area);

        self.render_top_bar(f, v[0]);
        self.render_main(f, v[1]);
        self.render_bottom_bar(f, v[2]);

        if let Some(ref popup) = self.popup {
            self.render_popup(f, popup, area);
        }
    }

    fn render_top_bar(&self, f: &mut Frame, area: Rect) {
        let (icon, icon_style) = if self.is_connected() {
            ("●", s_success())
        } else {
            ("○", s_disconnected())
        };

        let status_text = if self.is_connected() {
            "Connected"
        } else {
            "Disconnected"
        };

        let ip_display = if self.public_ip.is_empty() {
            "..."
        } else {
            &self.public_ip
        };

        let tab_line = Line::from(self.tab_spans());

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER))
            .title_alignment(Alignment::Right)
            .title_bottom(tab_line)
            .style(s_bg());

        let inner = block.inner(area);
        f.render_widget(block, area);

        let rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

        let status_line = Line::from(vec![
            Span::styled(" WhoisThat ", s_accent_bold().add_modifier(Modifier::BOLD)),
            Span::styled("│ ", s_faint()),
            Span::styled(format!(" {} {} ", icon, status_text), icon_style),
            Span::styled("│ ", s_faint()),
            Span::styled(ip_display, s_dim()),
        ]);
        f.render_widget(Paragraph::new(status_line), rows[0]);

        let ts = &self.traffic_stats;
        let stats_line = Line::from(vec![
            Span::styled(" P:", s_faint()),
            Span::styled(format!("↑{}", format_bytes(ts.proxy_up)), s_success()),
            Span::styled(format!(" ↓{}", format_bytes(ts.proxy_down)), s_success()),
            Span::styled("  D:", s_faint()),
            Span::styled(format!("↑{}", format_bytes(ts.direct_up)), s_dim()),
            Span::styled(format!(" ↓{}", format_bytes(ts.direct_down)), s_dim()),
        ]);
        f.render_widget(Paragraph::new(stats_line), rows[1]);
    }

    fn tab_spans(&self) -> Vec<Span<'_>> {
        let entries = [
            ('a', " add", ActiveTab::Profiles),
            ('l', " logs", ActiveTab::Logs),
            ('s', " settings", ActiveTab::Settings),
            ('v', " tun", ActiveTab::Profiles),
            ('h', " help", ActiveTab::Profiles),
            ('q', " detach", ActiveTab::Profiles),
            ('Q', " quit", ActiveTab::Profiles),
        ];

        let mut result = vec![Span::raw(" ")];
        
        for (i, (key, label, _tab)) in entries.iter().enumerate() {
            result.push(Span::styled(format!("[{}]", key), s_accent()));
            result.push(Span::styled(label.to_string(), s_dim()));
            if i < entries.len() - 1 {
                result.push(Span::raw("・"));
            }
        }
        
        result.push(Span::raw(" "));
        
        result
    }

    fn render_main(&self, f: &mut Frame, area: Rect) {
        match self.tab {
            ActiveTab::Profiles => self.render_profiles_view(f, area),
            ActiveTab::Logs => {
                let focused = self.focus == Focus::LeftPanel;
                render_logs(f, area, &self.logs_state, focused);
            }
            ActiveTab::Settings => {
                let focused = self.focus == Focus::LeftPanel;
                render_settings(
                    f,
                    area,
                    self.autoconnect,
                    &self.settings_state,
                    focused,
                );
            }
        }
    }

    fn render_profiles_view(&self, f: &mut Frame, area: Rect) {
        let h = Layout::horizontal([
            Constraint::Percentage(55),
            Constraint::Percentage(45),
        ])
        .split(area);

        self.render_profile_list(f, h[0]);
        self.render_details(f, h[1]);
    }

    fn render_profile_list(&self, f: &mut Frame, area: Rect) {
        let left_focus = self.focus == Focus::LeftPanel;
        let border_color = if left_focus { BORDER_ACTIVE } else { BORDER };
        let title = format!(" Profiles ({}) ", self.profile_count());

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color))
            .style(s_bg());

        let inner = block.inner(area);
        f.render_widget(block, area);

        if self.profiles().is_empty() {
            let msg = Paragraph::new("No profiles.\nPress [a] to import VLESS URI.")
                .style(s_dim())
                .alignment(Alignment::Center);
            let mid = Layout::vertical([
                Constraint::Percentage(45),
                Constraint::Min(2),
                Constraint::Percentage(55),
            ])
            .split(inner)[1];
            f.render_widget(msg, mid);
            return;
        }

        let vis = inner.height as usize;
        let total = self.profile_count();
        let max_scroll = total.saturating_sub(vis);
        let scroll = self.scroll.min(max_scroll);

        let items: Vec<ListItem> = self
            .profiles()
            .iter()
            .enumerate()
            .skip(scroll)
            .take(vis)
            .map(|(i, p)| {
                let is_cursor = i == self.cursor;
                self.render_list_item(p, is_cursor, left_focus)
            })
            .collect();

        f.render_widget(List::new(items), inner);
    }

    fn render_list_item(&self, p: &Profile, cursor: bool, panel_focused: bool) -> ListItem<'_> {
        let is_active = self
            .connected_id()
            .map(|(gid, pid)| gid == p.group_id && pid == p.id)
            .unwrap_or(false);

        let prefix = if cursor && panel_focused {
            Span::styled("  ", s_accent().bg(SURFACE_HL))
        } else if cursor {
            Span::styled("  ", s_accent().bg(SURFACE_HL))
        } else if is_active {
            Span::styled("● ", s_success())
        } else {
            Span::styled("  ", s_faint())
        };

        let name = if p.name.is_empty() {
            if p.address.is_empty() {
                "Unknown".to_string()
            } else {
                p.address.clone()
            }
        } else {
            p.name.clone()
        };

        let name_style = if cursor && panel_focused {
            s_accent()
        } else if cursor {
            s_accent()
        } else if is_active {
            s_success()
        } else {
            s_text()
        };

        let proto = Span::styled(format!(" {}", p.protocol.to_uppercase()), s_faint());

        let (test_txt, test_style) = match p.test_result {
            -2 => ("TESTING".to_string(), s_dim()),
            -1 => ("FAILED".to_string(), s_error()),
            x if x > 0 => (format!("{}ms", x), s_success()),
            _ => ("·".to_string(), s_faint()),
        };
        let test = Span::styled(format!(" {}", test_txt), test_style);

        let row_style = if cursor && panel_focused {
            Style::default().bg(SURFACE_HL)
        } else if cursor {
            Style::default().bg(SURFACE_HL)
        } else {
            s_bg()
        };

        ListItem::new(Line::from(vec![prefix, Span::styled(name, name_style), proto, test]))
            .style(row_style)
    }

    fn render_details(&self, f: &mut Frame, area: Rect) {
        let right_focus = self.focus == Focus::RightPanel;
        let border_color = if right_focus { BORDER_ACTIVE } else { BORDER };

        let block = Block::default()
            .title(" Details ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color))
            .style(s_bg());

        let inner = block.inner(area);
        f.render_widget(block, area);

        let Some(p) = self.selected_profile() else {
            let msg = Paragraph::new("No profile selected.\nUse j/k to navigate, [a] to import.")
                .style(s_dim())
                .alignment(Alignment::Center);
            let mid = Layout::vertical([
                Constraint::Percentage(45),
                Constraint::Min(3),
                Constraint::Percentage(55),
            ])
            .split(inner)[1];
            f.render_widget(msg, mid);
            return;
        };

        let uri_params = uri::parse_vless_uri(&p.uri);
        let group_name = self.current_group().map(|g| &g.group.name).unwrap_or(&p.name);

        let name = if p.name.is_empty() {
            if p.address.is_empty() { "Unknown".to_string() } else { p.address.clone() }
        } else {
            p.name.clone()
        };

        let conn_mark = self
            .connected_id()
            .map(|(gid, pid)| gid == p.group_id && pid == p.id)
            .unwrap_or(false);

        let protocol = if p.protocol.is_empty() {
            if uri_params.protocol.is_empty() { "—".into() }
            else { uri_params.protocol.to_uppercase() }
        } else {
            p.protocol.to_uppercase()
        };

        let port = if !uri_params.port.is_empty() { &uri_params.port }
            else if !p.uri.is_empty() { "—" } else { "—" };

        let sni = if !uri_params.sni.is_empty() { &uri_params.sni }
            else if !p.host.is_empty() { &p.host } else { "—" };

        let transport = if !uri_params.transport.is_empty() { &uri_params.transport } else { "tcp" };
        let security = if !uri_params.security.is_empty() { &uri_params.security } else { "none" };
        let flow = if !uri_params.flow.is_empty() { &uri_params.flow } else { "—" };

        let mut rows: Vec<Line> = Vec::new();

        // ---- Profile ----
        rows.push(section_header("Profile"));
        rows.push(kv_row("Name", &name));
        rows.push(kv_row("Protocol", &protocol));
        rows.push(kv_row("Group", group_name));
        rows.push(Line::from(""));

        // ---- Connection ----
        rows.push(section_header("Connection"));
        rows.push(kv_row("Address", if p.address.is_empty() { "—" } else { &p.address }));
        rows.push(kv_row("Port", port));
        rows.push(kv_row("SNI", sni));
        rows.push(kv_row("Transport", transport));
        rows.push(kv_row("Security", security));
        rows.push(kv_row("Flow", flow));
        rows.push(Line::from(""));

        // ---- Status ----
        rows.push(section_header("Status"));
        let state = if conn_mark {
            Span::styled("● Connected", s_success())
        } else {
            Span::styled("○ Disconnected", s_dim())
        };
        rows.push(kv_row_span("State", state));

        let uptime = if conn_mark {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let elapsed = (now - self.connection_status.connected_at).max(0) as u64;
            let h = elapsed / 3600;
            let m = (elapsed % 3600) / 60;
            let s = elapsed % 60;
            Span::styled(format!("{:02}:{:02}:{:02}", h, m, s), s_success())
        } else {
            Span::styled("—", s_dim())
        };
        rows.push(kv_row_span("Uptime", uptime));

        let latency = match p.test_result {
            -2 => Span::styled("Testing...", s_dim()),
            -1 => Span::styled("Failed", s_error()),
            x if x > 0 => Span::styled(format!("{} ms", x), s_success()),
            _ => Span::styled("Untested", s_faint()),
        };
        rows.push(kv_row_span("Latency", latency));

        let tun = if self.tun_enabled {
            Span::styled("● Active", s_success())
        } else {
            Span::styled("○ Inactive", s_disconnected())
        };
        rows.push(kv_row_span("TUN", tun));

        f.render_widget(Paragraph::new(rows).style(s_bg()), inner);
    }

    fn render_bottom_bar(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER))
            .style(s_bg());

        let inner = block.inner(area);
        f.render_widget(block, area);

        let left = Span::styled(" WhoisThat v0.1.10 · xray-core", s_faint());
        let left_w = left.width();

        let tun = if self.tun_enabled {
            Span::styled(" [TUN]", s_success())
        } else {
            Span::styled("", s_dim())
        };
        let tun_w = tun.width();

        let uptime = if self.connection_status.connected_at > 0 {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let elapsed = (now - self.connection_status.connected_at).max(0) as u64;
            let h = elapsed / 3600;
            let m = (elapsed % 3600) / 60;
            let s = elapsed % 60;
            Span::styled(format!(" [{:02}:{:02}:{:02}]", h, m, s), s_success())
        } else {
            Span::styled("", s_dim())
        };
        let uptime_w = uptime.width();

        let inner_w = inner.width as usize;
        let gap = 3;

        let msg = self.last_msg.as_deref().unwrap_or("");
        let max_right = inner_w
            .saturating_sub(left_w + tun_w + uptime_w + gap + gap);
        let right = if msg.is_empty() {
            Span::raw("")
        } else if msg.len() <= max_right {
            Span::styled(msg, s_dim())
        } else if max_right > 2 {
            Span::styled(
                format!("{}…", &msg[..max_right.saturating_sub(2)]),
                s_dim(),
            )
        } else {
            Span::raw("")
        };

        // Layout: [left] [tun] ... [right]
        let spans = vec![left, tun, uptime, Span::raw(" ".repeat(gap)), right];
        f.render_widget(Paragraph::new(Line::from(spans)), inner);
    }

    // ===================== POPUPS =====================

    fn render_popup(&self, f: &mut Frame, popup: &Popup, area: Rect) {
        match popup {
            Popup::Import { input, .. } => self.render_import_popup(f, input, area),
            Popup::ConfirmDelete { name, .. } => self.render_confirm_popup(f, name, area),
            Popup::Help => self.render_help_popup(f, area),
        }
    }

    fn render_import_popup(&self, f: &mut Frame, input: &str, area: Rect) {
        let pa = centered_rect(70, 32, area);
        f.render_widget(Clear, pa);

        let block = Block::default()
            .title(" Import VLESS URI ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ACCENT))
            .style(s_surface());

        let inner = block.inner(pa);
        f.render_widget(block, pa);

        let rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(2),
        ])
        .split(inner);

        f.render_widget(Paragraph::new("Paste or type VLESS URI:").style(s_dim()), rows[0]);

        let inp = if input.is_empty() { "vless://..." } else { input };
        f.render_widget(
            Paragraph::new(inp)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(ACCENT)),
                )
                .style(s_text()),
            rows[1],
        );

        f.render_widget(Paragraph::new("").style(s_dim()), rows[2]);

        f.render_widget(
            Paragraph::new(" Enter import | Esc cancel | Ctrl+V paste from clipboard ")
                .style(s_dim())
                .alignment(Alignment::Center),
            rows[3],
        );
    }

    fn render_confirm_popup(&self, f: &mut Frame, name: &str, area: Rect) {
        let pa = centered_rect(55, 25, area);
        f.render_widget(Clear, pa);

        let block = Block::default()
            .title(" Confirm Delete ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ERROR))
            .style(s_surface());

        let inner = block.inner(pa);
        f.render_widget(block, pa);

        let msg = format!("Delete profile \"{}\"?\nThis cannot be undone.", name);
        let chunks =
            Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(inner);

        f.render_widget(
            Paragraph::new(msg)
                .style(s_text())
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true }),
            chunks[0],
        );

        f.render_widget(
            Paragraph::new(" Enter yes | Esc no ")
                .style(s_dim())
                .alignment(Alignment::Center),
            chunks[1],
        );
    }

    fn render_help_popup(&self, f: &mut Frame, area: Rect) {
        let pa = centered_rect(52, 60, area);
        f.render_widget(Clear, pa);

        let block = Block::default()
            .title(" Help ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ACCENT))
            .style(s_surface());

        let inner = block.inner(pa);
        f.render_widget(block, pa);

        let help: &[(&str, &str)] = &[
            ("j / ↓", "Move cursor down"),
            ("k / ↑", "Move cursor up"),
            ("g", "Jump to top"),
            ("G", "Jump to bottom"),
            ("c / Enter", "Connect to profile"),
            ("d", "Disconnect"),
            ("t", "Test profile latency"),
            ("a", "Import VLESS profile"),
            ("x", "Delete selected profile"),
            ("v", "Toggle TUN mode"),
            ("Tab", "Switch focus (list ↔ details)"),
            ("l", "Logs tab"),
            ("s", "Settings tab"),
            ("1", "Profiles tab"),
            ("h / ?", "This help"),
            ("q", "Detach TUI (VPN stays on)"),
            ("Q / Ctrl+C", "Quit + stop VPN"),
        ];

        let lines: Vec<Line> = help
            .iter()
            .map(|(key, desc)| {
                Line::from(vec![
                    Span::styled(format!(" {:>10} ", key), s_accent()),
                    Span::styled(*desc, s_dim()),
                ])
            })
            .collect();

        let mid = Layout::vertical([
            Constraint::Min(help.len() as u16 + 2),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner)[0];

        f.render_widget(Paragraph::new(lines), mid);

        let hint = Layout::vertical([
            Constraint::Min(help.len() as u16 + 3),
            Constraint::Length(1),
        ])
        .split(inner)[1];

        f.render_widget(
            Paragraph::new(" Press any key to close ")
                .style(s_faint())
                .alignment(Alignment::Center),
            hint,
        );
    }
}

fn section_header(label: &str) -> Line<'static> {
    Line::from(vec![Span::styled(
        format!(" {} ", label),
        s_accent_bold().add_modifier(Modifier::BOLD),
    )])
}

fn kv_row(key: &str, val: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {:<10}", key), s_faint()),
        Span::styled(val.to_string(), s_text()),
    ])
}

fn kv_row_span(key: &str, val: Span<'static>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {:<10}", key), s_faint()),
        val,
    ])
}

fn format_bytes(bytes: i64) -> String {
    if bytes <= 0 {
        return "0".into();
    }
    let b = bytes as f64;
    if b < 1024.0 {
        format!("{}", bytes)
    } else if b < 1024.0 * 1024.0 {
        format!("{:.1}K", b / 1024.0)
    } else if b < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1}M", b / (1024.0 * 1024.0))
    } else {
        format!("{:.2}G", b / (1024.0 * 1024.0 * 1024.0))
    }
}

fn centered_rect(px: u16, py: u16, r: Rect) -> Rect {
    let pv = Layout::vertical([
        Constraint::Percentage((100 - py) / 2),
        Constraint::Percentage(py),
        Constraint::Percentage((100 - py) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - px) / 2),
        Constraint::Percentage(px),
        Constraint::Percentage((100 - px) / 2),
    ])
    .split(pv[1])[1]
}
