use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use chrono::Local;
use log::{Level, LevelFilter, Log, Metadata, Record};

use crate::config;

const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;
const BACKUP_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

struct Inner {
    file: File,
    path: PathBuf,
    written: u64,
    degraded: bool,
}

pub(crate) struct FileLogger {
    inner: Mutex<Inner>,
    /// Effective `LevelFilter` rank: 0 = off, 1 = error … 5 = trace.
    level: AtomicU8,
    /// Actual path, kept outside the mutex for the panic fast path.
    path: PathBuf,
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        level_rank(metadata.level()) <= self.level.load(Ordering::Relaxed)
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            self.write(record.level(), *record.args());
        }
    }

    fn flush(&self) {
        let _ = self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .file
            .flush();
    }
}

impl FileLogger {
    /// Write regardless of the configured level. Used for panics: a crash
    /// record must survive even when file logging is disabled.
    pub(crate) fn log_always(&self, level: Level, args: std::fmt::Arguments<'_>) {
        // A panic may fire while this thread already holds the lock (or while
        // another thread is writing). Never deadlock in the panic hook: try
        // the normal path, otherwise append directly to the file.
        match self.inner.try_lock() {
            Ok(mut inner) => self.write_locked(&mut inner, level, args),
            Err(_) => emergency_write(&self.path, level, args),
        }
    }

    fn write(&self, level: Level, args: std::fmt::Arguments<'_>) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        self.write_locked(&mut inner, level, args);
    }

    fn write_locked(&self, inner: &mut Inner, level: Level, args: std::fmt::Arguments<'_>) {
        // A previous rotation renamed the file but could not create the new
        // one. Keep appending to the old fd (now tui.log.1) and retry reopen
        // on each write; no records are lost.
        if inner.degraded {
            if let Ok(file) = open_log(&inner.path) {
                let len = file.metadata().map(|m| m.len()).unwrap_or(0);
                inner.file = file;
                inner.written = len;
                inner.degraded = false;
            }
        }

        let ts = Local::now().format("%Y/%m/%d %H:%M:%S");
        let tag = crate::logfmt::Level::from_log_level(level).tag();
        let line = format!("{} [{}] {}\n", ts, tag, args);
        if inner.file.write_all(line.as_bytes()).is_err() {
            return;
        }
        inner.written += line.len() as u64;
        if inner.written >= MAX_LOG_BYTES {
            rotate(inner);
        }
    }
}

/// Last-resort write used by the panic hook when the logger mutex is already
/// held (a panic inside `write_locked`). No rotation, no locks.
fn emergency_write(path: &Path, level: Level, args: std::fmt::Arguments<'_>) {
    let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
    else {
        return;
    };
    let ts = Local::now().format("%Y/%m/%d %H:%M:%S");
    let tag = crate::logfmt::Level::from_log_level(level).tag();
    let _ = writeln!(file, "{} [{}] {}", ts, tag, args);
}

fn level_rank(level: Level) -> u8 {
    match level {
        Level::Error => 1,
        Level::Warn => 2,
        Level::Info => 3,
        Level::Debug => 4,
        Level::Trace => 5,
    }
}

fn filter_rank(filter: LevelFilter) -> u8 {
    match filter {
        LevelFilter::Off => 0,
        LevelFilter::Error => 1,
        LevelFilter::Warn => 2,
        LevelFilter::Info => 3,
        LevelFilter::Debug => 4,
        LevelFilter::Trace => 5,
    }
}

fn filter_from_rank(rank: u8) -> LevelFilter {
    match rank {
        0 => LevelFilter::Off,
        1 => LevelFilter::Error,
        2 => LevelFilter::Warn,
        3 => LevelFilter::Info,
        4 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    }
}

fn parse_level(level: &str) -> Option<LevelFilter> {
    match level.trim().to_ascii_lowercase().as_str() {
        "error" => Some(LevelFilter::Error),
        "warn" => Some(LevelFilter::Warn),
        "info" => Some(LevelFilter::Info),
        "debug" => Some(LevelFilter::Debug),
        "trace" => Some(LevelFilter::Trace),
        _ => None,
    }
}

fn open_log(path: &Path) -> std::io::Result<File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)?;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    Ok(file)
}

fn rotate(inner: &mut Inner) {
    // Waiting for a degraded reopen: renaming again would just move whatever
    // file happens to appear at the path.
    if inner.degraded {
        return;
    }

    let mut backup = inner.path.clone().into_os_string();
    backup.push(".1");
    let backup = PathBuf::from(backup);

    if fs::rename(&inner.path, &backup).is_err() {
        // Keep writing to the current file; retry on the next write.
        return;
    }
    let _ = fs::set_permissions(&backup, fs::Permissions::from_mode(0o600));

    match open_log(&inner.path) {
        Ok(file) => {
            inner.file = file;
            inner.written = 0;
            inner.degraded = false;
        }
        Err(_) => {
            // The old fd now points at tui.log.1; keep it and retry open.
            inner.degraded = true;
        }
    }
}

pub(crate) fn init_logger() -> &'static FileLogger {
    let data_dir = config::data_dir();
    let _ = fs::create_dir_all(&data_dir);

    let primary = data_dir.join("tui.log");
    let fallback = config::fallback_log_path("tui.log");
    let (file, path) = match open_log(&primary) {
        Ok(file) => (file, primary),
        Err(_) => match open_log(&fallback) {
            Ok(file) => {
                // Only the per-user temp directory is hardened to 0700. The
                // XDG data dir is left as-is: xray may run as a dedicated uid
                // and still needs to traverse it to reach its runtime.
                if let Some(dir) = fallback.parent() {
                    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
                }
                (file, fallback)
            }
            Err(_) => (
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("/dev/null")
                    .expect("failed to open /dev/null"),
                primary,
            ),
        },
    };

    cleanup(&path);

    let written = file.metadata().map(|m| m.len()).unwrap_or(0);
    let logger: &'static FileLogger = Box::leak(Box::new(FileLogger {
        inner: Mutex::new(Inner {
            file,
            path: path.clone(),
            written,
            degraded: false,
        }),
        level: AtomicU8::new(filter_rank(LevelFilter::Warn)),
        path,
    }));
    log::set_logger(logger).ok();
    log::set_max_level(LevelFilter::Trace);
    logger
}

/// Tighten permissions and drop a stale backup. Runs at startup only.
fn cleanup(path: &Path) {
    let mut backup = path.to_path_buf().into_os_string();
    backup.push(".1");
    let backup = PathBuf::from(backup);
    if let Ok(meta) = fs::metadata(&backup) {
        let _ = fs::set_permissions(&backup, fs::Permissions::from_mode(0o600));
        let old = meta
            .modified()
            .ok()
            .and_then(|m| SystemTime::now().duration_since(m).ok())
            .map(|age| age >= BACKUP_MAX_AGE)
            .unwrap_or(false);
        if old {
            let _ = fs::remove_file(&backup);
        }
    }

    // Pre-v2 fallback lived in a predictable shared /tmp path.
    let legacy = std::env::temp_dir().join("whoisthat-tui.log");
    if let Ok(meta) = fs::metadata(&legacy) {
        if meta.uid() == config::uid() {
            let _ = fs::remove_file(&legacy);
        }
    }
}

pub(crate) fn configure_logger(logger: &FileLogger, enabled: bool, level: &str) {
    let configured = parse_level(level).unwrap_or_else(|| {
        log::warn!("unknown log level {level:?}, using warn");
        LevelFilter::Warn
    });

    // D1: "off" caps verbosity at WARN; warnings and errors still land on
    // disk. "on" uses the configured level as-is.
    let mut rank = filter_rank(configured);
    if !enabled {
        rank = rank.min(filter_rank(LevelFilter::Warn));
    }
    let effective = filter_from_rank(rank);
    logger.level.store(rank, Ordering::Relaxed);
    log::set_max_level(effective);
}

pub(crate) fn install_panic_hook(logger: &'static FileLogger) {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // `info` is multi-line; keep one tagged record per panic.
        let message = info.to_string().replace('\n', " | ");
        logger.log_always(Level::Error, format_args!("panic: {message}"));

        use std::io::IsTerminal;
        if std::io::stderr().is_terminal() {
            let _ = crossterm::terminal::disable_raw_mode();
            let _ = crossterm::execute!(
                std::io::stderr(),
                crossterm::terminal::LeaveAlternateScreen,
                crossterm::event::DisableMouseCapture,
                crossterm::cursor::Show
            );
        }

        default(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_logger(name: &str) -> (&'static FileLogger, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("whoisthat-logger-{}-{}", std::process::id(), name));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tui.log");
        let logger: &'static FileLogger = Box::leak(Box::new(FileLogger {
            inner: Mutex::new(Inner {
                file: open_log(&path).unwrap(),
                path: path.clone(),
                written: 0,
                degraded: false,
            }),
            level: AtomicU8::new(filter_rank(LevelFilter::Trace)),
            path: path.clone(),
        }));
        (logger, path)
    }

    #[test]
    fn writes_the_shared_format_and_tightens_permissions() {
        let (logger, path) = tmp_logger("format");
        logger.log_always(Level::Warn, format_args!("hello"));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(" [WARN] hello"));
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn panic_hook_writes_one_tagged_line() {
        let (logger, path) = tmp_logger("panic");
        let previous = std::panic::take_hook();
        install_panic_hook(logger);

        let result = std::panic::catch_unwind(|| panic!("boom"));
        std::panic::set_hook(previous);

        assert!(result.is_err());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[ERRO] panic:"), "missing panic line: {text}");
        assert!(text.contains("boom"), "missing payload: {text}");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn emergency_write_appends_without_the_mutex() {
        let (_, path) = tmp_logger("emergency");
        emergency_write(&path, Level::Error, format_args!("panic path"));
        emergency_write(&path, Level::Error, format_args!("second"));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[ERRO] panic path"));
        assert!(text.contains("[ERRO] second"));
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn rotation_keeps_old_lines_and_starts_a_new_file() {
        let (logger, path) = tmp_logger("rotate");
        // Pretend the file already reached the cap.
        {
            let mut inner = logger.inner.lock().unwrap();
            inner.written = MAX_LOG_BYTES;
        }
        logger.log_always(Level::Info, format_args!("after cap"));

        let mut backup = path.clone().into_os_string();
        backup.push(".1");
        let backup = PathBuf::from(backup);

        assert!(backup.exists(), "old log must be renamed to .1");
        assert!(path.exists(), "a fresh log must be created");
        // The record that crossed the cap stays in the backup; the next one
        // goes to the fresh file. Nothing is lost.
        assert!(std::fs::read_to_string(&backup)
            .unwrap()
            .contains("after cap"));
        logger.log_always(Level::Info, format_args!("new file"));
        assert!(std::fs::read_to_string(&path).unwrap().contains("new file"));
        assert_eq!(
            std::fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
            0o600
        );
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }
}
