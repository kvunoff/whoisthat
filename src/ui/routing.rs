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
    HappProfiles {
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
        && match mode {
            "manual" => false,
            "subscription" => true,
            "auto" => active_profile.is_some() && active_profile.unwrap().rules_count > 0,
            _ => false,
        };
    let is_inherited = !is_global && source != "custom";

    let (source_desc, source_style) = if is_global {
        ("Manual (Global rules)".to_string(), s_text())
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

    let header_height = 3 + usize::from(is_inherited) + usize::from(is_shadowed);
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

    let hint = if area.width < 85 {
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
            if m.is_empty() {
                "auto"
            } else {
                m
            }
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

fn truncate_dns_label(label: &str) -> String {
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
            network: "".into(),
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

    #[test]
    fn test_render_routing_tab_with_group_shadowed() {
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut term = ratatui::Terminal::new(backend).unwrap();
        let cfg = RoutingConfig::default();
        let group = GroupWithProfiles {
            group: Group {
                id: 1,
                name: "Test Group".into(),
                routing_mode: "auto".into(),
                active_routing_profile_id: "happ-1".into(),
                ..Default::default()
            },
            profiles: Vec::new(),
            routing_profiles: vec![RoutingProfileInfo {
                id: "happ-1".into(),
                name: "Happ Profile 1".into(),
                state: "ready".into(),
                active: true,
                rules_count: 5,
                domain_strategy: "AsIs".into(),
                ..Default::default()
            }],
        };

        term.draw(|f| {
            render_routing_tab(f, f.area(), &cfg, 0, true, Some(&group), 1, 2, "global");
        })
        .unwrap();

        let text = term
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol().to_string())
            .collect::<Vec<_>>()
            .join("");
        assert!(text.contains("Group: < [1/1] Test Group >"));
        assert!(text.contains("Mode: [auto]"));
        assert!(text.contains("Happ Profiles: [H] (1 profiles)"));
        assert!(text.contains("Effective Source: Auto (Happ: Happ Profile 1)"));
        assert!(text.contains("Notice: Manual rules below are shadowed"));
    }

    #[test]
    fn test_render_routing_tab_manual_mode_not_shadowed() {
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut term = ratatui::Terminal::new(backend).unwrap();
        let cfg = RoutingConfig::default();
        let group = GroupWithProfiles {
            group: Group {
                id: 1,
                name: "Manual Group".into(),
                routing_mode: "manual".into(),
                ..Default::default()
            },
            profiles: Vec::new(),
            routing_profiles: Vec::new(),
        };

        term.draw(|f| {
            render_routing_tab(f, f.area(), &cfg, 0, true, Some(&group), 1, 2, "custom");
        })
        .unwrap();

        let text = term
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol().to_string())
            .collect::<Vec<_>>()
            .join("");
        assert!(text.contains("Group: < [1/1] Manual Group >"));
        assert!(text.contains("Mode: [manual]"));
        assert!(text.contains("Effective Source: Manual (group rules)"));
        assert!(!text.contains("Notice: Manual rules below are shadowed"));
    }

    #[test]
    fn test_render_routing_tab_global_page() {
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut term = ratatui::Terminal::new(backend).unwrap();
        let cfg = RoutingConfig::default();

        term.draw(|f| {
            render_routing_tab(f, f.area(), &cfg, 0, true, None, 0, 3, "global");
        })
        .unwrap();

        let text = term
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol().to_string())
            .collect::<Vec<_>>()
            .join("");
        assert!(text.contains("Group: < Global >"));
        assert!(text.contains("Scope: shared base for 2 group(s)"));
        assert!(text.contains("Manual: Global rules"));
        assert!(text.contains("Effective Source: Manual (Global rules)"));
    }

    #[test]
    fn test_render_routing_tab_inherited_notice() {
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut term = ratatui::Terminal::new(backend).unwrap();
        let cfg = RoutingConfig::default();
        let group = GroupWithProfiles {
            group: Group {
                id: 1,
                name: "Inheriting Group".into(),
                routing_mode: "manual".into(),
                ..Default::default()
            },
            profiles: Vec::new(),
            routing_profiles: Vec::new(),
        };

        term.draw(|f| {
            render_routing_tab(f, f.area(), &cfg, 0, true, Some(&group), 1, 2, "global");
        })
        .unwrap();

        let text = term
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol().to_string())
            .collect::<Vec<_>>()
            .join("");
        assert!(text.contains("Global rules (inherited)"));
        assert!(text.contains("Press g or edit a rule to create group-specific rules"));
        assert!(text.contains("Effective Source: Manual (Global rules)"));
    }

    #[test]
    fn test_render_happ_profiles_popup() {
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut term = ratatui::Terminal::new(backend).unwrap();
        let cfg = RoutingConfig::default();
        let group = GroupWithProfiles {
            group: Group {
                id: 1,
                name: "Happ Group".into(),
                routing_mode: "subscription".into(),
                active_routing_profile_id: "happ-1".into(),
                ..Default::default()
            },
            profiles: Vec::new(),
            routing_profiles: vec![RoutingProfileInfo {
                id: "happ-1".into(),
                name: "My Happ Rules".into(),
                state: "ready".into(),
                active: true,
                rules_count: 42,
                domain_strategy: "IPIfNonMatch".into(),
                remote_dns: "DoH cloudflare-dns.com".into(),
                domestic_dns: "DoU 8.8.8.8".into(),
                fake_dns_enabled: true,
                ..Default::default()
            }],
        };

        term.draw(|f| {
            render_routing_popup(
                f,
                &RoutingPopup::HappProfiles { cursor: 0 },
                f.area(),
                &cfg,
                Some(&group),
            );
        })
        .unwrap();

        let text = term
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol().to_string())
            .collect::<Vec<_>>()
            .join("");
        assert!(text.contains("Happ Routing Profiles"));
        assert!(text.contains("[● Active]"));
        assert!(text.contains("[ready]"));
        assert!(text.contains("My Happ Rules"));
        assert!(text.contains("rules: 42"));
        assert!(text.contains("strategy: IPIfNonMatch"));
        assert!(text.contains("DNS: DoH cloudflare-dns.com │ DoU 8.8.8.8 │ FakeDNS: on"));
    }

    #[test]
    fn test_truncate_dns_label() {
        assert_eq!(truncate_dns_label("DoH dns.google"), "DoH dns.google");

        let long = "DoH a-very-long-doh-provider-hostname.example";
        let truncated = truncate_dns_label(long);
        assert_eq!(truncated.chars().count(), 28);
        assert!(truncated.ends_with('…'));
        assert!(truncated.starts_with("DoH a-very-long"));
    }

    #[test]
    fn test_render_happ_profiles_popup_with_error() {
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut term = ratatui::Terminal::new(backend).unwrap();
        let cfg = RoutingConfig::default();
        let group = GroupWithProfiles {
            group: Group {
                id: 1,
                name: "Happ Group".into(),
                routing_mode: "subscription".into(),
                active_routing_profile_id: "happ-1".into(),
                ..Default::default()
            },
            profiles: Vec::new(),
            routing_profiles: vec![RoutingProfileInfo {
                id: "happ-1".into(),
                name: "Broken Happ Rules".into(),
                state: "error".into(),
                active: true,
                rules_count: 10,
                last_error: "downloading geosite failed: connection timed out to cdn.jsdelivr.net"
                    .into(),
                ..Default::default()
            }],
        };

        term.draw(|f| {
            render_routing_popup(
                f,
                &RoutingPopup::HappProfiles { cursor: 0 },
                f.area(),
                &cfg,
                Some(&group),
            );
        })
        .unwrap();

        let text = term
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol().to_string())
            .collect::<Vec<_>>()
            .join("");
        assert!(text.contains("Error Details"));
        assert!(text.contains("downloading geosite failed"));
        assert!(text.contains("connection timed out"));
    }
}
