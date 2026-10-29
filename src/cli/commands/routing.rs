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

async fn list(g: &GlobalArgs, group: Option<&str>) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open(g).await?;
    let state = session.state().await?;
    let group_id = group_id_for(&state, group)?;
    let config = get_config(&mut session, group_id).await?;

    if g.json {
        out.json(&config);
        return Ok(());
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
    out.ok("Routing rule added.", serde_json::json!({"added": true}));
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
    let mut config = get_config(&mut session, group_id).await?;

    let len = config.rules.len();
    for index in &args.indexes {
        if *index == 0 || *index > len {
            return Err(CliError::Usage(format!(
                "rule index {index} out of range 1..{len}"
            )));
        }
    }
    match action {
        Action::Remove => {
            let mut indexes = args.indexes.clone();
            indexes.sort_unstable();
            indexes.dedup();
            for index in indexes.into_iter().rev() {
                config.rules.remove(index - 1);
            }
            out.ok(
                "Routing rule(s) removed.",
                serde_json::json!({"indexes": args.indexes}),
            );
        }
        Action::Enable | Action::Disable => {
            let enable = matches!(action, Action::Enable);
            for index in &args.indexes {
                config.rules[*index - 1].enabled = enable;
            }
            out.ok(
                format!(
                    "Routing rule(s) {}.",
                    if enable { "enabled" } else { "disabled" }
                ),
                serde_json::json!({"indexes": args.indexes, "enabled": enable}),
            );
        }
    }
    update_config(&mut session, group_id, config).await?;
    Ok(())
}

async fn move_rule(g: &GlobalArgs, args: RouteMoveArgs) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    let group_id = group_id_for(&state, args.group.as_deref())?;
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
        serde_json::json!({"from": args.index, "to": args.new_index}),
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
        serde_json::json!({"preset": preset.id, "applied": apply}),
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
