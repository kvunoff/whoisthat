use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};

use crate::core_client::protocol::*;
use crate::ui::theme::*;

use super::{OUTBOUND_LABELS, ROUTING_PRESETS, RoutingPopup, TYPE_LABELS};

pub fn render_routing_popup(
    f: &mut Frame,
    popup: &RoutingPopup,
    area: Rect,
    config: &RoutingConfig,
    group: Option<&GroupWithProfiles>,
) {
    match popup {
        RoutingPopup::Add {
            match_type,
            value,
            outbound,
            cursor,
            field,
        } => {
            render_rule_form(
                f,
                "Add Rule",
                *match_type,
                value,
                *outbound,
                *cursor,
                *field,
                area,
            );
        }
        RoutingPopup::Edit {
            match_type,
            value,
            outbound,
            cursor,
            field,
            ..
        } => {
            render_rule_form(
                f,
                "Edit Rule",
                *match_type,
                value,
                *outbound,
                *cursor,
                *field,
                area,
            );
        }
        RoutingPopup::ConfirmDelete { .. } => {
            render_routing_confirm(f, area);
        }
        RoutingPopup::Presets { cursor } => {
            render_presets_popup(f, *cursor, config, area);
        }
        RoutingPopup::HappProfiles { cursor } => {
            render_happ_profiles_popup(f, *cursor, group, area);
        }
    }
}

fn render_happ_profiles_popup(
    f: &mut Frame,
    cursor: usize,
    group: Option<&GroupWithProfiles>,
    area: Rect,
) {
    let pa = crate::ui::app::responsive_rect(96, 26, 96, 92, area);
    f.render_widget(ratatui::widgets::Clear, pa);

    let group_name = group.map(|g| g.group.name.as_str()).unwrap_or("No group");
    let mode = group
        .map(|g| {
            let m = g.group.routing_mode.as_str();
            if m.is_empty() { "auto" } else { m }
        })
        .unwrap_or("auto");

    let block = Block::default()
        .title(format!(
            " ⚡ Happ Routing Profiles — Group: {} [mode: {}] ",
            group_name, mode
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent()))
        .style(s_surface());

    let inner = block.inner(pa);
    f.render_widget(block, pa);

    let profiles = group.map(|g| g.routing_profiles.as_slice()).unwrap_or(&[]);
    let active_id = group
        .map(|g| g.group.active_routing_profile_id.as_str())
        .unwrap_or("");
    let has_dns = profiles
        .iter()
        .any(|p| !p.remote_dns.is_empty() || !p.domestic_dns.is_empty() || p.fake_dns_enabled);

    // Check if the currently selected profile has an error message
    let selected_error = profiles.get(cursor).and_then(|p| {
        if !p.last_error.is_empty() {
            Some(p.last_error.as_str())
        } else {
            None
        }
    });

    let (list_area, error_area, hint_area) = if let Some(err_msg) = selected_error {
        let content_width = inner.width.saturating_sub(4).max(1) as usize;
        let estimated_lines: usize = err_msg
            .lines()
            .map(|l| (l.chars().count() / content_width).max(1))
            .sum();
        let max_err_h = (inner.height.saturating_sub(6)).clamp(3, 10);
        let err_box_h = (estimated_lines as u16 + 2).clamp(3, max_err_h);
        let chunks = Layout::vertical([
            Constraint::Min(4),
            Constraint::Length(err_box_h),
            Constraint::Length(1),
        ])
        .split(inner);
        (chunks[0], Some((chunks[1], err_msg)), chunks[2])
    } else {
        let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
        (chunks[0], None, chunks[1])
    };

    if profiles.is_empty() {
        let empty_msg = Paragraph::new(
            "No Happ routing profiles for this group.\n\nPress [i] in Routing tab to import from clipboard,\nor fetch via subscription.",
        )
        .style(s_dim())
        .alignment(Alignment::Center);
        f.render_widget(empty_msg, list_area);
    } else {
        let items: Vec<ListItem> = profiles
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let is_selected = i == cursor;
                let is_active = p.id == active_id || (active_id.is_empty() && p.active);
                let status_badge = if is_active {
                    Span::styled(" [● Active] ", s_success().add_modifier(Modifier::BOLD))
                } else {
                    Span::styled(" [○ Inactive] ", s_dim())
                };

                let state_badge = match p.state.as_str() {
                    "ready" | "" => Span::styled(" [ready] ", s_success()),
                    "updating" => Span::styled(" [updating...] ", s_accent()),
                    "error" => Span::styled(" [error] ", s_error().add_modifier(Modifier::BOLD)),
                    other => Span::styled(format!(" [{other}] "), s_dim()),
                };

                let title_style = if is_selected {
                    s_accent_bold().add_modifier(Modifier::BOLD)
                } else {
                    s_text()
                };
                let cursor_mark = if is_selected { "▸ " } else { "  " };
                let p_name = if p.name.is_empty() {
                    "(unnamed)"
                } else {
                    p.name.as_str()
                };

                let line1 = vec![
                    Span::styled(cursor_mark, s_accent()),
                    status_badge,
                    state_badge,
                    Span::styled(p_name, title_style),
                ];

                let mut details = vec![
                    Span::styled("      ", s_faint()),
                    Span::styled(format!("rules: {} │ ", p.rules_count), s_dim()),
                    Span::styled(
                        format!(
                            "strategy: {} │ ",
                            if p.domain_strategy.is_empty() {
                                "AsIs"
                            } else {
                                &p.domain_strategy
                            }
                        ),
                        s_dim(),
                    ),
                ];

                if p.last_updated > 0 {
                    details.push(Span::styled(
                        format!(
                            "updated: {} │ ",
                            crate::ui::app::format_relative(p.last_updated)
                        ),
                        s_dim(),
                    ));
                }
                if p.geo_updated_at > 0 {
                    details.push(Span::styled(
                        format!(
                            "geo: {} │ ",
                            crate::ui::app::format_relative(p.geo_updated_at)
                        ),
                        s_dim(),
                    ));
                }
                if !p.last_error.is_empty() {
                    let max_snippet = 35;
                    let snippet = if p.last_error.chars().count() > max_snippet {
                        format!(
                            "err: {}…",
                            p.last_error
                                .chars()
                                .take(max_snippet - 1)
                                .collect::<String>()
                        )
                    } else {
                        format!("err: {}", p.last_error)
                    };
                    details.push(Span::styled(snippet, s_error()));
                } else if let Some(last) = details.last_mut() {
                    let text = last.content.trim_end_matches(" │ ");
                    *last = Span::styled(text.to_string(), s_dim());
                }

                let mut lines = vec![Line::from(line1), Line::from(details)];
                if has_dns {
                    lines.push(happ_dns_line(p));
                }

                let item_bg = if is_selected { surface() } else { bg() };
                ListItem::new(lines).style(Style::default().bg(item_bg))
            })
            .collect();

        let item_height = if has_dns { 3 } else { 2 };
        let visible_items = (list_area.height as usize) / item_height;
        let scroll_item = if visible_items > 0 && cursor >= visible_items {
            cursor - visible_items + 1
        } else {
            0
        };
        let mut list_state = ListState::default()
            .with_selected(Some(cursor))
            .with_offset(scroll_item);

        let list = List::new(items);
        f.render_stateful_widget(list, list_area, &mut list_state);
    }

    if let Some((err_rect, err_msg)) = error_area {
        let err_block = Block::default()
            .title(" ⚠ Error Details ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(error()))
            .style(s_surface());
        let err_p = Paragraph::new(err_msg)
            .block(err_block)
            .style(s_error())
            .wrap(ratatui::widgets::Wrap { trim: true });
        f.render_widget(err_p, err_rect);
    }

    let hint = Line::from(vec![
        Span::styled(" [Enter] ", s_accent_bold()),
        Span::styled("Activate  │ ", s_text()),
        Span::styled("[o] ", s_accent_bold()),
        Span::styled("Off  │ ", s_text()),
        Span::styled("[d/x] ", s_accent_bold()),
        Span::styled("Delete  │ ", s_text()),
        Span::styled("[m] ", s_accent_bold()),
        Span::styled(format!("Mode ({mode})  │ "), s_text()),
        Span::styled("[Esc] ", s_accent_bold()),
        Span::styled("Close", s_text()),
    ]);
    f.render_widget(Paragraph::new(hint).alignment(Alignment::Center), hint_area);
}

fn happ_dns_line(profile: &RoutingProfileInfo) -> Line<'static> {
    let mut spans = vec![Span::styled("      DNS: ", s_faint())];
    let mut parts = Vec::new();
    if !profile.remote_dns.is_empty() {
        parts.push(truncate_dns_label(&profile.remote_dns));
    }
    if !profile.domestic_dns.is_empty() {
        parts.push(truncate_dns_label(&profile.domestic_dns));
    }
    if profile.fake_dns_enabled {
        parts.push("FakeDNS: on".to_string());
    }
    let text = if parts.is_empty() {
        "—".to_string()
    } else {
        parts.join(" │ ")
    };
    spans.push(Span::styled(text, s_dim()));
    Line::from(spans)
}

pub(super) fn truncate_dns_label(label: &str) -> String {
    const MAX_CHARS: usize = 28;
    if label.chars().count() <= MAX_CHARS {
        return label.to_string();
    }
    format!("{}…", label.chars().take(MAX_CHARS - 1).collect::<String>())
}

fn render_presets_popup(f: &mut Frame, cursor: usize, config: &RoutingConfig, area: Rect) {
    let pa = crate::ui::app::responsive_rect(84, 22, 95, 90, area);
    f.render_widget(ratatui::widgets::Clear, pa);

    let block = Block::default()
        .title(" ⚡ Routing Presets (GeoIP / GeoSite) ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent()))
        .style(s_surface());

    let inner = block.inner(pa);
    f.render_widget(block, pa);

    let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);

    let list_area = chunks[0];
    let hint_area = chunks[1];

    let items: Vec<ListItem> = ROUTING_PRESETS
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let is_selected = i == cursor;
            let applied = p.is_applied(config);
            let status_badge = if applied {
                Span::styled(" [● Active] ", s_success().add_modifier(Modifier::BOLD))
            } else {
                Span::styled(" [○ Inactive] ", s_dim())
            };
            let title_style = if is_selected {
                s_accent_bold().add_modifier(Modifier::BOLD)
            } else {
                s_text()
            };
            let cursor_mark = if is_selected { "▸ " } else { "  " };

            let mut rules_spans = Vec::new();
            for (idx, r) in p.rules.iter().enumerate() {
                if idx > 0 {
                    rules_spans.push(Span::styled(" │ ", s_faint()));
                }
                rules_spans.push(Span::styled(r.value, s_dim()));
                rules_spans.push(Span::styled(" → ", s_faint()));
                let ob_style = match r.outbound {
                    "direct" => s_success(),
                    "block" => s_error(),
                    _ => s_accent(),
                };
                rules_spans.push(Span::styled(r.outbound, ob_style));
            }

            let line1 = vec![
                Span::styled(cursor_mark, s_accent()),
                status_badge,
                Span::styled(format!("{} ", p.title), title_style),
            ];
            let mut line2 = vec![
                Span::styled("      ", s_faint()),
                Span::styled(p.desc, s_dim()),
                Span::styled(" (", s_faint()),
            ];
            line2.extend(rules_spans);
            line2.push(Span::styled(")", s_faint()));

            let item_bg = if is_selected { surface() } else { bg() };
            ListItem::new(vec![Line::from(line1), Line::from(line2)])
                .style(Style::default().bg(item_bg))
        })
        .collect();

    let visible_items = (list_area.height as usize) / 2;
    let scroll_item = if visible_items > 0 && cursor >= visible_items {
        cursor - visible_items + 1
    } else {
        0
    };
    let mut list_state = ListState::default()
        .with_selected(Some(cursor))
        .with_offset(scroll_item);

    let list = List::new(items);
    f.render_stateful_widget(list, list_area, &mut list_state);

    let hint = Line::from(vec![
        Span::styled(" [Enter/Space] ", s_accent_bold()),
        Span::styled("Toggle  │ ", s_text()),
        Span::styled("[j/k / ↑/↓] ", s_accent_bold()),
        Span::styled("Navigate  │ ", s_text()),
        Span::styled("[Esc] ", s_accent_bold()),
        Span::styled("Close ", s_text()),
    ]);
    f.render_widget(Paragraph::new(hint).alignment(Alignment::Center), hint_area);
}

#[allow(clippy::too_many_arguments)]
fn render_rule_form(
    f: &mut Frame,
    title: &str,
    match_type: usize,
    value: &str,
    outbound: usize,
    _cursor: usize,
    field: usize,
    area: Rect,
) {
    let pa = crate::ui::app::responsive_rect(65, 14, 92, 90, area);
    f.render_widget(ratatui::widgets::Clear, pa);

    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent()))
        .style(s_surface());

    let inner = block.inner(pa);
    f.render_widget(block, pa);

    let active = s_accent();
    let inactive = s_dim();
    let f0 = if field == 0 { active } else { inactive };
    let f1 = if field == 1 { active } else { inactive };
    let f2 = if field == 2 { active } else { inactive };

    let w = inner.width.saturating_sub(2);
    let mut y = inner.y;

    let type_line = Line::from(vec![
        Span::styled("Type: ", f0),
        Span::styled("◄ ", if field == 0 { s_accent() } else { s_faint() }),
        Span::styled(
            TYPE_LABELS[match_type],
            if field == 0 { s_accent() } else { inactive },
        ),
        Span::styled(" ►", if field == 0 { s_accent() } else { s_faint() }),
    ]);
    f.render_widget(Paragraph::new(type_line), Rect::new(inner.x + 1, y, w, 1));
    y += 2;

    let value_label = match match_type {
        0 => "Domain (e.g. example.com)",
        1 => "IP (e.g. 10.0.0.0/8)",
        2 => "Protocol (e.g. http, tls)",
        3 => "Port (e.g. 443, 8000-9000)",
        4 => "GeoIP (e.g. geoip:ru)",
        5 => "GeoSite (e.g. geosite:youtube)",
        _ => "",
    };
    f.render_widget(
        Paragraph::new(value_label).style(f1),
        Rect::new(inner.x + 1, y, w, 1),
    );
    y += 1;
    let val_display = if value.is_empty() {
        match match_type {
            0 => "example.com",
            1 => "10.0.0.0/8",
            2 => "http",
            3 => "443",
            4 => "geoip:ru",
            5 => "geosite:youtube",
            _ => "",
        }
    } else {
        value
    };
    f.render_widget(
        Paragraph::new(val_display)
            .block(Block::default().borders(Borders::ALL).border_style(f1))
            .style(s_text()),
        Rect::new(inner.x + 1, y, w, 3),
    );
    y += 4;

    let ob_line = Line::from(vec![
        Span::styled("Outbound: ", f2),
        Span::styled("◄ ", if field == 2 { s_accent() } else { s_faint() }),
        Span::styled(
            OUTBOUND_LABELS[outbound],
            if field == 2 { s_accent() } else { inactive },
        ),
        Span::styled(" ►", if field == 2 { s_accent() } else { s_faint() }),
    ]);
    f.render_widget(Paragraph::new(ob_line), Rect::new(inner.x + 1, y, w, 1));
    y += 2;

    let hint = match field {
        0 => " ← / → change type  │  Tab next field  │  Esc cancel ",
        1 => " type to edit  │  ← / → move cursor  │  Tab next  │  Esc cancel ",
        2 => " ← / → change outbound  │  Enter save  │  Esc cancel ",
        _ => "",
    };
    f.render_widget(
        Paragraph::new(hint)
            .style(s_dim())
            .alignment(Alignment::Center),
        Rect::new(inner.x + 1, y, w, 2),
    );
}

fn render_routing_confirm(f: &mut Frame, area: Rect) {
    let pa = crate::ui::app::responsive_rect(48, 7, 90, 40, area);
    f.render_widget(ratatui::widgets::Clear, pa);

    let block = Block::default()
        .title(" Delete Rule ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(error()))
        .style(s_surface());

    let inner = block.inner(pa);
    f.render_widget(block, pa);

    let chunks =
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(inner);

    f.render_widget(
        Paragraph::new("Delete this rule?\nThis cannot be undone.")
            .style(s_text())
            .alignment(Alignment::Center)
            .wrap(ratatui::widgets::Wrap { trim: true }),
        chunks[0],
    );

    f.render_widget(
        Paragraph::new(" Enter confirm | Esc cancel ")
            .style(s_accent())
            .alignment(Alignment::Center),
        chunks[1],
    );
}
