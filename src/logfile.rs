use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

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
}
