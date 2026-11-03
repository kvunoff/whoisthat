use std::fs;

pub(super) fn permission_denied(stderr: &[u8]) -> bool {
    let text = String::from_utf8_lossy(stderr).to_lowercase();
    text.contains("operation not permitted")
        || text.contains("permission denied")
        || text.contains("must be root")
}

pub(super) fn truncate_line(line: &str, max: usize) -> String {
    if line.chars().count() <= max {
        return line.to_string();
    }
    let mut out: String = line.chars().take(max.saturating_sub(3)).collect();
    out.push_str("...");
    out
}

pub(super) fn human_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    let bytes = bytes as f64;
    if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
}

pub(super) fn age(meta: &fs::Metadata) -> String {
    meta.modified()
        .ok()
        .and_then(|time| std::time::SystemTime::now().duration_since(time).ok())
        .map(|elapsed| {
            let secs = elapsed.as_secs();
            if secs >= 86_400 {
                format!("{}d ago", secs / 86_400)
            } else if secs >= 3_600 {
                format!("{}h ago", secs / 3_600)
            } else {
                format!("{}m ago", secs / 60)
            }
        })
        .unwrap_or_else(|| "unknown".into())
}
