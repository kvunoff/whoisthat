use std::fmt::Display;

use serde::Serialize;

use super::args::GlobalArgs;

pub struct Out {
    pub json: bool,
    pub quiet: bool,
}

impl Out {
    pub fn new(g: &GlobalArgs) -> Self {
        Self {
            json: g.json,
            quiet: g.quiet,
        }
    }

    /// Machine-readable payload on stdout.
    pub fn json<T: Serialize>(&self, value: &T) {
        match serde_json::to_string(value) {
            Ok(s) => println!("{s}"),
            Err(e) => eprintln!("Error: failed to encode JSON: {e}"),
        }
    }

    /// Human data line on stdout.
    pub fn line(&self, s: impl Display) {
        if !self.json {
            println!("{s}");
        }
    }

    /// Progress/notes on stderr; suppressed by --quiet and by --json.
    pub fn note(&self, s: impl Display) {
        if !self.quiet && !self.json {
            eprintln!("{s}");
        }
    }

    /// Successful mutation result: a human line, or `{"ok":true,"result":...}`
    /// in JSON mode.
    pub fn ok(&self, human: impl Display, result: serde_json::Value) {
        if self.json {
            self.json(&serde_json::json!({"ok": true, "result": result}));
        } else {
            self.line(human);
        }
    }
}

/// Human byte sizes for new tables. `status` keeps using
/// `legacy::format_speed_bytes` so its JSON contract does not change.
pub fn format_bytes(bytes: i64) -> String {
    let b = bytes.max(0) as f64;
    if b < 1024.0 {
        format!("{b:.0} B")
    } else if b < 1024.0 * 1024.0 {
        format!("{:.1} KiB", b / 1024.0)
    } else if b < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MiB", b / (1024.0 * 1024.0))
    } else if b < 1024.0 * 1024.0 * 1024.0 * 1024.0 {
        format!("{:.2} GiB", b / (1024.0 * 1024.0 * 1024.0))
    } else {
        format!("{:.2} TiB", b / (1024.0 * 1024.0 * 1024.0 * 1024.0))
    }
}

pub fn format_epoch(ts: i64) -> String {
    if ts <= 0 {
        return "—".to_string();
    }
    use chrono::TimeZone;
    match chrono::Local.timestamp_opt(ts, 0).single() {
        Some(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        None => "—".to_string(),
    }
}

/// Keep the scheme and host, drop credentials and query. Handles opaque
/// payloads such as `ss://<base64>` that have no `@`/path.
pub fn redact_url(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return "…".to_string();
    };
    // Shadowsocks links are often an opaque base64 blob without userinfo.
    if scheme == "ss" && !rest.contains('@') {
        return "ss://…".to_string();
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit('@').next().unwrap_or(authority);
    if host.is_empty() {
        format!("{scheme}://…")
    } else {
        format!("{scheme}://{host}/…")
    }
}

pub fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(cell.chars().count());
            }
        }
    }
    let render = |cells: &[String]| {
        let mut out = String::new();
        for (i, cell) in cells.iter().enumerate() {
            if i > 0 {
                out.push_str("  ");
            }
            out.push_str(cell);
            if i + 1 < cells.len() {
                let pad = widths[i].saturating_sub(cell.chars().count());
                out.push_str(&" ".repeat(pad));
            }
        }
        out
    };
    println!(
        "{}",
        render(&headers.iter().map(|h| h.to_string()).collect::<Vec<_>>())
    );
    for row in rows {
        println!("{}", render(row));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_boundaries() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1024), "1.0 KiB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MiB");
    }

    #[test]
    fn redacts_credentials_and_queries() {
        assert_eq!(
            redact_url("vless://uuid@example.com:443?type=tcp#Node"),
            "vless://example.com:443/…"
        );
        assert_eq!(
            redact_url("https://example.com/sub/token?x=1"),
            "https://example.com/…"
        );
        assert_eq!(redact_url("ss://YWVzOnBhc3M"), "ss://…");
        assert_eq!(redact_url("not-a-url"), "…");
    }
}
