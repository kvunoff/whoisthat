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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogFilter {
    All,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogFilter {
    pub fn next(self) -> Self {
        match self {
            LogFilter::All => LogFilter::Error,
            LogFilter::Error => LogFilter::Warn,
            LogFilter::Warn => LogFilter::Info,
            LogFilter::Info => LogFilter::Debug,
            LogFilter::Debug => LogFilter::Trace,
            LogFilter::Trace => LogFilter::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LogFilter::All => "all",
            LogFilter::Error => "error+",
            LogFilter::Warn => "warn+",
            LogFilter::Info => "info+",
            LogFilter::Debug => "debug+",
            LogFilter::Trace => "trace+",
        }
    }
}

fn line_level(line: &str) -> u8 {
    if line.contains("[ERRO]") {
        0 // error
    } else if line.contains("[WARN]") {
        1
    } else if line.contains("[INFO]") {
        2
    } else if line.contains("[DEBU]") {
        3
    } else if line.contains("[TRAC]") {
        4
    } else {
        5 // unknown/no level
    }
}

fn filter_min_level(filter: LogFilter) -> u8 {
    match filter {
        LogFilter::All => 255,
        LogFilter::Error => 0,
        LogFilter::Warn => 1,
        LogFilter::Info => 2,
        LogFilter::Debug => 3,
        LogFilter::Trace => 4,
    }
}

fn line_passes_filter(line: &str, filter: LogFilter) -> bool {
    if filter == LogFilter::All {
        return true;
    }
    let level = line_level(line);
    if level == 5 {
        // Lines without a level tag: shown only in All
        return false;
    }
    level <= filter_min_level(filter)
}

fn line_style(line: &str) -> Style {
    match line_level(line) {
        0 => s_error(),
        1 => s_accent(),
        2 => s_success(),
        3 | 4 => s_faint(),
        _ => s_faint(),
    }
}

pub struct LogsState {
    pub lines: Vec<String>,
    pub scroll: usize,
    pub auto_scroll: bool,
    pub filter: LogFilter,
    reader: Option<BufReader<File>>,
    log_path: String,
}

impl LogsState {
    pub fn new(log_path: &str) -> Self {
        let (lines, reader) = read_log_file(log_path);
        let scroll = lines.len().saturating_sub(1);

        Self {
            lines,
            scroll,
            auto_scroll: true,
            filter: LogFilter::All,
            reader,
            log_path: log_path.to_string(),
        }
    }

    pub fn cycle_filter(&mut self) {
        self.filter = self.filter.next();
        self.scroll = self.filtered_lines().len().saturating_sub(1);
        self.auto_scroll = true;
    }

    fn filtered_lines(&self) -> Vec<&String> {
        self.lines
            .iter()
            .filter(|l| line_passes_filter(l, self.filter))
            .collect()
    }

    pub fn poll(&mut self) {
        if self.reader.is_none() {
            (self.lines, self.reader) = read_log_file(&self.log_path);
            if self.reader.is_some() {
                self.scroll = self.lines.len().saturating_sub(1);
            }
            return;
        }
        if let Some(ref mut reader) = self.reader {
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                if self.lines.len() == 1
                    && (self.lines[0] == "(log file not found)"
                        || self.lines[0] == "(log file empty)")
                {
                    self.lines.clear();
                }
                self.lines.push(line.clone());
                line.clear();
            }
            if self.lines.len() > 500 {
                let drain = self.lines.len() - 500;
                self.lines.drain(..drain);
            }
            if self.auto_scroll {
                self.scroll = self.lines.len().saturating_sub(1);
            }
        }
    }

    pub fn scroll_down(&mut self) {
        let vis = self.lines.len().saturating_sub(1);
        if self.scroll < vis {
            self.scroll += 1;
        }
        if self.scroll >= vis {
            self.auto_scroll = true;
        }
    }

    pub fn scroll_up(&mut self) {
        if self.scroll > 0 {
            self.scroll -= 1;
        }
        self.auto_scroll = false;
    }

    pub fn scroll_bottom(&mut self) {
        self.scroll = self.lines.len().saturating_sub(1);
        self.auto_scroll = true;
    }

    pub fn scroll_top(&mut self) {
        self.scroll = 0;
        self.auto_scroll = false;
    }
}

fn read_log_file(path: &str) -> (Vec<String>, Option<BufReader<File>>) {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return (vec!["(log file not found)".into()], None),
    };

    let mut lines = Vec::new();
    let mut line = String::new();
    let mut reader = BufReader::new(&file);
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
    drop(reader);

    // Seek to end for tailing
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

    let title = format!(" Logs [{}] ", state.filter.label());

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(s_bg());

    let inner = block.inner(area);
    f.render_widget(block, area);

    let filtered: Vec<&String> = state.filtered_lines();

    if filtered.is_empty() {
        let mid = Layout::vertical([
            Constraint::Percentage(45),
            Constraint::Min(1),
            Constraint::Percentage(55),
        ])
        .split(inner)[1];
        f.render_widget(
            Paragraph::new(format!("No logs at {} level.", state.filter.label()))
                .style(s_dim())
                .alignment(Alignment::Center),
            mid,
        );
        return;
    }

    let vis = inner.height as usize;
    let total = filtered.len();
    let max_scroll = total.saturating_sub(vis);
    let scroll = state.scroll.min(max_scroll);

    let items: Vec<ListItem> = filtered
        .iter()
        .skip(scroll)
        .take(vis)
        .map(|l| ListItem::new(Line::from(Span::styled(l.as_str(), line_style(l)))))
        .collect();

    f.render_widget(List::new(items).style(s_bg()), inner);
}
