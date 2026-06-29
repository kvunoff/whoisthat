use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::core_client::protocol::*;

use super::helpers::*;
use super::state::App;
use super::types::*;
use crate::ui::theme::*;
use crate::ui::uri::{self};

impl App {
    pub(super) fn render_details(&self, f: &mut Frame, area: Rect) {
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
                        .alignment(ratatui::layout::Alignment::Center),
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
        let total = rows.len();
        let visible = inner.height as usize;
        let scroll = self.details_scroll.min(total.saturating_sub(visible));
        let overflow = total > visible;

        for (i, line) in rows.iter().enumerate().skip(scroll).take(visible) {
            let y = y_start + (i - scroll) as u16;
            if y < inner.y + inner.height {
                f.render_widget(Paragraph::new(line.clone()), Rect::new(inner.x + 1, y, w, 1));
            }
        }

        if overflow && scroll + visible < total {
            f.render_widget(
                Paragraph::new(format!(" ↓ {} more ", total - scroll - visible))
                    .style(s_faint())
                    .alignment(ratatui::layout::Alignment::Right),
                Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1),
            );
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

        let port = if !uri_params.port.is_empty() { &uri_params.port } else { "—" };

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
        let total = rows.len();
        let visible = inner.height as usize;
        let scroll = self.details_scroll.min(total.saturating_sub(visible));
        let overflow = total > visible;

        for (i, line) in rows.iter().enumerate().skip(scroll).take(visible) {
            let y = y_start + (i - scroll) as u16;
            if y < inner.y + inner.height {
                f.render_widget(Paragraph::new(line.clone()), Rect::new(inner.x + 1, y, w, 1));
            }
        }

        if overflow && scroll + visible < total {
            f.render_widget(
                Paragraph::new(format!(" ↓ {} more ", total - scroll - visible))
                    .style(s_faint())
                    .alignment(ratatui::layout::Alignment::Right),
                Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1),
            );
        }
    }
}
