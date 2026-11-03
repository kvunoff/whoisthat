use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table, TableState},
};

use crate::core_client::protocol::*;
use crate::ui::theme::*;

use super::form::{rule_outbound_label, rule_type_label, rule_value};

#[allow(clippy::too_many_arguments)]
pub fn render_routing_tab(
    f: &mut Frame,
    area: Rect,
    config: &RoutingConfig,
    cursor: usize,
    focused: bool,
    group: Option<&GroupWithProfiles>,
    group_index: usize,
    total_pages: usize,
    source: &str,
) {
    let border_color = if focused { border_active() } else { border() };

    let is_global = group.is_none() || group_index == 0;
    let is_json = group
        .map(|g| g.group.subscription_format == "json")
        .unwrap_or(false);
    let total_groups = total_pages.saturating_sub(1);
    let manual_label = if source == "custom" {
        "group rules"
    } else {
        "Global rules"
    };

    let block = Block::default()
        .title(" Routing Rules ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(s_bg());

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Compute effective source & shadowed notice
    let raw_mode = group
        .map(|g| g.group.routing_mode.as_str())
        .unwrap_or("auto");
    let mode = if raw_mode.is_empty() {
        "auto"
    } else {
        raw_mode
    };
    let active_id = group
        .map(|g| g.group.active_routing_profile_id.as_str())
        .unwrap_or("");
    let active_profile = group.and_then(|g| g.routing_profiles.iter().find(|p| p.id == active_id));

    let is_shadowed = !is_global
        && !is_json
        && match mode {
            "manual" => false,
            "subscription" => true,
            "auto" => active_profile.is_some() && active_profile.unwrap().rules_count > 0,
            _ => false,
        };
    let is_inherited = !is_global && source != "custom";

    let (source_desc, source_style) = if is_global {
        ("Manual (Global rules)".to_string(), s_text())
    } else if is_json {
        (
            "Provider JSON baseline + manual rules (manual first)".to_string(),
            s_accent(),
        )
    } else {
        match mode {
            "manual" => (
                format!("Manual ({manual_label})"),
                if source == "custom" {
                    s_success()
                } else {
                    s_text()
                },
            ),
            "subscription" => {
                if let Some(p) = active_profile {
                    if p.state == "error" {
                        (
                            format!("Subscription (Happ: {}, error: fallback to LAN)", p.name),
                            s_error(),
                        )
                    } else {
                        (format!("Subscription (Happ: {})", p.name), s_success())
                    }
                } else {
                    (
                        "Subscription (No active profile — fallback to direct/LAN)".to_string(),
                        s_disconnected(),
                    )
                }
            }
            _ => {
                if let Some(p) = active_profile {
                    if p.state == "error" {
                        (
                            format!("Auto (Happ: {}, error: fallback to {manual_label})", p.name),
                            s_accent(),
                        )
                    } else if p.rules_count > 0 {
                        (format!("Auto (Happ: {})", p.name), s_success())
                    } else {
                        (format!("Auto (Manual: {manual_label})"), s_text())
                    }
                } else {
                    (format!("Auto (Manual: {manual_label})"), s_text())
                }
            }
        }
    };

    let header_height =
        3 + usize::from(is_inherited) + usize::from(is_shadowed) + usize::from(is_json);
    let v = Layout::vertical([
        Constraint::Length(header_height as u16),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(inner);
    let (header_area, list_area, help_area) = (v[0], v[1], v[2]);

    // Header rendering
    let mut header_lines = Vec::new();

    // Line 1: Group/page, Mode, Happ Profiles
    let group_str = if is_global {
        "Global".to_string()
    } else {
        let name = group.map(|g| g.group.name.as_str()).unwrap_or("Unknown");
        format!("[{}/{}] {}", group_index, total_groups, name)
    };
    let profiles_count = group.map(|g| g.routing_profiles.len()).unwrap_or(0);
    let mode_suffix = format!("[{mode}]");

    let line1 = if is_global {
        let scope = if total_groups == 0 {
            "shared base (no groups yet)".to_string()
        } else {
            format!("shared base for {total_groups} group(s)")
        };
        Line::from(vec![
            Span::styled(" Group: < ", s_faint()),
            Span::styled(group_str, s_accent_bold().add_modifier(Modifier::BOLD)),
            Span::styled(" > (←/→) │ Scope: ", s_faint()),
            Span::styled(scope, s_text()),
        ])
    } else if is_json {
        Line::from(vec![
            Span::styled(" Group: < ", s_faint()),
            Span::styled(group_str, s_accent_bold().add_modifier(Modifier::BOLD)),
            Span::styled(" > (←/→) │ Provider JSON: ", s_faint()),
            Span::styled("read-only", s_accent()),
            Span::styled(" (manual rules below still apply)", s_dim()),
        ])
    } else {
        Line::from(vec![
            Span::styled(" Group: < ", s_faint()),
            Span::styled(group_str, s_accent_bold().add_modifier(Modifier::BOLD)),
            Span::styled(" > (←/→) │ Mode: ", s_faint()),
            Span::styled(mode_suffix, s_accent()),
            Span::styled(" (m) │ Happ Profiles: ", s_faint()),
            Span::styled("[H] ", s_accent_bold()),
            Span::styled(format!("({profiles_count} profiles)"), s_dim()),
        ])
    };
    header_lines.push(line1);

    // Line 2: Manual rule set in use
    let (manual_desc, manual_style) = if is_global {
        (
            "Global rules — used by every group that inherits them".to_string(),
            s_text(),
        )
    } else if source == "custom" {
        ("Custom rules for this group".to_string(), s_success())
    } else {
        (
            "Global rules (inherited) — press g to customize".to_string(),
            s_dim(),
        )
    };
    let manual_hint = if is_global {
        Span::raw("")
    } else {
        Span::styled(" │ g: toggle source", s_faint())
    };
    header_lines.push(Line::from(vec![
        Span::styled(" Manual: ", s_faint()),
        Span::styled(manual_desc, manual_style),
        manual_hint,
    ]));

    // Line 3: Effective Source
    let line3 = Line::from(vec![
        Span::styled(" Effective Source: ", s_faint()),
        Span::styled(source_desc, source_style),
    ]);
    header_lines.push(line3);

    // Line 3b: JSON subscription notice
    if is_json {
        header_lines.push(Line::from(vec![Span::styled(
            " Provider JSON routing is immutable; manual rules above it are checked first.",
            s_dim(),
        )]));
    }

    // Line 4: Inheritance notice
    if is_inherited {
        header_lines.push(Line::from(vec![Span::styled(
            " ⚠ These are Global rules. Press g or edit a rule to create group-specific rules.",
            s_dim(),
        )]));
    }

    // Line 5: Notice if shadowed
    if is_shadowed {
        header_lines.push(Line::from(vec![Span::styled(
            " ⚠ Notice: Manual rules below are shadowed by active Happ routing for this group",
            s_accent().add_modifier(Modifier::BOLD),
        )]));
    }

    f.render_widget(Paragraph::new(header_lines), header_area);

    if config.rules.is_empty() {
        f.render_widget(
            Paragraph::new("No rules defined.\nPress [a] to add a rule.")
                .style(s_dim())
                .alignment(Alignment::Center),
            list_area,
        );
    } else {
        let is_compact = area.width < 75;
        let header = if is_compact {
            Row::new(vec![
                Cell::from(Span::styled("On", s_faint())),
                Cell::from(Span::styled("Type", s_faint())),
                Cell::from(Span::styled("Value", s_faint())),
                Cell::from(Span::raw("")),
                Cell::from(Span::styled("Out", s_faint())),
            ])
            .height(1)
            .bottom_margin(0)
        } else {
            Row::new(vec![
                Cell::from(Span::styled("On", s_faint())),
                Cell::from(Span::styled("Type", s_faint())),
                Cell::from(Span::styled("Value", s_faint())),
                Cell::from(Span::raw("")),
                Cell::from(Span::styled("Outbound", s_faint())),
            ])
            .height(1)
            .bottom_margin(0)
        };

        let rows: Vec<Row> = config
            .rules
            .iter()
            .enumerate()
            .map(|(i, rule)| {
                let is_cursor = i == cursor && focused;
                let on_style = if rule.enabled {
                    s_success()
                } else {
                    s_disconnected()
                };
                let on_span = Span::styled(if rule.enabled { "●" } else { "○" }, on_style);

                let type_str = rule_type_label(rule);
                let type_style = if is_cursor { s_text() } else { s_dim() };

                let value_str = rule_value(rule);
                let value_style = if is_cursor { s_text() } else { s_dim() };

                let outbound = rule_outbound_label(rule);
                let ob_style = match outbound {
                    "direct" => s_success(),
                    "block" => s_error(),
                    _ => s_accent(),
                };

                Row::new(vec![
                    Cell::from(on_span),
                    Cell::from(Span::styled(type_str, type_style)),
                    Cell::from(Span::styled(value_str, value_style)),
                    Cell::from(Span::styled("→", s_faint())),
                    Cell::from(Span::styled(outbound, ob_style)),
                ])
            })
            .collect();

        let col_constraints = if is_compact {
            [
                Constraint::Length(3),
                Constraint::Length(8),
                Constraint::Min(8),
                Constraint::Length(1),
                Constraint::Length(7),
            ]
        } else {
            [
                Constraint::Length(4),
                Constraint::Length(10),
                Constraint::Min(10),
                Constraint::Length(3),
                Constraint::Length(10),
            ]
        };

        let table = Table::new(rows, col_constraints)
            .header(header)
            .row_highlight_style(Style::default().bg(surface()))
            .highlight_symbol("> ");

        let mut ts = TableState::default();
        ts.select(Some(cursor));
        f.render_stateful_widget(table, list_area, &mut ts);
    }

    let hint = if is_json {
        if area.width < 85 {
            " a:add │ p:presets │ g:source │ e:edit │ x:del │ Spc │ ←/→:page "
        } else {
            " a add │ p presets │ g source │ e edit │ x delete │ Space │ ←/→ page "
        }
    } else if area.width < 85 {
        " a:add │ p:presets │ H:happ │ m:mode │ g:source │ i:clip │ e:edit │ x:del │ Spc │ ←/→:page "
    } else {
        " a add │ p presets │ H happ │ m mode │ g source │ i import │ e edit │ x delete │ Space │ ←/→ page "
    };
    f.render_widget(
        Paragraph::new(hint)
            .style(s_faint())
            .alignment(Alignment::Center),
        help_area,
    );
}
