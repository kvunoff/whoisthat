use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};

use crate::settings::registry::{self, Kind, SettingsId};
use crate::ui::App;

use super::theme::*;

#[derive(Debug, Clone)]
pub enum SettingsRow {
    Header(&'static str),
    Item { id: SettingsId },
}

/// Flat layout used until the settings tree lands. Headers keep their old
/// positions so commit 1 does not change the screen.
const LEGACY_HEADERS: &[(usize, &str)] = &[
    (0, "Startup"),
    (3, "Display"),
    (7, "Network"),
    (10, "Diagnostics"),
    (16, "Hardware"),
    (20, "Updates"),
    (21, "Anti-DPI"),
];

pub fn settings_layout() -> Vec<SettingsRow> {
    let mut rows = Vec::with_capacity(registry::all().count() + LEGACY_HEADERS.len());
    for (i, def) in registry::all().enumerate() {
        if let Some((_, title)) = LEGACY_HEADERS.iter().find(|(pos, _)| *pos == i) {
            rows.push(SettingsRow::Header(title));
        }
        rows.push(SettingsRow::Item { id: def.id });
    }
    rows
}

/// Human-readable label for a split-tunnel mode value.
pub fn split_tunnel_label(mode: &str) -> &'static str {
    match mode {
        "exclude" => "exclude (apps bypass)",
        "include" => "include (only apps)",
        _ => "off",
    }
}

/// The next split-tunnel mode when the setting is cycled.
pub fn next_split_tunnel_mode(mode: &str) -> &'static str {
    match mode {
        "off" => "exclude",
        "exclude" => "include",
        _ => "off",
    }
}

pub fn item_count() -> usize {
    registry::all().count()
}

pub fn id_at(cursor: usize) -> SettingsId {
    registry::all()
        .nth(cursor)
        .map(|d| d.id)
        .unwrap_or(SettingsId::Autoconnect)
}

/// Rendered value for one row. Values live here, not in the registry: the
/// registry must not depend on `ui::App`.
pub fn value_for(id: SettingsId, app: &App) -> String {
    let on_off = |on: bool| if on { "● on" } else { "○ off" }.to_string();
    let unsupported = || "unsupported (core)".to_string();
    match id {
        SettingsId::Autoconnect => on_off(app.autoconnect_enabled),
        SettingsId::AutoconnectMode => app.autoconnect_mode.clone(),
        SettingsId::Autostart => {
            if !app.autostart_available {
                "unavailable".into()
            } else if app.autostart_enabled {
                format!("● {}", app.autostart_backend.as_deref().unwrap_or("off"))
            } else {
                "○ off".into()
            }
        }
        SettingsId::Theme => crate::ui::theme::current_theme().name.to_string(),
        SettingsId::ShowIp => on_off(app.show_ip),
        SettingsId::VerboseTuiLog => on_off(app.log_enabled),
        SettingsId::LogLevel => app.log_level.clone(),
        SettingsId::TunName => app.tun_name.clone(),
        SettingsId::KillSwitch => {
            if app.kill_switch_enabled {
                if app.kill_switch_armed {
                    "● on".into()
                } else {
                    "▲ on (not armed)".into()
                }
            } else {
                "○ off".into()
            }
        }
        SettingsId::SplitTunnel => split_tunnel_label(&app.split_tunnel).to_string(),
        SettingsId::TestMethod => app.test_method.clone(),
        SettingsId::TestSamples => app.test_config.samples_per_test.to_string(),
        SettingsId::TestConcurrency => app.test_config.concurrency.to_string(),
        SettingsId::TestTimeout => format!("{}s", app.test_config.timeout_seconds),
        SettingsId::TestEndpoint => match app.test_config.test_endpoint.as_str() {
            "https://cp.cloudflare.com/generate_204" => "cloudflare".into(),
            "https://www.gstatic.com/generate_204" => "gstatic".into(),
            "https://www.bing.com/" => "bing".into(),
            other => other.to_string(),
        },
        SettingsId::AutoTestOnRefresh => on_off(app.test_config.auto_test_on_subscribe),
        SettingsId::HwidEnabled => on_off(app.hwid_info.as_ref().is_some_and(|h| h.enabled)),
        SettingsId::Hwid => app
            .hwid_info
            .as_ref()
            .map(|h| h.hwid.clone())
            .unwrap_or_default(),
        SettingsId::ResetHwid => "⏎".into(),
        SettingsId::UserAgent => app
            .hwid_info
            .as_ref()
            .map(|h| h.user_agent.clone())
            .unwrap_or_default(),
        SettingsId::CheckUpdates => on_off(app.check_updates),
        SettingsId::FragmentEnabled => app
            .anti_dpi
            .as_ref()
            .map(|a| on_off(a.fragment.enabled))
            .unwrap_or_else(unsupported),
        SettingsId::FragmentPackets => app
            .anti_dpi
            .as_ref()
            .map(|a| a.fragment.packets.clone())
            .unwrap_or_else(unsupported),
        SettingsId::FragmentLength => app
            .anti_dpi
            .as_ref()
            .map(|a| a.fragment.length.clone())
            .unwrap_or_else(unsupported),
        SettingsId::FragmentInterval => app
            .anti_dpi
            .as_ref()
            .map(|a| a.fragment.interval.clone())
            .unwrap_or_else(unsupported),
        SettingsId::NoiseEnabled => app
            .anti_dpi
            .as_ref()
            .map(|a| on_off(a.noise.enabled))
            .unwrap_or_else(unsupported),
        SettingsId::NoiseType => app
            .anti_dpi
            .as_ref()
            .map(|a| a.noise.noise_type.clone())
            .unwrap_or_else(unsupported),
        SettingsId::NoisePacket => app
            .anti_dpi
            .as_ref()
            .map(|a| a.noise.packet.clone())
            .unwrap_or_else(unsupported),
        SettingsId::NoiseDelay => app
            .anti_dpi
            .as_ref()
            .map(|a| a.noise.delay.clone())
            .unwrap_or_else(unsupported),
    }
}

#[derive(Debug, Clone)]
pub struct SettingsState {
    pub list_state: ListState,
    pub item_cursor: usize,
    pub scroll: usize,
}

impl SettingsState {
    pub fn new() -> Self {
        let mut s = Self {
            list_state: ListState::default(),
            item_cursor: 0,
            scroll: 0,
        };
        s.list_state.select(Some(1));
        s
    }

    pub fn cursor_up(&mut self) {
        if self.item_cursor > 0 {
            self.item_cursor -= 1;
        }
    }

    pub fn cursor_down(&mut self) {
        let max = item_count().saturating_sub(1);
        if self.item_cursor < max {
            self.item_cursor += 1;
        }
    }

    pub fn cursor(&self) -> usize {
        self.item_cursor
    }

    pub fn flat_index(&self) -> usize {
        let layout = settings_layout();
        let mut item_idx = 0;
        for (i, row) in layout.iter().enumerate() {
            if matches!(row, SettingsRow::Item { .. }) {
                if item_idx == self.item_cursor {
                    return i;
                }
                item_idx += 1;
            }
        }
        0
    }
}

pub fn render_settings(
    f: &mut Frame,
    area: Rect,
    values: &[String],
    state: &mut SettingsState,
    focused: bool,
) {
    let border_color = if focused { border_active() } else { border() };

    let block = Block::default()
        .title(" Settings ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(s_bg());

    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    let content_area = rows[0];

    let layout = settings_layout();
    let selected_flat = state.flat_index();

    let mut value_idx = 0usize;
    let items: Vec<ListItem> = layout
        .iter()
        .enumerate()
        .map(|(flat_i, row)| match row {
            SettingsRow::Header(title) => ListItem::new(Line::from(vec![Span::styled(
                format!(" {} ", title),
                s_accent(),
            )])),
            SettingsRow::Item { id } => {
                let def = registry::by_id(*id);
                let label = def.label;
                let kind = def.kind;
                let val = &values[value_idx];
                let is_selected = flat_i == selected_flat && focused;
                let val_style = match kind {
                    Kind::Toggle => {
                        if val.starts_with("●") {
                            s_success()
                        } else {
                            s_disconnected()
                        }
                    }
                    Kind::Cycle => s_accent(),
                    Kind::Editable => s_text(),
                    Kind::Action => s_accent(),
                    Kind::Display => s_dim(),
                };
                let extra = match kind {
                    Kind::Editable => "  ✎",
                    _ => "",
                };
                let prefix = if is_selected { "> " } else { "  " };
                value_idx += 1;
                let label_w = if content_area.width < 50 { 16 } else { 22 };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        prefix,
                        if is_selected {
                            s_accent_bold()
                        } else {
                            s_faint()
                        },
                    ),
                    Span::styled(
                        format!("{:<width$}", label, width = label_w),
                        if is_selected { s_text() } else { s_dim() },
                    ),
                    Span::styled(val.clone(), val_style),
                    Span::styled(extra, s_faint()),
                ]))
            }
        })
        .collect();

    // Keep cursor visible by adjusting scroll (mirrors tree.rs)
    let visible = content_area.height.saturating_sub(1) as usize;
    if state.item_cursor == 0 {
        state.scroll = 0;
    } else if selected_flat < state.scroll {
        state.scroll = selected_flat;
    } else if visible > 0 && selected_flat >= state.scroll + visible {
        state.scroll = selected_flat.saturating_sub(visible).saturating_add(1);
    }

    let mut list_state = ListState::default()
        .with_selected(Some(selected_flat))
        .with_offset(state.scroll);

    let list = List::new(items)
        .highlight_style(if focused {
            Style::default().bg(surface())
        } else {
            Style::default()
        })
        .scroll_padding(3);

    f.render_stateful_widget(list, content_area, &mut list_state);
    state.scroll = list_state.offset();
    state.list_state = list_state;

    let help_text = if area.width < 60 {
        " j/k:nav │ Enter/h/l:change"
    } else {
        " j/k navigate  │  Enter/Space/h/l toggle / cycle / edit"
    };
    let help = Paragraph::new(help_text)
        .style(s_faint())
        .alignment(Alignment::Center);
    f.render_widget(help, rows[1]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_for_covers_every_row() {
        use crate::core_client::protocol::TestConfig;
        let app = App::new(
            true,
            false,
            "warn".into(),
            "http-get".into(),
            "whoisthattun".into(),
            false,
            TestConfig::default(),
        );
        for (i, def) in registry::all().enumerate() {
            assert_eq!(id_at(i), def.id);
            let _ = value_for(def.id, &app);
        }
        assert_eq!(id_at(usize::MAX), SettingsId::Autoconnect);
    }

    #[test]
    fn test_settings_layout_item_count() {
        assert_eq!(item_count(), 29);
    }

    #[test]
    fn test_settings_layout_has_headers() {
        let layout = settings_layout();
        let headers: Vec<&str> = layout
            .iter()
            .filter_map(|r| match r {
                SettingsRow::Header(t) => Some(*t),
                _ => None,
            })
            .collect();
        assert_eq!(
            headers,
            vec![
                "Startup",
                "Display",
                "Network",
                "Diagnostics",
                "Hardware",
                "Updates",
                "Anti-DPI"
            ]
        );
    }

    #[test]
    fn test_settings_layout_anti_dpi_rows_stay_last() {
        let layout = settings_layout();
        let labels: Vec<&str> = layout
            .iter()
            .filter_map(|r| match r {
                SettingsRow::Item { id } => Some(registry::by_id(*id).label),
                _ => None,
            })
            .collect();
        assert_eq!(
            &labels[21..],
            &[
                "Fragment: enabled",
                "Fragment: packets",
                "Fragment: length",
                "Fragment: interval",
                "Noise: enabled",
                "Noise: type",
                "Noise: packet",
                "Noise: delay",
            ]
        );
    }

    #[test]
    fn test_settings_cursor_flat_index_skips_headers() {
        let mut s = SettingsState::new();
        assert_eq!(s.flat_index(), 1);

        s.cursor_down();
        assert_eq!(s.item_cursor, 1);
        assert_eq!(s.flat_index(), 2);

        s.cursor_down();
        assert_eq!(s.item_cursor, 2);
        assert_eq!(s.flat_index(), 3);

        s.cursor_down();
        assert_eq!(s.item_cursor, 3);
        assert_eq!(s.flat_index(), 5);

        s.cursor_down();
        assert_eq!(s.item_cursor, 4);
        assert_eq!(s.flat_index(), 6);
    }

    #[test]
    fn test_settings_cursor_clamps_at_bottom() {
        let max = item_count().saturating_sub(1);
        let mut s = SettingsState::new();
        for _ in 0..max + 5 {
            s.cursor_down();
        }
        assert_eq!(s.item_cursor, max);
    }

    #[test]
    fn test_settings_cursor_clamps_at_top() {
        let mut s = SettingsState::new();
        s.cursor_down();
        s.cursor_down();
        s.cursor_up();
        s.cursor_up();
        s.cursor_up();
        assert_eq!(s.item_cursor, 0);
    }

    #[test]
    fn test_settings_scroll_keeps_cursor_visible() {
        let mut s = SettingsState::new();
        let visible_height = 10usize;
        let visible = visible_height.saturating_sub(1);

        for _ in 0..item_count() {
            let flat = s.flat_index();
            if s.item_cursor == 0 {
                s.scroll = 0;
            } else if flat < s.scroll {
                s.scroll = flat;
            } else if visible > 0 && flat >= s.scroll + visible {
                s.scroll = flat.saturating_sub(visible).saturating_add(1);
            }
            assert!(flat >= s.scroll);
            assert!(flat <= s.scroll + visible);
            s.cursor_down();
        }
    }
}
