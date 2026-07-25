use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use chrono::Local;
use log::{LevelFilter, Log, Metadata, Record};

use crate::config;

pub(crate) struct FileLogger {
    file: Mutex<File>,
    enabled: AtomicBool,
    level: Mutex<LevelFilter>,
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.enabled.load(Ordering::Relaxed)
            && metadata.level() <= *self.level.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let ts = Local::now().format("%H:%M:%S");
            let _ = writeln!(
                self.file.lock().unwrap_or_else(|e| e.into_inner()),
                "{} {:5} {}",
                ts,
                record.level(),
                record.args()
            );
        }
    }
    fn flush(&self) {
        let _ = self.file.lock().unwrap_or_else(|e| e.into_inner()).flush();
    }
}

pub(crate) fn init_logger() -> &'static FileLogger {
    let log_dir = config::data_dir();
    let _ = std::fs::create_dir_all(&log_dir);
    let log_path = log_dir.join("tui.log");

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .or_else(|_| {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/whoisthat-tui.log")
        })
        .unwrap_or_else(|_| {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open("/dev/null")
                .expect("failed to open /dev/null")
        });

    let logger: &'static FileLogger = Box::leak(Box::new(FileLogger {
        file: Mutex::new(file),
        enabled: AtomicBool::new(false),
        level: Mutex::new(LevelFilter::Warn),
    }));
    log::set_logger(logger).ok();
    log::set_max_level(LevelFilter::Trace);
    logger
}

pub(crate) fn configure_logger(logger: &FileLogger, enabled: bool, level: &str) {
    let lf = match level.to_lowercase().as_str() {
        "error" => LevelFilter::Error,
        "warn" => LevelFilter::Warn,
        "info" => LevelFilter::Info,
        "debug" => LevelFilter::Debug,
        "trace" => LevelFilter::Trace,
        _ => LevelFilter::Warn,
    };
    *logger.level.lock().unwrap_or_else(|e| e.into_inner()) = lf;
    logger.enabled.store(enabled, Ordering::Relaxed);
}
