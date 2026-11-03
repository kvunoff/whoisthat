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
        routing_profiles: Vec::new(),
    }];
    app
}

#[test]
fn test_toggle_focus_reaches_group_details() {
    let mut app = make_app_with_profile();
    app.cursor = 0;
    assert!(app.on_group());
    app.toggle_focus();
    assert_eq!(app.focus, Focus::RightPanel);
    app.toggle_focus();
    assert_eq!(app.focus, Focus::LeftPanel);
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
fn apply_state_strips_terminal_escapes() {
    let mut app = make_app_with_profile();
    let state: ApplicationState = serde_json::from_value(serde_json::json!({
        "groups": [{
            "group": {"id": 3, "name": "evil\u{1b}[2Jname"},
            "profiles": [{
                "id": 9,
                "group_id": 3,
                "name": "p\r\nx",
                "address": "a\u{1b}]0;title\u{7}"
            }],
            "routing_profiles": [{"name": "r\u{7}"}]
        }],
        "connection-status": {"connection": "disconnected"}
    }))
    .unwrap();

    app.apply_state(state);

    let g = &app.groups[0];
    assert_eq!(g.group.name, "evil[2Jname");
    assert_eq!(g.profiles[0].name, "px");
    assert!(!g.profiles[0].address.contains('\x1b'));
    assert_eq!(g.routing_profiles[0].name, "r");
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
        routing_profiles: Vec::new(),
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

#[test]
fn test_move_profile_up_and_down() {
    let mut app = make_app_with_profile();
    app.groups[0].profiles[0].name = "P1".into();
    app.groups[0].profiles.push(Profile {
        id: 8,
        group_id: 1,
        name: "P2".into(),
        ..Default::default()
    });
    app.groups[0].profiles.push(Profile {
        id: 9,
        group_id: 1,
        name: "P3".into(),
        ..Default::default()
    });

    // Profiles in Group 1: id 7 (P1), id 8 (P2), id 9 (P3)
    // Cursor on P1 (index 1)
    app.cursor = 1;
    assert_eq!(app.move_item_up(), None);

    // Move P1 down -> swaps with P2
    let action = app.move_item_down().expect("should move down");
    assert_eq!(
        action,
        ItemMoveAction::ReorderProfiles {
            group_id: 1,
            profile_ids: vec![8, 7, 9],
            profile_name: "P1".into(),
            moved_up: false,
        }
    );
    assert_eq!(app.cursor, 2);
    assert_eq!(app.groups[0].profiles[0].id, 8);
    assert_eq!(app.groups[0].profiles[1].id, 7);
    assert_eq!(app.groups[0].profiles[2].id, 9);

    // Move P1 back up -> swaps back
    let action = app.move_item_up().expect("should move up");
    assert_eq!(
        action,
        ItemMoveAction::ReorderProfiles {
            group_id: 1,
            profile_ids: vec![7, 8, 9],
            profile_name: "P1".into(),
            moved_up: true,
        }
    );
    assert_eq!(app.cursor, 1);
    assert_eq!(app.groups[0].profiles[0].id, 7);
    assert_eq!(app.groups[0].profiles[1].id, 8);

    // Move to bottom profile P3 (cursor 3)
    app.cursor = 3;
    assert_eq!(app.move_item_down(), None);
}

#[test]
fn test_move_group_up_and_down() {
    let mut app = make_app_with_profile();
    app.groups.push(GroupWithProfiles {
        group: Group {
            id: 2,
            name: "Group 2".into(),
            ..Default::default()
        },
        profiles: vec![],
        routing_profiles: Vec::new(),
    });

    // Group 1 at cursor 0
    app.cursor = 0;
    assert_eq!(app.move_item_up(), None);

    // Move Group 1 down -> swaps with Group 2
    let action = app.move_item_down().expect("should move down");
    assert_eq!(
        action,
        ItemMoveAction::ReorderGroups {
            group_ids: vec![2, 1],
            group_name: "".into(),
            moved_up: false,
        }
    );
    assert_eq!(app.groups[0].group.id, 2);
    assert_eq!(app.groups[1].group.id, 1);
    // Group 2 has 0 profiles (pos 0), Group 1 is at pos 1
    assert_eq!(app.cursor, 1);

    // Move Group 1 back up
    let action = app.move_item_up().expect("should move up");
    assert_eq!(
        action,
        ItemMoveAction::ReorderGroups {
            group_ids: vec![1, 2],
            group_name: "".into(),
            moved_up: true,
        }
    );
    assert_eq!(app.groups[0].group.id, 1);
    assert_eq!(app.groups[1].group.id, 2);
    assert_eq!(app.cursor, 0);
}

#[test]
fn test_move_blocked_on_search() {
    let mut app = make_app_with_profile();
    app.cursor = 1;
    app.search_query = Some("test".into());
    assert_eq!(app.move_item_up(), None);
    assert_eq!(app.move_item_down(), None);
}

#[test]
fn test_apply_profile_moved_and_reordered() {
    let mut app = make_app_with_profile();
    app.groups.push(GroupWithProfiles {
        group: Group {
            id: 2,
            name: "Group 2".into(),
            ..Default::default()
        },
        profiles: vec![],
        routing_profiles: Vec::new(),
    });

    app.connection_status.connection = "connected".into();
    app.connection_status.profile = Some(app.groups[0].profiles[0].clone());

    let new_p = Profile {
        id: 99,
        group_id: 2,
        name: "Moved P".into(),
        ..Default::default()
    };
    app.apply_profile_moved(ProfileID { group_id: 1, id: 7 }, new_p);

    assert_eq!(app.groups[0].profiles.len(), 0);
    assert_eq!(app.groups[1].profiles.len(), 1);
    assert_eq!(app.groups[1].profiles[0].id, 99);
    assert_eq!(app.connection_status.profile.as_ref().unwrap().id, 99);
    assert_eq!(app.connection_status.profile.as_ref().unwrap().group_id, 2);

    app.apply_groups_reordered(&[2, 1]);
    assert_eq!(app.groups[0].group.id, 2);
    assert_eq!(app.groups[1].group.id, 1);
}

fn routing_config(domain: &str) -> RoutingConfig {
    RoutingConfig {
        domain_strategy: "IPIfNonMatch".into(),
        rules: vec![RoutingRule {
            r#type: "field".into(),
            domain: domain.into(),
            outbound_tag: "proxy".into(),
            enabled: true,
            ..Default::default()
        }],
        dns: None,
    }
}

#[test]
fn test_routing_pages_start_with_global() {
    let mut app = make_app_with_profile();

    assert_eq!(app.routing_group_index, 0);
    assert_eq!(app.routing_page_group_id(), 0);
    assert!(app.routing_selected_group().is_none());
    assert_eq!(app.routing_source(), "global");

    app.routing_group_next();
    assert_eq!(app.routing_group_index, 1);
    assert_eq!(app.routing_page_group_id(), 1);
    assert!(app.routing_selected_group().is_some());

    app.routing_group_next();
    assert_eq!(app.routing_group_index, 0);
}

#[test]
fn test_routing_inherit_displays_current_global_rules() {
    let mut app = make_app_with_profile();
    app.routing_group_index = 1;
    app.routing = routing_config("first");

    assert_eq!(app.routing_source(), "global");
    assert_eq!(app.active_routing_config().rules[0].domain, "first");

    // A later global update must be reflected on inheriting pages.
    app.apply_routing_updated(0, routing_config("second"), "global".into(), None);
    assert_eq!(app.active_routing_config().rules[0].domain, "second");
}

#[test]
fn test_ensure_custom_routing_seeds_from_global_without_touching_it() {
    let mut app = make_app_with_profile();
    app.routing_group_index = 1;
    app.routing = routing_config("global");

    let gid = app.ensure_custom_routing();
    assert_eq!(gid, 1);
    assert_eq!(app.routing_source(), "custom");
    assert_eq!(app.active_routing_config().rules[0].domain, "global");

    app.active_routing_config_mut().rules[0].domain = "edited".into();
    assert_eq!(app.routing.rules[0].domain, "global");
    assert_eq!(app.active_routing_config().rules[0].domain, "edited");
}

#[test]
fn test_ensure_custom_routing_reuses_stored_override() {
    let mut app = make_app_with_profile();
    app.routing_group_index = 1;
    app.routing = routing_config("global");
    app.group_routing.insert(
        1,
        GroupRoutingView {
            effective: routing_config("global"),
            custom: Some(routing_config("saved")),
        },
    );

    app.ensure_custom_routing();
    assert_eq!(app.active_routing_config().rules[0].domain, "saved");
}

#[test]
fn test_apply_routing_updated_by_group() {
    let mut app = make_app_with_profile();

    app.apply_routing_updated(0, routing_config("global"), "global".into(), None);
    assert_eq!(app.routing.rules[0].domain, "global");

    app.apply_routing_updated(1, routing_config("custom"), "custom".into(), None);
    let view = app.group_routing.get(&1).unwrap();
    assert_eq!(view.effective.rules[0].domain, "custom");
    assert_eq!(view.custom.as_ref().unwrap().rules[0].domain, "custom");

    app.apply_routing_updated(
        1,
        routing_config("global"),
        "global".into(),
        Some(routing_config("saved")),
    );
    let view = app.group_routing.get(&1).unwrap();
    assert_eq!(view.effective.rules[0].domain, "global");
    assert_eq!(view.custom.as_ref().unwrap().rules[0].domain, "saved");
}

#[test]
fn test_apply_group_deleted_resets_routing_page() {
    let mut app = make_app_with_profile();
    app.routing_group_index = 1;
    app.group_routing.insert(1, GroupRoutingView::default());

    app.apply_group_deleted(1);

    assert_eq!(app.routing_group_index, 0);
    assert!(!app.group_routing.contains_key(&1));
}

#[test]
fn test_sync_routing_group_to_connected_skips_global_page() {
    let mut app = make_app_with_profile();
    app.connection_status.profile = Some(app.groups[0].profiles[0].clone());

    app.sync_routing_group_to_connected();

    assert_eq!(app.routing_group_index, 1);
    assert_eq!(app.routing_page_group_id(), 1);
}
