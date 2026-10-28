use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;

use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::theme::*;

/// Rendered log tail length and how much of the file is read at startup.
const MAX_LINES: usize = 500;
const TAIL_BYTES: u64 = 64 * 1024;

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
        1 => s_warn(),
        2 => s_success(),
        3 | 4 => s_dim(),
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
    /// (device, inode) of the file the reader is attached to. A change means
    /// the file was rotated (lumberjack) and the tail must be reopened.
    file_id: Option<(u64, u64)>,
    file_len: u64,
}

impl LogsState {
    pub fn new(log_path: &str) -> Self {
        let (lines, reader) = read_log_file(log_path);
        let scroll = lines.len().saturating_sub(1);
        let (file_id, file_len) = file_state(log_path);

        Self {
            lines,
            scroll,
            auto_scroll: true,
            filter: LogFilter::All,
            reader,
            log_path: log_path.to_string(),
            file_id,
            file_len,
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
        let (file_id, file_len) = file_state(&self.log_path);

        let rotated = match (file_id, self.file_id) {
            (Some(current), Some(previous)) => current != previous,
            (Some(_), None) => true,
            (None, _) => false,
        };
        let truncated = file_len < self.file_len;

        if rotated || truncated || self.reader.is_none() {
            // A missing file stays missing: do not rebuild "(log file not
            // found)" on every tick.
            if file_id.is_none() && self.reader.is_none() {
                return;
            }
            (self.lines, self.reader) = read_log_file(&self.log_path);
            self.file_id = file_id;
            self.file_len = file_len;
            if self.auto_scroll {
                self.scroll = self.lines.len().saturating_sub(1);
            }
            return;
        }

        if let Some(reader) = self.reader.as_mut() {
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                let trimmed = line.trim_end_matches(['\n', '\r']).to_string();
                if self.lines.len() == 1
                    && (self.lines[0] == "(log file not found)"
                        || self.lines[0] == "(log file empty)")
                {
                    self.lines.clear();
                }
                self.lines.push(trimmed);
                line.clear();
            }
            if self.lines.len() > MAX_LINES {
                let drain = self.lines.len() - MAX_LINES;
                self.lines.drain(..drain);
            }
            if self.auto_scroll {
                self.scroll = self.lines.len().saturating_sub(1);
            }
        }
        self.file_len = file_len;
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

    pub fn scroll_page_down(&mut self, step: usize) {
        let vis = self.lines.len().saturating_sub(1);
        self.scroll = (self.scroll + step).min(vis);
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

    pub fn scroll_page_up(&mut self, step: usize) {
        self.scroll = self.scroll.saturating_sub(step);
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

fn file_state(path: &str) -> (Option<(u64, u64)>, u64) {
    match std::fs::metadata(path) {
        Ok(meta) => (Some((meta.dev(), meta.ino())), meta.len()),
        Err(_) => (None, 0),
    }
}

fn read_log_file(path: &str) -> (Vec<String>, Option<BufReader<File>>) {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return (vec!["(log file not found)".into()], None),
    };

    let truncated = file
        .metadata()
        .map(|m| m.len() > TAIL_BYTES)
        .unwrap_or(false);
    if truncated {
        let _ = file.seek(SeekFrom::End(-(TAIL_BYTES as i64)));
    }

    let mut buf = Vec::new();
    if file.read_to_end(&mut buf).is_err() {
        return (vec!["(log file not readable)".into()], None);
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    if truncated {
        // The first entry may be a partial line: we started mid-file.
        if !lines.is_empty() {
            lines.remove(0);
        }
    }
    if lines.is_empty() {
        lines.push("(log file empty)".into());
    }
    if lines.len() > MAX_LINES {
        lines.drain(..lines.len() - MAX_LINES);
    }

    // read_to_end leaves the cursor at EOF, which is where tailing starts.
    (lines, Some(BufReader::new(file)))
}

pub fn render_logs(f: &mut Frame, area: Rect, state: &LogsState, focused: bool) {
    let border_color = if focused { border_active() } else { border() };

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_path(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("whoisthat-logs-{}-{}", std::process::id(), name));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("core.log")
    }

    fn numbered_lines(count: usize) -> String {
        let mut out = String::new();
        for i in 0..count {
            out.push_str(&format!("line {i}\n"));
        }
        out
    }

    #[test]
    fn read_log_file_keeps_the_last_500_lines() {
        let path = tmp_path("tail");
        std::fs::write(&path, numbered_lines(1000)).unwrap();

        let (lines, _) = read_log_file(path.to_str().unwrap());

        assert_eq!(lines.len(), MAX_LINES);
        assert_eq!(lines.last().unwrap(), "line 999");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn read_log_file_reads_only_the_tail() {
        let path = tmp_path("tail-bytes");
        std::fs::write(&path, numbered_lines(10_000)).unwrap();
        assert!(std::fs::metadata(&path).unwrap().len() > TAIL_BYTES);

        let (lines, _) = read_log_file(path.to_str().unwrap());

        assert_eq!(lines.len(), MAX_LINES);
        assert_eq!(lines.last().unwrap(), "line 9999");
        // The first retained entry is a whole line, not a mid-file fragment.
        assert!(lines[0].starts_with("line "));
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn poll_reads_appended_lines_and_reopens_after_rotation() {
        let path = tmp_path("poll");
        std::fs::write(&path, "first\n").unwrap();
        let mut state = LogsState::new(path.to_str().unwrap());
        assert_eq!(state.lines, vec!["first"]);

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(b"second\n").unwrap();
        drop(file);

        state.poll();
        assert_eq!(state.lines.last().unwrap(), "second");

        // lumberjack-style rotation: the old file is renamed away.
        let rotated = path.with_extension("log.1");
        std::fs::rename(&path, &rotated).unwrap();
        std::fs::write(&path, "rotated\n").unwrap();

        state.poll();
        assert_eq!(state.lines.last().unwrap(), "rotated");
        assert!(!state.lines.iter().any(|l| l == "second"));

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }
}
