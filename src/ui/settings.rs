use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::core_client::protocol::HwidData;

use super::theme::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SettingsKind {
    Toggle,
    Cycle,
    Editable,
    Action,
    Display,
}

#[derive(Debug, Clone)]
pub enum SettingsRow {
    Header(&'static str),
    Item {
        label: &'static str,
        kind: SettingsKind,
    },
}

pub fn settings_layout() -> Vec<SettingsRow> {
    vec![
        SettingsRow::Header("Startup"),
        SettingsRow::Item {
            label: "Autoconnect",
            kind: SettingsKind::Toggle,
        },
        SettingsRow::Item {
            label: "Autostart mode",
            kind: SettingsKind::Cycle,
        },
        SettingsRow::Item {
            label: "Systemd autostart",
            kind: SettingsKind::Toggle,
        },
        SettingsRow::Header("Display"),
        SettingsRow::Item {
            label: "Show IP",
            kind: SettingsKind::Toggle,
        },
        SettingsRow::Item {
            label: "TUI log",
            kind: SettingsKind::Toggle,
        },
        SettingsRow::Item {
            label: "Log level",
            kind: SettingsKind::Cycle,
        },
        SettingsRow::Header("Network"),
        SettingsRow::Item {
            label: "TUN name",
            kind: SettingsKind::Editable,
        },
        SettingsRow::Item {
            label: "Kill Switch",
            kind: SettingsKind::Toggle,
        },
        SettingsRow::Header("Diagnostics"),
        SettingsRow::Item {
            label: "Test method",
            kind: SettingsKind::Cycle,
        },
        SettingsRow::Item {
            label: "Test samples",
            kind: SettingsKind::Cycle,
        },
        SettingsRow::Item {
            label: "Test concurrency",
            kind: SettingsKind::Cycle,
        },
        SettingsRow::Item {
            label: "Test timeout",
            kind: SettingsKind::Cycle,
        },
        SettingsRow::Item {
            label: "Test endpoint",
            kind: SettingsKind::Cycle,
        },
        SettingsRow::Item {
            label: "Auto-test on refresh",
            kind: SettingsKind::Toggle,
        },
        SettingsRow::Header("Hardware"),
        SettingsRow::Item {
            label: "HWID: Enabled",
            kind: SettingsKind::Toggle,
        },
        SettingsRow::Item {
            label: "HWID",
            kind: SettingsKind::Display,
        },
        SettingsRow::Item {
            label: "Reset HWID",
            kind: SettingsKind::Action,
        },
        SettingsRow::Item {
            label: "User-Agent",
            kind: SettingsKind::Editable,
        },
    ]
}

pub fn item_count() -> usize {
    settings_layout()
        .iter()
        .filter(|r| matches!(r, SettingsRow::Item { .. }))
        .count()
}

pub struct SettingsValues<'a> {
    pub autoconnect: bool,
    pub autostart_mode: &'a str,
    pub systemd_enabled: bool,
    pub show_ip: bool,
    pub log_enabled: bool,
    pub log_level: &'a str,
    pub test_method: &'a str,
    pub test_samples: &'a str,
    pub test_concurrency: &'a str,
    pub test_timeout: &'a str,
    pub test_endpoint: &'a str,
    pub auto_test_on_subscribe: bool,
    pub tun_name: &'a str,
    pub kill_switch_enabled: bool,
    pub hwid: Option<&'a HwidData>,
}

#[derive(Debug, Clone)]
pub struct SettingsState {
    pub list_state: ListState,
    pub item_cursor: usize,
}

impl SettingsState {
    pub fn new() -> Self {
        let mut s = Self {
            list_state: ListState::default(),
            item_cursor: 0,
        };
        s.list_state.select(Some(0));
        s
    }

    pub fn cursor_up(&mut self) {
        if self.item_cursor > 0 {
            self.item_cursor -= 1;
        }
    }

    pub fn cursor_down(&mut self) {
        let max = item_count().saturating_sub(1);
        if self.item_cursor < max {
            self.item_cursor += 1;
        }
    }

    pub fn cursor(&self) -> usize {
        self.item_cursor
    }

    pub fn flat_index(&self) -> usize {
        let layout = settings_layout();
        let mut item_idx = 0;
        for (i, row) in layout.iter().enumerate() {
            if matches!(row, SettingsRow::Item { .. }) {
                if item_idx == self.item_cursor {
                    return i;
                }
                item_idx += 1;
            }
        }
        0
    }
}

pub fn render_settings(
    f: &mut Frame,
    area: Rect,
    values: &SettingsValues,
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

    let hwid_enabled = values.hwid.map(|h| h.enabled).unwrap_or(false);
    let hwid_val = values.hwid.map(|h| h.hwid.as_str()).unwrap_or("");
    let hwid_ua = values.hwid.map(|h| h.user_agent.as_str()).unwrap_or("");

    let item_values: [String; 18] = [
        if values.autoconnect {
            "● on".into()
        } else {
            "○ off".into()
        },
        values.autostart_mode.to_string(),
        if values.systemd_enabled {
            "● on".into()
        } else {
            "○ off".into()
        },
        if values.show_ip {
            "● on".into()
        } else {
            "○ off".into()
        },
        if values.log_enabled {
            "● on".into()
        } else {
            "○ off".into()
        },
        values.log_level.to_string(),
        values.tun_name.to_string(),
        if values.kill_switch_enabled {
            "● on".into()
        } else {
            "○ off".into()
        },
        values.test_method.to_string(),
        values.test_samples.to_string(),
        values.test_concurrency.to_string(),
        values.test_timeout.to_string(),
        values.test_endpoint.to_string(),
        if values.auto_test_on_subscribe {
            "● on".into()
        } else {
            "○ off".into()
        },
        if hwid_enabled {
            "● on".into()
        } else {
            "○ off".into()
        },
        hwid_val.to_string(),
        "⏎".into(),
        hwid_ua.to_string(),
    ];

    let layout = settings_layout();
    let selected_flat = state.flat_index();

    let mut value_idx = 0usize;
    let items: Vec<ListItem> = layout
        .iter()
        .enumerate()
        .map(|(flat_i, row)| match row {
            SettingsRow::Header(title) => ListItem::new(Line::from(vec![Span::styled(
                format!(" {} ", title),
                s_accent(),
            )])),
            SettingsRow::Item { label, kind } => {
                let val = &item_values[value_idx];
                let is_selected = flat_i == selected_flat && focused;
                let val_style = match kind {
                    SettingsKind::Toggle => {
                        if val.starts_with("●") {
                            s_success()
                        } else {
                            s_disconnected()
                        }
                    }
                    SettingsKind::Cycle => s_accent(),
                    SettingsKind::Editable => s_text(),
                    SettingsKind::Action => s_accent(),
                    SettingsKind::Display => s_dim(),
                };
                let extra = match kind {
                    SettingsKind::Editable => "  ✎",
                    _ => "",
                };
                let prefix = if is_selected { "> " } else { "  " };
                value_idx += 1;
                ListItem::new(Line::from(vec![
                    Span::styled(prefix, if is_selected { s_accent() } else { s_faint() }),
                    Span::styled(format!("{:<22}", *label), s_dim()),
                    Span::styled(val.clone(), val_style),
                    Span::styled(extra, s_faint()),
                ]))
            }
        })
        .collect();

    let list = List::new(items)
        .highlight_style(Style::default().fg(ACCENT))
        .scroll_padding(3);

    let mut ls = state.list_state.clone();
    f.render_stateful_widget(list, content_area, &mut ls);
    state.list_state = ls;

    let help = Paragraph::new(" j/k navigate  │  Enter/Space toggle / cycle / edit")
        .style(s_faint())
        .alignment(Alignment::Center);
    f.render_widget(help, rows[1]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_layout_item_count() {
        assert_eq!(item_count(), 18);
    }

    #[test]
    fn test_settings_layout_has_headers() {
        let layout = settings_layout();
        let headers: Vec<&str> = layout
            .iter()
            .filter_map(|r| match r {
                SettingsRow::Header(t) => Some(*t),
                _ => None,
            })
            .collect();
        assert_eq!(
            headers,
            vec!["Startup", "Display", "Network", "Diagnostics", "Hardware"]
        );
    }

    #[test]
    fn test_settings_cursor_flat_index_skips_headers() {
        let mut s = SettingsState::new();
        assert_eq!(s.flat_index(), 1);

        s.cursor_down();
        assert_eq!(s.item_cursor, 1);
        assert_eq!(s.flat_index(), 2);

        s.cursor_down();
        assert_eq!(s.item_cursor, 2);
        assert_eq!(s.flat_index(), 3);

        s.cursor_down();
        assert_eq!(s.item_cursor, 3);
        assert_eq!(s.flat_index(), 5);

        s.cursor_down();
        assert_eq!(s.item_cursor, 4);
        assert_eq!(s.flat_index(), 6);
    }

    #[test]
    fn test_settings_cursor_clamps_at_bottom() {
        let max = item_count().saturating_sub(1);
        let mut s = SettingsState::new();
        for _ in 0..max + 5 {
            s.cursor_down();
        }
        assert_eq!(s.item_cursor, max);
    }

    #[test]
    fn test_settings_cursor_clamps_at_top() {
        let mut s = SettingsState::new();
        s.cursor_down();
        s.cursor_down();
        s.cursor_up();
        s.cursor_up();
        s.cursor_up();
        assert_eq!(s.item_cursor, 0);
    }
}
