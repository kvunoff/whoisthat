use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};

use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::theme::*;

pub struct LogsState {
    pub lines: Vec<String>,
    pub scroll: usize,
    pub auto_scroll: bool,
    reader: Option<BufReader<File>>,
    log_path: String,
}

impl LogsState {
    pub fn new(log_path: &str) -> Self {
        let (lines, reader) = read_log_file(log_path);

        Self {
            lines,
            scroll: 0,
            auto_scroll: true,
            reader,
            log_path: log_path.to_string(),
        }
    }

    pub fn poll(&mut self) {
        if let Some(ref mut reader) = self.reader {
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                self.lines.push(line.clone());
                line.clear();
            }
            // Keep only last 500 lines
            if self.lines.len() > 500 {
                let drain = self.lines.len() - 500;
                self.lines.drain(..drain);
            }
        }
    }

    pub fn scroll_down(&mut self) {
        let vis = self.lines.len().saturating_sub(1);
        if self.scroll < vis {
            self.scroll += 1;
        }
    }

    pub fn scroll_up(&mut self) {
        if self.scroll > 0 {
            self.scroll -= 1;
        }
    }

    pub fn scroll_bottom(&mut self) {
        self.scroll = self.lines.len().saturating_sub(1);
    }

    pub fn scroll_top(&mut self) {
        self.scroll = 0;
    }
}

fn read_log_file(path: &str) -> (Vec<String>, Option<BufReader<File>>) {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return (vec!["(log file not found)".into()], None),
    };

    let mut reader = BufReader::new(file);
    let mut lines = Vec::new();
    let mut line = String::new();
    while reader.read_line(&mut line).unwrap_or(0) > 0 {
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        }
        lines.push(line.clone());
        line.clear();
    }

    // Seek to end for tailing
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return (lines, None),
    };
    let _ = file.seek(SeekFrom::End(0));
    let reader = BufReader::new(file);

    // Keep last 500
    if lines.len() > 500 {
        lines.drain(..lines.len() - 500);
    }

    if lines.is_empty() {
        lines.push("(log file empty)".into());
    }

    (lines, Some(reader))
}

pub fn render_logs(f: &mut Frame, area: Rect, state: &LogsState, focused: bool) {
    let border_color = if focused { BORDER_ACTIVE } else { BORDER };

    let block = Block::default()
        .title(" Logs ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(s_bg());

    let inner = block.inner(area);
    f.render_widget(block, area);

    if state.lines.is_empty() {
        let mid = Layout::vertical([
            Constraint::Percentage(45),
            Constraint::Min(1),
            Constraint::Percentage(55),
        ])
        .split(inner)[1];
        f.render_widget(
            Paragraph::new("No log data yet.").style(s_dim()).alignment(Alignment::Center),
            mid,
        );
        return;
    }

    let vis = inner.height as usize;
    let total = state.lines.len();
    let max_scroll = total.saturating_sub(vis);
    let scroll = state.scroll.min(max_scroll);

    let items: Vec<ListItem> = state
        .lines
        .iter()
        .skip(scroll)
        .take(vis)
        .map(|l| {
            let style = if l.contains("[ERRO]") || l.contains("[WARN]") {
                s_error()
            } else {
                s_faint()
            };
            ListItem::new(Line::from(Span::styled(l, style)))
        })
        .collect();

    f.render_widget(List::new(items).style(s_bg()), inner);
}
