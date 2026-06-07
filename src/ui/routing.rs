use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::core_client::protocol::*;

use super::theme::*;

const TYPE_LABELS: &[&str] = &["domain", "ip", "protocol", "port"];
const OUTBOUND_LABELS: &[&str] = &["proxy", "direct", "block"];

#[derive(Debug, Clone, PartialEq)]
pub enum RoutingPopup {
    Add {
        match_type: usize,
        value: String,
        outbound: usize,
        cursor: usize,
        field: usize,
    },
    Edit {
        index: usize,
        match_type: usize,
        value: String,
        outbound: usize,
        cursor: usize,
        field: usize,
    },
    ConfirmDelete {
        index: usize,
    },
}

fn rule_match_label(rule: &RoutingRule) -> String {
    if !rule.domain.is_empty() {
        format!("domain: {}", rule.domain)
    } else if !rule.ip.is_empty() {
        format!("ip: {}", rule.ip)
    } else if !rule.protocol.is_empty() {
        format!("protocol: {}", rule.protocol)
    } else if !rule.port.is_empty() {
        format!("port: {}", rule.port)
    } else {
        "(empty)".into()
    }
}

fn rule_outbound_label(rule: &RoutingRule) -> &str {
    if rule.outbound_tag.is_empty() { "proxy" } else { &rule.outbound_tag }
}

pub fn rule_to_form(rule: &RoutingRule) -> (usize, String, usize) {
    let (match_type, value) = if !rule.domain.is_empty() {
        (0, rule.domain.clone())
    } else if !rule.ip.is_empty() {
        (1, rule.ip.clone())
    } else if !rule.protocol.is_empty() {
        (2, rule.protocol.clone())
    } else {
        (3, rule.port.clone())
    };
    let outbound = match rule.outbound_tag.as_str() {
        "direct" => 1,
        "block" => 2,
        _ => 0,
    };
    (match_type, value, outbound)
}

pub fn form_to_rule(match_type: usize, value: &str, outbound: usize) -> RoutingRule {
    let mut rule = RoutingRule {
        r#type: "field".into(),
        outbound_tag: OUTBOUND_LABELS[outbound].into(),
        enabled: true,
        ..Default::default()
    };
    match match_type {
        0 => rule.domain = value.to_string(),
        1 => rule.ip = value.to_string(),
        2 => rule.protocol = value.to_string(),
        3 => rule.port = value.to_string(),
        _ => {}
    }
    rule
}

pub fn render_routing_tab(
    f: &mut Frame,
    area: Rect,
    config: &RoutingConfig,
    cursor: usize,
    focused: bool,
) {
    let border_color = if focused { BORDER_ACTIVE } else { BORDER };

    let block = Block::default()
        .title(" Routing Rules ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(s_bg());

    let v = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(2),
    ])
    .split(block.inner(area));
    f.render_widget(block, area);

    let list_area = v[0];
    let help_area = v[1];

    if config.rules.is_empty() {
        f.render_widget(
            Paragraph::new("No rules defined.\nPress [a] to add a rule.")
                .style(s_dim())
                .alignment(Alignment::Center),
            list_area,
        );
    } else {
        let mut lines: Vec<Line> = Vec::new();
        for (i, rule) in config.rules.iter().enumerate() {
            let is_cursor = i == cursor && focused;
            let ls = if is_cursor { s_text() } else { s_dim() };
            let marker = if is_cursor && focused { "›" } else { " " };

            let on_off = if rule.enabled {
                Span::styled(" ● ", s_success())
            } else {
                Span::styled(" ○ ", s_disconnected())
            };

            let match_label = rule_match_label(rule);
            let outbound = rule_outbound_label(rule);
            let arrow = Span::styled(" → ", s_faint());
            let ob_style = match outbound {
                "direct" => s_success(),
                "block" => s_error(),
                _ => s_accent(),
            };

            lines.push(Line::from(vec![
                Span::raw(format!(" {} ", marker)),
                on_off,
                Span::styled(match_label, ls),
                arrow,
                Span::styled(outbound, ob_style),
            ]));
        }
        f.render_widget(Paragraph::new(lines).style(s_bg()), list_area);
    }

    let hint = " a add  |  e edit  |  x delete  |  Space toggle  |  j/k navigate ";
    f.render_widget(
        Paragraph::new(hint)
            .style(s_faint())
            .alignment(Alignment::Center),
        help_area,
    );
}

pub fn render_routing_popup(
    f: &mut Frame,
    popup: &RoutingPopup,
    area: Rect,
) {
    match popup {
        RoutingPopup::Add { match_type, value, outbound, cursor, field } => {
            render_rule_form(f, "Add Rule", *match_type, value, *outbound, *cursor, *field, area);
        }
        RoutingPopup::Edit { match_type, value, outbound, cursor, field, .. } => {
            render_rule_form(f, "Edit Rule", *match_type, value, *outbound, *cursor, *field, area);
        }
        RoutingPopup::ConfirmDelete { .. } => {
            render_routing_confirm(f, area);
        }
    }
}

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
    let v = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(14),
        Constraint::Fill(1),
    ])
    .split(area);
    let h = Layout::horizontal([
        Constraint::Percentage(15),
        Constraint::Percentage(70),
        Constraint::Percentage(15),
    ])
    .split(v[1]);
    let pa = h[1];
    f.render_widget(ratatui::widgets::Clear, pa);

    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .style(s_surface());

    let inner = block.inner(pa);
    f.render_widget(block, pa);

    let active = Style::default().fg(ACCENT);
    let inactive = s_dim();
    let f0 = if field == 0 { active } else { inactive };
    let f1 = if field == 1 { active } else { inactive };
    let f2 = if field == 2 { active } else { inactive };

    let w = inner.width.saturating_sub(2);
    let mut y = inner.y;

    let type_label = format!("Type: {}  (Tab to change)", TYPE_LABELS[match_type]);
    f.render_widget(Paragraph::new(type_label).style(f0), Rect::new(inner.x + 1, y, w, 1));
    y += 2;

    let value_label = match match_type {
        0 => "Domain:  (e.g. geosite:category-ads, example.com)",
        1 => "IP:  (e.g. geoip:private, 10.0.0.0/8)",
        2 => "Protocol:  (e.g. http, tls, bittorrent)",
        3 => "Port:  (e.g. 443, 8000-9000)",
        _ => "",
    };
    f.render_widget(Paragraph::new(value_label).style(f1), Rect::new(inner.x + 1, y, w, 1));
    y += 1;
    let val = if value.is_empty() { match match_type { 0 => "geosite:...", 1 => "geoip:...", 2 => "http", _ => "443" } } else { value };
    f.render_widget(
        Paragraph::new(val)
            .block(Block::default().borders(Borders::ALL).border_style(f1))
            .style(s_text()),
        Rect::new(inner.x + 1, y, w, 3),
    );
    y += 4;

    let ob_label = format!("Outbound: {}  (Tab to change)", OUTBOUND_LABELS[outbound]);
    f.render_widget(Paragraph::new(ob_label).style(f2), Rect::new(inner.x + 1, y, w, 1));
    y += 2;

    let hint = match field {
        0 => " Tab switch field  |  Enter next ",
        1 => " Tab switch field  |  Enter next ",
        2 => " Tab switch field  |  Enter save  |  Esc cancel ",
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
    let v = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(7),
        Constraint::Fill(1),
    ])
    .split(area);
    let h = Layout::horizontal([
        Constraint::Percentage(25),
        Constraint::Percentage(50),
        Constraint::Percentage(25),
    ])
    .split(v[1]);
    let pa = h[1];
    f.render_widget(ratatui::widgets::Clear, pa);

    let block = Block::default()
        .title(" Delete Rule ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ERROR))
        .style(s_surface());

    let inner = block.inner(pa);
    f.render_widget(block, pa);

    let chunks = Layout::vertical([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ])
    .split(inner);

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
