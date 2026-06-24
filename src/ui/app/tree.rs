use ratatui::layout::{Alignment, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use super::state::App;
use super::types::*;
use crate::ui::theme::*;
use crate::ui::uri::{self};

impl App {
    pub(super) fn render_tree(&mut self, f: &mut Frame, area: Rect) {
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

        for (_gi, g) in self.groups.iter().enumerate() {
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

            for (_pi, p) in g.profiles.iter().enumerate() {
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
}
