use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph, ListState},
    Frame,
};

use crate::core_client::protocol::HwidData;

use super::theme::*;

#[derive(Debug, Clone)]
pub struct SettingsState {
    pub list_state: ListState,
}

impl SettingsState {
    pub fn new() -> Self {
        let mut s = Self { list_state: ListState::default() };
        s.list_state.select(Some(0));
        s
    }

    pub fn cursor_up(&mut self) {
        let i = self.list_state.selected().unwrap_or(0);
        if i > 0 {
            self.list_state.select(Some(i - 1));
        }
    }

    pub fn cursor_down(&mut self, max: usize) {
        let i = self.list_state.selected().unwrap_or(0);
        if i < max {
            self.list_state.select(Some(i + 1));
        }
    }

    pub fn cursor(&self) -> usize {
        self.list_state.selected().unwrap_or(0)
    }
}

pub fn render_settings(
    f: &mut Frame,
    area: Rect,
    autoconnect: bool,
    autostart_mode: &str,
    systemd_enabled: bool,
    show_ip: bool,
    log_enabled: bool,
    log_level: &str,
    test_method: &str,
    tun_name: &str,
    kill_switch_enabled: bool,
    hwid: Option<&HwidData>,
    state: &mut SettingsState,
    focused: bool,
) {
    let border_color = if focused { BORDER_ACTIVE } else { BORDER };

    let block = Block::default()
        .title(" Settings ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(s_bg());

    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    let content_area = rows[0];

    let hwid_enabled = hwid.map(|h| h.enabled).unwrap_or(false);
    let hwid_val = hwid.map(|h| h.hwid.as_str()).unwrap_or("");
    let hwid_ua = hwid.map(|h| h.user_agent.as_str()).unwrap_or("");

    let items_data: [(&str, &str, bool, bool); 13] = [
        ("Autoconnect",       if autoconnect { "on" } else { "off" }, true,  false),
        ("Autostart mode",    autostart_mode,                                    false, false),
        ("Systemd autostart", if systemd_enabled { "on" } else { "off" },       true,  false),
        ("Show IP",           if show_ip     { "on" } else { "off" }, true,  false),
        ("TUI log",           if log_enabled { "on" } else { "off" }, true,  false),
        ("Log level",         log_level,                               false, false),
        ("Test method",       test_method,                             false, false),
        ("TUN name",          tun_name,                                false, false),
        ("Kill Switch",       if kill_switch_enabled { "on" } else { "off" }, true, false),
        ("HWID: Enabled",     if hwid_enabled { "on" } else { "off" }, true,  false),
        ("HWID",              hwid_val,                                false, false),
        ("Reset HWID",        "\u{23ce}",                              false, true),
        ("UA",                hwid_ua,                                 false, false),
    ];

    let items: Vec<ListItem> = items_data
        .iter()
        .map(|(label, val, is_toggle, is_action)| {
            let val_style = if *is_toggle && *val == "on" {
                s_success()
            } else if *is_toggle {
                s_disconnected()
            } else if *is_action {
                s_accent()
            } else {
                s_success()
            };
            let indicator = if *is_toggle {
                format!(" {}", if *val == "on" { "●" } else { "○" })
            } else {
                String::new()
            };
            ListItem::new(Line::from(vec![
                Span::styled(*label, s_dim()),
                Span::raw("  "),
                Span::styled(indicator, val_style),
                Span::styled(format!(" {}", val), val_style),
            ]))
        })
        .collect();

    let highlight_symbol = if focused { "> " } else { "  " };

    let list = List::new(items)
        .highlight_style(Style::default().fg(ACCENT))
        .highlight_symbol(highlight_symbol)
        .scroll_padding(3);

    let mut ls = state.list_state.clone();
    f.render_stateful_widget(list, content_area, &mut ls);
    state.list_state = ls;

    let help = Paragraph::new(" j/k navigate  │  Space/Enter toggle/action  │  l/r cycle value")
        .style(s_faint())
        .alignment(Alignment::Center);
    f.render_widget(help, rows[1]);
}
