use std::collections::HashSet;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};

use crate::settings::registry::{self, GroupId, Kind, SectionId, SettingsId};
use crate::ui::App;
use crate::ui::layout::WidthTier;

use super::theme::*;

/// Geometry shared by the renderer and the mouse handler. A local calculation
/// in either place would make clicks land on the wrong row.
#[derive(Debug, Clone, Copy)]
pub struct SettingsLayout {
    pub tree: Rect,
    pub items: Rect,
    pub compact: bool,
}

pub struct SettingsView<'a> {
    pub values: &'a [String],
    pub focused_tree: bool,
    pub focused_items: bool,
    pub search_query: Option<&'a str>,
    pub width_tier: WidthTier,
}

pub fn settings_layout(area: Rect, width_tier: WidthTier) -> SettingsLayout {
    if area.width < 60 || width_tier == WidthTier::Compact {
        return SettingsLayout {
            tree: Rect::default(),
            items: area,
            compact: true,
        };
    }
    let parts = Layout::horizontal([Constraint::Length(24), Constraint::Min(0)]).split(area);
    SettingsLayout {
        tree: parts[0],
        items: parts[1],
        compact: false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTreeRow {
    Section(SectionId),
    Group(SectionId, GroupId),
}

pub fn visible_rows(expanded: &HashSet<SectionId>) -> Vec<SettingsTreeRow> {
    let mut rows = Vec::new();
    for section in registry::sections() {
        rows.push(SettingsTreeRow::Section(section.id));
        if expanded.contains(&section.id) {
            for group in registry::groups(section.id) {
                rows.push(SettingsTreeRow::Group(section.id, group.id));
            }
        }
    }
    rows
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemRow {
    SubHeader(&'static str),
    Item(SettingsId),
}

/// Rows of the right panel. Shared with the mouse handler so clicks and the
/// renderer never disagree about subheader positions.
pub fn item_rows(section: SectionId, group: GroupId) -> Vec<ItemRow> {
    let mut rows = Vec::new();
    for (i, def) in registry::items(section, group).enumerate() {
        if group == GroupId::AntiDpiMasks && (i == 0 || i == 4) {
            rows.push(ItemRow::SubHeader(if i == 0 {
                "Fragment"
            } else {
                "Noise"
            }));
        }
        rows.push(ItemRow::Item(def.id));
    }
    rows
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactRow {
    Section(&'static str),
    Group(&'static str),
    Item(SettingsId),
}

/// Rows of the single-list Compact layout. Also shared with the mouse handler.
pub fn compact_rows() -> Vec<CompactRow> {
    let mut rows = Vec::new();
    let mut last_section: Option<SectionId> = None;
    let mut last_group: Option<GroupId> = None;
    for def in registry::all() {
        if last_section != Some(def.section) {
            let title = registry::sections()
                .iter()
                .find(|s| s.id == def.section)
                .map(|s| s.title)
                .unwrap_or("");
            rows.push(CompactRow::Section(title));
            last_section = Some(def.section);
            last_group = None;
        }
        if last_group != Some(def.group) {
            let title = registry::groups(def.section)
                .iter()
                .find(|g| g.id == def.group)
                .map(|g| g.title)
                .unwrap_or("");
            rows.push(CompactRow::Group(title));
            last_group = Some(def.group);
        }
        rows.push(CompactRow::Item(def.id));
    }
    rows
}

#[derive(Debug, Clone)]
pub struct SettingsState {
    pub tree_cursor: usize,
    pub tree_scroll: usize,
    pub expanded: HashSet<SectionId>,
    pub section: SectionId,
    pub group: GroupId,
    pub item_cursor: usize,
    pub item_scroll: usize,
    pub compact_cursor: usize,
    pub search_cursor: usize,
}

impl SettingsState {
    pub fn new() -> Self {
        let first = registry::sections().first().copied().expect("sections");
        let group = registry::groups(first.id)
            .first()
            .copied()
            .expect("first group")
            .id;
        let mut expanded = HashSet::new();
        expanded.insert(first.id);
        Self {
            tree_cursor: 1,
            tree_scroll: 0,
            expanded,
            section: first.id,
            group,
            item_cursor: 0,
            item_scroll: 0,
            compact_cursor: 0,
            search_cursor: 0,
        }
    }

    pub fn tree_rows(&self) -> Vec<SettingsTreeRow> {
        visible_rows(&self.expanded)
    }

    pub fn tree_down(&mut self) {
        let max = self.tree_rows().len().saturating_sub(1);
        if self.tree_cursor < max {
            self.tree_cursor += 1;
        }
    }

    pub fn tree_up(&mut self) {
        self.tree_cursor = self.tree_cursor.saturating_sub(1);
    }

    /// `Enter` on a section toggles it; `Enter` on a group selects it.
    pub fn activate_tree(&mut self) {
        let Some(row) = self.tree_rows().get(self.tree_cursor).copied() else {
            return;
        };
        match row {
            SettingsTreeRow::Section(id) => {
                if !self.expanded.remove(&id) {
                    self.expanded.insert(id);
                }
            }
            SettingsTreeRow::Group(section, group) => {
                self.section = section;
                self.group = group;
                self.item_cursor = 0;
                self.item_scroll = 0;
                self.sync_tree_cursor_to_selection();
            }
        }
    }

    pub fn collapse_or_expand(&mut self, expand: bool) {
        let Some(row) = self.tree_rows().get(self.tree_cursor).copied() else {
            return;
        };
        if let SettingsTreeRow::Section(id) = row {
            if expand {
                self.expanded.insert(id);
            } else {
                self.expanded.remove(&id);
            }
        }
    }

    fn sync_tree_cursor_to_selection(&mut self) {
        let rows = self.tree_rows();
        if let Some(i) = rows
            .iter()
            .position(|r| *r == SettingsTreeRow::Group(self.section, self.group))
        {
            self.tree_cursor = i;
        }
    }

    pub fn item_down(&mut self, compact: bool) {
        if compact {
            self.compact_cursor = (self.compact_cursor + 1).min(registry::all().count() - 1);
            return;
        }
        let max = registry::items(self.section, self.group)
            .count()
            .saturating_sub(1);
        if self.item_cursor < max {
            self.item_cursor += 1;
        }
    }

    pub fn item_up(&mut self, compact: bool) {
        if compact {
            self.compact_cursor = self.compact_cursor.saturating_sub(1);
            return;
        }
        self.item_cursor = self.item_cursor.saturating_sub(1);
    }

    pub fn current_id(&self, compact: bool) -> SettingsId {
        if compact {
            return id_at(self.compact_cursor);
        }
        registry::items(self.section, self.group)
            .nth(self.item_cursor)
            .map(|d| d.id)
            .unwrap_or(SettingsId::Autoconnect)
    }

    pub fn search_matches(&self, query: &str) -> Vec<SettingsId> {
        let needle = query.to_lowercase();
        registry::all()
            .filter(|d| {
                let section = registry::sections()
                    .iter()
                    .find(|s| s.id == d.section)
                    .map(|s| s.title)
                    .unwrap_or("");
                let group = registry::groups(d.section)
                    .iter()
                    .find(|g| g.id == d.group)
                    .map(|g| g.title)
                    .unwrap_or("");
                format!("{} {} {}", d.label, section, group)
                    .to_lowercase()
                    .contains(&needle)
            })
            .map(|d| d.id)
            .collect()
    }

    pub fn clamp_search_cursor(&mut self, len: usize) {
        self.search_cursor = self.search_cursor.min(len.saturating_sub(1));
    }

    pub fn apply_search_result(&mut self, id: SettingsId) {
        let def = registry::by_id(id);
        self.expanded.insert(def.section);
        self.section = def.section;
        self.group = def.group;
        self.item_cursor = registry::items(def.section, def.group)
            .position(|d| d.id == id)
            .unwrap_or(0);
        self.compact_cursor = registry::all().position(|d| d.id == id).unwrap_or(0);
        self.item_scroll = 0;
        self.sync_tree_cursor_to_selection();
    }
}

pub fn id_at(cursor: usize) -> SettingsId {
    registry::all()
        .nth(cursor)
        .map(|d| d.id)
        .unwrap_or(SettingsId::Autoconnect)
}

pub fn value_index(id: SettingsId) -> usize {
    registry::all()
        .position(|d| d.id == id)
        .expect("every id is in the registry")
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

fn row_line(
    prefix: &str,
    label: &str,
    value: &str,
    kind: Kind,
    selected: bool,
    width: usize,
) -> ListItem<'static> {
    let val_style = match kind {
        Kind::Toggle => {
            if value.starts_with('●') {
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
    let extra = if kind == Kind::Editable { "  ✎" } else { "" };
    ListItem::new(Line::from(vec![
        Span::styled(
            prefix.to_string(),
            if selected { s_accent_bold() } else { s_faint() },
        ),
        Span::styled(
            format!("{:<width$}", label, width = width),
            if selected { s_text() } else { s_dim() },
        ),
        Span::styled(value.to_string(), val_style),
        Span::styled(extra, s_faint()),
    ]))
}

fn panel_block(title: String, focused: bool) -> Block<'static> {
    Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused { border_active() } else { border() }))
        .style(s_bg())
}

fn render_list(
    f: &mut Frame,
    area: Rect,
    items: Vec<ListItem<'static>>,
    selected: Option<usize>,
    scroll: &mut usize,
    focused: bool,
) {
    let visible = area.height.saturating_sub(1) as usize;
    if let Some(sel) = selected {
        if sel < *scroll {
            *scroll = sel;
        } else if visible > 0 && sel >= *scroll + visible {
            *scroll = sel.saturating_sub(visible).saturating_add(1);
        }
    }
    let mut list_state = ListState::default()
        .with_selected(selected)
        .with_offset(*scroll);
    let list = List::new(items)
        .highlight_style(if focused {
            Style::default().bg(surface())
        } else {
            Style::default()
        })
        .scroll_padding(3);
    f.render_stateful_widget(list, area, &mut list_state);
    *scroll = list_state.offset();
}

pub fn render_settings(
    f: &mut Frame,
    area: Rect,
    state: &mut SettingsState,
    view: &SettingsView<'_>,
) {
    let layout = settings_layout(area, view.width_tier);
    let value = |id: SettingsId| view.values[value_index(id)].clone();

    if let Some(query) = view.search_query {
        render_search(f, layout.items, state, query, view.focused_items, &value);
        return;
    }

    if layout.compact {
        render_compact(f, layout.items, state, view.focused_items, &value);
        return;
    }

    let tree_block = panel_block("Sections".into(), view.focused_tree);
    let tree_inner = tree_block.inner(layout.tree);
    f.render_widget(tree_block, layout.tree);
    let tree_rows = state.tree_rows();
    let tree_items: Vec<ListItem<'static>> = tree_rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let (label, style) = match row {
                SettingsTreeRow::Section(id) => {
                    let title = registry::sections()
                        .iter()
                        .find(|s| s.id == *id)
                        .map(|s| s.title)
                        .unwrap_or("");
                    let mark = if state.expanded.contains(id) {
                        "▾ "
                    } else {
                        "▸ "
                    };
                    (format!("{mark}{title}"), s_accent())
                }
                SettingsTreeRow::Group(section, id) => {
                    let title = registry::groups(*section)
                        .iter()
                        .find(|g| g.id == *id)
                        .map(|g| g.title)
                        .unwrap_or("");
                    (format!("   {title}"), s_dim())
                }
            };
            let style = if i == state.tree_cursor {
                s_accent_bold()
            } else {
                style
            };
            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();
    let tree_split = Layout::vertical([Constraint::Min(0)]).split(tree_inner);
    render_list(
        f,
        tree_split[0],
        tree_items,
        Some(state.tree_cursor),
        &mut state.tree_scroll,
        view.focused_tree,
    );

    let section_title = registry::sections()
        .iter()
        .find(|s| s.id == state.section)
        .map(|s| s.title)
        .unwrap_or("");
    let group_title = registry::groups(state.section)
        .iter()
        .find(|g| g.id == state.group)
        .map(|g| g.title)
        .unwrap_or("");
    let item_block = panel_block(
        format!("{section_title} / {group_title}"),
        view.focused_items,
    );
    let item_inner = item_block.inner(layout.items);
    f.render_widget(item_block, layout.items);
    let item_split =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(item_inner);
    let content_area = item_split[0];

    let rows = item_rows(state.section, state.group);
    let current = state.current_id(false);
    let label_w = if content_area.width < 50 { 16 } else { 22 };
    let items: Vec<ListItem<'static>> = rows
        .iter()
        .map(|row| match row {
            ItemRow::SubHeader(title) => {
                ListItem::new(Line::from(Span::styled(format!(" {title} "), s_accent())))
            }
            ItemRow::Item(id) => {
                let def = registry::by_id(*id);
                row_line(
                    if *id == current { "> " } else { "  " },
                    def.label,
                    &value(*id),
                    def.kind,
                    *id == current,
                    label_w,
                )
            }
        })
        .collect();
    let selected = rows
        .iter()
        .position(|r| *r == ItemRow::Item(current))
        .unwrap_or(0);
    render_list(
        f,
        content_area,
        items,
        Some(selected),
        &mut state.item_scroll,
        view.focused_items,
    );

    let help_text = if area.width < 60 {
        " j/k:nav │ Tab:sections │ /:search"
    } else {
        " j/k navigate  │  Tab panels  │  Enter change  │  / search"
    };
    f.render_widget(
        Paragraph::new(help_text)
            .style(s_faint())
            .alignment(Alignment::Center),
        item_split[1],
    );
}

fn render_compact(
    f: &mut Frame,
    area: Rect,
    state: &mut SettingsState,
    focused: bool,
    value: &impl Fn(SettingsId) -> String,
) {
    let block = panel_block("Settings".into(), focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let split = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    let content_area = split[0];
    let label_w = if content_area.width < 50 { 16 } else { 22 };

    let rows = compact_rows();
    let current = state.current_id(true);
    let items: Vec<ListItem<'static>> = rows
        .iter()
        .map(|row| match row {
            CompactRow::Section(title) => ListItem::new(Line::from(Span::styled(
                format!(" ── {title} ── "),
                s_accent(),
            ))),
            CompactRow::Group(title) => {
                ListItem::new(Line::from(Span::styled(format!("   {title}"), s_dim())))
            }
            CompactRow::Item(id) => {
                let def = registry::by_id(*id);
                row_line(
                    if *id == current { "> " } else { "  " },
                    def.label,
                    &value(*id),
                    def.kind,
                    *id == current,
                    label_w,
                )
            }
        })
        .collect();
    let selected = rows
        .iter()
        .position(|r| *r == CompactRow::Item(current))
        .unwrap_or(0);
    render_list(
        f,
        content_area,
        items,
        Some(selected),
        &mut state.item_scroll,
        focused,
    );
    f.render_widget(
        Paragraph::new(" j/k navigate  │  Enter change  │  / search")
            .style(s_faint())
            .alignment(Alignment::Center),
        split[1],
    );
}

fn render_search(
    f: &mut Frame,
    area: Rect,
    state: &mut SettingsState,
    query: &str,
    focused: bool,
    value: &impl Fn(SettingsId) -> String,
) {
    let matches = state.search_matches(query);
    state.clamp_search_cursor(matches.len());
    let block = panel_block(format!("Search: /{query}"), focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let split = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    let label_w = if split[0].width < 50 { 16 } else { 22 };
    let items: Vec<ListItem<'static>> = matches
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let def = registry::by_id(*id);
            row_line(
                if i == state.search_cursor { "> " } else { "  " },
                def.label,
                &value(*id),
                def.kind,
                i == state.search_cursor,
                label_w,
            )
        })
        .collect();
    let selected = if matches.is_empty() {
        None
    } else {
        Some(state.search_cursor)
    };
    render_list(
        f,
        split[0],
        items,
        selected,
        &mut state.item_scroll,
        focused,
    );
    f.render_widget(
        Paragraph::new(" Enter: jump  │  Esc: clear")
            .style(s_faint())
            .alignment(Alignment::Center),
        split[1],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_client::protocol::TestConfig;

    fn app() -> App {
        App::new(
            true,
            false,
            "warn".into(),
            "http-get".into(),
            "whoisthattun".into(),
            false,
            TestConfig::default(),
        )
    }

    #[test]
    fn value_for_covers_every_row() {
        let app = app();
        for (i, def) in registry::all().enumerate() {
            assert_eq!(id_at(i), def.id);
            assert_eq!(value_index(def.id), i);
            let _ = value_for(def.id, &app);
        }
        assert_eq!(id_at(usize::MAX), SettingsId::Autoconnect);
    }

    #[test]
    fn tree_starts_on_the_first_group() {
        let s = SettingsState::new();
        assert_eq!(s.section, SectionId::Connection);
        assert_eq!(s.group, GroupId::Autoconnect);
        assert_eq!(
            s.tree_rows()[s.tree_cursor],
            SettingsTreeRow::Group(SectionId::Connection, GroupId::Autoconnect)
        );
        assert!(
            !registry::items(s.section, s.group)
                .collect::<Vec<_>>()
                .is_empty()
        );
    }

    #[test]
    fn expanding_sections_changes_visible_rows() {
        let mut s = SettingsState::new();
        let initial = s.tree_rows().len();
        s.expanded.insert(SectionId::Network);
        assert!(s.tree_rows().len() > initial);
        assert!(s.tree_rows().contains(&SettingsTreeRow::Group(
            SectionId::Network,
            GroupId::Interface
        )));
    }

    #[test]
    fn item_cursor_stays_inside_the_group() {
        let mut s = SettingsState::new();
        for _ in 0..20 {
            s.item_down(false);
        }
        assert_eq!(
            s.item_cursor,
            registry::items(s.section, s.group).count() - 1
        );
        for _ in 0..20 {
            s.item_up(false);
        }
        assert_eq!(s.item_cursor, 0);
    }

    #[test]
    fn compact_cursor_walks_the_whole_registry() {
        let mut s = SettingsState::new();
        for _ in 0..100 {
            s.item_down(true);
        }
        assert_eq!(s.current_id(true), registry::all().last().unwrap().id);
        for _ in 0..100 {
            s.item_up(true);
        }
        assert_eq!(s.current_id(true), SettingsId::Autoconnect);
    }

    #[test]
    fn search_matches_labels_sections_and_groups() {
        let s = SettingsState::new();
        assert!(s.search_matches("KILL").contains(&SettingsId::KillSwitch));
        assert!(s.search_matches("network").contains(&SettingsId::TunName));
        assert!(
            s.search_matches("engine")
                .contains(&SettingsId::TestSamples)
        );
        assert!(s.search_matches("zzzz").is_empty());
    }

    #[test]
    fn applying_a_search_result_reveals_its_path() {
        let mut s = SettingsState::new();
        s.apply_search_result(SettingsId::NoiseDelay);
        assert!(s.expanded.contains(&SectionId::Network));
        assert_eq!(s.group, GroupId::AntiDpiMasks);
        assert_eq!(s.current_id(false), SettingsId::NoiseDelay);
        assert_eq!(
            s.tree_rows()[s.tree_cursor],
            SettingsTreeRow::Group(SectionId::Network, GroupId::AntiDpiMasks)
        );
    }

    #[test]
    fn settings_layout_shared_geometry() {
        let wide = settings_layout(Rect::new(0, 0, 100, 30), WidthTier::Medium);
        assert!(!wide.compact);
        assert!(wide.tree.width > 0 && wide.items.width > 0);
        let narrow = settings_layout(Rect::new(0, 0, 79, 30), WidthTier::Compact);
        assert!(narrow.compact);
        assert_eq!(narrow.items, Rect::new(0, 0, 79, 30));
    }
}
