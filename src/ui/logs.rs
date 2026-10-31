use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;

use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::theme::*;
use crate::config;
use crate::logfmt::{self, Level};

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

    fn min_level(self) -> Option<Level> {
        match self {
            LogFilter::All => None,
            LogFilter::Error => Some(Level::Error),
            LogFilter::Warn => Some(Level::Warn),
            LogFilter::Info => Some(Level::Info),
            LogFilter::Debug => Some(Level::Debug),
            LogFilter::Trace => Some(Level::Trace),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogSource {
    Core,
    Tui,
}

impl LogSource {
    pub fn label(self) -> &'static str {
        match self {
            LogSource::Core => "core",
            LogSource::Tui => "tui",
        }
    }

    fn next(self) -> Self {
        match self {
            LogSource::Core => LogSource::Tui,
            LogSource::Tui => LogSource::Core,
        }
    }

    fn paths(self) -> Vec<std::path::PathBuf> {
        match self {
            LogSource::Core => config::core_log_paths(),
            LogSource::Tui => config::tui_log_paths(),
        }
    }
}

fn line_level(line: &str) -> Option<Level> {
    logfmt::parse(line).level
}

fn line_passes_filter(line: &str, filter: LogFilter) -> bool {
    let Some(min) = filter.min_level() else {
        return true;
    };
    line_level(line).is_some_and(|level| level <= min)
}

fn line_style(line: &str) -> Style {
    match line_level(line) {
        Some(Level::Error) => s_error(),
        Some(Level::Warn) => s_warn(),
        Some(Level::Info) => s_success(),
        Some(Level::Debug) | Some(Level::Trace) => s_dim(),
        None => s_faint(),
    }
}

pub struct LogsState {
    pub lines: Vec<String>,
    pub scroll: usize,
    pub auto_scroll: bool,
    pub filter: LogFilter,
    pub source: LogSource,
    reader: Option<BufReader<File>>,
    log_path: String,
    /// Height of the rendered list; used to clamp `scroll` (top line index).
    viewport: usize,
    /// (device, inode) of the file the reader is attached to. A change means
    /// the file was rotated (lumberjack) and the tail must be reopened.
    file_id: Option<(u64, u64)>,
    file_len: u64,
}

impl LogsState {
    pub fn new() -> Self {
        let path = config::first_existing(&config::core_log_paths());
        let (lines, reader) = read_log_file(&path.to_string_lossy());
        let (file_id, file_len) = file_state(&path.to_string_lossy());

        let mut state = Self {
            lines,
            scroll: 0,
            auto_scroll: true,
            filter: LogFilter::All,
            source: LogSource::Core,
            reader,
            log_path: path.to_string_lossy().into_owned(),
            viewport: 20,
            file_id,
            file_len,
        };
        state.scroll = state.max_scroll();
        state
    }

    pub fn cycle_filter(&mut self) {
        self.filter = self.filter.next();
        self.auto_scroll = true;
        self.scroll = self.max_scroll();
    }

    pub fn cycle_source(&mut self) {
        self.source = self.source.next();
        let path = config::first_existing(&self.source.paths());
        self.log_path = path.to_string_lossy().into_owned();
        (self.lines, self.reader) = read_log_file(&self.log_path);
        let (file_id, file_len) = file_state(&self.log_path);
        self.file_id = file_id;
        self.file_len = file_len;
        self.auto_scroll = true;
        self.scroll = self.max_scroll();
    }

    fn filtered_lines(&self) -> Vec<&String> {
        self.lines
            .iter()
            .filter(|l| line_passes_filter(l, self.filter))
            .collect()
    }

    /// Highest valid top-line index. `scroll` is the first visible line, not
    /// the last, so the maximum is `total - viewport`.
    fn max_scroll(&self) -> usize {
        self.filtered_lines()
            .len()
            .saturating_sub(self.viewport.max(1))
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
                self.scroll = self.max_scroll();
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
                self.scroll = self.max_scroll();
            }
        }
        self.file_len = file_len;
    }

    pub fn scroll_down(&mut self) {
        let max = self.max_scroll();
        if self.scroll < max {
            self.scroll += 1;
        }
        if self.scroll >= max {
            self.auto_scroll = true;
        }
    }

    pub fn scroll_page_down(&mut self, step: usize) {
        let max = self.max_scroll();
        self.scroll = (self.scroll + step).min(max);
        if self.scroll >= max {
            self.auto_scroll = true;
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
        self.auto_scroll = false;
    }

    pub fn scroll_page_up(&mut self, step: usize) {
        self.scroll = self.scroll.saturating_sub(step);
        self.auto_scroll = false;
    }

    pub fn scroll_bottom(&mut self) {
        self.scroll = self.max_scroll();
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
    let path_ref = std::path::Path::new(path);
    let (mut lines, offset) = match crate::logfile::read_tail(path_ref, TAIL_BYTES) {
        Ok(value) => value,
        Err(_) => return (vec!["(log file not found)".into()], None),
    };

    if lines.is_empty() {
        lines.push("(log file empty)".into());
    }
    if lines.len() > MAX_LINES {
        lines.drain(..lines.len() - MAX_LINES);
    }

    let mut file = match File::open(path_ref) {
        Ok(file) => file,
        Err(_) => return (lines, None),
    };
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return (lines, None);
    }
    (lines, Some(BufReader::new(file)))
}

pub fn render_logs(f: &mut Frame, area: Rect, state: &mut LogsState, focused: bool) {
    let border_color = if focused { border_active() } else { border() };

    let title = format!(
        " Logs [{}] [{}] ",
        state.source.label(),
        state.filter.label()
    );

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(s_bg());

    let inner = block.inner(area);
    f.render_widget(block, area);

    state.viewport = inner.height as usize;

    let vis = inner.height as usize;
    let total = state.filtered_lines().len();

    if total == 0 {
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

    let max_scroll = total.saturating_sub(vis);
    let scroll = state.scroll.min(max_scroll);
    state.scroll = scroll;

    let filtered: Vec<&String> = state.filtered_lines();
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
        let mut state = LogsState {
            lines: Vec::new(),
            scroll: 0,
            auto_scroll: true,
            filter: LogFilter::All,
            source: LogSource::Core,
            reader: None,
            log_path: path.to_str().unwrap().to_string(),
            viewport: 20,
            file_id: None,
            file_len: 0,
        };
        (state.lines, state.reader) = read_log_file(&state.log_path);
        assert_eq!(state.lines, vec!["first"]);

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        std::io::Write::write_all(&mut file, b"second\n").unwrap();
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

    #[test]
    fn fatal_is_an_error_for_filtering() {
        assert!(line_passes_filter(
            "2026/10/31 13:37:11 [FATA] encryption key is invalid",
            LogFilter::Error
        ));
        assert!(!line_passes_filter(
            "2026/10/31 13:37:11 [INFO] fine",
            LogFilter::Error
        ));
    }

    #[test]
    fn old_tui_lines_are_filtered_too() {
        assert!(line_passes_filter("12:00:00 WARN  boom", LogFilter::Warn));
        assert!(!line_passes_filter("12:00:00 INFO  ok", LogFilter::Warn));
    }

    #[test]
    fn scroll_bottom_uses_the_viewport() {
        let path = tmp_path("scroll");
        std::fs::write(&path, numbered_lines(100)).unwrap();
        let mut state = LogsState {
            lines: Vec::new(),
            scroll: 0,
            auto_scroll: true,
            filter: LogFilter::All,
            source: LogSource::Core,
            reader: None,
            log_path: path.to_str().unwrap().to_string(),
            viewport: 10,
            file_id: None,
            file_len: 0,
        };
        (state.lines, state.reader) = read_log_file(&state.log_path);

        state.scroll_bottom();
        assert_eq!(state.scroll, 90);
        state.scroll_up();
        assert_eq!(state.scroll, 89);

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }
}
