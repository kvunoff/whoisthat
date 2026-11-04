use serde_json::json;

use crate::cli::args::*;
use crate::cli::client::{Reply, Session};
use crate::cli::error::CliError;
use crate::cli::output::Out;
use crate::core_client::protocol::*;
use crate::ui::routing::ROUTING_PRESETS;

use super::effective::{manual_shadowed_for_edit, shadow_warning};
use super::group_id_for;
use super::profile_group_id_for;
use super::rules::{get_config, update_config};

pub(super) async fn import(g: &GlobalArgs, args: RouteImportArgs) -> Result<(), CliError> {
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

pub(super) async fn preset_dispatch(g: &GlobalArgs, args: RoutePresetArgs) -> Result<(), CliError> {
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

pub(super) async fn preset_apply(
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
