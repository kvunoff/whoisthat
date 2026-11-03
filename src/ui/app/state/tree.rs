use crate::ui::app::types::{ItemMoveAction, TreeNode};

use super::App;

impl App {
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

    pub fn cursor_page_down(&mut self, step: usize) {
        let len = self.tree_len();
        if len == 0 {
            return;
        }
        self.cursor = (self.cursor + step).min(len - 1);
        self.details_scroll = 0;
    }

    pub fn cursor_page_up(&mut self, step: usize) {
        self.cursor = self.cursor.saturating_sub(step);
        self.details_scroll = 0;
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

    pub fn cursor_for_node(&self, target: TreeNode) -> Option<usize> {
        if self.search_query.is_some() {
            return None;
        }
        let mut pos = 0;
        for (gi, g) in self.groups.iter().enumerate() {
            if target == TreeNode::Group(gi) {
                return Some(pos);
            }
            pos += 1;
            let collapsed = self.collapsed_groups.contains(&g.group.id);
            if !collapsed {
                if let TreeNode::Profile(tgi, tpi) = target
                    && tgi == gi
                {
                    if tpi < g.profiles.len() {
                        return Some(pos + tpi);
                    } else {
                        return None;
                    }
                }
                pos += g.profiles.len();
            }
        }
        None
    }

    pub fn move_item_up(&mut self) -> Option<ItemMoveAction> {
        if self.search_query.is_some() {
            return None;
        }
        match self.tree_node_at(self.cursor)? {
            TreeNode::Group(gi) => {
                if gi == 0 {
                    return None;
                }
                self.groups.swap(gi, gi - 1);
                if let Some(new_pos) = self.cursor_for_group(gi - 1) {
                    self.cursor = new_pos;
                }
                let group_ids: Vec<i32> = self.groups.iter().map(|g| g.group.id).collect();
                let group_name = self.groups[gi - 1].group.name.clone();
                Some(ItemMoveAction::ReorderGroups {
                    group_ids,
                    group_name,
                    moved_up: true,
                })
            }
            TreeNode::Profile(gi, pi) => {
                if pi == 0 {
                    return None;
                }
                self.groups[gi].profiles.swap(pi, pi - 1);
                if let Some(new_pos) = self.cursor_for_node(TreeNode::Profile(gi, pi - 1)) {
                    self.cursor = new_pos;
                }
                let profile_ids: Vec<i32> = self.groups[gi].profiles.iter().map(|p| p.id).collect();
                let profile_name = self.groups[gi].profiles[pi - 1].name.clone();
                let group_id = self.groups[gi].group.id;
                Some(ItemMoveAction::ReorderProfiles {
                    group_id,
                    profile_ids,
                    profile_name,
                    moved_up: true,
                })
            }
        }
    }

    pub fn move_item_down(&mut self) -> Option<ItemMoveAction> {
        if self.search_query.is_some() {
            return None;
        }
        match self.tree_node_at(self.cursor)? {
            TreeNode::Group(gi) => {
                if gi + 1 >= self.groups.len() {
                    return None;
                }
                self.groups.swap(gi, gi + 1);
                if let Some(new_pos) = self.cursor_for_group(gi + 1) {
                    self.cursor = new_pos;
                }
                let group_ids: Vec<i32> = self.groups.iter().map(|g| g.group.id).collect();
                let group_name = self.groups[gi + 1].group.name.clone();
                Some(ItemMoveAction::ReorderGroups {
                    group_ids,
                    group_name,
                    moved_up: false,
                })
            }
            TreeNode::Profile(gi, pi) => {
                if pi + 1 >= self.groups[gi].profiles.len() {
                    return None;
                }
                self.groups[gi].profiles.swap(pi, pi + 1);
                if let Some(new_pos) = self.cursor_for_node(TreeNode::Profile(gi, pi + 1)) {
                    self.cursor = new_pos;
                }
                let profile_ids: Vec<i32> = self.groups[gi].profiles.iter().map(|p| p.id).collect();
                let profile_name = self.groups[gi].profiles[pi + 1].name.clone();
                let group_id = self.groups[gi].group.id;
                Some(ItemMoveAction::ReorderProfiles {
                    group_id,
                    profile_ids,
                    profile_name,
                    moved_up: false,
                })
            }
        }
    }
}
