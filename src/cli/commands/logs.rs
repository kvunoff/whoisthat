use std::fs::{File, Metadata};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::json;

use crate::config;
use crate::logfmt::{self, Level};

use super::super::args::{GlobalArgs, LogsArgs};
use super::super::error::CliError;
use super::super::output::Out;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Parsed {
    timestamp: Option<String>,
    time: String,
    level: Option<Level>,
    tag: Option<String>,
    message: String,
    raw: String,
}

pub async fn run(g: &GlobalArgs, args: LogsArgs) -> Result<(), CliError> {
    let out = Out::new(g);
    let paths = if args.tui {
        config::tui_log_paths()
    } else {
        config::core_log_paths()
    };
    let path = config::first_existing(&paths);

    if !path.exists() {
        out.note(format!("log file not found: {}", path.display()));
        return Ok(());
    }

    let (raw, offset) = crate::logfile::read_tail(&path, tail_budget(args.lines))?;
    let mut lines: Vec<Parsed> = raw.iter().map(|line| parse_line(line)).collect();
    if lines.len() > args.lines {
        lines.drain(..lines.len() - args.lines);
    }
    lines = filter(lines, args.level.map(|level| level.level()));

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
        let new_lines = tail.poll();
        let new_lines = filter(new_lines, args.level.map(|level| level.level()));
        emit_follow(&out, g, &new_lines);
    }
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

fn filter(lines: Vec<Parsed>, min: Option<Level>) -> Vec<Parsed> {
    let Some(min) = min else {
        return lines;
    };
    lines
        .into_iter()
        .filter(|line| line.level.is_some_and(|level| level <= min))
        .collect()
}

fn parse_line(line: &str) -> Parsed {
    let parsed = logfmt::parse(line);
    Parsed {
        timestamp: parsed.timestamp.map(str::to_string),
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

    #[test]
    fn parses_core_and_tui_lines() {
        let core = parse_line("2026/10/29 12:00:00 [INFO] connected");
        assert_eq!(core.time, "12:00:00");
        assert_eq!(core.timestamp.as_deref(), Some("2026/10/29 12:00:00"));
        assert_eq!(core.level, Some(Level::Info));
        assert_eq!(core.message, "connected");

        let tui = parse_line("12:00:00 WARN  something happened");
        assert_eq!(tui.level, Some(Level::Warn));
        assert_eq!(tui.message, "something happened");
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
        let only_errors = filter(lines, Some(Level::Error));
        assert_eq!(only_errors.len(), 1);
        assert_eq!(only_errors[0].message, "bad");
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
}
