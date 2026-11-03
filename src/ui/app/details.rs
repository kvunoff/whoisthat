use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::core_client::protocol::*;
use crate::ui::layout::ProfilesLayoutMode;
use crate::ui::theme::*;
use crate::ui::uri;

use super::helpers::*;
use super::state::App;
use super::types::*;

/// The panel is built once and rendered either as two columns (when both fit)
/// or as one scrollable column. `left`/`right` are ordered sections.
struct DetailsContent {
    title: String,
    hints: &'static str,
    left: Vec<Line<'static>>,
    right: Vec<Line<'static>>,
}

impl DetailsContent {
    fn flatten(&self) -> Vec<Line<'static>> {
        let mut rows = self.left.clone();
        if !self.right.is_empty() {
            if !rows.is_empty() {
                rows.push(Line::from(""));
            }
            rows.extend(self.right.iter().cloned());
        }
        rows
    }

    fn fits_two_columns(&self, height: usize) -> bool {
        self.left.len() <= height && self.right.len() <= height
    }
}

fn empty_content() -> DetailsContent {
    DetailsContent {
        title: " Details ".to_string(),
        hints: "[a] import",
        left: vec![
            Line::from(Span::styled("No profile selected.", s_dim())),
            Line::from(Span::styled("Use j/k to navigate, [a] to import.", s_dim())),
        ],
        right: Vec::new(),
    }
}

fn push_section(rows: &mut Vec<Line<'static>>, title: &str) {
    if !rows.is_empty() {
        rows.push(Line::from(""));
    }
    rows.push(section_header(title));
}

fn push_kv(rows: &mut Vec<Line<'static>>, key: &str, value: impl Into<String>) {
    rows.push(kv_row(key, value));
}

fn push_kv_opt(rows: &mut Vec<Line<'static>>, key: &str, value: Option<String>) {
    if let Some(value) = value.filter(|v| !v.is_empty()) {
        rows.push(kv_row(key, value));
    }
}

fn first_non_empty<'a>(values: &[&'a str]) -> Option<&'a str> {
    values.iter().copied().find(|value| !value.is_empty())
}

/// Map a URI query key to the matching core-provided field. Only fields that
/// are safe to display are exposed here.
fn detail_field<'a>(details: Option<&'a ProfileDetails>, keys: &[&str]) -> Option<&'a str> {
    let details = details?;
    for key in keys {
        let value = match *key {
            "alpn" => &details.alpn,
            "path" => &details.path,
            "host" => &details.host,
            "serviceName" | "service_name" => &details.service_name,
            "mode" => &details.mode,
            "fp" => &details.fingerprint,
            "pbk" => &details.public_key,
            "sid" => &details.short_id,
            "spx" => &details.spider_x,
            "obfs" | "obfs-type" | "obfs_type" => &details.obfs,
            _ => continue,
        };
        if !value.is_empty() {
            return Some(value.as_str());
        }
    }
    None
}

fn param_or_detail(
    uri: &crate::ui::uri::ParsedUri,
    details: Option<&ProfileDetails>,
    keys: &[&str],
) -> Option<String> {
    uri.param(keys)
        .map(str::to_string)
        .or_else(|| detail_field(details, keys).map(str::to_string))
}

fn format_time(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| d.format("%H:%M:%S").to_string())
        .unwrap_or_default()
}

fn auto_update_label(g: &Group) -> String {
    if g.sub_auto_update_disabled {
        "off".to_string()
    } else if g.sub_update_interval_override > 0 {
        format!(
            "on (manual {})",
            format_interval_secs(g.sub_update_interval_override)
        )
    } else if g.sub_update_interval > 0 {
        format!(
            "on (provider {})",
            format_interval_secs(g.sub_update_interval)
        )
    } else {
        "on (no interval)".to_string()
    }
}

/// Truncate a styled line to the available width, adding an ellipsis.
///
/// shortcut: character count approximates display width; ratatui clips any
/// remaining overflow from wide CJK/emoji glyphs.
fn truncate_line(line: Line<'static>, width: usize) -> Line<'static> {
    if width == 0 {
        return Line::from("");
    }
    if line.width() <= width {
        return line;
    }

    let Line {
        style,
        alignment,
        spans,
    } = line;
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;
    for span in spans {
        let span_width = span.width();
        if used + span_width <= width {
            used += span_width;
            out.push(span);
            continue;
        }
        let remaining = width.saturating_sub(used);
        if remaining > 0 {
            let mut text: String = span
                .content
                .chars()
                .take(remaining.saturating_sub(1))
                .collect();
            text.push('…');
            out.push(Span::styled(text, span.style));
        }
        break;
    }

    let mut result = Line::from(out);
    result.style = style;
    result.alignment = alignment;
    result
}

fn render_rows(f: &mut Frame, rows: &[Line<'static>], area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let x = area.x.saturating_add(1);
    let width = area.width.saturating_sub(1);
    for (i, line) in rows.iter().take(area.height as usize).enumerate() {
        f.render_widget(
            Paragraph::new(truncate_line(line.clone(), width as usize)),
            Rect::new(x, area.y + i as u16, width, 1),
        );
    }
}

impl App {
    /// Number of scrollable rows. When the two-column layout fits, scrolling is
    /// not needed and this returns the viewport height so max_scroll becomes 0.
    pub(crate) fn details_line_count(&self) -> usize {
        let content = self.details_content();
        let visible = self.details_visible();
        let area = match self.layout.profiles_mode {
            ProfilesLayoutMode::SinglePanel => self.layout.tree_area,
            _ => self.layout.details_area,
        };
        let width = area.width.saturating_sub(2) as usize;
        if width >= 70 && content.fits_two_columns(visible) {
            return visible;
        }
        content.flatten().len()
    }

    pub(super) fn render_details(&self, f: &mut Frame, area: Rect) {
        let right_focus = self.focus == Focus::RightPanel;
        let border_color = if right_focus {
            border_active()
        } else {
            border()
        };

        let content = self.details_content();
        let title = if self.layout.profiles_mode == ProfilesLayoutMode::SinglePanel {
            format!("{} [Tab] ", content.title.trim_end())
        } else {
            content.title.clone()
        };

        let mut block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color))
            .style(s_bg());
        if area.width >= 80 && !content.hints.is_empty() {
            block = block
                .title_bottom(Line::from(Span::styled(content.hints, s_faint())).right_aligned());
        }

        let inner = block.inner(area);
        f.render_widget(block, area);
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let visible = inner.height as usize;
        if inner.width >= 70 && content.fits_two_columns(visible) {
            let cols = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(inner);
            render_rows(f, &content.left, cols[0]);
            render_rows(f, &content.right, cols[1]);
            return;
        }

        let rows = content.flatten();
        let max_scroll = rows.len().saturating_sub(visible);
        let scroll = self.details_scroll.min(max_scroll);
        let x = inner.x.saturating_add(1);
        let width = inner.width.saturating_sub(1);

        for (i, line) in rows.iter().enumerate().skip(scroll).take(visible) {
            let y = inner.y + (i - scroll) as u16;
            f.render_widget(
                Paragraph::new(truncate_line(line.clone(), width as usize)),
                Rect::new(x, y, width, 1),
            );
        }

        if max_scroll > 0 && scroll < max_scroll {
            f.render_widget(
                Paragraph::new(format!(" ↓ {} more ", max_scroll - scroll))
                    .style(s_faint())
                    .alignment(Alignment::Right),
                Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1),
            );
        }
    }

    fn details_content(&self) -> DetailsContent {
        match self.tree_node_at(self.cursor) {
            Some(TreeNode::Group(gi)) => self
                .groups
                .get(gi)
                .map(|g| self.group_details_content(g))
                .unwrap_or_else(empty_content),
            Some(TreeNode::Profile(_, _)) => self
                .selected_profile()
                .map(|p| self.profile_details_content(p))
                .unwrap_or_else(empty_content),
            None => empty_content(),
        }
    }

    fn group_details_content(&self, g: &GroupWithProfiles) -> DetailsContent {
        let group = &g.group;
        let mut left = Vec::new();

        push_section(&mut left, "Group");
        push_kv(&mut left, "Name", group.name.as_str());
        let format = if group.subscription_format == "json" {
            "JSON (provider routing, read-only)"
        } else {
            "URI / share links"
        };
        push_kv(&mut left, "Format", format);
        push_kv(&mut left, "Profiles", g.profiles.len().to_string());

        if !group.subscription_url.is_empty() {
            push_section(&mut left, "Subscription");
            push_kv(&mut left, "URL", redact_url(&group.subscription_url));
            if group.sub_last_updated > 0 {
                push_kv(
                    &mut left,
                    "Updated",
                    format_relative(group.sub_last_updated),
                );
            }
            push_kv(&mut left, "Auto-update", auto_update_label(group));
            if group.sub_expires > 0 {
                push_kv(&mut left, "Expires", format_expiry(group.sub_expires));
            }
            if group.sub_upload > 0 || group.sub_download > 0 {
                let used = format_bytes(group.sub_upload + group.sub_download);
                let limit = if group.sub_total > 0 {
                    format_bytes(group.sub_total)
                } else {
                    "∞".to_string()
                };
                push_kv(&mut left, "Traffic", format!("{used} / {limit}"));
            }
        }

        let mut right = Vec::new();
        let has_routing = !group.routing_mode.is_empty()
            || group.manual_routing_mode == "custom"
            || !group.active_routing_profile_id.is_empty()
            || !g.routing_profiles.is_empty();
        if has_routing {
            push_section(&mut right, "Routing");
            let mode = if group.routing_mode.is_empty() {
                "auto"
            } else {
                group.routing_mode.as_str()
            };
            push_kv(&mut right, "Mode", mode);
            let manual = if group.manual_routing_mode == "custom" {
                "group rules"
            } else {
                "global rules"
            };
            push_kv(&mut right, "Manual", manual);

            let active = g
                .routing_profiles
                .iter()
                .find(|p| p.id == group.active_routing_profile_id);
            match active {
                Some(p) => {
                    let state = if p.state.is_empty() {
                        "ok"
                    } else {
                        p.state.as_str()
                    };
                    let style = if p.state == "error" {
                        s_error()
                    } else {
                        s_success()
                    };
                    right.push(kv_row_span(
                        "Active",
                        Span::styled(
                            format!("{} ({}, {} rules)", p.name, state, p.rules_count),
                            style,
                        ),
                    ));
                }
                None => push_kv(&mut right, "Active", "—"),
            }

            if !group.pending_active_routing_profile_id.is_empty()
                && group.pending_active_routing_profile_id != group.active_routing_profile_id
            {
                let pending = g
                    .routing_profiles
                    .iter()
                    .find(|p| p.id == group.pending_active_routing_profile_id)
                    .map(|p| p.name.as_str())
                    .unwrap_or(group.pending_active_routing_profile_id.as_str());
                push_kv(&mut right, "Pending", pending);
            }
        }

        push_section(&mut right, "Status");
        let connected = self
            .connected_id()
            .map(|(gid, _)| gid == group.id)
            .unwrap_or(false);
        right.push(kv_row_span(
            "State",
            if connected {
                Span::styled("● Connected", s_success())
            } else {
                Span::styled("○ Disconnected", s_dim())
            },
        ));
        if connected && self.connection_status.connected_at > 0 {
            right.push(kv_row(
                "Since",
                format_time(self.connection_status.connected_at),
            ));
        }

        DetailsContent {
            title: format!(" Details — {} ", clean_label(&group.name)),
            hints: "[u] update · [e] edit · [X] delete · [Tab] tree",
            left,
            right,
        }
    }

    fn profile_details_content(&self, p: &Profile) -> DetailsContent {
        let uri = self.cached_uri_params(p);
        let fetched = self.profile_details_for(p);
        let group = self.current_group();
        let group_name = group.map(|g| g.group.name.as_str()).unwrap_or("?");

        let name = if p.name.is_empty() {
            if p.address.is_empty() {
                "Unknown".to_string()
            } else {
                p.address.clone()
            }
        } else {
            p.name.clone()
        };

        let protocol = if p.protocol.is_empty() {
            if uri.protocol.is_empty() {
                "—".to_string()
            } else {
                uri.protocol.to_uppercase()
            }
        } else {
            p.protocol.to_uppercase()
        };

        let port = if !uri.port.is_empty() {
            uri.port.clone()
        } else if p.port > 0 {
            p.port.to_string()
        } else {
            "—".to_string()
        };

        let sni = first_non_empty(&[
            uri.sni.as_str(),
            fetched.map(|d| d.sni.as_str()).unwrap_or(""),
            uri.host.as_str(),
            p.host.as_str(),
        ])
        .unwrap_or("—");

        let transport = first_non_empty(&[
            uri.transport.as_str(),
            fetched.map(|d| d.transport.as_str()).unwrap_or(""),
        ])
        .unwrap_or("tcp");
        let security = first_non_empty(&[
            uri.security.as_str(),
            fetched.map(|d| d.security.as_str()).unwrap_or(""),
        ])
        .unwrap_or("none");
        let flow = first_non_empty(&[
            uri.flow.as_str(),
            fetched.map(|d| d.flow.as_str()).unwrap_or(""),
        ])
        .unwrap_or("—");

        let mut left = Vec::new();
        push_section(&mut left, "Profile");
        push_kv(&mut left, "Name", name.as_str());
        push_kv(&mut left, "Protocol", protocol);
        push_kv(&mut left, "Group", group_name);
        if let Some(g) = group
            && !g.group.subscription_url.is_empty()
        {
            push_kv(&mut left, "Sub", redact_url(&g.group.subscription_url));
        }

        if let Some(g) = group {
            let has_routing = p.managed
                || !g.group.routing_mode.is_empty()
                || !g.group.active_routing_profile_id.is_empty();
            if has_routing {
                push_section(&mut left, "Routing");
                if p.managed {
                    push_kv(&mut left, "Provider", "JSON (read-only)");
                }
                let mode = if g.group.routing_mode.is_empty() {
                    "auto"
                } else {
                    g.group.routing_mode.as_str()
                };
                push_kv(&mut left, "Group mode", mode);
                let active = g
                    .routing_profiles
                    .iter()
                    .find(|r| r.id == g.group.active_routing_profile_id);
                if let Some(r) = active {
                    let state = if r.state.is_empty() {
                        "ok"
                    } else {
                        r.state.as_str()
                    };
                    let style = if r.state == "error" {
                        s_error()
                    } else {
                        s_success()
                    };
                    left.push(kv_row_span(
                        "Active",
                        Span::styled(format!("{} ({})", r.name, state), style),
                    ));
                }
            }
        }

        push_section(&mut left, "Endpoint");
        push_kv(
            &mut left,
            "Address",
            if p.address.is_empty() {
                "—"
            } else {
                p.address.as_str()
            },
        );
        push_kv(&mut left, "Port", port);
        push_kv(&mut left, "SNI", sni);
        push_kv(&mut left, "Transport", transport);
        push_kv(&mut left, "Security", security);
        push_kv(&mut left, "Flow", flow);

        let shadowsocks =
            p.uri.starts_with("ss://") || p.protocol.eq_ignore_ascii_case("shadowsocks");
        let cipher = if p.uri.starts_with("ss://") {
            uri::parse_ss_method(&p.uri)
        } else {
            None
        }
        .or_else(|| uri.param(&["scy"]).map(str::to_string))
        .or_else(|| fetched.map(|d| d.cipher.clone()).filter(|v| !v.is_empty()));
        match cipher {
            Some(method) if shadowsocks && uri::is_insecure_ss_cipher(&method) => {
                left.push(kv_row_span(
                    "Cipher",
                    Span::styled(format!("⚠ {method} (insecure)"), s_warn()),
                ));
            }
            Some(method) => push_kv(&mut left, "Cipher", method),
            None => {}
        }

        push_kv_opt(&mut left, "ALPN", param_or_detail(&uri, fetched, &["alpn"]));
        push_kv_opt(&mut left, "Path", param_or_detail(&uri, fetched, &["path"]));
        push_kv_opt(&mut left, "Host", param_or_detail(&uri, fetched, &["host"]));
        push_kv_opt(
            &mut left,
            "Service",
            param_or_detail(&uri, fetched, &["serviceName", "service_name"]),
        );
        push_kv_opt(&mut left, "Mode", param_or_detail(&uri, fetched, &["mode"]));
        push_kv_opt(
            &mut left,
            "Fingerprint",
            param_or_detail(&uri, fetched, &["fp"]),
        );
        push_kv_opt(
            &mut left,
            "Public key",
            param_or_detail(&uri, fetched, &["pbk"]),
        );
        push_kv_opt(
            &mut left,
            "Short ID",
            param_or_detail(&uri, fetched, &["sid"]),
        );
        push_kv_opt(
            &mut left,
            "SpiderX",
            param_or_detail(&uri, fetched, &["spx"]),
        );
        push_kv_opt(
            &mut left,
            "Obfs",
            param_or_detail(&uri, fetched, &["obfs", "obfs-type", "obfs_type"]),
        );
        push_kv_opt(
            &mut left,
            "Header",
            param_or_detail(&uri, fetched, &["headerType"]),
        );
        push_kv_opt(
            &mut left,
            "Authority",
            param_or_detail(&uri, fetched, &["authority"]),
        );
        push_kv_opt(
            &mut left,
            "Encryption",
            param_or_detail(&uri, fetched, &["encryption"]),
        );
        let insecure = uri
            .param(&["allowInsecure", "insecure"])
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            || fetched.map(|d| d.allow_insecure).unwrap_or(false);
        if insecure {
            left.push(kv_row_span("Insecure", Span::styled("⚠ enabled", s_warn())));
        }

        let mut right = Vec::new();
        push_section(&mut right, "Status");
        let connected = self
            .connected_id()
            .map(|(gid, pid)| gid == p.group_id && pid == p.id)
            .unwrap_or(false);
        right.push(kv_row_span(
            "State",
            if connected {
                Span::styled("● Connected", s_success())
            } else {
                Span::styled("○ Disconnected", s_dim())
            },
        ));
        if connected && self.connection_status.connected_at > 0 {
            right.push(kv_row(
                "Since",
                format_time(self.connection_status.connected_at),
            ));
        }
        push_kv(
            &mut right,
            "TUN",
            if self.tun_enabled { "on" } else { "off" },
        );

        push_section(&mut right, "Diagnostics");
        if p.test_result > 0 {
            push_kv(&mut right, "Latency", format!("{} ms", p.test_result));
            if p.jitter_ms > 0 {
                push_kv(&mut right, "Jitter", format!("±{} ms", p.jitter_ms));
            }
            if p.loss_pct > 0 {
                push_kv(&mut right, "Loss", format!("{}%", p.loss_pct));
            }
        } else if p.test_result == -2 {
            push_kv(&mut right, "Latency", "testing...");
        } else if p.test_result == -1 {
            push_kv(&mut right, "Latency", "failed");
        }
        if p.tested_at > 0 {
            push_kv(&mut right, "Last test", format_relative(p.tested_at));
        }

        DetailsContent {
            title: format!(" Details — {} ", clean_label(&name)),
            hints: "[c] connect · [t] test · [y] copy · [e] rename · [x] delete · [Tab] tree",
            left,
            right,
        }
    }
}
