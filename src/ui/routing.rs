use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Table,
        TableState,
    },
    Frame,
};

use crate::core_client::protocol::*;

use super::theme::*;

const TYPE_LABELS: &[&str] = &["domain", "ip", "protocol", "port", "geoip", "geosite"];
const OUTBOUND_LABELS: &[&str] = &["proxy", "direct", "block"];

#[derive(Debug, Clone)]
pub struct PresetRule {
    pub match_type: &'static str,
    pub value: &'static str,
    pub outbound: &'static str,
}

impl PresetRule {
    pub fn to_routing_rule(&self) -> RoutingRule {
        let mut rule = RoutingRule {
            r#type: "field".into(),
            outbound_tag: self.outbound.into(),
            enabled: true,
            ..Default::default()
        };
        match self.match_type {
            "geoip" | "ip" => rule.ip = self.value.into(),
            "geosite" | "domain" => rule.domain = self.value.into(),
            "protocol" => rule.protocol = self.value.into(),
            "port" => rule.port = self.value.into(),
            _ => {}
        }
        rule
    }

    pub fn matches_rule(&self, r: &RoutingRule) -> bool {
        if r.outbound_tag != self.outbound {
            return false;
        }
        match self.match_type {
            "geoip" | "ip" => r.ip == self.value,
            "geosite" | "domain" => r.domain == self.value,
            "protocol" => r.protocol == self.value,
            "port" => r.port == self.value,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RoutingPreset {
    #[allow(dead_code)]
    pub id: &'static str,
    pub title: &'static str,
    pub desc: &'static str,
    pub rules: &'static [PresetRule],
}

impl RoutingPreset {
    pub fn is_applied(&self, config: &RoutingConfig) -> bool {
        self.rules
            .iter()
            .all(|pr| config.rules.iter().any(|r| r.enabled && pr.matches_rule(r)))
    }
}

pub static ROUTING_PRESETS: &[RoutingPreset] = &[
    RoutingPreset {
        id: "bypass_ru",
        title: "🇷🇺 Bypass Russia",
        desc: "Direct access to Russian domains, banks, government & services bypassing VPN",
        rules: &[
            PresetRule {
                match_type: "geosite",
                value: "geosite:category-ru",
                outbound: "direct",
            },
            PresetRule {
                match_type: "geoip",
                value: "geoip:ru",
                outbound: "direct",
            },
        ],
    },
    RoutingPreset {
        id: "block_ads",
        title: "🛡️ Block Ads & Trackers",
        desc: "Global ad, tracker and telemetry blocking via Xray blackhole",
        rules: &[PresetRule {
            match_type: "geosite",
            value: "geosite:category-ads-all",
            outbound: "block",
        }],
    },
    RoutingPreset {
        id: "bypass_lan",
        title: "🏠 Bypass LAN / Local",
        desc: "Direct access to local network, router and RFC1918 subnets without proxying",
        rules: &[PresetRule {
            match_type: "geoip",
            value: "geoip:private",
            outbound: "direct",
        }],
    },
    RoutingPreset {
        id: "proxy_media",
        title: "📺 Proxy Media (YouTube, Discord, Twitch)",
        desc: "Route video streaming and voice communication platforms through proxy",
        rules: &[
            PresetRule {
                match_type: "geosite",
                value: "geosite:youtube",
                outbound: "proxy",
            },
            PresetRule {
                match_type: "geosite",
                value: "geosite:discord",
                outbound: "proxy",
            },
            PresetRule {
                match_type: "geosite",
                value: "geosite:twitch",
                outbound: "proxy",
            },
        ],
    },
    RoutingPreset {
        id: "proxy_telegram",
        title: "💬 Proxy Telegram",
        desc: "Route all Telegram domains and official IP CIDR ranges through proxy",
        rules: &[
            PresetRule {
                match_type: "geosite",
                value: "geosite:telegram",
                outbound: "proxy",
            },
            PresetRule {
                match_type: "geoip",
                value: "geoip:telegram",
                outbound: "proxy",
            },
        ],
    },
    RoutingPreset {
        id: "proxy_ai",
        title: "🤖 Proxy AI Services (OpenAI, Claude)",
        desc: "Route ChatGPT, Claude, and major AI platforms through proxy",
        rules: &[
            PresetRule {
                match_type: "geosite",
                value: "geosite:openai",
                outbound: "proxy",
            },
            PresetRule {
                match_type: "geosite",
                value: "geosite:anthropic",
                outbound: "proxy",
            },
        ],
    },
    RoutingPreset {
        id: "proxy_socials",
        title: "📸 Proxy Blocked Socials (X/Twitter, Instagram, Meta)",
        desc: "Route blocked social media platforms through proxy",
        rules: &[
            PresetRule {
                match_type: "geosite",
                value: "geosite:twitter",
                outbound: "proxy",
            },
            PresetRule {
                match_type: "geosite",
                value: "geosite:instagram",
                outbound: "proxy",
            },
            PresetRule {
                match_type: "geosite",
                value: "geosite:facebook",
                outbound: "proxy",
            },
        ],
    },
    RoutingPreset {
        id: "block_torrent",
        title: "🛑 Block BitTorrent / P2P",
        desc: "Block P2P torrent protocol to protect VPS from DMCA/ISP abuse complaints",
        rules: &[PresetRule {
            match_type: "protocol",
            value: "bittorrent",
            outbound: "block",
        }],
    },
    RoutingPreset {
        id: "direct_torrent",
        title: "⚡ Direct BitTorrent / P2P",
        desc: "Download torrents directly at maximum ISP speed bypassing VPN",
        rules: &[PresetRule {
            match_type: "protocol",
            value: "bittorrent",
            outbound: "direct",
        }],
    },
    RoutingPreset {
        id: "bypass_cn",
        title: "🇨🇳 Bypass China",
        desc: "Direct access to Chinese websites and services (geosite:cn + geoip:cn)",
        rules: &[
            PresetRule {
                match_type: "geosite",
                value: "geosite:cn",
                outbound: "direct",
            },
            PresetRule {
                match_type: "geoip",
                value: "geoip:cn",
                outbound: "direct",
            },
        ],
    },
    RoutingPreset {
        id: "proxy_steam",
        title: "🎮 Proxy Gaming (Steam Community)",
        desc: "Route Steam Community and gaming services through proxy",
        rules: &[PresetRule {
            match_type: "geosite",
            value: "geosite:steam",
            outbound: "proxy",
        }],
    },
];

pub fn apply_preset(config: &mut RoutingConfig, preset: &RoutingPreset) {
    for pr in preset.rules {
        if let Some(existing) = config.rules.iter_mut().find(|r| pr.matches_rule(r)) {
            existing.enabled = true;
        } else {
            config.rules.push(pr.to_routing_rule());
        }
    }
}

pub fn remove_preset(config: &mut RoutingConfig, preset: &RoutingPreset) {
    config
        .rules
        .retain(|r| !preset.rules.iter().any(|pr| pr.matches_rule(r)));
}

pub fn toggle_preset(config: &mut RoutingConfig, preset: &RoutingPreset) -> bool {
    if preset.is_applied(config) {
        remove_preset(config, preset);
        false
    } else {
        apply_preset(config, preset);
        true
    }
}

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
    Presets {
        cursor: usize,
    },
}

fn rule_type_label(rule: &RoutingRule) -> &'static str {
    if rule.domain.starts_with("geosite:") {
        "geosite"
    } else if !rule.domain.is_empty() {
        "domain"
    } else if rule.ip.starts_with("geoip:") {
        "geoip"
    } else if !rule.ip.is_empty() {
        "ip"
    } else if !rule.protocol.is_empty() {
        "protocol"
    } else if !rule.port.is_empty() {
        "port"
    } else {
        ""
    }
}

fn rule_value(rule: &RoutingRule) -> &str {
    if rule.domain.starts_with("geosite:") {
        &rule.domain[8..]
    } else if !rule.domain.is_empty() {
        &rule.domain
    } else if rule.ip.starts_with("geoip:") {
        &rule.ip[6..]
    } else if !rule.ip.is_empty() {
        &rule.ip
    } else if !rule.protocol.is_empty() {
        &rule.protocol
    } else if !rule.port.is_empty() {
        &rule.port
    } else {
        ""
    }
}

fn rule_outbound_label(rule: &RoutingRule) -> &str {
    if rule.outbound_tag.is_empty() {
        "proxy"
    } else {
        &rule.outbound_tag
    }
}

pub fn rule_to_form(rule: &RoutingRule) -> (usize, String, usize) {
    let (match_type, value) = if rule.domain.starts_with("geosite:") {
        (5, rule.domain.clone())
    } else if rule.ip.starts_with("geoip:") {
        (4, rule.ip.clone())
    } else if !rule.domain.is_empty() {
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
        4 => rule.ip = value.to_string(),
        5 => rule.domain = value.to_string(),
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
    is_hy2: bool,
) {
    let border_color = if focused { BORDER_ACTIVE } else { BORDER };

    let block = Block::default()
        .title(" Routing Rules ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(s_bg());

    let inner = block.inner(area);
    f.render_widget(block, area);

    let (list_area, help_area) = if is_hy2 {
        let v = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(inner);

        let warn_line = Line::from(vec![
            Span::styled(" ⚠ ", s_warn()),
            Span::styled(
                "Active connection is Hysteria2: routing rules apply only to Xray protocols",
                s_warn(),
            ),
        ]);
        f.render_widget(Paragraph::new(warn_line).style(s_bg()), v[0]);
        (v[1], v[2])
    } else {
        let v = Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).split(inner);
        (v[0], v[1])
    };

    if config.rules.is_empty() {
        f.render_widget(
            Paragraph::new("No rules defined.\nPress [a] to add a rule.")
                .style(s_dim())
                .alignment(Alignment::Center),
            list_area,
        );
    } else {
        let header = Row::new(vec![
            Cell::from(Span::styled("On", s_faint())),
            Cell::from(Span::styled("Type", s_faint())),
            Cell::from(Span::styled("Value", s_faint())),
            Cell::from(Span::raw("")),
            Cell::from(Span::styled("Outbound", s_faint())),
        ])
        .height(1)
        .bottom_margin(0);

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

        let table = Table::new(
            rows,
            [
                Constraint::Length(4),
                Constraint::Length(10),
                Constraint::Min(10),
                Constraint::Length(3),
                Constraint::Length(10),
            ],
        )
        .header(header)
        .row_highlight_style(Style::default().bg(SURFACE))
        .highlight_symbol("> ");

        let mut ts = TableState::default();
        ts.select(Some(cursor));
        f.render_stateful_widget(table, list_area, &mut ts);
    }

    let hint = " a add  │  p presets  │  e edit  │  x delete  │  Space toggle  │  j/k navigate ";
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
    config: &RoutingConfig,
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
    }
}

fn render_presets_popup(f: &mut Frame, cursor: usize, config: &RoutingConfig, area: Rect) {
    let popup_h = 22.min(area.height.saturating_sub(2));
    let popup_w = 84.min(area.width.saturating_sub(4));
    let v = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(popup_h),
        Constraint::Fill(1),
    ])
    .split(area);
    let h = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(popup_w),
        Constraint::Fill(1),
    ])
    .split(v[1]);
    let pa = h[1];
    f.render_widget(ratatui::widgets::Clear, pa);

    let block = Block::default()
        .title(" ⚡ Routing Presets (GeoIP / GeoSite) ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
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

            let item_bg = if is_selected { SURFACE } else { BG };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_client::protocol::RoutingRule;

    fn rule(domain: &str, ip: &str, protocol: &str, port: &str, outbound_tag: &str) -> RoutingRule {
        RoutingRule {
            r#type: "field".into(),
            domain: domain.into(),
            ip: ip.into(),
            protocol: protocol.into(),
            port: port.into(),
            outbound_tag: outbound_tag.into(),
            enabled: true,
        }
    }

    // --- form_to_rule ---

    #[test]
    fn form_to_rule_domain() {
        let r = form_to_rule(0, "example.com", 0);
        assert_eq!(r.domain, "example.com");
        assert!(r.ip.is_empty());
        assert_eq!(r.outbound_tag, "proxy");
        assert_eq!(r.r#type, "field");
        assert!(r.enabled);
    }

    #[test]
    fn form_to_rule_ip() {
        let r = form_to_rule(1, "10.0.0.0/8", 1);
        assert_eq!(r.ip, "10.0.0.0/8");
        assert!(r.domain.is_empty());
        assert_eq!(r.outbound_tag, "direct");
    }

    #[test]
    fn form_to_rule_protocol() {
        let r = form_to_rule(2, "bittorrent", 2);
        assert_eq!(r.protocol, "bittorrent");
        assert_eq!(r.outbound_tag, "block");
    }

    #[test]
    fn form_to_rule_port() {
        let r = form_to_rule(3, "8000-9000", 0);
        assert_eq!(r.port, "8000-9000");
    }

    #[test]
    fn form_to_rule_geoip() {
        let r = form_to_rule(4, "geoip:ru", 1);
        assert_eq!(r.ip, "geoip:ru");
        assert!(r.domain.is_empty());
    }

    #[test]
    fn form_to_rule_geosite() {
        let r = form_to_rule(5, "geosite:youtube", 2);
        assert_eq!(r.domain, "geosite:youtube");
        assert!(r.ip.is_empty());
    }

    // --- rule_to_form ---

    #[test]
    fn rule_to_form_domain() {
        let (mt, val, ob) = rule_to_form(&rule("example.com", "", "", "", ""));
        assert_eq!(mt, 0);
        assert_eq!(val, "example.com");
        assert_eq!(ob, 0); // proxy
    }

    #[test]
    fn rule_to_form_ip() {
        let (mt, val, ob) = rule_to_form(&rule("", "192.168.1.0/24", "", "", "direct"));
        assert_eq!(mt, 1);
        assert_eq!(val, "192.168.1.0/24");
        assert_eq!(ob, 1);
    }

    #[test]
    fn rule_to_form_protocol() {
        let (mt, val, ob) = rule_to_form(&rule("", "", "http", "", "block"));
        assert_eq!(mt, 2);
        assert_eq!(val, "http");
        assert_eq!(ob, 2);
    }

    #[test]
    fn rule_to_form_port() {
        let (mt, val, _ob) = rule_to_form(&rule("", "", "", "443", ""));
        assert_eq!(mt, 3);
        assert_eq!(val, "443");
    }

    #[test]
    fn rule_to_form_geoip() {
        let (mt, val, _ob) = rule_to_form(&rule("", "geoip:private", "", "", ""));
        assert_eq!(mt, 4);
        assert_eq!(val, "geoip:private");
    }

    #[test]
    fn rule_to_form_geosite() {
        let (mt, val, _ob) = rule_to_form(&rule("geosite:netflix", "", "", "", ""));
        assert_eq!(mt, 5);
        assert_eq!(val, "geosite:netflix");
    }

    // --- round-trip ---

    #[test]
    fn round_trip_domain_proxy() {
        let original = rule("example.com", "", "", "", "proxy");
        let (mt, val, ob) = rule_to_form(&original);
        let reconstructed = form_to_rule(mt, &val, ob);
        assert_eq!(reconstructed.domain, original.domain);
        assert_eq!(reconstructed.outbound_tag, original.outbound_tag);
    }

    #[test]
    fn round_trip_geosite_block() {
        let original = rule("geosite:category-ads", "", "", "", "block");
        let (mt, val, ob) = rule_to_form(&original);
        let reconstructed = form_to_rule(mt, &val, ob);
        assert_eq!(reconstructed.domain, original.domain);
        assert_eq!(reconstructed.outbound_tag, "block");
    }

    // --- presets tests ---

    #[test]
    fn test_presets_definitions() {
        assert_eq!(ROUTING_PRESETS.len(), 11);
        for p in ROUTING_PRESETS {
            assert!(!p.id.is_empty());
            assert!(!p.title.is_empty());
            assert!(!p.desc.is_empty());
            assert!(!p.rules.is_empty());
            for r in p.rules {
                assert!(!r.value.is_empty());
                assert!(matches!(r.outbound, "direct" | "proxy" | "block"));
                assert!(matches!(
                    r.match_type,
                    "geosite" | "geoip" | "protocol" | "domain" | "ip" | "port"
                ));
                let routing_rule = r.to_routing_rule();
                assert_eq!(routing_rule.outbound_tag, r.outbound);
                assert!(routing_rule.enabled);
            }
        }
    }

    #[test]
    fn test_preset_is_applied_and_toggle() {
        let preset = &ROUTING_PRESETS[0]; // bypass_ru
        let mut cfg = RoutingConfig::default();

        assert!(!preset.is_applied(&cfg));

        // Toggle on
        let added = toggle_preset(&mut cfg, preset);
        assert!(added);
        assert!(preset.is_applied(&cfg));
        assert_eq!(cfg.rules.len(), preset.rules.len());

        // Toggle off
        let added_again = toggle_preset(&mut cfg, preset);
        assert!(!added_again);
        assert!(!preset.is_applied(&cfg));
        assert!(cfg.rules.is_empty());
    }

    #[test]
    fn test_apply_preset_idempotent() {
        let preset = &ROUTING_PRESETS[0]; // bypass_ru
        let mut cfg = RoutingConfig::default();

        apply_preset(&mut cfg, preset);
        let len_first = cfg.rules.len();

        apply_preset(&mut cfg, preset);
        assert_eq!(cfg.rules.len(), len_first);
        assert!(preset.is_applied(&cfg));
    }

    #[test]
    fn test_apply_preset_re_enables_disabled_rule() {
        let preset = &ROUTING_PRESETS[1]; // block_ads
        let mut cfg = RoutingConfig::default();

        apply_preset(&mut cfg, preset);
        assert!(preset.is_applied(&cfg));

        // Disable rule manually
        cfg.rules[0].enabled = false;
        assert!(!preset.is_applied(&cfg));

        // Re-apply preset -> should enable existing rule without creating duplicate
        apply_preset(&mut cfg, preset);
        assert_eq!(cfg.rules.len(), 1);
        assert!(cfg.rules[0].enabled);
        assert!(preset.is_applied(&cfg));
    }

    #[test]
    fn test_remove_preset_leaves_other_rules_intact() {
        let p_ru = &ROUTING_PRESETS[0]; // bypass_ru
        let p_ads = &ROUTING_PRESETS[1]; // block_ads
        let mut cfg = RoutingConfig::default();

        apply_preset(&mut cfg, p_ru);
        apply_preset(&mut cfg, p_ads);
        assert!(p_ru.is_applied(&cfg));
        assert!(p_ads.is_applied(&cfg));

        remove_preset(&mut cfg, p_ru);
        assert!(!p_ru.is_applied(&cfg));
        assert!(p_ads.is_applied(&cfg));
        assert_eq!(cfg.rules.len(), p_ads.rules.len());
    }
}
