use crate::core_client::protocol::{Profile, ProfileDetails};
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

    pub fn details_visible(&self) -> usize {
        let area = match self.layout.profiles_mode {
            crate::ui::layout::ProfilesLayoutMode::SinglePanel => self.layout.tree_area,
            _ => self.layout.details_area,
        };
        area.height.saturating_sub(2) as usize
    }

    /// Fetch the next managed profile whose stream metadata still needs to be
    /// requested from the core. Empty for URI profiles and cached entries.
    pub fn profile_details_request(&self) -> Option<(i32, i32)> {
        let p = self.selected_profile()?;
        if !p.managed {
            return None;
        }
        let key = (p.group_id, p.id);
        if self.profile_details.contains_key(&key) || self.profile_details_pending.contains(&key) {
            return None;
        }
        Some(key)
    }

    pub fn mark_profile_details_pending(&mut self, key: (i32, i32)) {
        self.profile_details_pending.insert(key);
    }

    pub fn apply_profile_details(&mut self, details: ProfileDetails) {
        let key = (details.group_id, details.id);
        self.profile_details_pending.remove(&key);
        self.profile_details.insert(key, details);
    }

    pub fn clear_profile_details(&mut self) {
        self.profile_details.clear();
        self.profile_details_pending.clear();
    }

    pub fn profile_details_for(&self, p: &Profile) -> Option<&ProfileDetails> {
        self.profile_details.get(&(p.group_id, p.id))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_client::protocol::{Group, GroupWithProfiles, TestConfig};
    use crate::ui::layout::{LayoutGeometry, ProfilesLayoutMode};
    use ratatui::layout::Rect;

    fn test_app() -> App {
        let mut app = App::new(
            false,
            false,
            "warn".to_string(),
            "http-get".to_string(),
            "whoisthattun".to_string(),
            false,
            TestConfig::default(),
        );
        app.groups = vec![GroupWithProfiles {
            group: Group {
                id: 1,
                name: "Test Group".to_string(),
                ..Default::default()
            },
            profiles: vec![Profile {
                id: 7,
                group_id: 1,
                name: "Vienna".to_string(),
                protocol: "vless".to_string(),
                address: "example.com".to_string(),
                port: 443,
                uri: "vless://uuid@example.com:443?security=reality&sni=example.com#Vienna"
                    .to_string(),
                ..Default::default()
            }],
            routing_profiles: Vec::new(),
        }];
        app.cursor = 1;
        app
    }

    #[test]
    fn details_visible_uses_the_real_panel_height() {
        let mut app = test_app();

        app.layout = LayoutGeometry::compute(Rect::new(0, 0, 100, 30));
        assert_eq!(app.layout.profiles_mode, ProfilesLayoutMode::SideBySide);
        assert_eq!(
            app.details_visible(),
            app.layout.details_area.height as usize - 2
        );

        app.layout = LayoutGeometry::compute(Rect::new(0, 0, 70, 30));
        assert_eq!(app.layout.profiles_mode, ProfilesLayoutMode::Stacked);
        assert!(app.details_visible() > 0);

        app.layout = LayoutGeometry::compute(Rect::new(0, 0, 70, 15));
        assert_eq!(app.layout.profiles_mode, ProfilesLayoutMode::SinglePanel);
        assert_eq!(
            app.details_visible(),
            app.layout.tree_area.height as usize - 2
        );
    }

    #[test]
    fn details_line_count_is_not_a_hardcoded_guess() {
        let mut app = test_app();
        app.layout = LayoutGeometry::compute(Rect::new(0, 0, 100, 30));
        assert!(app.details_line_count() > 5);
    }

    #[test]
    fn scroll_stays_within_bounds() {
        let mut app = test_app();
        for _ in 0..20 {
            app.details_scroll_down(10, 4);
        }
        assert_eq!(app.details_scroll, 6);
        app.details_scroll_down(10, 4);
        assert_eq!(app.details_scroll, 6);
        app.details_scroll_bottom(10, 4);
        assert_eq!(app.details_scroll, 6);
        app.details_scroll_top();
        assert_eq!(app.details_scroll, 0);
    }

    #[test]
    fn managed_profile_details_are_requested_once_and_cached() {
        let mut app = test_app();
        app.groups[0].profiles[0].managed = true;

        assert_eq!(app.profile_details_request(), Some((1, 7)));
        app.mark_profile_details_pending((1, 7));
        assert_eq!(app.profile_details_request(), None);

        app.apply_profile_details(ProfileDetails {
            group_id: 1,
            id: 7,
            transport: "ws".to_string(),
            ..Default::default()
        });
        assert_eq!(app.profile_details_request(), None);
        assert_eq!(
            app.profile_details_for(&app.groups[0].profiles[0])
                .map(|d| d.transport.as_str()),
            Some("ws")
        );
    }

    #[test]
    fn zero_height_viewport_does_not_underflow() {
        let mut app = test_app();
        for _ in 0..5 {
            app.details_scroll_down(5, 0);
        }
        assert_eq!(app.details_scroll, 5);
        app.details_scroll_bottom(5, 0);
        assert_eq!(app.details_scroll, 5);
    }
}
