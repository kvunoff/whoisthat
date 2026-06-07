use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use super::theme::*;

#[derive(Debug, Clone)]
pub struct SettingsState {
    pub cursor: usize,
}

impl SettingsState {
    pub fn new() -> Self {
        Self { cursor: 0 }
    }

    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn cursor_down(&mut self) {
        if self.cursor < 1 {
            self.cursor += 1;
        }
    }
}

pub fn render_settings(
    f: &mut Frame,
    area: Rect,
    autoconnect: bool,
    show_ip: bool,
    state: &SettingsState,
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

    let rows = Layout::vertical([Constraint::Percentage(30), Constraint::Min(0)]).split(inner);
    let content_area = rows[1];

    let items = [
        ("Autoconnect", if autoconnect { "on" } else { "off" }),
        ("Show IP",     if show_ip     { "on" } else { "off" }),
    ];

    let lines: Vec<Line> = items
        .iter()
        .enumerate()
        .map(|(i, (label, val))| {
            let cursor = i == state.cursor && focused;
            let ls = if cursor { s_accent() } else { s_dim() };
            let marker = if cursor && focused { ">" } else { " " };
            let on = *val == "on";
            let val_style = if on { s_success() } else { s_disconnected() };
            let toggle = format!(" {}", if on { "●" } else { "○" });
            Line::from(vec![
                Span::raw(format!(" {} ", marker)),
                Span::styled(*label, ls),
                Span::raw("  "),
                Span::styled(toggle, val_style),
                Span::styled(format!(" {}", val), val_style),
            ])
        })
        .collect();

    let help = Paragraph::new(" j/k navigate  │  Space/Enter toggle ")
        .style(s_faint())
        .alignment(Alignment::Center);

    f.render_widget(Paragraph::new(lines).style(s_bg()), content_area);
    f.render_widget(help, rows[0]);
}
