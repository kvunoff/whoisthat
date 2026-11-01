use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Read at most `max_bytes` from the end of `path` and return complete lines
/// plus the file offset that was read up to. The first line is dropped when
/// the read started mid-file.
pub fn read_tail(path: &Path, max_bytes: u64) -> io::Result<(Vec<String>, u64)> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start))?;

    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    // Use the position after the read, not the pre-read length: bytes appended
    // while we were reading are already in `buf`.
    let end = file.stream_position()?;

    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    if start > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    Ok((lines, end))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ClearAction {
    Truncated,
    Removed,
    Missing,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct Cleared {
    pub path: PathBuf,
    pub action: ClearAction,
    pub bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Truncate in place. The inode is kept, so a writer that already holds the
/// file open (lumberjack, the TUI logger) keeps appending from offset 0.
pub fn truncate(path: &Path) -> Cleared {
    let file = match std::fs::OpenOptions::new().write(true).open(path) {
        Ok(file) => file,
        Err(e) => return failed_or_missing(path, e),
    };
    let bytes = file.metadata().map(|m| m.len()).unwrap_or(0);
    match file.set_len(0) {
        Ok(()) => cleared(path, ClearAction::Truncated, bytes),
        Err(e) => cleared_error(path, e),
    }
}

pub fn remove_if_exists(path: &Path) -> Cleared {
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(e) => return failed_or_missing(path, e),
    };
    match std::fs::remove_file(path) {
        Ok(()) => cleared(path, ClearAction::Removed, meta.len()),
        Err(e) => cleared_error(path, e),
    }
}

/// `tui.log` + `tui.log.1` in each candidate directory.
pub fn clear_tui(paths: &[PathBuf]) -> Vec<Cleared> {
    let mut out = Vec::new();
    for path in paths {
        out.push(truncate(path));
        out.push(remove_if_exists(&backup_path(path)));
    }
    out
}

/// `core.log` in each candidate directory plus lumberjack's timestamped
/// backups next to it.
pub fn clear_core(paths: &[PathBuf]) -> Vec<Cleared> {
    let mut out = Vec::new();
    for path in paths {
        out.push(truncate(path));
        out.push(remove_if_exists(&backup_path(path)));
        for backup in rotated_backups(path) {
            out.push(remove_if_exists(&backup));
        }
    }
    out
}

/// Lumberjack's timestamped backups for `path` (e.g. `core-2026-….log`).
pub fn rotated_backups(path: &Path) -> Vec<PathBuf> {
    let Some(dir) = path.parent() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| is_lumberjack_backup(&entry.file_name().to_string_lossy()))
        .map(|entry| entry.path())
        .collect();
    out.sort();
    out
}

/// `core-crash.log` and its single `.1` backup.
pub fn clear_crash(data_dir: &Path) -> Vec<Cleared> {
    let path = data_dir.join("core-crash.log");
    vec![truncate(&path), remove_if_exists(&backup_path(&path))]
}

fn cleared(path: &Path, action: ClearAction, bytes: u64) -> Cleared {
    Cleared {
        path: path.to_path_buf(),
        action,
        bytes,
        error: None,
    }
}

fn cleared_error(path: &Path, error: io::Error) -> Cleared {
    Cleared {
        path: path.to_path_buf(),
        action: ClearAction::Failed,
        bytes: 0,
        error: Some(error.to_string()),
    }
}

fn failed_or_missing(path: &Path, error: io::Error) -> Cleared {
    if error.kind() == io::ErrorKind::NotFound {
        cleared(path, ClearAction::Missing, 0)
    } else {
        cleared_error(path, error)
    }
}

/// `<path>.1`: the TUI and crash-log backup name.
pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".1");
    PathBuf::from(name)
}

/// Lumberjack names backups `core-YYYY-MM-DDTHH-MM-SS.mmm.log`. The exact
/// length and separators keep unrelated `core-*.log` files out of the way.
fn is_lumberjack_backup(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() != 32 || !name.starts_with("core-") || !name.ends_with(".log") {
        return false;
    }
    (5..28).all(|i| match i {
        9 | 12 | 18 | 21 => bytes[i] == b'-',
        15 => bytes[i] == b'T',
        24 => bytes[i] == b'.',
        _ => bytes[i].is_ascii_digit(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::MetadataExt;

    fn tmp_path(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("whoisthat-logfile-{}-{}", std::process::id(), name));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("log")
    }

    #[test]
    fn reads_the_tail_and_skips_the_partial_first_line() {
        let path = tmp_path("tail");
        let mut file = File::create(&path).unwrap();
        for i in 0..1000 {
            writeln!(file, "line {i} padding padding padding").unwrap();
        }
        drop(file);

        let (lines, offset) = read_tail(&path, 1024).unwrap();
        assert_eq!(offset, std::fs::metadata(&path).unwrap().len());
        assert!(lines.len() < 100);
        assert!(lines.last().unwrap().starts_with("line 999 "));
        assert!(lines[0].starts_with("line "));
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn short_file_is_read_whole() {
        let path = tmp_path("short");
        std::fs::write(&path, "one\ntwo\n").unwrap();
        let (lines, _) = read_tail(&path, 64 * 1024).unwrap();
        assert_eq!(lines, vec!["one", "two"]);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn invalid_utf8_is_lossy_not_an_error() {
        let path = tmp_path("utf8");
        std::fs::write(&path, b"ok\n\xff\xfe bad\n").unwrap();
        let (lines, _) = read_tail(&path, 64 * 1024).unwrap();
        assert_eq!(lines.len(), 2);
        assert!(lines[1].contains("bad"));
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn truncate_keeps_the_inode() {
        let path = tmp_path("clear");
        std::fs::write(&path, "some lines\n").unwrap();
        let inode_before = std::fs::metadata(&path).unwrap().ino();

        let cleared = truncate(&path);
        assert_eq!(cleared.action, ClearAction::Truncated);
        assert!(cleared.bytes > 0);
        let meta = std::fs::metadata(&path).unwrap();
        assert_eq!(meta.len(), 0);
        assert_eq!(meta.ino(), inode_before);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn clearing_a_missing_file_does_not_create_it() {
        let path = tmp_path("missing-clear");
        let cleared = truncate(&path);
        assert_eq!(cleared.action, ClearAction::Missing);
        assert!(!path.exists());
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn clear_tui_truncates_the_log_and_removes_the_backup() {
        let path = tmp_path("tui-clear");
        std::fs::write(&path, "current\n").unwrap();
        std::fs::write(backup_path(&path), "old\n").unwrap();

        let results = clear_tui(std::slice::from_ref(&path));
        assert!(results.iter().any(|r| r.action == ClearAction::Truncated));
        assert!(results.iter().any(|r| r.action == ClearAction::Removed));
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
        assert!(!backup_path(&path).exists());
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn clear_core_removes_lumberjack_backups_only() {
        let path = tmp_path("core-clear");
        let dir = path.parent().unwrap();
        std::fs::write(&path, "current\n").unwrap();
        let backup = dir.join("core-2026-10-31T13-37-11.000.log");
        let unrelated = dir.join("core-notes.log");
        std::fs::write(&backup, "old\n").unwrap();
        std::fs::write(&unrelated, "keep\n").unwrap();

        clear_core(std::slice::from_ref(&path));
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
        assert!(!backup.exists());
        assert!(unrelated.exists());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn backup_name_matching_is_strict() {
        assert!(is_lumberjack_backup("core-2026-10-31T13-37-11.000.log"));
        assert!(!is_lumberjack_backup("core-notes.log"));
        assert!(!is_lumberjack_backup("mycore-2026-10-31T13-37-11.000.log"));
        assert!(!is_lumberjack_backup("core-2026-10-31T13-37-11.00.log"));
    }

    #[test]
    fn truncate_reports_a_failure_instead_of_panicking() {
        let path = tmp_path("dir-clear");
        std::fs::create_dir_all(&path).unwrap();
        let cleared = truncate(&path);
        assert_eq!(cleared.action, ClearAction::Failed);
        assert!(cleared.error.is_some());
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }
}
