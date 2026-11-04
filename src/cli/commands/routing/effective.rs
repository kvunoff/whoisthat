use crate::core_client::protocol::*;

/// Routing selection the core will apply for URI subscriptions, mirroring
/// `db.ResolveRouting`: `auto` prefers a usable Happ profile and falls back to
/// manual rules, `subscription` never falls back to manual, `manual` ignores
/// Happ. JSON subscriptions are different: the core always loads manual rules
/// on top of the provider config, so `json` overrides the source to manual.
pub(super) struct EffectiveRouting {
    pub(super) mode: &'static str,
    pub(super) source: &'static str, // "happ" | "manual" | "lan"
    pub(super) manual_source: &'static str,
    pub(super) profile: Option<String>,
    pub(super) rules: usize,
    /// JSON subscriptions always use manual rules on top of the provider
    /// baseline, regardless of an active Happ profile.
    pub(super) json: bool,
}

pub(super) fn effective_routing(groups: &[GroupWithProfiles], group_id: i32) -> EffectiveRouting {
    if group_id == 0 {
        return EffectiveRouting {
            mode: "global",
            source: "manual",
            manual_source: "global",
            profile: None,
            rules: 0,
            json: false,
        };
    }
    let Some(group) = groups.iter().find(|g| g.group.id == group_id) else {
        return EffectiveRouting {
            mode: "auto",
            source: "manual",
            manual_source: "global",
            profile: None,
            rules: 0,
            json: false,
        };
    };
    let mode = match group.group.routing_mode.as_str() {
        "manual" => "manual",
        "subscription" => "subscription",
        _ => "auto",
    };
    let manual_source = if group.group.manual_routing_mode == "custom" {
        "custom"
    } else {
        "global"
    };
    let active = group
        .routing_profiles
        .iter()
        .find(|p| p.id == group.group.active_routing_profile_id);
    // A profile in the error state has no usable geo assets; the core falls
    // back to manual rules in `auto` and to LAN-only in `subscription`.
    // shortcut: readiness comes from the profile state, so geo files deleted
    // after a successful download are not detected here (the core would fall back).
    let usable = active.filter(|p| p.state != "error" && p.rules_count > 0);
    let json = group.group.subscription_format == "json";
    let source = if json {
        // mainproxy always loads LoadEffectiveManualRouting for managed JSON
        // profiles and prepends it to the provider config.
        "manual"
    } else {
        match mode {
            "manual" => "manual",
            "subscription" => {
                if usable.is_some() {
                    "happ"
                } else {
                    "lan"
                }
            }
            _ => {
                if usable.is_some() {
                    "happ"
                } else {
                    "manual"
                }
            }
        }
    };
    let (profile, rules) = if json {
        // Happ rules are not used for JSON groups; do not expose them in the
        // effective-routing payload.
        (None, 0)
    } else {
        (
            active.map(|p| p.name.clone()),
            usable.map_or(0, |p| p.rules_count),
        )
    };
    EffectiveRouting {
        mode,
        source,
        manual_source,
        profile,
        rules,
        json,
    }
}

pub(super) fn manual_shadowed(eff: &EffectiveRouting) -> bool {
    !eff.json && (eff.mode == "subscription" || (eff.mode == "auto" && eff.source == "happ"))
}

/// Groups that use the global manual rules and currently route through Happ.
pub(super) fn global_shadowed_groups(groups: &[GroupWithProfiles]) -> Vec<String> {
    groups
        .iter()
        .filter(|g| g.group.subscription_format != "json")
        .filter(|g| g.group.manual_routing_mode != "custom")
        .filter(|g| manual_shadowed(&effective_routing(groups, g.group.id)))
        .map(|g| g.group.name.clone())
        .collect()
}

pub(super) fn describe_effective(eff: &EffectiveRouting) -> String {
    if eff.json {
        return "Provider JSON baseline + manual rules (manual first)".to_string();
    }
    match eff.source {
        "happ" => format!(
            "Happ \"{}\" (rules: {})",
            eff.profile.as_deref().unwrap_or("?"),
            eff.rules
        ),
        "lan" => match &eff.profile {
            Some(name) => format!("Happ \"{name}\" — LAN only (profile not usable)"),
            None => "LAN only (no active Happ profile)".to_string(),
        },
        _ => format!("manual ({} rules)", eff.manual_source),
    }
}

pub(super) fn shadow_warning(groups: &[GroupWithProfiles], group_id: i32) -> Option<String> {
    if group_id == 0 {
        let names = global_shadowed_groups(groups);
        if names.is_empty() {
            return None;
        }
        return Some(format!(
            "warning: global manual rules are shadowed in groups with active Happ routing: {}; edits are saved but will not affect them",
            names.join(", ")
        ));
    }
    let eff = effective_routing(groups, group_id);
    if !manual_shadowed(&eff) {
        return None;
    }
    let name = groups
        .iter()
        .find(|g| g.group.id == group_id)
        .map(|g| g.group.name.as_str())
        .unwrap_or("GROUP");
    let label = match eff.source {
        "happ" => format!("Happ \"{}\"", eff.profile.as_deref().unwrap_or("?")),
        "lan" => "Happ routing".to_string(),
        _ => "manual routing".to_string(),
    };
    let hint = if eff.mode == "subscription" {
        format!("use 'whoisthat route mode {name} manual' to apply them")
    } else {
        format!("use 'whoisthat route mode {name} manual' or 'whoisthat route profile off {name}'")
    };
    Some(format!(
        "warning: manual rules are shadowed by {label} (mode {}); edits are saved but will not affect traffic — {hint}",
        eff.mode
    ))
}

pub(super) fn manual_shadowed_for_edit(groups: &[GroupWithProfiles], group_id: i32) -> bool {
    if group_id == 0 {
        !global_shadowed_groups(groups).is_empty()
    } else {
        manual_shadowed(&effective_routing(groups, group_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn happ(state: &str, rules: usize) -> RoutingProfileInfo {
        RoutingProfileInfo {
            id: "happ".into(),
            name: "Happ".into(),
            state: state.into(),
            rules_count: rules,
            ..Default::default()
        }
    }

    fn group(
        id: i32,
        mode: &str,
        manual: &str,
        profile: Option<RoutingProfileInfo>,
    ) -> GroupWithProfiles {
        let active = profile.as_ref().map(|p| p.id.clone()).unwrap_or_default();
        GroupWithProfiles {
            group: Group {
                id,
                name: format!("g{id}"),
                routing_mode: mode.into(),
                manual_routing_mode: manual.into(),
                active_routing_profile_id: active,
                ..Default::default()
            },
            routing_profiles: profile.into_iter().collect(),
            ..Default::default()
        }
    }

    #[test]
    fn auto_with_happ_shadows_manual() {
        let groups = vec![group(1, "auto", "custom", Some(happ("ready", 1)))];
        let eff = effective_routing(&groups, 1);
        assert_eq!(eff.mode, "auto");
        assert_eq!(eff.source, "happ");
        assert_eq!(eff.manual_source, "custom");
        assert_eq!(eff.rules, 1);
        assert!(manual_shadowed(&eff));
        assert!(shadow_warning(&groups, 1).is_some());
    }

    #[test]
    fn json_group_uses_manual_rules_even_with_happ() {
        let mut g = group(1, "auto", "custom", Some(happ("ready", 1)));
        g.group.subscription_format = "json".into();
        let groups = vec![g];
        let eff = effective_routing(&groups, 1);
        assert!(eff.json);
        assert_eq!(eff.source, "manual");
        assert!(eff.profile.is_none());
        assert_eq!(eff.rules, 0);
        assert!(!manual_shadowed(&eff));
        assert!(shadow_warning(&groups, 1).is_none());
        assert!(describe_effective(&eff).contains("Provider JSON baseline"));
    }

    #[test]
    fn manual_mode_ignores_happ() {
        let groups = vec![group(1, "manual", "custom", Some(happ("ready", 3)))];
        let eff = effective_routing(&groups, 1);
        assert_eq!(eff.source, "manual");
        assert!(!manual_shadowed(&eff));
        assert!(shadow_warning(&groups, 1).is_none());
    }

    #[test]
    fn auto_without_usable_profile_uses_manual() {
        let groups = vec![group(1, "auto", "global", None)];
        let eff = effective_routing(&groups, 1);
        assert_eq!(eff.source, "manual");
        assert!(!manual_shadowed(&eff));

        // A ready profile with no rules is unusable too.
        let groups = vec![group(1, "auto", "global", Some(happ("ready", 0)))];
        let eff = effective_routing(&groups, 1);
        assert_eq!(eff.source, "manual");
    }

    #[test]
    fn subscription_falls_back_to_lan_and_ignores_manual() {
        let groups = vec![group(1, "subscription", "custom", Some(happ("error", 3)))];
        let eff = effective_routing(&groups, 1);
        assert_eq!(eff.source, "lan");
        assert_eq!(eff.rules, 0);
        assert!(manual_shadowed(&eff));
        assert!(describe_effective(&eff).contains("LAN only"));
    }

    #[test]
    fn global_shadowing_skips_custom_manual_and_json_groups() {
        let mut json = group(5, "subscription", "global", Some(happ("ready", 1)));
        json.group.subscription_format = "json".into();
        let groups = vec![
            group(1, "auto", "global", Some(happ("ready", 1))),
            group(2, "auto", "custom", Some(happ("ready", 1))),
            group(3, "manual", "global", None),
            group(4, "subscription", "global", None),
            json,
        ];
        assert_eq!(global_shadowed_groups(&groups), vec!["g1", "g4"]);
        assert!(shadow_warning(&groups, 0).is_some());
    }

    #[test]
    fn global_scope_is_manual() {
        let groups = vec![group(1, "auto", "global", Some(happ("ready", 1)))];
        let eff = effective_routing(&groups, 0);
        assert_eq!(eff.mode, "global");
        assert_eq!(eff.source, "manual");
        assert!(!manual_shadowed(&eff));
    }
}
