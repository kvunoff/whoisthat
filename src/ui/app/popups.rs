use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use super::helpers::*;
use super::state::App;
use super::types::*;
use crate::ui::theme::*;

impl App {
    pub(super) fn render_popup(&self, f: &mut Frame, popup: &Popup, area: Rect) {
        match popup {
            Popup::Import { input, .. } => self.render_text_popup(f, " Import Profile URI ", "Paste or type URI (vless:// vmess:// trojan:// ss:// socks://):", input, area),
            Popup::EditUserAgent { input, .. } => self.render_text_popup(f, " Edit User-Agent ", "Enter custom User-Agent:", input, area),
            Popup::EditTunName { input, .. } => self.render_text_popup(f, " Edit TUN Name ", "Enter TUN interface name (1-15 chars, letters/digits/underscore/dash):", input, area),
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
            Constraint::Length(2),
        ])
        .split(inner);

        f.render_widget(Paragraph::new(hint).style(s_dim()), rows[0]);

        f.render_widget(
            Paragraph::new(input)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(ACCENT)),
                )
                .style(s_text()),
            rows[1],
        );

        f.render_widget(
            Paragraph::new(" Enter import | Esc cancel | Ctrl+V paste from clipboard ")
                .style(s_dim())
                .alignment(Alignment::Center),
            rows[2],
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
}
