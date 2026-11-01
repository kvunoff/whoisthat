use std::fs::{File, Metadata};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Local, NaiveDate, NaiveDateTime};
use serde_json::json;

use crate::config;
use crate::logfile::{self, ClearAction, Cleared};
use crate::logfmt::{self, Level};

use super::super::args::{ClearLogsArgs, ClearSource, GlobalArgs, LogSourceArg, LogsArgs, LogsCmd};
use super::super::error::CliError;
use super::super::output::{format_bytes, Out};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Parsed {
    timestamp: Option<String>,
    at: Option<NaiveDateTime>,
    time: String,
    level: Option<Level>,
    tag: Option<String>,
    message: String,
    raw: String,
}

struct Filter {
    min: Option<Level>,
    since: Option<NaiveDateTime>,
    grep: Option<String>,
}

impl Filter {
    fn new(min: Option<Level>, since: Option<&str>, grep: Option<&str>) -> Result<Self, CliError> {
        let since = match since {
            Some(value) => {
                Some(parse_since(value, Local::now().naive_local()).map_err(CliError::Usage)?)
            }
            None => None,
        };
        Ok(Self {
            min,
            since,
            grep: grep.map(str::to_lowercase),
        })
    }

    fn passes(&self, line: &Parsed) -> bool {
        if let Some(min) = self.min {
            if !line.level.is_some_and(|level| level <= min) {
                return false;
            }
        }
        // Lines from the old format carry no date; keep them rather than
        // silently dropping data that might be recent.
        if let Some(since) = self.since {
            if line.at.is_some_and(|at| at < since) {
                return false;
            }
        }
        if let Some(grep) = &self.grep {
            if !line.raw.to_lowercase().contains(grep) {
                return false;
            }
        }
        true
    }

    fn apply(&self, lines: Vec<Parsed>) -> Vec<Parsed> {
        lines.into_iter().filter(|line| self.passes(line)).collect()
    }
}

pub async fn run(g: &GlobalArgs, args: LogsArgs) -> Result<(), CliError> {
    if let Some(LogsCmd::Clear(clear)) = args.cmd {
        return clear_logs(g, clear);
    }

    let source = resolve_source(args.source, args.tui)?;
    let paths = source_paths(source);
    let path = config::first_existing(&paths);
    let filter = Filter::new(
        args.level.map(|level| level.level()),
        args.since.as_deref(),
        args.grep.as_deref(),
    )?;

    if !path.exists() {
        let out = Out::new(g);
        out.note(format!("log file not found: {}", path.display()));
        return Ok(());
    }

    let (raw, offset) = crate::logfile::read_tail(&path, tail_budget(args.lines))?;
    let mut lines: Vec<Parsed> = raw.iter().map(|line| parse_line(line)).collect();
    if lines.len() > args.lines {
        lines.drain(..lines.len() - args.lines);
    }
    lines = filter.apply(lines);

    let out = Out::new(g);

    if !args.follow {
        if g.json {
            let list: Vec<serde_json::Value> = lines.iter().map(line_json).collect();
            out.json(&list);
        } else {
            for line in &lines {
                println!("{}", line.raw);
            }
        }
        return Ok(());
    }

    // Start following from the exact byte the initial read stopped at: lines
    // appended after `read_tail` are picked up, nothing is duplicated.
    let mut tail = Tail::open_at(&path, offset)?;
    emit_follow(&out, g, &lines);

    loop {
        tokio::time::sleep(Duration::from_millis(250)).await;
        let new_lines = filter.apply(tail.poll());
        emit_follow(&out, g, &new_lines);
    }
}

fn resolve_source(source: Option<LogSourceArg>, tui: bool) -> Result<LogSourceArg, CliError> {
    match (source, tui) {
        (Some(_), true) => Err(CliError::Usage(
            "--tui conflicts with --source; use one of them".into(),
        )),
        (Some(source), false) => Ok(source),
        (None, true) => Ok(LogSourceArg::Tui),
        (None, false) => Ok(LogSourceArg::Core),
    }
}

fn source_paths(source: LogSourceArg) -> Vec<PathBuf> {
    match source {
        LogSourceArg::Core => config::core_log_paths(),
        LogSourceArg::Tui => config::tui_log_paths(),
    }
}

fn clear_logs(g: &GlobalArgs, args: ClearLogsArgs) -> Result<(), CliError> {
    if !args.yes {
        return Err(CliError::Usage(
            "logs clear is destructive; pass --yes to confirm".into(),
        ));
    }

    let out = Out::new(g);
    let mut results: Vec<Cleared> = Vec::new();
    if matches!(args.source, ClearSource::Core | ClearSource::All) {
        results.extend(logfile::clear_core(&config::core_log_paths()));
        results.extend(logfile::clear_crash(&config::data_dir()));
    }
    if matches!(args.source, ClearSource::Tui | ClearSource::All) {
        results.extend(logfile::clear_tui(&config::tui_log_paths()));
    }

    if g.json {
        out.json(&results);
    } else {
        for item in &results {
            if item.action == ClearAction::Missing {
                continue;
            }
            let action = match item.action {
                ClearAction::Truncated => "truncated",
                ClearAction::Removed => "removed",
                ClearAction::Missing => "missing",
                ClearAction::Failed => "failed",
            };
            let detail = match item.action {
                ClearAction::Truncated | ClearAction::Removed => {
                    format!(" ({})", format_bytes(item.bytes as i64))
                }
                ClearAction::Failed => {
                    format!(": {}", item.error.as_deref().unwrap_or("unknown error"))
                }
                ClearAction::Missing => String::new(),
            };
            out.line(format!("{action} {}{detail}", item.path.display()));
        }
    }

    let failed = results
        .iter()
        .filter(|item| item.action == ClearAction::Failed)
        .count();
    if failed > 0 {
        return Err(CliError::Core(format!("failed to clear {failed} file(s)")));
    }
    Ok(())
}

fn tail_budget(lines: usize) -> u64 {
    (lines.max(1) as u64).saturating_mul(512).max(64 * 1024)
}

fn emit_follow(out: &Out, g: &GlobalArgs, lines: &[Parsed]) {
    use std::io::Write;
    if g.json {
        for line in lines {
            out.json(&line_json(line));
        }
    } else {
        for line in lines {
            println!("{}", line.raw);
        }
    }
    let _ = std::io::stdout().flush();
}

fn line_json(line: &Parsed) -> serde_json::Value {
    json!({
        "time": line.time,
        "timestamp": line.timestamp,
        // Keep the original tag for JSON consumers: `FATA` stays `fata`,
        // while filtering still treats it as an error.
        "level": line.tag.as_deref().map(|tag| tag.to_ascii_lowercase()),
        "message": line.message,
    })
}

/// Parse `--since`. Accepts an absolute local time or a relative duration
/// (`30m`, `2h`, `3d`); `today`/`yesterday` mean local midnight.
fn parse_since(value: &str, now: NaiveDateTime) -> Result<NaiveDateTime, String> {
    let s = value.trim();
    if s.eq_ignore_ascii_case("today") {
        return Ok(start_of_day(now));
    }
    if s.eq_ignore_ascii_case("yesterday") {
        return Ok(start_of_day(now - ChronoDuration::days(1)));
    }
    if let Some((amount, unit)) = relative(s) {
        let delta = match unit {
            'm' => ChronoDuration::minutes(amount),
            'h' => ChronoDuration::hours(amount),
            _ => ChronoDuration::days(amount),
        };
        return Ok(now - delta);
    }
    for format in [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
    ] {
        if let Ok(at) = NaiveDateTime::parse_from_str(s, format) {
            return Ok(at);
        }
    }
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(date.and_hms_opt(0, 0, 0).expect("midnight"));
    }
    Err(format!(
        "invalid --since value {value:?}; expected YYYY-MM-DD[ HH:MM[:SS]], 30m/2h/3d, today or yesterday"
    ))
}

fn start_of_day(now: NaiveDateTime) -> NaiveDateTime {
    now.date().and_hms_opt(0, 0, 0).expect("midnight")
}

fn relative(s: &str) -> Option<(i64, char)> {
    let (number, unit) = s.split_at(s.len().checked_sub(1)?);
    let unit = unit.chars().next()?;
    if !matches!(unit, 'm' | 'h' | 'd') {
        return None;
    }
    let amount: i64 = number.parse().ok()?;
    (amount > 0).then_some((amount, unit))
}

fn parse_line(line: &str) -> Parsed {
    let parsed = logfmt::parse(line);
    Parsed {
        timestamp: parsed.timestamp.map(str::to_string),
        at: parsed
            .timestamp
            .and_then(|ts| NaiveDateTime::parse_from_str(ts, "%Y/%m/%d %H:%M:%S").ok()),
        time: parsed.time.to_string(),
        level: parsed.level,
        tag: parsed.tag.map(str::to_string),
        message: parsed.message.to_string(),
        raw: line.to_string(),
    }
}

/// Byte-oriented follower. `pending` must stay bytes: a read can split a
/// multi-byte character, and decoding per chunk would drop log lines.
struct Tail {
    path: PathBuf,
    file: File,
    offset: u64,
    inode: u64,
    pending: Vec<u8>,
}

impl Tail {
    fn open_at(path: &Path, offset: u64) -> Result<Self, CliError> {
        let mut file = File::open(path)?;
        let meta = file.metadata()?;
        let len = meta.len();
        let offset = offset.min(len);
        file.seek(SeekFrom::Start(offset))?;
        Ok(Self {
            path: path.to_path_buf(),
            file,
            offset,
            inode: inode(&meta),
            pending: Vec::new(),
        })
    }

    fn poll(&mut self) -> Vec<Parsed> {
        let rotated = std::fs::metadata(&self.path)
            .map(|m| inode(&m) != self.inode || m.len() < self.offset)
            .unwrap_or(true);
        if rotated {
            if let Ok(file) = File::open(&self.path) {
                self.inode = file.metadata().map(|m| inode(&m)).unwrap_or(self.inode);
                self.offset = 0;
                self.pending.clear();
                self.file = file;
            } else {
                return Vec::new();
            }
        }

        let mut buf = Vec::new();
        if self.file.read_to_end(&mut buf).is_err() {
            return Vec::new();
        }
        self.offset += buf.len() as u64;
        self.pending.extend_from_slice(&buf);

        let mut out = Vec::new();
        while let Some(pos) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=pos).collect();
            let text = String::from_utf8_lossy(&line);
            let text = text.trim_end_matches(['\n', '\r']);
            if !text.is_empty() {
                out.push(parse_line(text));
            }
        }
        out
    }
}

fn inode(meta: &Metadata) -> u64 {
    meta.ino()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "whoisthat-cli-logs-{}-{}",
            std::process::id(),
            name
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("log")
    }

    fn at(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    #[test]
    fn parses_core_and_tui_lines() {
        let core = parse_line("2026/10/29 12:00:00 [INFO] connected");
        assert_eq!(core.time, "12:00:00");
        assert_eq!(core.timestamp.as_deref(), Some("2026/10/29 12:00:00"));
        assert_eq!(core.level, Some(Level::Info));
        assert_eq!(core.message, "connected");
        assert_eq!(core.at, Some(at("2026-10-29 12:00:00")));

        let tui = parse_line("12:00:00 WARN  something happened");
        assert_eq!(tui.level, Some(Level::Warn));
        assert_eq!(tui.message, "something happened");
        assert_eq!(tui.at, None);
    }

    #[test]
    fn json_keeps_the_original_fatal_tag() {
        let line = parse_line("2026/10/31 10:00:03 [FATA] fatal");
        let value = line_json(&line);
        assert_eq!(value["level"], "fata");
    }

    #[test]
    fn filters_by_min_level() {
        let lines = vec![
            parse_line("2026/10/29 12:00:00 [ERRO] bad"),
            parse_line("2026/10/29 12:00:01 [INFO] ok"),
        ];
        let only_errors = Filter::new(Some(Level::Error), None, None)
            .unwrap()
            .apply(lines);
        assert_eq!(only_errors.len(), 1);
        assert_eq!(only_errors[0].message, "bad");
    }

    #[test]
    fn filters_by_since_and_grep() {
        let lines = vec![
            parse_line("2026/10/29 11:59:59 [WARN] old Boom"),
            parse_line("2026/10/29 12:00:00 [WARN] new boom"),
            parse_line("12:00:01 WARN  undated boom"),
            parse_line("2026/10/29 12:00:02 [WARN] new quiet"),
        ];
        let filtered = Filter::new(None, Some("2026-10-29 12:00:00"), Some("BOOM"))
            .unwrap()
            .apply(lines);
        assert_eq!(filtered.len(), 2);
        assert!(filtered[0].message.contains("new boom"));
        // Undated old-format lines are kept rather than silently dropped.
        assert!(filtered[1].message.contains("undated boom"));
    }

    #[test]
    fn parses_since_formats() {
        let now = at("2026-11-01 15:30:00");
        assert_eq!(
            parse_since("2026-10-31", now).unwrap(),
            at("2026-10-31 00:00:00")
        );
        assert_eq!(
            parse_since("2026-10-31 13:37", now).unwrap(),
            at("2026-10-31 13:37:00")
        );
        assert_eq!(
            parse_since("2026-10-31T13:37:05", now).unwrap(),
            at("2026-10-31 13:37:05")
        );
        assert_eq!(
            parse_since("today", now).unwrap(),
            at("2026-11-01 00:00:00")
        );
        assert_eq!(
            parse_since("yesterday", now).unwrap(),
            at("2026-10-31 00:00:00")
        );
        assert_eq!(parse_since("90m", now).unwrap(), at("2026-11-01 14:00:00"));
        assert_eq!(parse_since("2h", now).unwrap(), at("2026-11-01 13:30:00"));
        assert!(parse_since("nonsense", now).is_err());
        assert!(parse_since("0m", now).is_err());
    }

    #[test]
    fn tui_and_source_conflict() {
        assert!(resolve_source(Some(LogSourceArg::Core), true).is_err());
        assert_eq!(resolve_source(None, true).unwrap(), LogSourceArg::Tui);
        assert_eq!(resolve_source(None, false).unwrap(), LogSourceArg::Core);
    }

    #[test]
    fn follow_survives_invalid_utf8_and_advances_the_offset() {
        let path = tmp_path("utf8");
        std::fs::write(&path, b"ok\n\xff bad\n").unwrap();
        let mut tail = Tail::open_at(&path, 0).unwrap();

        let lines = tail.poll();
        assert_eq!(lines.len(), 2);
        assert!(lines[1].message.contains("bad"));
        assert_eq!(tail.offset, std::fs::metadata(&path).unwrap().len());

        // No re-reading of the same invalid bytes: the stream keeps moving.
        assert!(tail.poll().is_empty());
        assert_eq!(tail.offset, std::fs::metadata(&path).unwrap().len());
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn follow_handles_a_multibyte_character_split_across_reads() {
        let path = tmp_path("multibyte");
        std::fs::write(&path, b"").unwrap();
        let mut tail = Tail::open_at(&path, 0).unwrap();

        let text = "12:00:00 [INFO] привет\n".as_bytes();
        let split = text.len() - 4;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(&text[..split]).unwrap();
        file.flush().unwrap();
        assert!(tail.poll().is_empty());

        file.write_all(&text[split..]).unwrap();
        drop(file);
        let lines = tail.poll();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].message.ends_with("привет"));
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn follow_applies_the_filter_to_new_lines() {
        let path = tmp_path("follow-filter");
        std::fs::write(&path, b"").unwrap();
        let mut tail = Tail::open_at(&path, 0).unwrap();
        let filter = Filter::new(None, None, Some("needle")).unwrap();

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(file, "2026/10/31 10:00:00 [INFO] hay").unwrap();
        writeln!(file, "2026/10/31 10:00:01 [INFO] a needle here").unwrap();
        drop(file);

        let lines = filter.apply(tail.poll());
        assert_eq!(lines.len(), 1);
        assert!(lines[0].message.contains("needle"));
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }
}
