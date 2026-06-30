use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};

use crate::ui::theme::*;

pub fn section_header(title: &str) -> Line<'static> {
    Line::from(Span::styled(format!("─── {} ───", title), s_accent()))
}

pub fn kv_row(key: &str, val: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{:>10}  ", key), s_faint()),
        Span::styled(val.into(), s_text()),
    ])
}

pub fn kv_row_span<'a>(key: &str, val: Span<'a>) -> Line<'a> {
    Line::from(vec![Span::styled(format!("{:>10}  ", key), s_faint()), val])
}

pub fn format_bytes(bytes: i64) -> String {
    if bytes == 0 {
        return "0".to_string();
    }
    let b = bytes as f64;
    if b < 1024.0 {
        format!("{} B", bytes)
    } else if b < 1024.0 * 1024.0 {
        format!("{:.1} KB", b / 1024.0)
    } else if b < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB", b / (1024.0 * 1024.0))
    } else if b < 1024.0 * 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} GB", b / (1024.0 * 1024.0 * 1024.0))
    } else {
        format!("{:.1} TB", b / (1024.0 * 1024.0 * 1024.0 * 1024.0))
    }
}

pub fn format_relative(unix_ts: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let diff = now - unix_ts;
    if diff < 0 {
        return "just now".to_string();
    }
    if diff < 60 {
        return format!("{}s ago", diff);
    }
    if diff < 3600 {
        return format!("{}m ago", diff / 60);
    }
    if diff < 86400 {
        return format!("{}h ago", diff / 3600);
    }
    format!("{}d ago", diff / 86400)
}

pub fn format_expiry(unix_ts: i64) -> String {
    let dt = chrono::DateTime::from_timestamp(unix_ts, 0);
    let Some(dt) = dt else {
        return "—".to_string();
    };
    let now = chrono::Utc::now();
    let days = (dt - now).num_days();
    if days < 0 {
        format!("{} (expired)", dt.format("%Y-%m-%d"))
    } else if days == 0 {
        format!("{} (today)", dt.format("%Y-%m-%d"))
    } else if days == 1 {
        format!("{} (tomorrow)", dt.format("%Y-%m-%d"))
    } else {
        format!("{} ({}d left)", dt.format("%Y-%m-%d"), days)
    }
}

pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}
