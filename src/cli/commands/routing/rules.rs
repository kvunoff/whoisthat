use serde_json::json;

use crate::cli::args::*;
use crate::cli::client::{Reply, Session};
use crate::cli::error::CliError;
use crate::cli::output::{Out, print_table};
use crate::cli::resolve;
use crate::core_client::protocol::*;

use super::effective::*;
use super::group_id_for;

pub(super) enum Action {
    Remove,
    Enable,
    Disable,
}

pub(super) async fn list(g: &GlobalArgs, group: Option<&str>) -> Result<(), CliError> {
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
            "json": eff.json,
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

pub(super) async fn add(g: &GlobalArgs, args: RouteAddArgs) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    let group_id = group_id_for(&state, args.group.as_deref())?;
    if group_id != 0
        && let Some(group) = state.groups.iter().find(|g| g.group.id == group_id)
        && (group.group.manual_routing_mode.is_empty()
            || group.group.manual_routing_mode == "inherit")
    {
        out.note("note: editing rules switches this group from inherit to custom");
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
            ));
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

pub(super) async fn mutate_indexes(
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

pub(super) async fn move_rule(g: &GlobalArgs, args: RouteMoveArgs) -> Result<(), CliError> {
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

pub(super) async fn set_mode(g: &GlobalArgs, args: RouteModeArgs) -> Result<(), CliError> {
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
            ));
        }
    }
    out.ok(
        format!("Routing mode set to {}.", args.mode),
        serde_json::json!({"mode": args.mode}),
    );
    Ok(())
}

pub(super) async fn get_config(
    session: &mut Session,
    group_id: i32,
) -> Result<RoutingConfig, CliError> {
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

pub(super) async fn update_config(
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

pub(super) fn rule_field(rule: &RoutingRule) -> String {
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
