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
            Popup::Import { input, .. } => self.render_text_popup(
                f,
                " Import Profile URI ",
                "Paste or type URI (vless:// vmess:// trojan:// ss:// socks://):",
                input,
                area,
            ),
            Popup::EditUserAgent { input, .. } => self.render_text_popup(
                f,
                " Edit User-Agent ",
                "Enter custom User-Agent:",
                input,
                area,
            ),
            Popup::EditTunName { input, .. } => self.render_text_popup(
                f,
                " Edit TUN Name ",
                "Enter TUN interface name (1-15 chars, letters/digits/underscore/dash):",
                input,
                area,
            ),
            Popup::EditProfileName { input, .. } => self.render_text_popup(
                f,
                " Rename Profile ",
                "Enter new profile name:",
                input,
                area,
            ),
            Popup::ConfirmDelete { name, .. } => {
                self.render_confirm_popup(f, "profile", name, area)
            }
            Popup::ConfirmDeleteGroup { name, .. } => {
                self.render_confirm_popup(f, "group", name, area)
            }
            Popup::EditSubscription {
                name,
                url,
                cursor,
                field,
                ..
            } => self.render_group_form(f, "Edit Group", name, url, *cursor, *field, area),
            Popup::Help => self.render_help_popup(f, area),
            Popup::AddGroup {
                name,
                url,
                cursor,
                field,
            } => self.render_group_form(f, "Add Group", name, url, *cursor, *field, area),
            Popup::TabSwitcher { cursor } => self.render_tab_switcher_popup(f, *cursor, area),
        }
    }

    fn render_tab_switcher_popup(&self, f: &mut Frame, cursor: usize, area: Rect) {
        let pa = responsive_rect(56, 9, 94, 80, area);
        f.render_widget(Clear, pa);

        let block = Block::default()
            .title(" Switch View ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(accent()))
            .style(s_surface());

        let inner = block.inner(pa);
        f.render_widget(block, pa);

        let tabs: &[(&str, &str, &str)] = &[
            ("[1]", "Profiles", "Active server configs & subscriptions"),
            ("[2]", "Routing", "Traffic routing rules & domain bypass"),
            ("[3]", "Traffic", "Real-time speed charts & totals"),
            ("[4]", "Logs", "Core logs & daemon event output"),
            ("[5]", "Settings", "TUI preferences & core configuration"),
        ];

        let mut lines = Vec::new();
        let is_compact = inner.width < 45;
        for (i, (num, name, desc)) in tabs.iter().enumerate() {
            let is_selected = i == cursor;
            let (indicator, num_style, name_style, desc_style) = if is_selected {
                (" ▸ ", s_accent_bold(), s_accent_bold(), s_text())
            } else {
                ("   ", s_faint(), s_text(), s_dim())
            };

            if is_compact {
                lines.push(Line::from(vec![
                    Span::styled(indicator, num_style),
                    Span::styled(format!("{} ", num), num_style),
                    Span::styled(*name, name_style),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::styled(indicator, num_style),
                    Span::styled(format!("{} ", num), num_style),
                    Span::styled(format!("{:<9} ", name), name_style),
                    Span::styled(*desc, desc_style),
                ]));
            }
        }

        lines.push(Line::from(""));
        let hint = if is_compact {
            " [j/k] Nav · [1-5] Jump · [Enter] Select "
        } else {
            " [↑/↓/j/k] Navigate  ·  [1-5] Jump  ·  [Enter] Select "
        };
        lines.push(Line::from(vec![Span::styled(hint, s_faint())]));

        f.render_widget(Paragraph::new(lines).style(s_surface()), inner);
    }

    fn render_text_popup(&self, f: &mut Frame, title: &str, hint: &str, input: &str, area: Rect) {
        let pa = responsive_rect(65, 8, 92, 70, area);
        f.render_widget(Clear, pa);

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(accent()))
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
                        .border_style(Style::default().fg(accent())),
                )
                .style(s_text()),
            rows[1],
        );

        let bottom_hint = if inner.width < 50 {
            " Enter import | Esc cancel "
        } else {
            " Enter import | Esc cancel | Ctrl+V paste from clipboard "
        };
        f.render_widget(
            Paragraph::new(bottom_hint)
                .style(s_dim())
                .alignment(Alignment::Center),
            rows[2],
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn render_group_form(
        &self,
        f: &mut Frame,
        title: &str,
        name: &str,
        url: &str,
        _cursor: usize,
        field: usize,
        area: Rect,
    ) {
        let pa = responsive_rect(65, 14, 92, 90, area);
        f.render_widget(Clear, pa);

        let block = Block::default()
            .title(format!(" {} ", title))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(accent()))
            .style(s_surface());

        let inner = block.inner(pa);
        f.render_widget(block, pa);

        let field0_style = if field == 0 { s_accent() } else { s_dim() };
        let field1_style = if field == 1 { s_accent() } else { s_dim() };

        let w = inner.width.saturating_sub(2);
        let mut y = inner.y;

        f.render_widget(
            Paragraph::new("Name:").style(field0_style),
            Rect::new(inner.x + 1, y, w, 1),
        );
        y += 1;
        let name_display = if name.is_empty() { "My Group" } else { name };
        f.render_widget(
            Paragraph::new(name_display)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(field0_style),
                )
                .style(s_text()),
            Rect::new(inner.x + 1, y, w, 3),
        );
        y += 4;

        f.render_widget(
            Paragraph::new("Subscription URL:").style(field1_style),
            Rect::new(inner.x + 1, y, w, 1),
        );
        y += 1;
        let url_display = if url.is_empty() { "https://..." } else { url };
        f.render_widget(
            Paragraph::new(url_display)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(field1_style),
                )
                .style(s_text()),
            Rect::new(inner.x + 1, y, w, 3),
        );
        y += 4;

        let hint = if field == 0 {
            " Tab to switch field | Enter next | Esc cancel "
        } else {
            " Tab to switch field | Enter save | Esc cancel "
        };
        f.render_widget(
            Paragraph::new(hint)
                .style(s_dim())
                .alignment(Alignment::Center),
            Rect::new(inner.x + 1, y, w, 2),
        );
    }

    fn render_confirm_popup(&self, f: &mut Frame, kind: &str, name: &str, area: Rect) {
        let pa = responsive_rect(50, 7, 90, 40, area);
        f.render_widget(Clear, pa);

        let block = Block::default()
            .title(" Confirm Delete ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(error()))
            .style(s_surface());

        let inner = block.inner(pa);
        f.render_widget(block, pa);

        let msg = format!("Delete {} \"{}\"?\nThis cannot be undone.", kind, name);
        let chunks =
            Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(inner);

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
        let pa = responsive_rect(60, 24, 94, 90, area);
        f.render_widget(Clear, pa);

        let block = Block::default()
            .title(" Keyboard Shortcuts ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(accent()))
            .style(s_surface());

        let inner = block.inner(pa);
        f.render_widget(block, pa);

        let global: &[(&str, &str)] = &[
            ("Tab", "Open Pages / View Switcher"),
            ("1/Esc", "Profiles tab"),
            ("2/r", "Routing tab"),
            ("3/m", "Traffic tab"),
            ("4/l", "Logs tab"),
            ("5/s", "Settings tab"),
            ("Mouse", "Wheel scroll, click tabs/items"),
            ("h/?", "This help"),
            ("q", "Detach (VPN stays on)"),
            ("Q/C-c", "Quit + stop VPN"),
        ];

        let profiles: &[(&str, &str)] = &[
            ("j/k", "Navigate cursor"),
            ("Space/Enter", "Fold / unfold group"),
            ("h/l / ←/→", "Collapse/expand / panel focus"),
            ("g/G", "Top / bottom"),
            ("c/Enter", "Connect to profile"),
            ("d", "Disconnect"),
            ("t/T", "Test latency / test group"),
            ("y", "Copy profile URI"),
            ("a", "Import profile URI"),
            ("x/X", "Delete profile / group"),
            ("u", "Update subscription"),
            ("e/U", "Edit group/profile / add group"),
            ("v", "Toggle TUN mode"),
            ("/", "Search / filter profiles"),
        ];

        let routing: &[(&str, &str)] = &[
            ("j/k", "Navigate rules"),
            ("a", "Add rule"),
            ("e", "Edit rule"),
            ("x", "Delete rule"),
            ("Space", "Toggle rule enabled"),
            ("←/→", "Cycle type/outbound in form"),
        ];

        let settings: &[(&str, &str)] = &[
            ("j/k", "Navigate settings"),
            ("Enter/Space", "Toggle / cycle / edit / execute"),
        ];

        let logs: &[(&str, &str)] = &[
            ("j/k", "Scroll up/down"),
            ("g/G", "Top / bottom"),
            ("f", "Cycle log level filter"),
        ];

        let traffic: &[(&str, &str)] = &[
            ("1/Esc", "Back to profiles"),
            ("v", "Toggle TUN mode"),
            ("d", "Disconnect"),
        ];

        let (tab_name, tab_help): (&str, &[(&str, &str)]) = match self.tab {
            ActiveTab::Profiles => ("Profiles", profiles),
            ActiveTab::Routing => ("Routing", routing),
            ActiveTab::Settings => ("Settings", settings),
            ActiveTab::Logs => ("Logs", logs),
            ActiveTab::Traffic => ("Traffic", traffic),
        };

        let sections: [(&str, &[(&str, &str)]); 2] = [("Global", global), (tab_name, tab_help)];

        let mut help: Vec<(&str, &str, bool)> = Vec::new();
        for (name, entries) in &sections {
            help.push((name, "", true));
            for (k, d) in *entries {
                help.push((k, d, false));
            }
        }

        let slot_rows = inner.height.saturating_sub(1) as usize;
        let has_more = self.help_scroll + slot_rows < help.len();
        let visible = if has_more {
            slot_rows.saturating_sub(1)
        } else {
            slot_rows
        };
        let end = (self.help_scroll + visible).min(help.len());

        f.render_widget(
            Paragraph::new(" j/k scroll · any other key close ")
                .style(s_faint())
                .alignment(Alignment::Center),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );

        let mut y = inner.y + 1;
        for (key, desc, is_header) in help.iter().take(end).skip(self.help_scroll) {
            if y >= inner.y + inner.height {
                break;
            }
            if *is_header {
                f.render_widget(
                    Paragraph::new(Span::styled(format!(" {} ", key), s_accent())),
                    Rect::new(inner.x + 1, y, inner.width.saturating_sub(2), 1),
                );
            } else {
                let line = Line::from(vec![
                    Span::styled(format!(" {:>12} ", key), s_accent()),
                    Span::styled(*desc, s_dim()),
                ]);
                f.render_widget(
                    Paragraph::new(line),
                    Rect::new(inner.x + 1, y, inner.width.saturating_sub(2), 1),
                );
            }
            y += 1;
        }

        if has_more && y < inner.y + inner.height {
            f.render_widget(
                Paragraph::new(" …")
                    .style(s_faint())
                    .alignment(Alignment::Center),
                Rect::new(inner.x, y, inner.width, 1),
            );
        }
    }
}
