use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use ratatui::layout::Rect;

use crate::core_client::protocol::*;
use crate::ui::logs::LogsState;
use crate::ui::routing::RoutingPopup;
use crate::ui::settings::SettingsState;
use crate::ui::traffic::TrafficHistory;
use crate::ui::uri::ParsedUri;

use super::types::*;

mod details;
mod events;
mod routing;
mod tree;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Default)]
pub struct GroupRoutingView {
    pub effective: RoutingConfig,
    pub custom: Option<RoutingConfig>,
}

pub struct App {
    pub groups: Vec<GroupWithProfiles>,
    pub connection_status: ProxyStatus,
    pub tun_enabled: bool,
    pub last_msg: Option<String>,
    pub show_ip: bool,
    pub check_updates: bool,
    pub log_enabled: bool,
    pub log_level: String,
    pub test_method: String,
    pub tun_name: String,
    pub hwid_info: Option<HwidData>,
    pub kill_switch_enabled: bool,
    pub kill_switch_armed: bool,
    pub split_tunnel: String,
    pub autoconnect_enabled: bool,
    pub autoconnect_mode: String,
    pub autostart_backend: Option<String>,
    pub autostart_enabled: bool,
    pub autostart_available: bool,
    pub public_ip: String,
    pub public_ipv6: String,

    pub tab: ActiveTab,
    pub focus: Focus,
    pub cursor: usize,
    pub tree_scroll: usize,
    pub details_scroll: usize,
    pub popup: Option<Popup>,
    pub help_scroll: usize,
    /// A newer release that arrived while another modal was open.
    pub pending_update: Option<crate::update_check::UpdateInfo>,
    /// Set once the "settings/database need repair" popup was offered, so it
    /// does not reappear after the user dismisses it.
    pub recovery_prompted: bool,

    pub settings_state: SettingsState,
    pub logs_state: LogsState,
    pub traffic_stats: TrafficStats,
    pub traffic_history: TrafficHistory,
    pub routing: RoutingConfig,
    pub profile_details: HashMap<(i32, i32), ProfileDetails>,
    pub profile_details_pending: HashSet<(i32, i32)>,
    pub group_routing: HashMap<i32, GroupRoutingView>,
    pub routing_cursor: usize,
    pub routing_group_index: usize,
    pub routing_popup: Option<RoutingPopup>,
    pub search_query: Option<String>,
    pub search_input: String,
    pub search_mode: bool,

    pub test_progress: Option<TestProgress>,
    pub test_config: TestConfig,

    pub collapsed_groups: HashSet<i32>,
    pub last_area: Rect,
    pub layout: crate::ui::layout::LayoutGeometry,
    pub xray_status: Option<XrayStatusInfo>,
    pub core_version: String,
    pub core_protocol_version: u32,

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
                reason: String::new(),
            },
            tun_enabled: false,
            last_msg: None,
            show_ip,
            check_updates: false,
            log_enabled,
            log_level,
            test_method,
            tun_name,
            hwid_info: None,
            kill_switch_enabled,
            kill_switch_armed: false,
            split_tunnel: "off".to_string(),
            autoconnect_enabled: false,
            autoconnect_mode: "proxy".to_string(),
            autostart_backend: None,
            autostart_enabled: false,
            autostart_available: false,
            public_ip: String::new(),
            public_ipv6: String::new(),
            tab: ActiveTab::Profiles,
            focus: Focus::LeftPanel,
            cursor: 0,
            tree_scroll: 0,
            details_scroll: 0,
            popup: None,
            help_scroll: 0,
            pending_update: None,
            recovery_prompted: false,
            settings_state: SettingsState::new(),
            logs_state: LogsState::new(),
            traffic_stats: TrafficStats::default(),
            traffic_history: TrafficHistory::new(),
            routing: RoutingConfig::default(),
            profile_details: HashMap::new(),
            profile_details_pending: HashSet::new(),
            group_routing: HashMap::new(),
            routing_cursor: 0,
            routing_group_index: 0,
            routing_popup: None,
            search_query: None,
            search_input: String::new(),
            search_mode: false,
            test_progress: None,
            test_config,
            collapsed_groups: HashSet::new(),
            last_area: Rect::default(),
            layout: crate::ui::layout::LayoutGeometry::default(),
            xray_status: None,
            core_version: String::new(),
            core_protocol_version: 0,
            uri_cache: RefCell::new(None),
        }
    }

    /// Show a pending update once no other modal or search is active.
    pub fn promote_pending_update(&mut self) {
        if self.popup.is_some() || self.routing_popup.is_some() || self.search_mode {
            return;
        }
        if let Some(info) = self.pending_update.take() {
            self.popup = Some(Popup::Update { info });
            self.focus = Focus::Popup;
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

    pub fn next_tab(&mut self) -> ActiveTab {
        let next_idx = (self.tab_index() + 1) % 5;
        self.tab = Self::tab_from_index(next_idx);
        self.tab
    }

    pub fn prev_tab(&mut self) -> ActiveTab {
        let cur = self.tab_index();
        let prev_idx = if cur == 0 { 4 } else { cur - 1 };
        self.tab = Self::tab_from_index(prev_idx);
        self.tab
    }

    pub fn toggle_focus(&mut self) {
        if self.tab == ActiveTab::Profiles {
            self.focus = match self.focus {
                Focus::LeftPanel => {
                    if self.tree_node_at(self.cursor).is_some() {
                        Focus::RightPanel
                    } else {
                        Focus::LeftPanel
                    }
                }
                Focus::RightPanel => Focus::LeftPanel,
                Focus::Popup => Focus::Popup,
            };
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connection_status.connection == "connected"
    }

    pub fn connected_id(&self) -> Option<(i32, i32)> {
        self.connection_status
            .profile
            .as_ref()
            .map(|p| (p.group_id, p.id))
    }

    pub fn connected_profile_name(&self) -> Option<&str> {
        self.connection_status
            .profile
            .as_ref()
            .map(|p| p.name.as_str())
    }

    pub fn clear_msg(&mut self) {
        self.last_msg = None;
    }
}
