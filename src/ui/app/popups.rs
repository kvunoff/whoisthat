use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};

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
            Popup::ConfirmClearLogs { source } => self.render_clear_logs_popup(f, *source, area),
            Popup::EditSubscription {
                name,
                url,
                cursor,
                field,
                auto_update,
                interval_hours,
                provider_interval_secs,
                ..
            } => self.render_group_form(
                f,
                "Edit Group",
                Some(name),
                url,
                *cursor,
                *field,
                *auto_update,
                interval_hours,
                *provider_interval_secs,
                area,
            ),
            Popup::Help => self.render_help_popup(f, area),
            Popup::Update { info } => self.render_update_popup(f, info, area),
            Popup::AddGroup { url, cursor } => {
                self.render_group_form(f, "Add Group", None, url, *cursor, 0, true, "", 0, area)
            }
            Popup::TabSwitcher { cursor } => self.render_tab_switcher_popup(f, *cursor, area),
            Popup::Recovery { config, db, day } => {
                self.render_recovery_popup(f, *config, *db, day, area)
            }
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
        name: Option<&str>,
        url: &str,
        _cursor: usize,
        field: usize,
        auto_update: bool,
        interval_hours: &str,
        provider_interval_secs: i64,
        area: Rect,
    ) {
        let pa = responsive_rect(65, if name.is_some() { 20 } else { 10 }, 92, 90, area);
        f.render_widget(Clear, pa);

        let block = Block::default()
            .title(format!(" {} ", title))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(accent()))
            .style(s_surface());

        let inner = block.inner(pa);
        f.render_widget(block, pa);

        let style_for = |field_no: usize| {
            if field == field_no {
                s_accent()
            } else {
                s_dim()
            }
        };

        let w = inner.width.saturating_sub(2);
        let mut y = inner.y;

        if let Some(name) = name {
            let name_style = style_for(0);
            f.render_widget(
                Paragraph::new("Name:").style(name_style),
                Rect::new(inner.x + 1, y, w, 1),
            );
            y += 1;
            let name_display = if name.is_empty() { "My Group" } else { name };
            f.render_widget(
                Paragraph::new(name_display)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(name_style),
                    )
                    .style(s_text()),
                Rect::new(inner.x + 1, y, w, 3),
            );
            y += 4;
        }

        let url_field = if name.is_some() { 1 } else { 0 };
        let url_style = style_for(url_field);
        f.render_widget(
            Paragraph::new("Subscription URL:").style(url_style),
            Rect::new(inner.x + 1, y, w, 1),
        );
        y += 1;
        let url_display = if url.is_empty() { "https://..." } else { url };
        f.render_widget(
            Paragraph::new(url_display)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(url_style),
                )
                .style(s_text()),
            Rect::new(inner.x + 1, y, w, 3),
        );
        y += 4;

        if name.is_some() {
            let auto_style = style_for(2);
            let mark = if auto_update { "● on" } else { "○ off" };
            f.render_widget(
                Paragraph::new(format!("Auto-update: {mark}   (Space to toggle)"))
                    .style(auto_style),
                Rect::new(inner.x + 1, y, w, 1),
            );
            y += 1;

            let interval_style = style_for(3);
            let provider_hint = if provider_interval_secs > 0 {
                format!(
                    "blank = provider: {}",
                    format_interval_secs(provider_interval_secs)
                )
            } else {
                "blank = provider: not set".to_string()
            };
            f.render_widget(
                Paragraph::new(format!("Interval, hours ({provider_hint})")).style(interval_style),
                Rect::new(inner.x + 1, y, w, 1),
            );
            y += 1;
            let interval_display = if interval_hours.is_empty() {
                "auto"
            } else {
                interval_hours
            };
            f.render_widget(
                Paragraph::new(interval_display)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(interval_style),
                    )
                    .style(s_text()),
                Rect::new(inner.x + 1, y, w, 3),
            );
            y += 4;
        }

        let hint = match (name.is_some(), field) {
            (false, _) => " Enter save | Esc cancel ",
            (true, 0 | 1) => " Tab to switch field | Ctrl+R reset name | Enter next | Esc cancel ",
            (true, 2) => " Tab to switch field | Space toggle | Enter next | Esc cancel ",
            (true, _) => " Tab to switch field | Enter save | Esc cancel ",
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

    fn render_clear_logs_popup(
        &self,
        f: &mut Frame,
        source: crate::ui::logs::LogSource,
        area: Rect,
    ) {
        let pa = responsive_rect(56, 8, 90, 40, area);
        f.render_widget(Clear, pa);

        let block = Block::default()
            .title(" Clear Logs ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(error()))
            .style(s_surface());

        let inner = block.inner(pa);
        f.render_widget(block, pa);

        let message = match source {
            crate::ui::logs::LogSource::Core => {
                "Clear core logs?\ncore.log is truncated; backups and core-crash.log are removed.\nThis cannot be undone."
            }
            crate::ui::logs::LogSource::Tui => {
                "Clear TUI log?\ntui.log is truncated and tui.log.1 is removed.\nThis cannot be undone."
            }
        };
        let chunks =
            Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(inner);

        f.render_widget(
            Paragraph::new(message)
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

    fn render_update_popup(
        &self,
        f: &mut Frame,
        info: &crate::update_check::UpdateInfo,
        area: Rect,
    ) {
        let pa = responsive_rect(64, 12, 94, 70, area);
        f.render_widget(Clear, pa);

        let block = Block::default()
            .title(" Update available ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(accent()))
            .style(s_surface());

        let inner = block.inner(pa);
        f.render_widget(block, pa);

        let chunks = Layout::vertical([Constraint::Min(5), Constraint::Length(2)]).split(inner);

        let mut lines = vec![
            Line::from(vec![
                Span::styled("WhoisThat ", s_dim()),
                Span::styled(format!("v{}", info.current), s_text()),
                Span::styled(" → ", s_dim()),
                Span::styled(format!("v{}", info.latest), s_accent_bold()),
            ]),
            Line::from(vec![
                Span::styled("Installed via: ", s_dim()),
                Span::styled(info.source.label(), s_text()),
            ]),
            Line::from(""),
        ];

        match &info.command {
            Some(command) => {
                lines.push(Line::from(Span::styled("Update command:", s_dim())));
                lines.push(Line::from(Span::styled(command.clone(), s_text())));
            }
            None => {
                lines.push(Line::from(Span::styled("Open the release page:", s_dim())));
                lines.push(Line::from(Span::styled(
                    info.release_url.clone(),
                    s_accent(),
                )));
            }
        }

        f.render_widget(
            Paragraph::new(lines)
                .style(s_text())
                .wrap(Wrap { trim: true }),
            chunks[0],
        );

        let hint = if info.command.is_some() {
            " Enter/u copy command  |  s skip this version  |  Esc later "
        } else {
            " s skip this version  |  Esc later "
        };
        f.render_widget(
            Paragraph::new(hint)
                .style(s_accent())
                .alignment(Alignment::Center),
            chunks[1],
        );
    }

    fn render_recovery_popup(&self, f: &mut Frame, config: bool, db: bool, day: &str, area: Rect) {
        let pa = responsive_rect(62, 10, 94, 60, area);
        f.render_widget(Clear, pa);

        let block = Block::default()
            .title(" Repair WhoisThat ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(error()))
            .style(s_surface());

        let inner = block.inner(pa);
        f.render_widget(block, pa);

        let mut msg = String::from("WhoisThat could not read:\n");
        if config {
            msg.push_str("  - settings (tun name, split tunnel, autoconnect)\n");
        }
        if db {
            msg.push_str("  - profiles and groups\n");
        }
        msg.push_str("\nThe damaged files were kept. WhoisThat can restore them");
        if db && !day.is_empty() {
            msg.push_str(&format!(" from the backup of {}.", day));
        } else {
            msg.push('.');
        }

        let chunks = Layout::vertical([Constraint::Min(4), Constraint::Length(2)]).split(inner);

        f.render_widget(
            Paragraph::new(msg)
                .style(s_text())
                .alignment(Alignment::Left)
                .wrap(Wrap { trim: true }),
            chunks[0],
        );

        f.render_widget(
            Paragraph::new(" Enter restore | Esc later ")
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

        let ctx = match self.tab {
            ActiveTab::Profiles => crate::keymap::KeyContext::ProfilesTree,
            ActiveTab::Routing => crate::keymap::KeyContext::Routing,
            ActiveTab::Settings => crate::keymap::KeyContext::Settings,
            ActiveTab::Logs => crate::keymap::KeyContext::Logs,
            ActiveTab::Traffic => crate::keymap::KeyContext::Traffic,
        };

        let (tab_name, tab_help) = crate::keymap::get_tab_help(ctx);

        let mut help: Vec<(&str, &str, bool)> = Vec::new();
        help.push(("Global Shortcuts", "", true));
        for entry in crate::keymap::GLOBAL_HELP {
            help.push((entry.key, entry.desc, false));
        }
        help.push((tab_name, "", true));
        for entry in tab_help {
            help.push((entry.key, entry.desc, false));
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
                    Span::styled(format!(" {:>15} ", key), s_accent()),
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
