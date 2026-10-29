use std::fs::{File, Metadata};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::json;

use crate::config;

use super::super::args::{GlobalArgs, LogsArgs};
use super::super::error::CliError;
use super::super::output::Out;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Parsed {
    time: String,
    level: Option<String>,
    message: String,
    raw: String,
}

pub async fn run(g: &GlobalArgs, args: LogsArgs) -> Result<(), CliError> {
    let out = Out::new(g);
    let path = if args.tui {
        config::data_dir().join("tui.log")
    } else {
        config::config_dir().join("core.log")
    };

    let Some(mut tail) = Tail::open(&path)? else {
        out.note(format!("log file not found: {}", path.display()));
        return Ok(());
    };

    let mut lines = tail.lines(args.lines);
    lines = filter(lines, args.level.as_deref());

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

    emit_follow(&out, g, &lines);
    loop {
        tokio::time::sleep(Duration::from_millis(250)).await;
        let new_lines = tail.poll();
        let new_lines = filter(new_lines, args.level.as_deref());
        emit_follow(&out, g, &new_lines);
    }
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
        "level": line.level,
        "message": line.message,
    })
}

fn filter(lines: Vec<Parsed>, level: Option<&str>) -> Vec<Parsed> {
    let Some(min) = level.and_then(level_rank) else {
        return lines;
    };
    lines
        .into_iter()
        .filter(|l| {
            l.level
                .as_deref()
                .and_then(level_rank)
                .is_some_and(|r| r <= min)
        })
        .collect()
}

fn level_rank(level: &str) -> Option<u8> {
    match level {
        "error" | "erro" | "fata" => Some(0),
        "warn" => Some(1),
        "info" => Some(2),
        "debug" | "debu" => Some(3),
        "trace" | "trac" => Some(4),
        _ => None,
    }
}

fn parse_line(line: &str) -> Parsed {
    let raw = line.to_string();
    // Core: "2026/10/29 12:00:00 [INFO] message"
    if line.len() >= 20 && line.as_bytes().get(4) == Some(&b'/') {
        let time = line.get(11..19).unwrap_or("").to_string();
        let rest = line.get(20..).unwrap_or("").trim_start();
        let (level, message) = split_level(rest);
        return Parsed {
            time,
            level,
            message,
            raw,
        };
    }
    // TUI: "12:00:00 INFO  message"
    if line.len() >= 9 && line.as_bytes().get(2) == Some(&b':') {
        let time = line.get(0..8).unwrap_or("").to_string();
        let rest = line.get(9..).unwrap_or("").trim_start();
        let (level, message) = split_level(rest);
        return Parsed {
            time,
            level,
            message,
            raw,
        };
    }
    Parsed {
        time: String::new(),
        level: None,
        message: raw.clone(),
        raw,
    }
}

fn split_level(rest: &str) -> (Option<String>, String) {
    let rest = rest.trim_start();
    if let Some(stripped) = rest.strip_prefix('[') {
        if let Some((tag, tail)) = stripped.split_once(']') {
            return (
                Some(tag.to_ascii_lowercase()),
                tail.trim_start().to_string(),
            );
        }
    }
    let mut parts = rest.splitn(2, char::is_whitespace);
    let first = parts.next().unwrap_or("");
    let tail = parts.next().unwrap_or("").trim_start();
    if level_rank(&first.to_ascii_lowercase()).is_some() {
        (Some(first.to_ascii_lowercase()), tail.to_string())
    } else {
        (None, rest.to_string())
    }
}

struct Tail {
    path: PathBuf,
    file: File,
    offset: u64,
    inode: u64,
    pending: String,
}

impl Tail {
    fn open(path: &std::path::Path) -> Result<Option<Self>, CliError> {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let meta = file.metadata()?;
        let offset = meta.len();
        Ok(Some(Self {
            path: path.to_path_buf(),
            file,
            offset,
            inode: inode(&meta),
            pending: String::new(),
        }))
    }

    fn lines(&mut self, count: usize) -> Vec<Parsed> {
        let mut buf = String::new();
        let _ = self.file.seek(SeekFrom::Start(0));
        let _ = self.file.read_to_string(&mut buf);
        let mut lines: Vec<Parsed> = buf.lines().map(parse_line).collect();
        if lines.len() > count {
            lines.drain(..lines.len() - count);
        }
        lines
    }

    fn poll(&mut self) -> Vec<Parsed> {
        let rotated = std::fs::metadata(&self.path)
            .map(|m| inode(&m) != self.inode || m.len() < self.offset)
            .unwrap_or(true);
        if rotated {
            if let Ok(file) = File::open(&self.path) {
                self.file = file;
                self.offset = 0;
                self.inode = self
                    .file
                    .metadata()
                    .map(|m| inode(&m))
                    .unwrap_or(self.inode);
                self.pending.clear();
            } else {
                return Vec::new();
            }
        }

        let mut buf = String::new();
        if self.file.seek(SeekFrom::Start(self.offset)).is_err() {
            return Vec::new();
        }
        if self.file.read_to_string(&mut buf).is_err() {
            return Vec::new();
        }
        self.offset += buf.len() as u64;

        self.pending.push_str(&buf);
        let mut out = Vec::new();
        while let Some(pos) = self.pending.find('\n') {
            let line: String = self.pending.drain(..=pos).collect();
            let line = line.trim_end_matches(['\n', '\r']);
            if !line.is_empty() {
                out.push(parse_line(line));
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

    #[test]
    fn parses_core_and_tui_lines() {
        let core = parse_line("2026/10/29 12:00:00 [INFO] connected");
        assert_eq!(core.time, "12:00:00");
        assert_eq!(core.level.as_deref(), Some("info"));
        assert_eq!(core.message, "connected");

        let tui = parse_line("12:00:00 WARN  something happened");
        assert_eq!(tui.level.as_deref(), Some("warn"));
        assert_eq!(tui.message, "something happened");
    }

    #[test]
    fn filters_by_min_level() {
        let lines = vec![
            parse_line("2026/10/29 12:00:00 [ERRO] bad"),
            parse_line("2026/10/29 12:00:01 [INFO] ok"),
        ];
        let only_errors = filter(lines, Some("error"));
        assert_eq!(only_errors.len(), 1);
        assert_eq!(only_errors[0].message, "bad");
    }
}
