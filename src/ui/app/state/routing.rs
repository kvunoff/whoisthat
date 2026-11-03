use crate::core_client::protocol::{GroupWithProfiles, RoutingConfig};

use super::{App, GroupRoutingView};

impl App {
    /// Page 0 is the global rule set; pages 1..=groups.len() are groups.
    pub fn routing_page_group_id(&self) -> i32 {
        if self.routing_group_index == 0 {
            return 0;
        }
        self.groups
            .get(self.routing_group_index - 1)
            .map(|g| g.group.id)
            .unwrap_or(0)
    }

    pub fn routing_selected_group(&self) -> Option<&GroupWithProfiles> {
        if self.routing_group_index == 0 {
            None
        } else {
            self.groups.get(self.routing_group_index - 1)
        }
    }

    pub fn routing_source(&self) -> &'static str {
        match self.routing_selected_group() {
            Some(g) if g.group.manual_routing_mode == "custom" => "custom",
            _ => "global",
        }
    }

    pub fn active_routing_config(&self) -> &RoutingConfig {
        let gid = self.routing_page_group_id();
        if gid != 0
            && self.routing_source() == "custom"
            && let Some(view) = self.group_routing.get(&gid)
        {
            return &view.effective;
        }
        &self.routing
    }

    pub fn active_routing_config_mut(&mut self) -> &mut RoutingConfig {
        let gid = self.routing_page_group_id();
        if gid == 0 {
            return &mut self.routing;
        }
        if !self.group_routing.contains_key(&gid) {
            let fallback = self.routing.clone();
            self.group_routing.insert(
                gid,
                GroupRoutingView {
                    effective: fallback,
                    custom: None,
                },
            );
        }
        &mut self.group_routing.get_mut(&gid).unwrap().effective
    }

    /// Makes the current group page editable. Inheriting groups are switched
    /// to a group-specific rule set, seeded from their stored custom rules if
    /// any, otherwise from the global rules. Returns the affected group id
    /// (0 for the global page).
    pub fn ensure_custom_routing(&mut self) -> i32 {
        let gid = self.routing_page_group_id();
        if gid == 0 {
            return 0;
        }

        let already_custom = self
            .routing_selected_group()
            .map(|g| g.group.manual_routing_mode == "custom")
            .unwrap_or(false);

        if !already_custom {
            let seed = self
                .group_routing
                .get(&gid)
                .and_then(|v| v.custom.clone())
                .unwrap_or_else(|| self.routing.clone());
            self.group_routing.insert(
                gid,
                GroupRoutingView {
                    effective: seed,
                    custom: None,
                },
            );
            if let Some(g) = self.groups.iter_mut().find(|g| g.group.id == gid) {
                g.group.manual_routing_mode = "custom".to_string();
            }
        }

        gid
    }

    pub fn apply_routing_updated(
        &mut self,
        group_id: i32,
        config: RoutingConfig,
        source: String,
        custom: Option<RoutingConfig>,
    ) {
        if group_id == 0 {
            self.routing = config;
        } else {
            let custom = if source == "custom" {
                custom.or_else(|| Some(config.clone()))
            } else {
                custom
            };
            self.group_routing.insert(
                group_id,
                GroupRoutingView {
                    effective: config,
                    custom,
                },
            );
        }

        if group_id == 0 || group_id == self.routing_page_group_id() {
            self.clamp_routing_cursor();
        }
    }

    pub fn clamp_routing_cursor(&mut self) {
        let len = self.active_routing_config().rules.len();
        if len == 0 {
            self.routing_cursor = 0;
        } else if self.routing_cursor >= len {
            self.routing_cursor = len - 1;
        }
    }

    pub fn routing_group_next(&mut self) {
        let pages = self.groups.len() + 1;
        self.routing_group_index = (self.routing_group_index + 1) % pages;
        self.routing_cursor = 0;
    }

    pub fn routing_group_prev(&mut self) {
        let pages = self.groups.len() + 1;
        self.routing_group_index = if self.routing_group_index == 0 {
            pages - 1
        } else {
            self.routing_group_index - 1
        };
        self.routing_cursor = 0;
    }

    pub fn sync_routing_group_to_connected(&mut self) {
        if let Some(ref p) = self.connection_status.profile
            && let Some(idx) = self.groups.iter().position(|g| g.group.id == p.group_id)
        {
            self.routing_group_index = idx + 1;
        }
    }
}
