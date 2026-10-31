use serde_json::json;

use crate::core_client::protocol::*;
use crate::ui::routing::ROUTING_PRESETS;

use super::super::args::*;
use super::super::client::{Reply, Session};
use super::super::error::CliError;
use super::super::output::{print_table, Out};
use super::super::resolve;

pub async fn dispatch(g: &GlobalArgs, cmd: RouteCmd) -> Result<(), CliError> {
    match cmd {
        RouteCmd::List(a) => list(g, a.group.as_deref()).await,
        RouteCmd::Add(a) => add(g, a).await,
        RouteCmd::Rm(a) => mutate_indexes(g, a, Action::Remove).await,
        RouteCmd::Enable(a) => mutate_indexes(g, a, Action::Enable).await,
        RouteCmd::Disable(a) => mutate_indexes(g, a, Action::Disable).await,
        RouteCmd::Move(a) => move_rule(g, a).await,
        RouteCmd::Mode(a) => set_mode(g, a).await,
        RouteCmd::Profile(a) => profile_dispatch(g, a).await,
        RouteCmd::Import(a) => import(g, a).await,
        RouteCmd::Preset(a) => preset_dispatch(g, a).await,
    }
}

enum Action {
    Remove,
    Enable,
    Disable,
}

/// Routing selection the core will apply, mirroring `db.ResolveRouting`:
/// `auto` prefers a usable Happ profile and falls back to manual rules,
/// `subscription` never falls back to manual, `manual` ignores Happ.
struct EffectiveRouting {
    mode: &'static str,
    source: &'static str, // "happ" | "manual" | "lan"
    manual_source: &'static str,
    profile: Option<String>,
    rules: usize,
}

fn effective_routing(groups: &[GroupWithProfiles], group_id: i32) -> EffectiveRouting {
    if group_id == 0 {
        return EffectiveRouting {
            mode: "global",
            source: "manual",
            manual_source: "global",
            profile: None,
            rules: 0,
        };
    }
    let Some(group) = groups.iter().find(|g| g.group.id == group_id) else {
        return EffectiveRouting {
            mode: "auto",
            source: "manual",
            manual_source: "global",
            profile: None,
            rules: 0,
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
    let source = match mode {
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
    };
    EffectiveRouting {
        mode,
        source,
        manual_source,
        profile: active.map(|p| p.name.clone()),
        rules: usable.map_or(0, |p| p.rules_count),
    }
}

fn manual_shadowed(eff: &EffectiveRouting) -> bool {
    eff.mode == "subscription" || (eff.mode == "auto" && eff.source == "happ")
}

/// Groups that use the global manual rules and currently route through Happ.
fn global_shadowed_groups(groups: &[GroupWithProfiles]) -> Vec<String> {
    groups
        .iter()
        .filter(|g| g.group.manual_routing_mode != "custom")
        .filter(|g| manual_shadowed(&effective_routing(groups, g.group.id)))
        .map(|g| g.group.name.clone())
        .collect()
}

fn describe_effective(eff: &EffectiveRouting) -> String {
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

fn shadow_warning(groups: &[GroupWithProfiles], group_id: i32) -> Option<String> {
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

fn manual_shadowed_for_edit(groups: &[GroupWithProfiles], group_id: i32) -> bool {
    if group_id == 0 {
        !global_shadowed_groups(groups).is_empty()
    } else {
        manual_shadowed(&effective_routing(groups, group_id))
    }
}

async fn list(g: &GlobalArgs, group: Option<&str>) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open(g).await?;
    let state = session.state().await?;
    let group_id = group_id_for(&state, group)?;
    let config = get_config(&mut session, group_id).await?;
    let eff = effective_routing(&state.groups, group_id);
    let shadowed = manual_shadowed_for_edit(&state.groups, group_id);
    let warning = shadow_warning(&state.groups, group_id);

    if g.json {
        let mut payload = serde_json::to_value(&config).unwrap_or_default();
        payload["effective"] = json!({
            "mode": eff.mode,
            "source": eff.source,
            "manual_source": eff.manual_source,
            "profile": eff.profile,
            "profile_rules": eff.rules,
        });
        payload["manual_shadowed"] = json!(shadowed);
        if group_id == 0 {
            payload["shadowed_groups"] = json!(global_shadowed_groups(&state.groups));
        }
        out.json(&payload);
        return Ok(());
    }
    out.line(format!("Effective: {}", describe_effective(&eff)));
    if let Some(warning) = &warning {
        out.line(warning);
    }
    let mut rows = Vec::new();
    for (i, rule) in config.rules.iter().enumerate() {
        rows.push(vec![
            (i + 1).to_string(),
            rule_field(rule).to_string(),
            rule.outbound_tag.clone(),
            if rule.enabled { "on" } else { "off" }.into(),
        ]);
    }
    if rows.is_empty() {
        out.line("No routing rules.");
    } else {
        print_table(&["#", "MATCH", "OUTBOUND", "ENABLED"], &rows);
    }
    Ok(())
}

async fn add(g: &GlobalArgs, args: RouteAddArgs) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    let group_id = group_id_for(&state, args.group.as_deref())?;
    if group_id != 0 {
        if let Some(group) = state.groups.iter().find(|g| g.group.id == group_id) {
            if group.group.manual_routing_mode.is_empty()
                || group.group.manual_routing_mode == "inherit"
            {
                out.note("note: editing rules switches this group from inherit to custom");
            }
        }
    }
    if let Some(warning) = shadow_warning(&state.groups, group_id) {
        out.note(warning);
    }
    let shadowed = manual_shadowed_for_edit(&state.groups, group_id);
    if !matches!(args.outbound.as_str(), "proxy" | "direct" | "block") {
        return Err(CliError::Usage(
            "outbound must be proxy|direct|block".into(),
        ));
    }
    let mut rule = RoutingRule {
        r#type: "field".into(),
        outbound_tag: args.outbound,
        enabled: true,
        ..Default::default()
    };
    match args.match_type.as_str() {
        "domain" | "geosite" => rule.domain = args.value,
        "ip" | "geoip" => rule.ip = args.value,
        "protocol" => rule.protocol = args.value,
        "port" => rule.port = args.value,
        _ => {
            return Err(CliError::Usage(
                "type must be domain|ip|protocol|port|geoip|geosite".into(),
            ))
        }
    }

    let mut config = get_config(&mut session, group_id).await?;
    config.rules.push(rule);
    update_config(&mut session, group_id, config).await?;
    out.ok(
        "Routing rule added.",
        serde_json::json!({"added": true, "manual_shadowed": shadowed}),
    );
    Ok(())
}

async fn mutate_indexes(
    g: &GlobalArgs,
    args: RouteIndexArgs,
    action: Action,
) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    let group_id = group_id_for(&state, args.group.as_deref())?;
    if let Some(warning) = shadow_warning(&state.groups, group_id) {
        out.note(warning);
    }
    let shadowed = manual_shadowed_for_edit(&state.groups, group_id);
    let mut config = get_config(&mut session, group_id).await?;

    let len = config.rules.len();
    for index in &args.indexes {
        if *index == 0 || *index > len {
            return Err(CliError::Usage(format!(
                "rule index {index} out of range 1..{len}"
            )));
        }
    }
    let (human, result) = match action {
        Action::Remove => {
            let mut indexes = args.indexes.clone();
            indexes.sort_unstable();
            indexes.dedup();
            for index in indexes.into_iter().rev() {
                config.rules.remove(index - 1);
            }
            (
                "Routing rule(s) removed.".to_string(),
                json!({"indexes": args.indexes}),
            )
        }
        Action::Enable | Action::Disable => {
            let enable = matches!(action, Action::Enable);
            for index in &args.indexes {
                config.rules[*index - 1].enabled = enable;
            }
            (
                format!(
                    "Routing rule(s) {}.",
                    if enable { "enabled" } else { "disabled" }
                ),
                json!({"indexes": args.indexes, "enabled": enable}),
            )
        }
    };
    update_config(&mut session, group_id, config).await?;
    let mut result = result;
    result["manual_shadowed"] = json!(shadowed);
    out.ok(human, result);
    Ok(())
}

async fn move_rule(g: &GlobalArgs, args: RouteMoveArgs) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    let group_id = group_id_for(&state, args.group.as_deref())?;
    if let Some(warning) = shadow_warning(&state.groups, group_id) {
        out.note(warning);
    }
    let shadowed = manual_shadowed_for_edit(&state.groups, group_id);
    let mut config = get_config(&mut session, group_id).await?;
    let len = config.rules.len();
    if args.index == 0 || args.index > len || args.new_index == 0 || args.new_index > len {
        return Err(CliError::Usage(format!("rule indexes must be in 1..{len}")));
    }
    let rule = config.rules.remove(args.index - 1);
    config.rules.insert(args.new_index - 1, rule);
    update_config(&mut session, group_id, config).await?;
    out.ok(
        "Routing rule moved.",
        serde_json::json!({
            "from": args.index,
            "to": args.new_index,
            "manual_shadowed": shadowed,
        }),
    );
    Ok(())
}

async fn set_mode(g: &GlobalArgs, args: RouteModeArgs) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    let group = resolve::find_group(&state.groups, &args.selector)?;
    match args.mode.as_str() {
        "inherit" | "custom" => {
            if group.id == 0 {
                return Err(CliError::Usage(
                    "global routing cannot inherit; use auto|manual|subscription".into(),
                ));
            }
            let _: RoutingUpdated = session
                .request(
                    "set-manual-routing-source",
                    &SetManualRoutingSourceData {
                        group_id: group.id,
                        source: args.mode.clone(),
                    },
                    Reply::Id {
                        expect: &["routing-updated"],
                        fatal_warns: &["set-manual-routing-source-failed"],
                    },
                )
                .await?;
        }
        "auto" | "manual" | "subscription" => {
            let _: Group = session
                .request(
                    "set-group-routing-mode",
                    &SetGroupRoutingModeData {
                        group_id: group.id,
                        mode: args.mode.clone(),
                    },
                    Reply::Id {
                        expect: &["group-updated"],
                        fatal_warns: &["set-group-routing-mode-failed"],
                    },
                )
                .await?;
        }
        _ => {
            return Err(CliError::Usage(
                "mode must be inherit|custom|auto|manual|subscription".into(),
            ))
        }
    }
    out.ok(
        format!("Routing mode set to {}.", args.mode),
        serde_json::json!({"mode": args.mode}),
    );
    Ok(())
}

async fn profile_dispatch(g: &GlobalArgs, args: RouteProfileArgs) -> Result<(), CliError> {
    match args.cmd {
        RouteProfileCmd::List(a) => {
            let out = Out::new(g);
            let mut session = Session::open(g).await?;
            let state = session.state().await?;
            let gid = profile_group_id_for(&state, a.group.as_deref())?;
            let group = state
                .groups
                .iter()
                .find(|x| x.group.id == gid)
                .ok_or_else(|| CliError::Usage("group not found".into()))?;
            if g.json {
                out.json(&group.routing_profiles);
            } else {
                for rp in &group.routing_profiles {
                    println!(
                        "{} {} [{}] rules={}{}",
                        if rp.active { "*" } else { " " },
                        rp.name,
                        rp.state,
                        rp.rules_count,
                        if rp.fake_dns_enabled { " fake-dns" } else { "" }
                    );
                }
            }
            Ok(())
        }
        RouteProfileCmd::Activate(a) => {
            let out = Out::new(g);
            let mut session = Session::open_or_spawn(g).await?;
            let state = session.state().await?;
            let gid = profile_group_id_for(&state, a.group.as_deref())?;
            let group = state
                .groups
                .iter()
                .find(|x| x.group.id == gid)
                .ok_or_else(|| CliError::Usage("group not found".into()))?;
            let rp = resolve::find_routing_profile(group, &a.profile)?;
            let _: Group = session
                .request(
                    "set-active-routing-profile",
                    &SetActiveRoutingProfileData {
                        group_id: gid,
                        profile_id: rp.id.clone(),
                    },
                    Reply::Id {
                        expect: &["group-updated"],
                        fatal_warns: &["set-active-routing-profile-failed"],
                    },
                )
                .await?;
            out.ok(
                format!("Activated routing profile '{}'.", rp.name),
                serde_json::json!({"routing_profile": rp.name}),
            );
            Ok(())
        }
        RouteProfileCmd::Off(a) => {
            let out = Out::new(g);
            let mut session = Session::open_or_spawn(g).await?;
            let state = session.state().await?;
            let gid = profile_group_id_for(&state, a.group.as_deref())?;
            let _: Group = session
                .request(
                    "set-active-routing-profile",
                    &SetActiveRoutingProfileData {
                        group_id: gid,
                        profile_id: "off".into(),
                    },
                    Reply::Id {
                        expect: &["group-updated"],
                        fatal_warns: &["set-active-routing-profile-failed"],
                    },
                )
                .await?;
            out.ok(
                "Routing profile deactivated.",
                serde_json::json!({"active": false}),
            );
            Ok(())
        }
        RouteProfileCmd::Rm(a) => {
            let out = Out::new(g);
            let mut session = Session::open_or_spawn(g).await?;
            let state = session.state().await?;
            let gid = profile_group_id_for(&state, a.group.as_deref())?;
            let group = state
                .groups
                .iter()
                .find(|x| x.group.id == gid)
                .ok_or_else(|| CliError::Usage("group not found".into()))?;
            let rp = resolve::find_routing_profile(group, &a.profile)?;
            session.set_timeout(g.timeout.unwrap_or(30));
            let _: SubscriptionUpdated = session
                .request(
                    "delete-routing-profile",
                    &DeleteRoutingProfileData {
                        group_id: gid,
                        profile_id: rp.id.clone(),
                    },
                    Reply::Id {
                        expect: &["subscription-updated"],
                        fatal_warns: &["delete-routing-profile-failed"],
                    },
                )
                .await?;
            out.ok(
                format!("Deleted routing profile '{}'.", rp.name),
                serde_json::json!({"deleted": rp.name}),
            );
            Ok(())
        }
    }
}

async fn import(g: &GlobalArgs, args: RouteImportArgs) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    let gid = profile_group_id_for(&state, args.group.as_deref())?;
    session.set_timeout(g.timeout.unwrap_or(30));
    let _: serde_json::Value = session
        .request(
            "import-happ-routing",
            &ImportHappRoutingData {
                group_id: gid,
                link: args.link,
            },
            Reply::Id {
                expect: &["subscription-updated", "group-updated"],
                fatal_warns: &["import-happ-routing-failed"],
            },
        )
        .await?;
    out.ok(
        "Happ routing imported.",
        serde_json::json!({"imported": true}),
    );
    Ok(())
}

async fn preset_dispatch(g: &GlobalArgs, args: RoutePresetArgs) -> Result<(), CliError> {
    match args.cmd {
        RoutePresetCmd::List => {
            let out = Out::new(g);
            if g.json {
                let list: Vec<serde_json::Value> = ROUTING_PRESETS
                    .iter()
                    .map(|p| {
                        json!({
                            "id": p.id,
                            "title": p.title,
                            "description": p.desc,
                            "rules": p.rules.len(),
                        })
                    })
                    .collect();
                out.json(&list);
            } else {
                for p in ROUTING_PRESETS {
                    println!("{}  {}", p.id, p.title);
                    println!("    {}", p.desc);
                }
            }
            Ok(())
        }
        RoutePresetCmd::Apply(a) => preset_apply(g, &a.id, a.group.as_deref(), true).await,
        RoutePresetCmd::Remove(a) => preset_apply(g, &a.id, a.group.as_deref(), false).await,
    }
}

async fn preset_apply(
    g: &GlobalArgs,
    id: &str,
    group: Option<&str>,
    apply: bool,
) -> Result<(), CliError> {
    let out = Out::new(g);
    let preset = ROUTING_PRESETS
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| CliError::Usage(format!("unknown preset '{id}'")))?;
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    let gid = group_id_for(&state, group)?;
    if let Some(warning) = shadow_warning(&state.groups, gid) {
        out.note(warning);
    }
    let shadowed = manual_shadowed_for_edit(&state.groups, gid);
    let mut config = get_config(&mut session, gid).await?;

    if apply {
        for rule in preset.rules {
            let r = rule.to_routing_rule();
            if !config.rules.iter().any(|x| {
                x.outbound_tag == r.outbound_tag
                    && x.domain == r.domain
                    && x.ip == r.ip
                    && x.protocol == r.protocol
                    && x.port == r.port
            }) {
                config.rules.push(r);
            }
        }
    } else {
        config
            .rules
            .retain(|x| !preset.rules.iter().any(|pr| pr.matches_rule(x)));
    }
    update_config(&mut session, gid, config).await?;
    out.ok(
        format!(
            "Preset '{}' {}.",
            preset.title,
            if apply { "applied" } else { "removed" }
        ),
        serde_json::json!({
            "preset": preset.id,
            "applied": apply,
            "manual_shadowed": shadowed,
        }),
    );
    Ok(())
}

async fn get_config(session: &mut Session, group_id: i32) -> Result<RoutingConfig, CliError> {
    let updated: RoutingUpdated = session
        .request(
            "get-routing",
            &GetRoutingData { group_id },
            Reply::Id {
                expect: &["routing-updated"],
                fatal_warns: &["get-routing-failed", "update-routing-failed"],
            },
        )
        .await?;
    Ok(updated.config)
}

async fn update_config(
    session: &mut Session,
    group_id: i32,
    config: RoutingConfig,
) -> Result<(), CliError> {
    let _: RoutingUpdated = session
        .request(
            "update-routing",
            &UpdateRoutingData { group_id, config },
            Reply::Id {
                expect: &["routing-updated"],
                fatal_warns: &["get-routing-failed", "update-routing-failed"],
            },
        )
        .await?;
    Ok(())
}

/// Routing rules without `--group` target the global config (group 0).
pub fn group_id_for(state: &ApplicationState, group: Option<&str>) -> Result<i32, CliError> {
    match group {
        Some(sel) => Ok(resolve::find_group(&state.groups, sel)?.id),
        None => {
            let _ = state;
            Ok(0)
        }
    }
}

/// Routing profiles always belong to a concrete group.
pub fn profile_group_id_for(
    state: &ApplicationState,
    group: Option<&str>,
) -> Result<i32, CliError> {
    if let Some(sel) = group {
        return Ok(resolve::find_group(&state.groups, sel)?.id);
    }
    if state.connection_status.connection == "connected" {
        if let Some(p) = &state.connection_status.profile {
            return Ok(p.group_id);
        }
    }
    state
        .groups
        .first()
        .map(|g| g.group.id)
        .ok_or_else(|| CliError::Usage("no groups yet".into()))
}

fn rule_field(rule: &RoutingRule) -> String {
    if !rule.domain.is_empty() {
        rule.domain.clone()
    } else if !rule.ip.is_empty() {
        rule.ip.clone()
    } else if !rule.protocol.is_empty() {
        format!("protocol:{}", rule.protocol)
    } else if !rule.port.is_empty() {
        format!("port:{}", rule.port)
    } else {
        "—".into()
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
    fn global_shadowing_skips_custom_and_manual_groups() {
        let groups = vec![
            group(1, "auto", "global", Some(happ("ready", 1))),
            group(2, "auto", "custom", Some(happ("ready", 1))),
            group(3, "manual", "global", None),
            group(4, "subscription", "global", None),
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
