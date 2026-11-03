use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::config;
use crate::doctor::types::{CheckCategory, CheckItem};

use super::util::{age, human_bytes, truncate_line};

pub(super) fn check_logs() -> CheckCategory {
    let mut items = Vec::new();

    for (label, paths) in [
        ("core", config::core_log_paths()),
        ("tui", config::tui_log_paths()),
    ] {
        let path = config::first_existing(&paths);
        let Ok(meta) = fs::metadata(&path) else {
            items.push(CheckItem::info(
                format!("{label} log"),
                format!("{} (not created yet)", path.display()),
            ));
            continue;
        };

        let mode = meta.permissions().mode() & 0o777;
        let message = format!(
            "{} ({}, mode {:04o}, modified {})",
            path.display(),
            human_bytes(meta.len()),
            mode,
            age(&meta)
        );
        if mode & 0o077 != 0 {
            items.push(CheckItem::warn(
                format!("{label} log"),
                message,
                Some(format!("chmod 600 {}", path.display())),
            ));
        } else {
            items.push(CheckItem::pass(format!("{label} log"), message));
        }

        items.push(rotation_item(label, &path));
        items.push(recent_errors_item(label, &path));
    }

    let crash = config::data_dir().join("core-crash.log");
    match fs::metadata(&crash) {
        Ok(meta) if meta.len() > 0 => items.push(CheckItem::warn(
            "core crash log",
            format!(
                "{} ({}, mode {:04o}) — a previous crash was recorded",
                crash.display(),
                human_bytes(meta.len()),
                meta.permissions().mode() & 0o777
            ),
            Some("inspect it, then clear with: whoisthat logs clear --source core --yes".into()),
        )),
        Ok(_) => items.push(CheckItem::pass(
            "core crash log",
            format!("{} (empty)", crash.display()),
        )),
        Err(_) => items.push(CheckItem::info(
            "core crash log",
            format!("{} (not created)", crash.display()),
        )),
    }
    let crash_backup = crate::logfile::backup_path(&crash);
    if let Ok(meta) = fs::metadata(&crash_backup) {
        items.push(CheckItem::info(
            "core crash backup",
            format!("{} ({})", crash_backup.display(), human_bytes(meta.len())),
        ));
    }

    CheckCategory::new("Logs", items)
}

fn rotation_item(label: &str, path: &Path) -> CheckItem {
    if label == "core" {
        let backups = crate::logfile::rotated_backups(path);
        if backups.is_empty() {
            return CheckItem::pass("core log backups", "none");
        }
        let bytes: u64 = backups
            .iter()
            .filter_map(|path| fs::metadata(path).ok())
            .map(|meta| meta.len())
            .sum();
        return CheckItem::info(
            "core log backups",
            format!("{} file(s), {}", backups.len(), human_bytes(bytes)),
        );
    }

    let backup = crate::logfile::backup_path(path);
    match fs::metadata(&backup) {
        Ok(meta) => CheckItem::info(
            "tui log backup",
            format!(
                "{} ({}, mode {:04o})",
                backup.display(),
                human_bytes(meta.len()),
                meta.permissions().mode() & 0o777
            ),
        ),
        Err(_) => CheckItem::pass("tui log backup", "none"),
    }
}

fn recent_errors_item(label: &str, path: &Path) -> CheckItem {
    let errors = recent_errors(path);
    let name = format!("{label} recent errors");
    if errors.is_empty() {
        CheckItem::pass(name, "no error/fatal lines in the last 256 KiB")
    } else {
        CheckItem::warn(
            name,
            errors.join(" | "),
            Some(format!("whoisthat logs --source {label} --level error")),
        )
    }
}

pub(super) fn recent_errors(path: &Path) -> Vec<String> {
    let Ok((lines, _)) = crate::logfile::read_tail(path, 256 * 1024) else {
        return Vec::new();
    };
    let mut errors: Vec<String> = lines
        .iter()
        .filter(|line| crate::logfmt::parse(line).level == Some(crate::logfmt::Level::Error))
        .map(|line| truncate_line(line, 160))
        .collect();
    let start = errors.len().saturating_sub(5);
    errors.drain(..start);
    errors
}
