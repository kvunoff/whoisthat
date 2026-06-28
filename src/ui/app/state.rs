use std::cell::RefCell;

use crate::config;
use crate::core_client::protocol::*;

use crate::ui::logs::LogsState;
use crate::ui::routing::RoutingPopup;
use crate::ui::settings::SettingsState;
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
    pub autoconnect_enabled: bool,
    pub autostart_mode: String,
    pub systemd_enabled: bool,
    pub public_ip: String,
    pub public_ipv6: String,

    pub tab: ActiveTab,
    pub focus: Focus,
    pub cursor: usize,
    pub tree_scroll: usize,
    pub popup: Option<Popup>,
    pub help_scroll: usize,

    pub settings_state: SettingsState,
    pub logs_state: LogsState,
    pub traffic_stats: TrafficStats,
    pub routing: RoutingConfig,
    pub routing_cursor: usize,
    pub routing_popup: Option<RoutingPopup>,

    uri_cache: RefCell<Option<(i32, i32, ParsedUri)>>,
}

impl App {
    pub fn new(show_ip: bool, log_enabled: bool, log_level: String, test_method: String, tun_name: String, kill_switch_enabled: bool) -> Self {
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
            autoconnect_enabled: false,
            autostart_mode: "proxy".to_string(),
            systemd_enabled: false,
            public_ip: String::new(),
            public_ipv6: String::new(),
            tab: ActiveTab::Profiles,
            focus: Focus::LeftPanel,
            cursor: 0,
            tree_scroll: 0,
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
            routing: RoutingConfig::default(),
            routing_cursor: 0,
            routing_popup: None,
            uri_cache: RefCell::new(None),
        }
    }

    // --- tree helpers ---

    fn tree_len(&self) -> usize {
        let mut n = 0;
        for g in &self.groups {
            n += 1;
            n += g.profiles.len();
        }
        n
    }

    pub(super) fn tree_node_at(&self, cursor: usize) -> Option<TreeNode> {
        let mut pos = 0;
        for (gi, g) in self.groups.iter().enumerate() {
            if pos == cursor {
                return Some(TreeNode::Group(gi));
            }
            pos += 1;
            let plen = g.profiles.len();
            if cursor < pos + plen {
                return Some(TreeNode::Profile(gi, cursor - pos));
            }
            pos += plen;
        }
        None
    }

    fn clamp_cursor(&mut self) {
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

    pub fn is_connected(&self) -> bool {
        self.connection_status.connection == "connected"
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

    pub fn cursor_down(&mut self) {
        let len = self.tree_len();
        if len == 0 || self.cursor + 1 >= len {
            return;
        }
        self.cursor += 1;
    }

    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn cursor_top(&mut self) {
        self.cursor = 0;
        self.tree_scroll = 0;
    }

    pub fn cursor_bottom(&mut self) {
        let len = self.tree_len();
        if len > 0 {
            self.cursor = len - 1;
        }
    }

    pub fn clear_msg(&mut self) {
        self.last_msg = None;
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

    pub fn apply_group_added(&mut self, g: Group) {
        self.groups.push(GroupWithProfiles {
            group: g,
            profiles: Vec::new(),
        });
    }

    pub fn apply_group_deleted(&mut self, id: i32) {
        self.groups.retain(|g| g.group.id != id);
        self.clamp_cursor();
    }

    pub fn apply_group_updated(&mut self, g: &Group) {
        if let Some(existing) = self.groups.iter_mut().find(|gw| gw.group.id == g.id) {
            existing.group = g.clone();
        }
    }
}
