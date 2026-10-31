use serde_json::json;

use crate::core_client::protocol::*;

use super::super::args::GlobalArgs;
use super::super::client::{Reply, Session};
use super::super::error::CliError;
use super::super::output::{format_bytes, format_epoch, print_table, redact_url, Out};
use super::super::resolve;

pub async fn list(g: &GlobalArgs, reveal: bool) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open(g).await?;
    let state = session.state().await?;

    if g.json {
        let groups: Vec<serde_json::Value> = state
            .groups
            .iter()
            .map(|g| group_json(&state, g, reveal))
            .collect();
        out.json(&groups);
        return Ok(());
    }

    let rows: Vec<Vec<String>> = state
        .groups
        .iter()
        .map(|g| {
            let gr = &g.group;
            let used = gr.sub_upload + gr.sub_download;
            vec![
                gr.id.to_string(),
                gr.name.clone(),
                g.profiles.len().to_string(),
                format_epoch(gr.sub_last_updated),
                format_epoch(gr.sub_expires),
                format!("{} / {}", format_bytes(used), format_bytes(gr.sub_total)),
            ]
        })
        .collect();
    print_table(
        &[
            "ID",
            "NAME",
            "PROFILES",
            "UPDATED",
            "EXPIRES",
            "USED / TOTAL",
        ],
        &rows,
    );
    Ok(())
}

pub async fn show(g: &GlobalArgs, selector: &str, reveal: bool) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open(g).await?;
    let state = session.state().await?;
    let group_id = resolve::find_group(&state.groups, selector)?.id;
    let group = state
        .groups
        .iter()
        .find(|x| x.group.id == group_id)
        .expect("group from state");

    if g.json {
        out.json(&group_json(&state, group, reveal));
        return Ok(());
    }

    let gr = &group.group;
    println!("Group #{}: {}", gr.id, gr.name);
    println!("  profiles:  {}", group.profiles.len());
    println!("  updated:   {}", format_epoch(gr.sub_last_updated));
    println!("  expires:   {}", format_epoch(gr.sub_expires));
    println!(
        "  traffic:   {} / {}",
        format_bytes(gr.sub_upload + gr.sub_download),
        format_bytes(gr.sub_total)
    );
    println!(
        "  routing:   {} / {}",
        gr.routing_mode, gr.manual_routing_mode
    );
    if reveal {
        println!("  url:       {}", gr.subscription_url);
    } else if !gr.subscription_url.is_empty() {
        println!(
            "  url:       {} (use --reveal)",
            redact_url(&gr.subscription_url)
        );
    }
    if !group.routing_profiles.is_empty() {
        println!("  routing profiles:");
        for rp in &group.routing_profiles {
            println!(
                "    {} {} ({} rules{})",
                if rp.active { "*" } else { " " },
                rp.name,
                rp.rules_count,
                if rp.fake_dns_enabled {
                    ", fake-dns"
                } else {
                    ""
                }
            );
        }
    }
    Ok(())
}

pub async fn add(
    g: &GlobalArgs,
    url: &str,
    name: Option<&str>,
    no_wait: bool,
) -> Result<(), CliError> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(CliError::Usage(
            "subscription URL must start with http:// or https://".into(),
        ));
    }
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    // Without an explicit name the core uses profile-title from the
    // subscription response, falling back to the URL host.
    out.note("Adding subscription...".to_string());
    let id = session
        .send(
            "add-group",
            &AddGroupData {
                name: name.unwrap_or_default().to_string(),
                subscription_url: url.to_string(),
            },
        )
        .await?;
    let added: GroupAdded = session
        .wait(
            &id,
            Reply::Id {
                expect: &["group-added"],
                fatal_warns: &["add-group-failed"],
            },
        )
        .await?;

    if no_wait {
        out.ok(
            format!("Subscription '{}' added (#{}).", added.name, added.id),
            json!({"group_id": added.id, "updated": false}),
        );
        return Ok(());
    }

    session.set_timeout(g.timeout.unwrap_or(60));
    match session
        .wait::<SubscriptionUpdated>(
            &id,
            Reply::Id {
                expect: &["subscription-updated"],
                fatal_warns: &["update-subscription-failed", "command-ignored"],
            },
        )
        .await
    {
        Ok(updated) => out.ok(
            format!(
                "Subscription '{}' added ({} profiles imported).",
                updated.group.name,
                updated.profiles.len()
            ),
            json!({"group_id": updated.group_id, "profiles": updated.profiles.len()}),
        ),
        Err(CliError::Core(msg)) => {
            out.ok(
                format!(
                    "Subscription '{}' added (#{}), but update failed: {msg}",
                    added.name, added.id
                ),
                json!({"group_id": added.id, "updated": false, "error": msg}),
            );
        }
        Err(e) => return Err(e),
    }
    Ok(())
}

pub async fn remove(g: &GlobalArgs, selector: &str, yes: bool) -> Result<(), CliError> {
    if !yes {
        return Err(CliError::Usage(
            "sub rm deletes the group and its profiles; pass --yes to confirm".into(),
        ));
    }
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    let group = resolve::find_group(&state.groups, selector)?;
    if state.connection_status.connection == "connected"
        && state
            .connection_status
            .profile
            .as_ref()
            .is_some_and(|p| p.group_id == group.id)
    {
        out.note(format!(
            "warning: group '{}' is currently connected",
            group.name
        ));
    }
    let id = group.id;
    let _: GroupDeleted = session
        .request(
            "delete-group",
            &DeleteGroupData { id },
            Reply::Id {
                expect: &["group-deleted"],
                fatal_warns: &["delete-group-failed"],
            },
        )
        .await?;
    out.ok(
        format!("Deleted subscription '{name}' (#{id}).", name = group.name),
        json!({"group_id": id, "deleted": true}),
    );
    Ok(())
}

pub async fn refresh(
    g: &GlobalArgs,
    selector: Option<&str>,
    all: bool,
    no_wait: bool,
) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;

    let targets: Vec<Group> = if all {
        state.groups.iter().map(|g| g.group.clone()).collect()
    } else if let Some(sel) = selector {
        vec![resolve::find_group(&state.groups, sel)?.clone()]
    } else if state.connection_status.connection == "connected" {
        state
            .connection_status
            .profile
            .as_ref()
            .and_then(|p| {
                state
                    .groups
                    .iter()
                    .find(|g| g.group.id == p.group_id)
                    .map(|g| g.group.clone())
            })
            .into_iter()
            .collect()
    } else {
        let id = session.cfg.last_group_id;
        state
            .groups
            .iter()
            .find(|g| g.group.id == id)
            .map(|g| g.group.clone())
            .into_iter()
            .collect()
    };

    if targets.is_empty() {
        return Err(CliError::Usage(
            "no subscription selected; pass a group or --all".into(),
        ));
    }

    for group in targets {
        let id = session
            .send(
                "update-subscription",
                &UpdateSubscriptionData { group_id: group.id },
            )
            .await?;
        if no_wait {
            out.ok(
                format!("Refreshing '{}' (#{})...", group.name, group.id),
                json!({"group_id": group.id, "refreshing": true}),
            );
            continue;
        }
        session.set_timeout(g.timeout.unwrap_or(60));
        let updated: SubscriptionUpdated = session
            .wait(
                &id,
                Reply::Id {
                    expect: &["subscription-updated"],
                    fatal_warns: &["update-subscription-failed", "command-ignored"],
                },
            )
            .await?;
        out.ok(
            format!(
                "Refreshed '{}' (#{}): {} profiles.",
                group.name,
                group.id,
                updated.profiles.len()
            ),
            json!({"group_id": updated.group_id, "profiles": updated.profiles.len()}),
        );
    }
    Ok(())
}

pub async fn rename(
    g: &GlobalArgs,
    selector: &str,
    new_name: Option<&str>,
) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    let group = resolve::find_group(&state.groups, selector)?.clone();
    let updated: Group = session
        .request(
            "update-group",
            &UpdateGroupData {
                id: group.id,
                // An empty name resets the group to the provider's name.
                name: new_name.unwrap_or_default().to_string(),
                subscription_url: group.subscription_url.clone(),
            },
            Reply::Id {
                expect: &["group-updated"],
                fatal_warns: &["update-group-failed"],
            },
        )
        .await?;
    let action = if new_name.is_some() {
        "Renamed"
    } else {
        "Reset"
    };
    out.ok(
        format!("{action} group #{}: '{}'.", updated.id, updated.name),
        json!({"group_id": updated.id, "name": updated.name}),
    );
    Ok(())
}

pub async fn url(
    g: &GlobalArgs,
    selector: &str,
    set: Option<&str>,
    reveal: bool,
) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    let group = resolve::find_group(&state.groups, selector)?.clone();

    if let Some(new_url) = set {
        if !new_url.starts_with("http://") && !new_url.starts_with("https://") {
            return Err(CliError::Usage(
                "subscription URL must start with http:// or https://".into(),
            ));
        }
        let updated: Group = session
            .request(
                "update-group",
                &UpdateGroupData {
                    id: group.id,
                    name: group.name.clone(),
                    subscription_url: new_url.to_string(),
                },
                Reply::Id {
                    expect: &["group-updated"],
                    fatal_warns: &["update-group-failed"],
                },
            )
            .await?;
        out.ok(
            format!(
                "Updated subscription URL for '{}' (#{}).",
                updated.name, updated.id
            ),
            json!({"group_id": updated.id, "updated": true}),
        );
        return Ok(());
    }

    let shown = if reveal {
        group.subscription_url.clone()
    } else {
        redact_url(&group.subscription_url)
    };
    if g.json {
        out.json(&json!({"group_id": group.id, "url": shown}));
    } else {
        println!("{shown}");
    }
    Ok(())
}

pub async fn reorder(g: &GlobalArgs, selector: &str, new_index: usize) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    let group = resolve::find_group(&state.groups, selector)?;
    let mut ids: Vec<i32> = state.groups.iter().map(|g| g.group.id).collect();
    let from = ids
        .iter()
        .position(|id| *id == group.id)
        .ok_or_else(|| CliError::Usage("group not found".into()))?;
    let value = ids.remove(from);
    let to = new_index.saturating_sub(1).min(ids.len());
    ids.insert(to, value);

    let _: GroupsReordered = session
        .request(
            "reorder-groups",
            &ReorderGroupsData { group_ids: ids },
            Reply::Id {
                expect: &["groups-reordered"],
                fatal_warns: &["reorder-groups-failed"],
            },
        )
        .await?;
    out.ok("Groups reordered.", json!({"reordered": true}));
    Ok(())
}

fn group_json(
    state: &ApplicationState,
    group: &GroupWithProfiles,
    reveal: bool,
) -> serde_json::Value {
    let gr = &group.group;
    let active = state.connection_status.connection == "connected"
        && state
            .connection_status
            .profile
            .as_ref()
            .is_some_and(|p| p.group_id == gr.id);
    json!({
        "id": gr.id,
        "name": gr.name,
        "profiles_count": group.profiles.len(),
        "subscription_url": if reveal { Some(gr.subscription_url.as_str()) } else { None },
        "last_updated": gr.sub_last_updated,
        "expires": gr.sub_expires,
        "upload": gr.sub_upload,
        "download": gr.sub_download,
        "total": gr.sub_total,
        "routing_mode": gr.routing_mode,
        "manual_routing_mode": gr.manual_routing_mode,
        "active": active,
        "routing_profiles": group.routing_profiles,
    })
}
