use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::config;
use crate::core_client::protocol::*;

use super::logs::{render_logs, LogsState};
use super::routing::{render_routing_popup, render_routing_tab, RoutingPopup};
use super::settings::{render_settings, SettingsState};
use super::theme::*;
use super::uri::{self, ParsedUri};

use std::cell::RefCell;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActiveTab {
    Profiles,
    Logs,
    Settings,
    Routing,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    LeftPanel,
    RightPanel,
    Popup,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Popup {
    Import { input: String, cursor: usize },
    ConfirmDelete { gid: i32, pid: i32, name: String },
    ConfirmDeleteGroup { gid: i32, name: String },
    AddGroup { name: String, url: String, cursor: usize, field: usize },
    EditSubscription { name: String, url: String, group_id: i32, cursor: usize, field: usize },
    EditUserAgent { input: String, cursor: usize },
    Help,
}

#[derive(Debug, Clone, Copy)]
enum TreeNode {
    Group(usize),
    Profile(usize, usize),
}

pub struct App {
    pub groups: Vec<GroupWithProfiles>,
    pub connection_status: ProxyStatus,
    pub tun_enabled: bool,
    pub last_msg: Option<String>,
    pub autoconnect: bool,
    pub show_ip: bool,
    pub log_enabled: bool,
    pub log_level: String,
    pub test_method: String,
    pub hwid_info: Option<HwidData>,
    pub public_ip: String,

    pub tab: ActiveTab,
    pub focus: Focus,
    pub cursor: usize,
    pub tree_scroll: usize,
    pub popup: Option<Popup>,
    pub help_scroll: usize,

    pub settings_state: SettingsState,
    pub logs_state: LogsState,
    pub traffic_stats: TrafficStats,
    pub routing: RoutingConfig,
    pub routing_cursor: usize,
    pub routing_popup: Option<RoutingPopup>,

    uri_cache: RefCell<Option<(i32, i32, ParsedUri)>>,
}

impl App {
    pub fn new(autoconnect: bool, show_ip: bool, log_enabled: bool, log_level: String, test_method: String) -> Self {
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
            show_ip,
            log_enabled,
            log_level,
            test_method,
            hwid_info: None,
            public_ip: String::new(),
            tab: ActiveTab::Profiles,
            focus: Focus::LeftPanel,
            cursor: 0,
            tree_scroll: 0,
            popup: None,
            help_scroll: 0,
            settings_state: SettingsState::new(),
            logs_state: LogsState::new(
                config::config_dir()
                    .join("core.log")
                    .to_str()
                    .unwrap_or("core.log"),
            ),
            traffic_stats: TrafficStats::default(),
            routing: RoutingConfig::default(),
            routing_cursor: 0,
            routing_popup: None,
            uri_cache: RefCell::new(None),
        }
    }

    // --- tree helpers ---

    fn tree_len(&self) -> usize {
        let mut n = 0;
        for g in &self.groups {
            n += 1; // group header
            n += g.profiles.len();
        }
        n
    }

    fn tree_node_at(&self, cursor: usize) -> Option<TreeNode> {
        let mut pos = 0;
        for (gi, g) in self.groups.iter().enumerate() {
            if pos == cursor {
                return Some(TreeNode::Group(gi));
            }
            pos += 1;
            let plen = g.profiles.len();
            if cursor < pos + plen {
                return Some(TreeNode::Profile(gi, cursor - pos));
            }
            pos += plen;
        }
        None
    }

    fn clamp_cursor(&mut self) {
        let len = self.tree_len();
        if len == 0 {
            self.cursor = 0;
        } else if self.cursor >= len {
            self.cursor = len - 1;
        }
    }

    // --- helpers (used by main.rs and rendering) ---

    pub fn current_group(&self) -> Option<&GroupWithProfiles> {
        match self.tree_node_at(self.cursor)? {
            TreeNode::Group(gi) => self.groups.get(gi),
            TreeNode::Profile(gi, _) => self.groups.get(gi),
        }
    }

    pub fn selected_profile(&self) -> Option<&Profile> {
        match self.tree_node_at(self.cursor)? {
            TreeNode::Profile(gi, pi) => self.groups.get(gi)?.profiles.get(pi),
            TreeNode::Group(_) => None,
        }
    }

    pub fn on_group(&self) -> bool {
        matches!(self.tree_node_at(self.cursor), Some(TreeNode::Group(_)))
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

    pub fn connected_group_name(&self) -> Option<&str> {
        let (gid, _) = self.connected_id()?;
        self.groups
            .iter()
            .find(|g| g.group.id == gid)
            .map(|g| g.group.name.as_str())
    }

    pub fn cursor_down(&mut self) {
        let len = self.tree_len();
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
        self.tree_scroll = 0;
    }

    pub fn cursor_bottom(&mut self) {
        let len = self.tree_len();
        if len > 0 {
            self.cursor = len - 1;
        }
    }

    pub fn clear_msg(&mut self) {
        self.last_msg = None;
    }

    pub fn cached_uri_params(&self, p: &Profile) -> ParsedUri {
        let mut cache = self.uri_cache.borrow_mut();
        match *cache {
            Some((gid, pid, ref params)) if gid == p.group_id && pid == p.id => {
                return params.clone();
            }
            _ => {}
        }
        let params = uri::parse_uri(&p.uri);
        *cache = Some((p.group_id, p.id, params.clone()));
        params
    }

    pub fn msg(&mut self, text: impl Into<String>) {
        self.last_msg = Some(text.into());
    }

    // --- state mutations ---

    pub fn apply_state(&mut self, state: ApplicationState) {
        self.groups = state.groups;
        self.connection_status = state.connection_status;
        self.tun_enabled = state.tun_status;
        self.hwid_info = state.hwid_info;
        self.clamp_cursor();
    }

    pub fn invalidate_uri_cache(&self) {
        self.uri_cache.borrow_mut().take();
    }

    pub fn apply_profiles_added(&mut self, profiles: Vec<Profile>) {
        self.invalidate_uri_cache();
        for p in profiles {
            if let Some(g) = self.groups.iter_mut().find(|g| g.group.id == p.group_id) {
                g.profiles.push(p);
            }
        }
    }

    pub fn apply_profiles_deleted(&mut self, deleted: &[ProfileID]) {
        self.invalidate_uri_cache();
        for d in deleted {
            if let Some(g) = self.groups.iter_mut().find(|g| g.group.id == d.group_id) {
                g.profiles.retain(|p| p.id != d.id);
            }
        }
        self.clamp_cursor();
    }

    pub fn apply_subscription_updated(&mut self, group: Group, profiles: Vec<Profile>) {
        if let Some(g) = self.groups.iter_mut().find(|g| g.group.id == group.id) {
            g.group = group;
            g.profiles = profiles;
        }
        self.clamp_cursor();
    }

    pub fn apply_profile_updated(&mut self, p: &Profile) {
        self.invalidate_uri_cache();
        if let Some(g) = self.groups.iter_mut().find(|g| g.group.id == p.group_id) {
            if let Some(existing) = g.profiles.iter_mut().find(|pr| pr.id == p.id) {
                *existing = p.clone();
            }
        }
    }

    pub fn apply_group_added(&mut self, g: Group) {
        self.groups.push(GroupWithProfiles {
            group: g,
            profiles: Vec::new(),
        });
    }

    pub fn apply_group_deleted(&mut self, id: i32) {
        self.groups.retain(|g| g.group.id != id);
        self.clamp_cursor();
    }

    pub fn apply_group_updated(&mut self, g: &Group) {
        if let Some(existing) = self.groups.iter_mut().find(|gw| gw.group.id == g.id) {
            existing.group = g.clone();
        }
    }
}

// ===================== RENDER =====================

impl App {
    pub fn render(&mut self, f: &mut Frame) {
        let area = f.area();
        f.render_widget(Block::default().style(s_bg()), area);

        let v = Layout::vertical([
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);

        self.render_top_bar(f, v[0]);
        self.render_main(f, v[1]);
        self.render_bottom_bar(f, v[2]);

        if let Some(ref popup) = self.popup {
            self.render_popup(f, popup, area);
        }
        if let Some(ref popup) = self.routing_popup {
            render_routing_popup(f, popup, area);
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

        let mut status_spans = vec![
            Span::styled(" WhoisThat ", s_accent_bold().add_modifier(Modifier::BOLD)),
            Span::styled("│ ", s_faint()),
            Span::styled(format!(" {} {} ", icon, status_text), icon_style),
        ];
        if self.show_ip {
            let ip_display = if self.public_ip.is_empty() {
                "..."
            } else {
                &self.public_ip
            };
            status_spans.push(Span::styled("│ ", s_faint()));
            status_spans.push(Span::styled(ip_display, s_dim()));
        }
        let status_line = Line::from(status_spans);

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

        f.render_widget(Paragraph::new(status_line), rows[0]);

        let ts = &self.traffic_stats;
        let stats_line = Line::from(vec![
            Span::styled(" P:", s_faint()),
            Span::styled(format!("↑{}", format_bytes(ts.proxy_up)), s_success()),
            Span::styled(format!(" ↓{}", format_bytes(ts.proxy_down)), s_success()),
        ]);
        f.render_widget(Paragraph::new(stats_line), rows[1]);
    }

    fn tab_spans(&self) -> Vec<Span<'_>> {
        let entries = [
            ('a', " add", ActiveTab::Profiles),
            ('u', " sub", ActiveTab::Profiles),
            ('r', " route", ActiveTab::Routing),
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
        result
    }

    fn render_main(&mut self, f: &mut Frame, area: Rect) {
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
                    self.show_ip,
                    self.log_enabled,
                    &self.log_level,
                    &self.test_method,
                    self.hwid_info.as_ref(),
                    &mut self.settings_state,
                    focused,
                );
            }
            ActiveTab::Routing => {
                let focused = self.focus == Focus::LeftPanel;
                render_routing_tab(f, area, &self.routing, self.routing_cursor, focused);
            }
        }
    }

    fn render_profiles_view(&mut self, f: &mut Frame, area: Rect) {
        let h = Layout::horizontal([
            Constraint::Percentage(55),
            Constraint::Percentage(45),
        ])
        .split(area);

        self.render_tree(f, h[0]);
        self.render_details(f, h[1]);
    }

    fn render_tree(&mut self, f: &mut Frame, area: Rect) {
        let left_focus = self.focus == Focus::LeftPanel;
        let border_color = if left_focus { BORDER_ACTIVE } else { BORDER };

        let block = Block::default()
            .title(" Profiles ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color))
            .style(s_bg());

        let inner = block.inner(area);
        f.render_widget(block, area);

        if self.groups.is_empty() {
            let msg = Paragraph::new("No groups.\nPress [a] to import profiles.")
                .style(s_dim())
                .alignment(Alignment::Center);
            f.render_widget(msg, inner);
            return;
        }

        let (gid, pid) = self.connected_id().unwrap_or((-1, -1));
        let mut items: Vec<ListItem> = Vec::new();
        let mut cursor_pos_in_list = 0;
        let mut pos = 0;

        for (gi, g) in self.groups.iter().enumerate() {
            if pos == self.cursor {
                cursor_pos_in_list = items.len();
            }
            pos += 1;

            let has_sub = !g.group.subscription_url.is_empty();
            let marker = if has_sub { " ↻" } else { "" };
            let conn_mark = if g.profiles.iter().any(|p| p.group_id == gid && p.id == pid) {
                " ●"
            } else {
                ""
            };
            let group_style = if pos - 1 == self.cursor && left_focus {
                s_accent_bold()
            } else {
                s_accent()
            };
            items.push(ListItem::new(Line::from(vec![
                Span::styled(format!("▸{}{}", marker, conn_mark), group_style),
                Span::styled(format!(" {} ({})", g.group.name, g.profiles.len()), s_text()),
            ])));

            for (pi, p) in g.profiles.iter().enumerate() {
                if pos == self.cursor {
                    cursor_pos_in_list = items.len();
                }
                pos += 1;

                let conn_mark = if p.group_id == gid && p.id == pid {
                    Span::styled("● ", s_success())
                } else {
                    Span::styled("  ", s_dim())
                };
                let name = if p.name.is_empty() {
                    if p.address.is_empty() { "Unknown" } else { &p.address }
                } else {
                    &p.name
                };
                let proto = if p.protocol.is_empty() { "—" } else { &p.protocol };
                let test = match p.test_result {
                    -2 => Span::styled(" ...", s_dim()),
                    -1 => Span::styled(" err", s_error()),
                    0 => Span::styled("", s_dim()),
                    ms => Span::styled(format!(" {}ms", ms), s_success()),
                };
                let style = if pos - 1 == self.cursor && left_focus {
                    s_text()
                } else {
                    s_dim()
                };
                let warn = if p.uri.starts_with("ss://") {
                    if let Some(method) = uri::parse_ss_method(&p.uri) {
                        if uri::is_insecure_ss_cipher(&method) {
                            Span::styled(" ⚠", s_error())
                        } else {
                            Span::styled("", s_dim())
                        }
                    } else {
                        Span::styled("", s_dim())
                    }
                } else {
                    Span::styled("", s_dim())
                };
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("  ", s_dim()),
                    conn_mark,
                    Span::styled(name, style),
                    warn,
                    Span::styled(format!(" [{}]", proto.to_uppercase()), s_faint()),
                    test,
                ])));
            }
        }

        // Keep cursor visible by adjusting scroll
        let visible = inner.height.saturating_sub(1) as usize;
        if cursor_pos_in_list < self.tree_scroll {
            self.tree_scroll = cursor_pos_in_list;
        } else if visible > 0 && cursor_pos_in_list >= self.tree_scroll + visible {
            self.tree_scroll = cursor_pos_in_list.saturating_sub(visible).saturating_add(1);
        }

        let mut list_state = ListState::default()
            .with_selected(Some(cursor_pos_in_list))
            .with_offset(self.tree_scroll);

        let list = List::new(items)
            .highlight_style(Style::default())
            .scroll_padding(5);

        f.render_stateful_widget(list, inner, &mut list_state);
    }

    fn render_details(&self, f: &mut Frame, area: Rect) {
        let right_focus = self.focus == Focus::RightPanel;
        let border_color = if right_focus { BORDER_ACTIVE } else { BORDER };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color))
            .style(s_bg());

        let inner = block.inner(area);
        f.render_widget(block, area);

        let node = self.tree_node_at(self.cursor);

        match node {
            Some(TreeNode::Group(gi)) => {
                let g = &self.groups[gi];
                let title = format!(" {} ", g.group.name);
                f.render_widget(
                    Paragraph::new(title).style(s_accent_bold()),
                    Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(inner)[0],
                );
                self.render_group_details(f, g, inner);
            }
            Some(TreeNode::Profile(_, _)) => {
                let Some(p) = self.selected_profile() else {
                    f.render_widget(
                        Paragraph::new("No profile selected.").style(s_dim()),
                        inner,
                    );
                    return;
                };
                self.render_profile_details(f, p, inner);
            }
            None => {
                f.render_widget(
                    Paragraph::new("No profile selected.\nUse j/k to navigate, [a] to import.")
                        .style(s_dim())
                        .alignment(Alignment::Center),
                    inner,
                );
            }
        }
    }

    fn render_group_details(&self, f: &mut Frame, g: &GroupWithProfiles, area: Rect) {
        let block = Block::default()
            .style(s_bg());
        let inner = block.inner(area);
        f.render_widget(block, area);

        let mut rows: Vec<Line> = Vec::new();

        let conn = self.connected_group_name();
        let connected = conn == Some(g.group.name.as_str());

        rows.push(section_header("Group"));
        rows.push(kv_row("Name", &g.group.name));
        let plen = g.profiles.len().to_string();
        rows.push(kv_row("Profiles", &plen));

        if !g.group.subscription_url.is_empty() {
            rows.push(Line::from(""));
            rows.push(section_header("Subscription"));
            rows.push(kv_row("URL", &g.group.subscription_url));

            if g.group.sub_last_updated > 0 {
                let ago = format_relative(g.group.sub_last_updated);
                rows.push(kv_row("Updated", &ago));
            }
            if g.group.sub_expires > 0 {
                let expiry = format_expiry(g.group.sub_expires);
                rows.push(kv_row("Expires", &expiry));
            }
            if g.group.sub_upload > 0 || g.group.sub_download > 0 {
                let used = format_bytes(g.group.sub_upload + g.group.sub_download);
                let limit = if g.group.sub_total > 0 {
                    format_bytes(g.group.sub_total)
                } else {
                    "∞".to_string()
                };
                let traffic = format!("{} / {}", used, limit);
                rows.push(kv_row("Traffic", &traffic));
            }
        }

        if connected {
            rows.push(Line::from(""));
            rows.push(Line::from(Span::styled("● Connected to this group", s_success())));
        }

        let y_start = inner.y + 1;
        let w = inner.width.saturating_sub(2);
        for (i, line) in rows.iter().enumerate() {
            let y = y_start + i as u16;
            if y < inner.y + inner.height {
                f.render_widget(Paragraph::new(line.clone()), Rect::new(inner.x + 1, y, w, 1));
            }
        }
    }

    fn render_profile_details(&self, f: &mut Frame, p: &Profile, area: Rect) {
        let block = Block::default()
            .style(s_bg());
        let inner = block.inner(area);
        f.render_widget(block, area);

        let uri_params = self.cached_uri_params(p);
        let group_name = self.current_group().map(|g| g.group.name.as_str()).unwrap_or("?");

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

        rows.push(section_header("Profile"));
        rows.push(kv_row("Name", &name));
        rows.push(kv_row("Protocol", &protocol));
        rows.push(kv_row("Group", group_name));

        // Subscription URL for the group
        if let Some(g) = self.current_group() {
            if !g.group.subscription_url.is_empty() {
                rows.push(kv_row("Sub", &g.group.subscription_url));
            }
        }

        rows.push(Line::from(""));

        rows.push(section_header("Connection"));
        rows.push(kv_row("Address", if p.address.is_empty() { "—" } else { &p.address }));
        rows.push(kv_row("Port", port));
        rows.push(kv_row("SNI", sni));
        rows.push(kv_row("Transport", transport));
        rows.push(kv_row("Security", security));
        rows.push(kv_row("Flow", flow));

        if p.uri.starts_with("ss://") {
            if let Some(method) = uri::parse_ss_method(&p.uri) {
                let method_display = if uri::is_insecure_ss_cipher(&method) {
                    format!("⚠ {} (insecure)", method)
                } else {
                    method
                };
                rows.push(kv_row("Cipher", method_display));
            }
        }

        rows.push(Line::from(""));

        rows.push(section_header("Status"));
        let state = if conn_mark {
            Span::styled("● Connected", s_success())
        } else {
            Span::styled("○ Disconnected", s_dim())
        };
        rows.push(kv_row_span("State", state));

        if conn_mark {
            let ts = self.connection_status.connected_at;
            if ts > 0 {
                let dt = chrono::DateTime::from_timestamp(ts, 0)
                    .map(|d| d.format("%H:%M:%S").to_string())
                    .unwrap_or_default();
                rows.push(kv_row("Since", &dt));
            }
        }
        if p.test_result > 0 {
            rows.push(kv_row("Latency", &format!("{} ms", p.test_result)));
        } else if p.test_result == -2 {
            rows.push(kv_row("Latency", "testing..."));
        }
        rows.push(kv_row("TUN", if self.tun_enabled { "on" } else { "off" }));

        let y_start = inner.y;
        let w = inner.width.saturating_sub(2);
        for (i, line) in rows.iter().enumerate() {
            let y = y_start + i as u16;
            if y < inner.y + inner.height {
                f.render_widget(Paragraph::new(line.clone()), Rect::new(inner.x + 1, y, w, 1));
            }
        }
    }

    // ===================== POPUPS =====================

    fn render_popup(&self, f: &mut Frame, popup: &Popup, area: Rect) {
        match popup {
            Popup::Import { input, .. } => self.render_text_popup(f, " Import Profile URI ", "Paste or type URI (vless:// vmess:// trojan:// ss:// socks://):", input, area),
            Popup::EditUserAgent { input, .. } => self.render_text_popup(f, " Edit User-Agent ", "Enter custom User-Agent:", input, area),
            Popup::ConfirmDelete { name, .. } => self.render_confirm_popup(f, "profile", name, area),
            Popup::ConfirmDeleteGroup { name, .. } => self.render_confirm_popup(f, "group", name, area),
            Popup::EditSubscription { name, url, cursor, field, .. } => self.render_group_form(f, "Edit Group", name, url, *cursor, *field, area),
            Popup::Help => self.render_help_popup(f, area),
            Popup::AddGroup { name, url, cursor, field } => self.render_group_form(f, "Add Group", name, url, *cursor, *field, area),
        }
    }

    fn render_text_popup(&self, f: &mut Frame, title: &str, hint: &str, input: &str, area: Rect) {
        let pa = centered_rect(70, 32, area);
        f.render_widget(Clear, pa);

        let block = Block::default()
            .title(title)
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

        f.render_widget(Paragraph::new(hint).style(s_dim()), rows[0]);

        let inp = if input.is_empty() { "" } else { input };
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

    fn render_group_form(&self, f: &mut Frame, title: &str, name: &str, url: &str, _cursor: usize, field: usize, area: Rect) {
        let v = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(14),
            Constraint::Fill(1),
        ]).split(area);
        let h = Layout::horizontal([
            Constraint::Percentage(15),
            Constraint::Percentage(70),
            Constraint::Percentage(15),
        ]).split(v[1]);
        let pa = h[1];
        f.render_widget(Clear, pa);

        let block = Block::default()
            .title(format!(" {} ", title))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ACCENT))
            .style(s_surface());

        let inner = block.inner(pa);
        f.render_widget(block, pa);

        let field0_style = if field == 0 {
            Style::default().fg(ACCENT)
        } else {
            s_dim()
        };
        let field1_style = if field == 1 {
            Style::default().fg(ACCENT)
        } else {
            s_dim()
        };

        let w = inner.width.saturating_sub(2);
        let mut y = inner.y;

        f.render_widget(Paragraph::new("Name:").style(field0_style), Rect::new(inner.x + 1, y, w, 1));
        y += 1;
        let name_display = if name.is_empty() { "My Group" } else { name };
        f.render_widget(
            Paragraph::new(name_display)
                .block(Block::default().borders(Borders::ALL).border_style(field0_style))
                .style(s_text()),
            Rect::new(inner.x + 1, y, w, 3),
        );
        y += 4;

        f.render_widget(Paragraph::new("Subscription URL:").style(field1_style), Rect::new(inner.x + 1, y, w, 1));
        y += 1;
        let url_display = if url.is_empty() { "https://..." } else { url };
        f.render_widget(
            Paragraph::new(url_display)
                .block(Block::default().borders(Borders::ALL).border_style(field1_style))
                .style(s_text()),
            Rect::new(inner.x + 1, y, w, 3),
        );
        y += 4;

        let hint = if field == 0 { " Tab to switch field | Enter next | Esc cancel " }
            else { " Tab to switch field | Enter save | Esc cancel " };
        f.render_widget(
            Paragraph::new(hint)
                .style(s_dim())
                .alignment(Alignment::Center),
            Rect::new(inner.x + 1, y, w, 2),
        );
    }

    fn render_confirm_popup(&self, f: &mut Frame, kind: &str, name: &str, area: Rect) {
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

        let msg = format!("Delete {} \"{}\"?\nThis cannot be undone.", kind, name);
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
            Paragraph::new(" Enter confirm | Esc cancel ")
                .style(s_accent())
                .alignment(Alignment::Center),
            chunks[1],
        );
    }

    fn render_help_popup(&self, f: &mut Frame, area: Rect) {
        let pa = centered_rect(52, 60, area);
        f.render_widget(Clear, pa);

        let block = Block::default()
            .title(" Keyboard Shortcuts ")
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
            ("a", "Import profile"),
            ("x", "Delete selected profile"),
            ("X", "Delete current group"),
            ("u", "Update subscription"),
            ("e", "Edit group"),
            ("U", "Add new group"),
            ("v", "Toggle TUN mode"),
            ("Tab", "Switch focus (list ↔ details)"),
            ("l", "Logs tab"),
            ("r", "Routing tab"),
            ("s", "Settings tab"),
            ("1", "Profiles tab"),
            ("h / ?", "This help"),
            ("q", "Detach TUI (VPN stays on)"),
            ("Q / Ctrl+C", "Quit + stop VPN"),
        ];

        let slot_rows = inner.height.saturating_sub(1) as usize;
        let has_more = self.help_scroll + slot_rows < help.len();
        let visible = if has_more { slot_rows.saturating_sub(1) } else { slot_rows };
        let end = (self.help_scroll + visible).min(help.len());

        f.render_widget(
            Paragraph::new(" j/k scroll · any other key close ")
                .style(s_faint())
                .alignment(Alignment::Center),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );

        let mut y = inner.y + 1;
        for i in self.help_scroll..end {
            let (key, desc) = &help[i];
            if y >= inner.y + inner.height {
                break;
            }
            let line = Line::from(vec![
                Span::styled(format!(" {:>10} ", key), s_accent()),
                Span::styled(*desc, s_dim()),
            ]);
            f.render_widget(
                Paragraph::new(line),
                Rect::new(inner.x + 1, y, inner.width.saturating_sub(2), 1),
            );
            y += 1;
        }

        if has_more && y < inner.y + inner.height {
            f.render_widget(
                Paragraph::new(" …").style(s_faint()).alignment(Alignment::Center),
                Rect::new(inner.x, y, inner.width, 1),
            );
        }
    }

    // ===================== BOTTOM BAR =====================

    fn render_bottom_bar(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER))
            .style(s_bg());

        let inner = block.inner(area);
        f.render_widget(block, area);

        let left = Span::styled(
            format!(" WhoisThat v{} · xray-core", env!("CARGO_PKG_VERSION")),
            s_faint(),
        );
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
            let truncated: String = msg.chars().take(max_right.saturating_sub(2)).collect();
            Span::styled(
                format!("{}…", truncated),
                s_dim(),
            )
        } else {
            Span::raw("")
        };

        let spans = vec![left, tun, uptime, Span::raw(" ".repeat(gap)), right];
        f.render_widget(Paragraph::new(Line::from(spans)), inner);
    }
}

// ===================== HELPERS =====================

fn section_header(title: &str) -> Line<'_> {
    Line::from(Span::styled(format!("─── {} ───", title), s_accent()))
}

fn kv_row(key: &str, val: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{:>10}  ", key), s_faint()),
        Span::styled(val.into(), s_text()),
    ])
}

fn kv_row_span<'a>(key: &str, val: Span<'a>) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{:>10}  ", key), s_faint()),
        val,
    ])
}

fn format_bytes(bytes: i64) -> String {
    if bytes == 0 {
        return "0".to_string();
    }
    let b = bytes as f64;
    if b < 1024.0 {
        format!("{} B", bytes)
    } else if b < 1024.0 * 1024.0 {
        format!("{:.1} KB", b / 1024.0)
    } else if b < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB", b / (1024.0 * 1024.0))
    } else if b < 1024.0 * 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} GB", b / (1024.0 * 1024.0 * 1024.0))
    } else {
        format!("{:.1} TB", b / (1024.0 * 1024.0 * 1024.0 * 1024.0))
    }
}

fn format_relative(unix_ts: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let diff = now - unix_ts;
    if diff < 0 {
        return "just now".to_string();
    }
    if diff < 60 {
        return format!("{}s ago", diff);
    }
    if diff < 3600 {
        return format!("{}m ago", diff / 60);
    }
    if diff < 86400 {
        return format!("{}h ago", diff / 3600);
    }
    format!("{}d ago", diff / 86400)
}

fn format_expiry(unix_ts: i64) -> String {
    let dt = chrono::DateTime::from_timestamp(unix_ts, 0);
    let Some(dt) = dt else { return "—".to_string() };
    let now = chrono::Utc::now();
    let days = (dt - now).num_days();
    if days < 0 {
        format!("{} (expired)", dt.format("%Y-%m-%d"))
    } else if days == 0 {
        format!("{} (today)", dt.format("%Y-%m-%d"))
    } else if days == 1 {
        format!("{} (tomorrow)", dt.format("%Y-%m-%d"))
    } else {
        format!("{} ({}d left)", dt.format("%Y-%m-%d"), days)
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}
