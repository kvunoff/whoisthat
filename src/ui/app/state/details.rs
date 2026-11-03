use crate::core_client::protocol::Profile;
use crate::ui::app::types::TreeNode;
use crate::ui::uri::{self, ParsedUri};

use super::App;

impl App {
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
                if self.is_connected()
                    && let Some((gid, pid)) = self.connected_id()
                    && gid == p.group_id
                    && pid == p.id
                {
                    n += 1;
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
        if let Some((gid, pid, ref params)) = *cache
            && gid == p.group_id
            && pid == p.id
        {
            return params.clone();
        }
        let params = uri::parse_uri(&p.uri);
        *cache = Some((p.group_id, p.id, params.clone()));
        params
    }
}
