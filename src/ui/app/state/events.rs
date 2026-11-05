use crate::core_client::protocol::*;
use crate::ui::app::helpers::clean_label;

use super::App;

impl App {
    pub fn msg(&mut self, text: impl Into<String>) {
        self.last_msg = Some(clean_label(&text.into()));
    }

    // --- state mutations ---

    pub fn apply_state(&mut self, mut state: ApplicationState) {
        for g in state.groups.iter_mut() {
            clean_group(&mut g.group);
            for p in g.profiles.iter_mut() {
                clean_profile(p);
            }
            for r in g.routing_profiles.iter_mut() {
                clean_routing_profile(r);
            }
        }
        if let Some(p) = state.connection_status.profile.as_mut() {
            clean_profile(p);
        }
        self.groups = state.groups;
        self.connection_status = state.connection_status;
        self.tun_enabled = state.tun_status;
        self.kill_switch_enabled = state.kill_switch;
        self.kill_switch_armed = state.kill_switch_armed;
        if state.split_tunnel.is_empty() {
            self.split_tunnel = "off".to_string();
        } else {
            self.split_tunnel = state.split_tunnel;
        }
        self.autoconnect_enabled = state.autoconnect.enabled;
        self.autostart_mode = state.autoconnect.mode;
        self.hwid_info = state.hwid_info;
        self.xray_status = state.xray_status;
        if !state.core_version.is_empty() {
            self.core_version = state.core_version;
        }
        if state.protocol_version != 0 {
            self.core_protocol_version = state.protocol_version;
        }
        // Core is the source of truth for these network/test settings; the
        // config.toml copies are only an offline cache.
        if !state.tun_name.is_empty() {
            self.tun_name = state.tun_name;
        }
        if !state.test_config.test_endpoint.is_empty() {
            self.test_config = state.test_config;
        }
        self.clamp_cursor();
    }

    pub fn invalidate_uri_cache(&self) {
        self.uri_cache.borrow_mut().take();
    }

    pub fn apply_profiles_added(&mut self, profiles: Vec<Profile>) {
        self.invalidate_uri_cache();
        for mut p in profiles {
            clean_profile(&mut p);
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

    pub fn apply_subscription_updated(
        &mut self,
        mut group: Group,
        profiles: Vec<Profile>,
        routing_profiles: Vec<RoutingProfileInfo>,
    ) {
        self.invalidate_uri_cache();
        clean_group(&mut group);
        let mut profiles = profiles;
        for p in profiles.iter_mut() {
            clean_profile(p);
        }
        let mut routing_profiles = routing_profiles;
        for r in routing_profiles.iter_mut() {
            clean_routing_profile(r);
        }
        if let Some(g) = self.groups.iter_mut().find(|g| g.group.id == group.id) {
            g.group = group;
            g.profiles = profiles;
            g.routing_profiles = routing_profiles;
        }
        self.clamp_cursor();
    }

    pub fn apply_profile_updated(&mut self, p: &Profile) {
        self.invalidate_uri_cache();
        let mut fresh = p.clone();
        clean_profile(&mut fresh);
        if let Some(g) = self
            .groups
            .iter_mut()
            .find(|g| g.group.id == fresh.group_id)
            && let Some(existing) = g.profiles.iter_mut().find(|pr| pr.id == fresh.id)
        {
            *existing = fresh;
        }
    }

    /// Mark a profile as "test in flight" so the tree immediately shows
    /// `…` instead of the stale last-known value. Cleared when the real
    /// `profile-updated` arrives via `apply_profile_updated`.
    pub fn mark_pending(&mut self, group_id: i32, profile_id: i32) {
        if let Some(g) = self.groups.iter_mut().find(|g| g.group.id == group_id)
            && let Some(p) = g.profiles.iter_mut().find(|pr| pr.id == profile_id)
        {
            p.test_result = -2;
        }
    }

    pub fn apply_group_added(&mut self, mut g: Group) {
        clean_group(&mut g);
        self.groups.push(GroupWithProfiles {
            group: g,
            profiles: Vec::new(),
            routing_profiles: Vec::new(),
        });
    }

    pub fn apply_group_deleted(&mut self, id: i32) {
        self.groups.retain(|g| g.group.id != id);
        self.collapsed_groups.remove(&id);
        self.group_routing.remove(&id);
        if self.routing_group_index > self.groups.len() {
            self.routing_group_index = self.groups.len();
        }
        self.routing_cursor = 0;
        self.clamp_cursor();
    }

    pub fn apply_group_updated(&mut self, g: &Group) {
        let mut fresh = g.clone();
        clean_group(&mut fresh);
        if let Some(existing) = self.groups.iter_mut().find(|gw| gw.group.id == fresh.id) {
            existing.group = fresh;
        }
    }

    pub fn apply_profiles_reordered(&mut self, group_id: i32, profile_ids: &[i32]) {
        self.invalidate_uri_cache();
        if let Some(g) = self.groups.iter_mut().find(|g| g.group.id == group_id) {
            let mut id_to_profile = std::collections::HashMap::new();
            for p in g.profiles.drain(..) {
                id_to_profile.insert(p.id, p);
            }
            for &pid in profile_ids {
                if let Some(p) = id_to_profile.remove(&pid) {
                    g.profiles.push(p);
                }
            }
            for (_, p) in id_to_profile {
                g.profiles.push(p);
            }
        }
        self.clamp_cursor();
    }

    pub fn apply_groups_reordered(&mut self, group_ids: &[i32]) {
        let mut id_to_group = std::collections::HashMap::new();
        for g in self.groups.drain(..) {
            id_to_group.insert(g.group.id, g);
        }
        for &gid in group_ids {
            if let Some(g) = id_to_group.remove(&gid) {
                self.groups.push(g);
            }
        }
        for (_, g) in id_to_group {
            self.groups.push(g);
        }
        self.clamp_cursor();
    }

    pub fn apply_profile_moved(&mut self, old: ProfileID, new_p: Profile) {
        self.invalidate_uri_cache();
        if let Some(g) = self.groups.iter_mut().find(|g| g.group.id == old.group_id) {
            g.profiles.retain(|p| p.id != old.id);
        }
        if let Some(g) = self
            .groups
            .iter_mut()
            .find(|g| g.group.id == new_p.group_id)
        {
            g.profiles.push(new_p.clone());
        }
        if let Some(ref mut cur_p) = self.connection_status.profile
            && cur_p.group_id == old.group_id
            && cur_p.id == old.id
        {
            *cur_p = new_p.clone();
        }
        self.clamp_cursor();
    }
}

fn clean_group(g: &mut Group) {
    g.name = clean_label(&g.name);
}

fn clean_profile(p: &mut Profile) {
    p.name = clean_label(&p.name);
    p.address = clean_label(&p.address);
    p.host = clean_label(&p.host);
}

fn clean_routing_profile(r: &mut RoutingProfileInfo) {
    r.name = clean_label(&r.name);
    r.last_error = clean_label(&r.last_error);
    r.remote_dns = clean_label(&r.remote_dns);
    r.domestic_dns = clean_label(&r.domestic_dns);
}
