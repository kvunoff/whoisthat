use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use super::helpers::*;
use super::state::App;
use super::types::*;
use crate::ui::logs::render_logs;
use crate::ui::routing::{render_routing_popup, render_routing_tab};
use crate::ui::settings::render_settings;
use crate::ui::theme::*;

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
            let ip4 = if self.public_ip.is_empty() { "..." } else { &self.public_ip };
            let ip6 = if self.public_ipv6.is_empty() { "..." } else { &self.public_ipv6 };
            status_spans.push(Span::styled("│ ", s_faint()));
            status_spans.push(Span::styled(format!("{} {}", ip4, ip6), s_dim()));
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
                    &self.tun_name,
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

    pub(super) fn render_profiles_view(&mut self, f: &mut Frame, area: Rect) {
        let h = Layout::horizontal([
            Constraint::Percentage(55),
            Constraint::Percentage(45),
        ])
        .split(area);

        self.render_tree(f, h[0]);
        self.render_details(f, h[1]);
    }

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
