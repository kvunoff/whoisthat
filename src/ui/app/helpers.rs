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

/// Hide credentials in a provider URL before it reaches the screen: userinfo
/// and the whole query string are the usual token carriers.
///
/// shortcut: only query and userinfo are masked; a token embedded in the path
/// stays visible. Extend if a provider starts doing that.
pub fn redact_url(url: &str) -> String {
    let (base, has_query) = match url.split_once('?') {
        Some((b, _)) => (b, true),
        None => (url, false),
    };
    let base = match base.split_once("://") {
        Some((scheme, rest)) => {
            let authority_end = rest.find('/').unwrap_or(rest.len());
            let authority = &rest[..authority_end];
            let path = &rest[authority_end..];
            let host = authority
                .rsplit_once('@')
                .map(|(_, h)| h)
                .unwrap_or(authority);
            format!("{scheme}://{host}{path}")
        }
        None => base.to_string(),
    };
    if has_query {
        format!("{base}?…")
    } else {
        base
    }
}

/// Provider-controlled labels end up in the terminal verbatim. Strip control
/// characters (ANSI escapes, CR/LF, tabs) and cap the length so a hostile
/// subscription cannot clear the screen or wrap the layout.
///
/// shortcut: strips all control chars, including tabs/newlines that a provider
/// might use for formatting; names are single-line labels.
pub fn clean_label(s: &str) -> String {
    const MAX_LABEL: usize = 200;
    s.chars()
        .filter(|c| !c.is_control())
        .take(MAX_LABEL)
        .collect()
}

#[allow(dead_code)]
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

pub fn centered_rect_fixed(width: u16, height: u16, r: Rect) -> Rect {
    let w = width.min(r.width.saturating_sub(2));
    let h = height.min(r.height.saturating_sub(2));
    let x = r.x + (r.width.saturating_sub(w)) / 2;
    let y = r.y + (r.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

pub fn responsive_rect(
    preferred_w: u16,
    preferred_h: u16,
    max_pct_w: u16,
    max_pct_h: u16,
    screen: Rect,
) -> Rect {
    let max_w = ((screen.width as u32 * max_pct_w.clamp(10, 100) as u32) / 100) as u16;
    let max_h = ((screen.height as u32 * max_pct_h.clamp(10, 100) as u32) / 100) as u16;
    let available_w = screen.width.saturating_sub(2).max(1);
    let available_h = screen.height.saturating_sub(2).max(1);

    let w = preferred_w.min(max_w).min(available_w).max(1);
    let h = preferred_h.min(max_h).min(available_h).max(1);

    let x = screen.x + (screen.width.saturating_sub(w)) / 2;
    let y = screen.y + (screen.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_responsive_rect_large_screen() {
        let screen = Rect::new(0, 0, 140, 50);
        let r = responsive_rect(60, 16, 90, 80, screen);
        assert_eq!(r.width, 60);
        assert_eq!(r.height, 16);
        assert_eq!(r.x, 40);
        assert_eq!(r.y, 17);
    }

    #[test]
    fn test_redact_url_hides_tokens() {
        assert_eq!(
            redact_url("https://example.com/sub?token=abc&id=1"),
            "https://example.com/sub?…"
        );
        assert_eq!(
            redact_url("https://user:pass@example.com/sub"),
            "https://example.com/sub"
        );
        assert_eq!(
            redact_url("https://example.com/sub"),
            "https://example.com/sub"
        );
    }

    #[test]
    fn test_responsive_rect_small_screen() {
        let screen = Rect::new(0, 0, 50, 15);
        let r = responsive_rect(70, 20, 95, 90, screen);
        assert!(r.width <= 48);
        assert!(r.height <= 13);
        assert!(r.x >= 1);
        assert!(r.y >= 1);
    }
}
