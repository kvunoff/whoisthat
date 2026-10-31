//! Shared parsing/formatting for core and TUI log lines.
//!
//! New format (both components): `2026/10/31 13:37:11 [WARN] message`.
//! Older files are still parsed: TUI `12:00:00 WARN  message` and core
//! `12:00:00 [WARN] message`.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
    Trace = 4,
}

impl Level {
    /// Four-letter tag used in log files. `FATA` (Go `log.Fatal`) maps to
    /// `Error` on read but is never emitted by the TUI.
    pub fn tag(self) -> &'static str {
        match self {
            Level::Error => "ERRO",
            Level::Warn => "WARN",
            Level::Info => "INFO",
            Level::Debug => "DEBU",
            Level::Trace => "TRAC",
        }
    }

    pub fn from_log_level(level: log::Level) -> Self {
        match level {
            log::Level::Error => Level::Error,
            log::Level::Warn => Level::Warn,
            log::Level::Info => Level::Info,
            log::Level::Debug => Level::Debug,
            log::Level::Trace => Level::Trace,
        }
    }

    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag.to_ascii_lowercase().as_str() {
            "erro" | "error" | "fata" | "fatal" => Some(Level::Error),
            "warn" | "warning" => Some(Level::Warn),
            "info" => Some(Level::Info),
            "debu" | "debug" => Some(Level::Debug),
            "trac" | "trace" => Some(Level::Trace),
            _ => None,
        }
    }
}

pub struct LogLine<'a> {
    /// Full `YYYY/MM/DD HH:MM:SS` when the line carries a date.
    pub timestamp: Option<&'a str>,
    /// `HH:MM:SS` (empty when the line has no timestamp).
    pub time: &'a str,
    pub level: Option<Level>,
    /// Original tag (`ERRO`, `FATA`, `WARN`, …); kept for JSON compatibility.
    pub tag: Option<&'a str>,
    pub message: &'a str,
}

pub fn parse(line: &str) -> LogLine<'_> {
    if let Some(ts) = line.get(0..19) {
        if looks_like_timestamp(ts) && line.as_bytes().get(19) == Some(&b' ') {
            let rest = line[19..].trim_start();
            let (level, tag, message) = split_level(rest);
            return LogLine {
                timestamp: Some(ts),
                time: &ts[11..19],
                level,
                tag,
                message,
            };
        }
    }

    if let Some(ts) = line.get(0..8) {
        if looks_like_time(ts) && line.as_bytes().get(8) == Some(&b' ') {
            let rest = line[9..].trim_start();
            let (level, tag, message) = split_level(rest);
            return LogLine {
                timestamp: None,
                time: ts,
                level,
                tag,
                message,
            };
        }
    }

    let (level, tag, message) = split_level(line);
    LogLine {
        timestamp: None,
        time: "",
        level,
        tag,
        message,
    }
}

fn split_level(rest: &str) -> (Option<Level>, Option<&str>, &str) {
    let rest = rest.trim_start();
    if let Some(stripped) = rest.strip_prefix('[') {
        if let Some((tag, tail)) = stripped.split_once(']') {
            if let Some(level) = Level::from_tag(tag) {
                return (Some(level), Some(tag), tail.trim_start());
            }
        }
    }

    // Older TUI format: `WARN  message`.
    let mut parts = rest.splitn(2, char::is_whitespace);
    let first = parts.next().unwrap_or("");
    if let Some(level) = Level::from_tag(first) {
        return (
            Some(level),
            Some(first),
            parts.next().unwrap_or("").trim_start(),
        );
    }

    (None, None, rest)
}

fn looks_like_timestamp(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 19
        && b[4] == b'/'
        && b[7] == b'/'
        && b[10] == b' '
        && b[13] == b':'
        && b[16] == b':'
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'/',
            10 => *c == b' ',
            13 | 16 => *c == b':',
            _ => c.is_ascii_digit(),
        })
}

fn looks_like_time(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 8
        && b[2] == b':'
        && b[5] == b':'
        && b.iter().enumerate().all(|(i, c)| match i {
            2 | 5 => *c == b':',
            _ => c.is_ascii_digit(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_new_core_and_tui_format() {
        let line = parse("2026/10/31 13:37:11 [WARN] xray stopped");
        assert_eq!(line.timestamp, Some("2026/10/31 13:37:11"));
        assert_eq!(line.time, "13:37:11");
        assert_eq!(line.level, Some(Level::Warn));
        assert_eq!(line.message, "xray stopped");
    }

    #[test]
    fn parses_old_tui_format() {
        let line = parse("12:00:00 WARN  something happened");
        assert_eq!(line.timestamp, None);
        assert_eq!(line.time, "12:00:00");
        assert_eq!(line.level, Some(Level::Warn));
        assert_eq!(line.message, "something happened");
    }

    #[test]
    fn parses_old_core_format() {
        let line = parse("12:00:00 [INFO] connected");
        assert_eq!(line.time, "12:00:00");
        assert_eq!(line.level, Some(Level::Info));
        assert_eq!(line.message, "connected");
    }

    #[test]
    fn fatal_maps_to_error() {
        let line = parse("2026/10/31 13:37:11 [FATA] encryption key is invalid");
        assert_eq!(line.level, Some(Level::Error));
        assert_eq!(line.tag, Some("FATA"));
    }

    #[test]
    fn line_without_level_keeps_whole_message() {
        let line = parse("hello [ERRO] world");
        assert_eq!(line.level, None);
        assert_eq!(line.message, "hello [ERRO] world");
    }

    #[test]
    fn levels_sort_by_severity() {
        assert!(Level::Error < Level::Warn);
        assert!(Level::Warn < Level::Info);
        assert!(Level::Info < Level::Debug);
        assert!(Level::Debug < Level::Trace);
    }
}
