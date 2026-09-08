use std::cell::RefCell;
use std::collections::HashSet;

use ratatui::layout::Rect;

use crate::config;
use crate::core_client::protocol::*;

use crate::ui::logs::LogsState;
use crate::ui::routing::RoutingPopup;
use crate::ui::settings::SettingsState;
use crate::ui::traffic::TrafficHistory;
use crate::ui::uri::{self, ParsedUri};

use super::types::*;

pub struct App {
    pub groups: Vec<GroupWithProfiles>,
    pub connection_status: ProxyStatus,
    pub tun_enabled: bool,
    pub last_msg: Option<String>,
    pub show_ip: bool,
    pub log_enabled: bool,
    pub log_level: String,
    pub test_method: String,
    pub tun_name: String,
    pub hwid_info: Option<HwidData>,
    pub kill_switch_enabled: bool,
    pub split_tunnel: String,
    pub autoconnect_enabled: bool,
    pub autostart_mode: String,
    pub systemd_enabled: bool,
    pub public_ip: String,
    pub public_ipv6: String,

    pub tab: ActiveTab,
    pub focus: Focus,
    pub cursor: usize,
    pub tree_scroll: usize,
    pub details_scroll: usize,
    pub popup: Option<Popup>,
    pub help_scroll: usize,

    pub settings_state: SettingsState,
    pub logs_state: LogsState,
    pub traffic_stats: TrafficStats,
    pub traffic_history: TrafficHistory,
    pub routing: RoutingConfig,
    pub routing_cursor: usize,
    pub routing_popup: Option<RoutingPopup>,
    pub search_query: Option<String>,
    pub search_input: String,
    pub search_mode: bool,

    pub test_progress: Option<TestProgress>,
    pub test_config: TestConfig,

    pub collapsed_groups: HashSet<i32>,
    pub last_area: Rect,

    uri_cache: RefCell<Option<(i32, i32, ParsedUri)>>,
}

impl App {
    pub fn new(
        show_ip: bool,
        log_enabled: bool,
        log_level: String,
        test_method: String,
        tun_name: String,
        kill_switch_enabled: bool,
        test_config: TestConfig,
    ) -> Self {
        Self {
            groups: Vec::new(),
            connection_status: ProxyStatus {
                connection: "disconnected".into(),
                profile: None,
                connected_at: 0,
            },
            tun_enabled: false,
            last_msg: None,
            show_ip,
            log_enabled,
            log_level,
            test_method,
            tun_name,
            hwid_info: None,
            kill_switch_enabled,
            split_tunnel: "off".to_string(),
            autoconnect_enabled: false,
            autostart_mode: "proxy".to_string(),
            systemd_enabled: false,
            public_ip: String::new(),
            public_ipv6: String::new(),
            tab: ActiveTab::Profiles,
            focus: Focus::LeftPanel,
            cursor: 0,
            tree_scroll: 0,
            details_scroll: 0,
            popup: None,
            help_scroll: 0,
            settings_state: SettingsState::new(),
            logs_state: LogsState::new(
                config::config_dir()
                    .join("core.log")
                    .to_str()
                    .unwrap_or("core.log"),
            ),
            traffic_stats: TrafficStats::default(),
            traffic_history: TrafficHistory::new(),
            routing: RoutingConfig::default(),
            routing_cursor: 0,
            routing_popup: None,
            search_query: None,
            search_input: String::new(),
            search_mode: false,
            test_progress: None,
            test_config,
            collapsed_groups: HashSet::new(),
            last_area: Rect::default(),
            uri_cache: RefCell::new(None),
        }
    }

    // --- tree helpers ---

    pub fn tree_len(&self) -> usize {
        match &self.search_query {
            None => {
                let mut n = 0;
                for g in &self.groups {
                    n += 1;
                    if !self.collapsed_groups.contains(&g.group.id) {
                        n += g.profiles.len();
                    }
                }
                n
            }
            Some(q) => {
                let ql = q.to_lowercase();
                let mut n = 0;
                for g in &self.groups {
                    for p in &g.profiles {
                        if self.profile_matches(&ql, p) {
                            n += 1;
                        }
                    }
                }
                n
            }
        }
    }

    pub(super) fn profile_matches(&self, query: &str, p: &Profile) -> bool {
        p.name.to_lowercase().contains(query)
            || p.protocol.to_lowercase().contains(query)
            || p.address.to_lowercase().contains(query)
            || p.host.to_lowercase().contains(query)
    }

    pub fn tree_node_at(&self, cursor: usize) -> Option<TreeNode> {
        match &self.search_query {
            None => {
                let mut pos = 0;
                for (gi, g) in self.groups.iter().enumerate() {
                    if pos == cursor {
                        return Some(TreeNode::Group(gi));
                    }
                    pos += 1;
                    if !self.collapsed_groups.contains(&g.group.id) {
                        let plen = g.profiles.len();
                        if cursor < pos + plen {
                            return Some(TreeNode::Profile(gi, cursor - pos));
                        }
                        pos += plen;
                    }
                }
                None
            }
            Some(q) => {
                let ql = q.to_lowercase();
                let mut pos = 0;
                for (gi, g) in self.groups.iter().enumerate() {
                    for (pi, p) in g.profiles.iter().enumerate() {
                        if self.profile_matches(&ql, p) {
                            if pos == cursor {
                                return Some(TreeNode::Profile(gi, pi));
                            }
                            pos += 1;
                        }
                    }
                }
                None
            }
        }
    }

    pub fn clamp_cursor(&mut self) {
        let len = self.tree_len();
        if len == 0 {
            self.cursor = 0;
        } else if self.cursor >= len {
            self.cursor = len - 1;
        }
    }

    // --- helpers (used by main.rs and rendering) ---

    pub fn current_group(&self) -> Option<&GroupWithProfiles> {
        match self.tree_node_at(self.cursor)? {
            TreeNode::Group(gi) => self.groups.get(gi),
            TreeNode::Profile(gi, _) => self.groups.get(gi),
        }
    }

    pub fn selected_profile(&self) -> Option<&Profile> {
        match self.tree_node_at(self.cursor)? {
            TreeNode::Profile(gi, pi) => self.groups.get(gi)?.profiles.get(pi),
            TreeNode::Group(_) => None,
        }
    }

    pub fn on_group(&self) -> bool {
        matches!(self.tree_node_at(self.cursor), Some(TreeNode::Group(_)))
    }

    pub fn tab_index(&self) -> usize {
        match self.tab {
            ActiveTab::Profiles => 0,
            ActiveTab::Routing => 1,
            ActiveTab::Traffic => 2,
            ActiveTab::Logs => 3,
            ActiveTab::Settings => 4,
        }
    }

    pub fn tab_from_index(idx: usize) -> ActiveTab {
        match idx {
            1 => ActiveTab::Routing,
            2 => ActiveTab::Traffic,
            3 => ActiveTab::Logs,
            4 => ActiveTab::Settings,
            _ => ActiveTab::Profiles,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connection_status.connection == "connected"
    }

    pub fn is_connected_hy2(&self) -> bool {
        if !self.is_connected() {
            return false;
        }
        self.connection_status
            .profile
            .as_ref()
            .map(|p| p.protocol == "hysteria2" || p.protocol == "hy2")
            .unwrap_or(false)
    }

    pub fn connected_id(&self) -> Option<(i32, i32)> {
        self.connection_status
            .profile
            .as_ref()
            .map(|p| (p.group_id, p.id))
    }

    pub fn connected_group_name(&self) -> Option<&str> {
        let (gid, _) = self.connected_id()?;
        self.groups
            .iter()
            .find(|g| g.group.id == gid)
            .map(|g| g.group.name.as_str())
    }

    pub fn connected_profile_name(&self) -> Option<&str> {
        self.connection_status
            .profile
            .as_ref()
            .map(|p| p.name.as_str())
    }

    pub fn cursor_down(&mut self) {
        let len = self.tree_len();
        if len == 0 || self.cursor + 1 >= len {
            return;
        }
        self.cursor += 1;
        self.details_scroll = 0;
    }

    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.details_scroll = 0;
        }
    }

    pub fn cursor_top(&mut self) {
        self.cursor = 0;
        self.tree_scroll = 0;
        self.details_scroll = 0;
    }

    pub fn cursor_bottom(&mut self) {
        let len = self.tree_len();
        if len > 0 {
            self.cursor = len - 1;
            self.details_scroll = 0;
        }
    }

    pub fn clear_msg(&mut self) {
        self.last_msg = None;
    }

    pub fn details_scroll_down(&mut self, total: usize, visible: usize) {
        if total > visible {
            self.details_scroll = (self.details_scroll + 1).min(total - visible);
        }
    }

    pub fn details_scroll_up(&mut self) {
        self.details_scroll = self.details_scroll.saturating_sub(1);
    }

    pub fn details_scroll_top(&mut self) {
        self.details_scroll = 0;
    }

    pub fn details_scroll_bottom(&mut self, total: usize, visible: usize) {
        if total > visible {
            self.details_scroll = total - visible;
        }
    }

    pub fn details_line_count(&self) -> usize {
        match self.tree_node_at(self.cursor) {
            Some(TreeNode::Group(gi)) => {
                let g = &self.groups[gi];
                let mut n = 4;
                if !g.group.subscription_url.is_empty() {
                    n += 4;
                }
                if self.connected_group_name() == Some(g.group.name.as_str()) {
                    n += 2;
                }
                n
            }
            Some(TreeNode::Profile(_, _)) => {
                let p = match self.selected_profile() {
                    Some(p) => p,
                    None => return 1,
                };
                let mut n = 22;
                if p.uri.starts_with("ss://") && crate::ui::uri::parse_ss_method(&p.uri).is_some() {
                    n += 1;
                }
                if self.is_connected() {
                    if let Some((gid, pid)) = self.connected_id() {
                        if gid == p.group_id && pid == p.id {
                            n += 1;
                        }
                    }
                }
                if p.test_result > 0 {
                    n += 1;
                    if p.jitter_ms > 0 {
                        n += 1;
                    }
                    if p.loss_pct > 0 {
                        n += 1;
                    }
                } else if p.test_result == -2 || p.test_result == -1 {
                    n += 1;
                }
                if p.tested_at > 0 {
                    n += 1;
                }
                n
            }
            None => 1,
        }
    }

    pub fn details_visible(&self) -> usize {
        30
    }

    pub fn cached_uri_params(&self, p: &Profile) -> ParsedUri {
        let mut cache = self.uri_cache.borrow_mut();
        if let Some((gid, pid, ref params)) = *cache {
            if gid == p.group_id && pid == p.id {
                return params.clone();
            }
        }
        let params = uri::parse_uri(&p.uri);
        *cache = Some((p.group_id, p.id, params.clone()));
        params
    }

    pub fn msg(&mut self, text: impl Into<String>) {
        self.last_msg = Some(text.into());
    }

    // --- state mutations ---

    pub fn apply_state(&mut self, state: ApplicationState) {
        self.groups = state.groups;
        self.connection_status = state.connection_status;
        self.tun_enabled = state.tun_status;
        self.kill_switch_enabled = state.kill_switch;
        if state.split_tunnel.is_empty() {
            self.split_tunnel = "off".to_string();
        } else {
            self.split_tunnel = state.split_tunnel;
        }
        self.autoconnect_enabled = state.autoconnect.enabled;
        self.autostart_mode = state.autoconnect.mode;
        self.hwid_info = state.hwid_info;
        self.clamp_cursor();
    }

    pub fn invalidate_uri_cache(&self) {
        self.uri_cache.borrow_mut().take();
    }

    pub fn apply_profiles_added(&mut self, profiles: Vec<Profile>) {
        self.invalidate_uri_cache();
        for p in profiles {
            if let Some(g) = self.groups.iter_mut().find(|g| g.group.id == p.group_id) {
                g.profiles.push(p);
            }
        }
    }

    pub fn apply_profiles_deleted(&mut self, deleted: &[ProfileID]) {
        self.invalidate_uri_cache();
        for d in deleted {
            if let Some(g) = self.groups.iter_mut().find(|g| g.group.id == d.group_id) {
                g.profiles.retain(|p| p.id != d.id);
            }
        }
        self.clamp_cursor();
    }

    pub fn apply_subscription_updated(&mut self, group: Group, profiles: Vec<Profile>) {
        self.invalidate_uri_cache();
        if let Some(g) = self.groups.iter_mut().find(|g| g.group.id == group.id) {
            g.group = group;
            g.profiles = profiles;
        }
        self.clamp_cursor();
    }

    pub fn apply_profile_updated(&mut self, p: &Profile) {
        self.invalidate_uri_cache();
        if let Some(g) = self.groups.iter_mut().find(|g| g.group.id == p.group_id) {
            if let Some(existing) = g.profiles.iter_mut().find(|pr| pr.id == p.id) {
                *existing = p.clone();
            }
        }
    }

    /// Mark a profile as "test in flight" so the tree immediately shows
    /// `…` instead of the stale last-known value. Cleared when the real
    /// `profile-updated` arrives via `apply_profile_updated`.
    pub fn mark_pending(&mut self, group_id: i32, profile_id: i32) {
        if let Some(g) = self.groups.iter_mut().find(|g| g.group.id == group_id) {
            if let Some(p) = g.profiles.iter_mut().find(|pr| pr.id == profile_id) {
                p.test_result = -2;
            }
        }
    }

    pub fn apply_group_added(&mut self, g: Group) {
        self.groups.push(GroupWithProfiles {
            group: g,
            profiles: Vec::new(),
        });
    }

    pub fn apply_group_deleted(&mut self, id: i32) {
        self.groups.retain(|g| g.group.id != id);
        self.collapsed_groups.remove(&id);
        self.clamp_cursor();
    }

    pub fn apply_group_updated(&mut self, g: &Group) {
        if let Some(existing) = self.groups.iter_mut().find(|gw| gw.group.id == g.id) {
            existing.group = g.clone();
        }
    }

    pub fn is_group_collapsed(&self, group_id: i32) -> bool {
        self.collapsed_groups.contains(&group_id)
    }

    pub fn toggle_group_collapsed(&mut self, group_id: i32) {
        if self.collapsed_groups.contains(&group_id) {
            self.collapsed_groups.remove(&group_id);
        } else {
            self.collapsed_groups.insert(group_id);
        }
        self.clamp_cursor();
    }

    pub fn collapse_group(&mut self, group_id: i32) {
        if self.collapsed_groups.insert(group_id) {
            self.clamp_cursor();
        }
    }

    pub fn expand_group(&mut self, group_id: i32) {
        if self.collapsed_groups.remove(&group_id) {
            self.clamp_cursor();
        }
    }

    pub fn parent_group_cursor(&self) -> Option<usize> {
        match self.tree_node_at(self.cursor)? {
            TreeNode::Group(gi) => self.cursor_for_group(gi),
            TreeNode::Profile(gi, _) => self.cursor_for_group(gi),
        }
    }

    pub fn cursor_for_group(&self, target_gi: usize) -> Option<usize> {
        if self.search_query.is_some() {
            return None;
        }
        let mut pos = 0;
        for (gi, g) in self.groups.iter().enumerate() {
            if gi == target_gi {
                return Some(pos);
            }
            pos += 1;
            if !self.collapsed_groups.contains(&g.group.id) {
                pos += g.profiles.len();
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_app_with_profile() -> App {
        let test_cfg = TestConfig {
            concurrency: 16,
            timeout_seconds: 5,
            samples_per_test: 3,
            test_endpoint: "https://cp.cloudflare.com/generate_204".to_string(),
            auto_test_on_subscribe: true,
        };
        let mut app = App::new(
            true,
            false,
            "warn".to_string(),
            "http-get".to_string(),
            "whoisthattun".to_string(),
            false,
            test_cfg,
        );
        app.groups = vec![GroupWithProfiles {
            group: Group {
                id: 1,
                ..Default::default()
            },
            profiles: vec![Profile {
                id: 7,
                group_id: 1,
                test_result: 100,
                tested_at: 12345,
                ..Default::default()
            }],
        }];
        app
    }

    #[test]
    fn test_mark_pending_flips_test_result_to_negative_two() {
        let mut app = make_app_with_profile();
        assert_eq!(app.groups[0].profiles[0].test_result, 100);
        app.mark_pending(1, 7);
        assert_eq!(app.groups[0].profiles[0].test_result, -2);
    }

    #[test]
    fn test_mark_pending_unknown_profile_is_noop() {
        let mut app = make_app_with_profile();
        app.mark_pending(1, 999);
        app.mark_pending(999, 7);
        assert_eq!(app.groups[0].profiles[0].test_result, 100);
    }

    #[test]
    fn test_apply_profile_updated_overwrites_pending_marker() {
        let mut app = make_app_with_profile();
        app.mark_pending(1, 7);
        let mut fresh = app.groups[0].profiles[0].clone();
        fresh.test_result = 42;
        fresh.tested_at = 99999;
        app.apply_profile_updated(&fresh);
        assert_eq!(app.groups[0].profiles[0].test_result, 42);
        assert_eq!(app.groups[0].profiles[0].tested_at, 99999);
    }

    #[test]
    fn test_is_connected_hy2() {
        let mut app = make_app_with_profile();
        assert!(!app.is_connected_hy2());

        app.connection_status.connection = "connected".to_string();
        app.connection_status.profile = Some(Profile {
            protocol: "vless".to_string(),
            ..Default::default()
        });
        assert!(!app.is_connected_hy2());

        app.connection_status.profile = Some(Profile {
            protocol: "hysteria2".to_string(),
            ..Default::default()
        });
        assert!(app.is_connected_hy2());

        app.connection_status.profile = Some(Profile {
            protocol: "hy2".to_string(),
            ..Default::default()
        });
        assert!(app.is_connected_hy2());
    }

    #[test]
    fn test_collapsed_group_tree_len_and_node_at() {
        let mut app = make_app_with_profile();
        // Add second group with 2 profiles
        app.groups.push(GroupWithProfiles {
            group: Group {
                id: 2,
                ..Default::default()
            },
            profiles: vec![
                Profile {
                    id: 20,
                    group_id: 2,
                    ..Default::default()
                },
                Profile {
                    id: 21,
                    group_id: 2,
                    ..Default::default()
                },
            ],
        });

        // Group 1 (1 profile) + Group 2 (2 profiles) = 2 groups + 3 profiles = 5 items
        assert_eq!(app.tree_len(), 5);
        assert_eq!(app.tree_node_at(0), Some(TreeNode::Group(0)));
        assert_eq!(app.tree_node_at(1), Some(TreeNode::Profile(0, 0)));
        assert_eq!(app.tree_node_at(2), Some(TreeNode::Group(1)));
        assert_eq!(app.tree_node_at(3), Some(TreeNode::Profile(1, 0)));
        assert_eq!(app.tree_node_at(4), Some(TreeNode::Profile(1, 1)));

        // Collapse Group 1
        app.collapse_group(1);
        assert!(app.is_group_collapsed(1));
        // Total should now be 1 (group 1) + 1 (group 2) + 2 (profiles of group 2) = 4
        assert_eq!(app.tree_len(), 4);
        assert_eq!(app.tree_node_at(0), Some(TreeNode::Group(0)));
        // Next visible node is Group 2!
        assert_eq!(app.tree_node_at(1), Some(TreeNode::Group(1)));
        assert_eq!(app.tree_node_at(2), Some(TreeNode::Profile(1, 0)));
        assert_eq!(app.tree_node_at(3), Some(TreeNode::Profile(1, 1)));

        // Collapse Group 2 as well
        app.collapse_group(2);
        assert_eq!(app.tree_len(), 2);
        assert_eq!(app.tree_node_at(0), Some(TreeNode::Group(0)));
        assert_eq!(app.tree_node_at(1), Some(TreeNode::Group(1)));
        assert_eq!(app.tree_node_at(2), None);

        // Toggle Group 1 back open
        app.toggle_group_collapsed(1);
        assert!(!app.is_group_collapsed(1));
        assert_eq!(app.tree_len(), 3); // Group 1 (1 profile) + Group 2 (collapsed, 0 profiles)
        assert_eq!(app.tree_node_at(0), Some(TreeNode::Group(0)));
        assert_eq!(app.tree_node_at(1), Some(TreeNode::Profile(0, 0)));
        assert_eq!(app.tree_node_at(2), Some(TreeNode::Group(1)));
    }

    #[test]
    fn test_cursor_clamp_on_collapse() {
        let mut app = make_app_with_profile();
        app.cursor = 1; // On profile 0 of group 1
        assert_eq!(app.tree_node_at(app.cursor), Some(TreeNode::Profile(0, 0)));

        app.collapse_group(1);
        // Only 1 item visible now (Group 1 at cursor 0)
        assert_eq!(app.cursor, 0);
        assert_eq!(app.tree_node_at(app.cursor), Some(TreeNode::Group(0)));
    }

    #[test]
    fn test_tab_index_conversions() {
        let mut app = make_app_with_profile();
        app.tab = ActiveTab::Profiles;
        assert_eq!(app.tab_index(), 0);
        assert_eq!(App::tab_from_index(0), ActiveTab::Profiles);

        app.tab = ActiveTab::Routing;
        assert_eq!(app.tab_index(), 1);
        assert_eq!(App::tab_from_index(1), ActiveTab::Routing);

        app.tab = ActiveTab::Traffic;
        assert_eq!(app.tab_index(), 2);
        assert_eq!(App::tab_from_index(2), ActiveTab::Traffic);

        app.tab = ActiveTab::Logs;
        assert_eq!(app.tab_index(), 3);
        assert_eq!(App::tab_from_index(3), ActiveTab::Logs);

        app.tab = ActiveTab::Settings;
        assert_eq!(app.tab_index(), 4);
        assert_eq!(App::tab_from_index(4), ActiveTab::Settings);
    }
}
