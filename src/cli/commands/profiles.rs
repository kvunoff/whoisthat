use std::collections::HashSet;
use std::io::BufRead;

use serde_json::json;

use crate::core_client::protocol::*;

use super::super::args::GlobalArgs;
use super::super::client::{Reply, Session};
use super::super::error::CliError;
use super::super::output::{redact_url, Out};
use super::super::resolve;

pub async fn list(
    g: &GlobalArgs,
    group_filter: Option<&str>,
    reveal: bool,
) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open(g).await?;
    let state = session.state().await?;
    let groups: Vec<&GroupWithProfiles> = match group_filter {
        Some(sel) => {
            let group = resolve::find_group(&state.groups, sel)?;
            state
                .groups
                .iter()
                .filter(|x| x.group.id == group.id)
                .collect()
        }
        None => state.groups.iter().collect(),
    };

    if g.json {
        let mut result = Vec::new();
        for group in groups {
            for p in &group.profiles {
                result.push(profile_json(&state, group, p, reveal));
            }
        }
        out.json(&result);
        return Ok(());
    }

    let mut rows = Vec::new();
    for group in groups {
        for p in &group.profiles {
            rows.push(vec![
                format!("{}:{}", group.group.id, p.id),
                group.group.name.clone(),
                p.name.clone(),
                p.protocol.clone(),
                ping_label(p.test_result),
                if p.loss_pct > 0 {
                    format!("{}%", p.loss_pct)
                } else {
                    "—".into()
                },
                if is_active(&state, p) {
                    "ACTIVE".into()
                } else {
                    String::new()
                },
            ]);
        }
    }
    print_table_owned(
        &["ID", "GROUP", "NAME", "PROTO", "PING", "LOSS", "ACTIVE"],
        &rows,
    );
    Ok(())
}

pub async fn show(
    g: &GlobalArgs,
    selector: &str,
    group: Option<&str>,
    reveal: bool,
) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open(g).await?;
    let state = session.state().await?;
    let (group, profile) = resolve::find_profile(&state.groups, selector, group)?;
    let group_data = state
        .groups
        .iter()
        .find(|x| x.group.id == group.id)
        .expect("group from state");

    if g.json {
        out.json(&profile_json(&state, group_data, profile, reveal));
        return Ok(());
    }

    println!("Profile: {}", profile.name);
    println!("  id:       {}:{}", group.id, profile.id);
    println!("  group:    {}", group.name);
    println!("  protocol: {}", profile.protocol);
    println!("  address:  {}", profile.address);
    if !profile.host.is_empty() {
        println!("  host:     {}", profile.host);
    }
    println!("  ping:     {}", ping_label(profile.test_result));
    if profile.loss_pct > 0 || profile.jitter_ms > 0 {
        println!(
            "  loss:     {}%  jitter: {} ms",
            profile.loss_pct, profile.jitter_ms
        );
    }
    println!("  active:   {}", is_active(&state, profile));
    if reveal {
        println!("  uri:      {}", profile.uri);
    } else {
        println!("  uri:      {} (use --reveal)", redact_url(&profile.uri));
    }
    Ok(())
}

pub async fn add(g: &GlobalArgs, uris: &[String], group: Option<&str>) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;

    let mut payload = Vec::new();
    for uri in uris {
        if uri == "-" {
            let stdin = std::io::stdin();
            for line in stdin.lock().lines() {
                let line = line?;
                let line = line.trim();
                if !line.is_empty() {
                    payload.push(line.to_string());
                }
            }
        } else if !uri.trim().is_empty() {
            payload.push(uri.trim().to_string());
        }
    }
    if payload.is_empty() {
        return Err(CliError::Usage("no profile URIs given".into()));
    }

    let group_id = match group {
        Some(sel) => match exact_group(&state.groups, sel) {
            Some(group) => group.id,
            None => {
                out.note(format!("Creating group '{sel}'..."));
                let id = session
                    .send(
                        "add-group",
                        &AddGroupData {
                            name: sel.to_string(),
                            subscription_url: String::new(),
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
                added.id
            }
        },
        None => state.groups.first().map(|g| g.group.id).ok_or_else(|| {
            CliError::Usage("no groups yet; run `whoisthat sub add <url>`".into())
        })?,
    };

    let id = session
        .send(
            "add-profiles",
            &AddProfilesData {
                uris: payload.join("\n"),
                group_id,
            },
        )
        .await?;
    let added: ProfilesAdded = session
        .wait(
            &id,
            Reply::Id {
                expect: &["profiles-added"],
                fatal_warns: &[],
            },
        )
        .await?;
    if added.profiles.is_empty() {
        return Err(CliError::Core(
            "no valid profiles were imported (check the URI syntax)".into(),
        ));
    }
    out.ok(
        format!(
            "Imported {} profile(s) into group #{group_id}.",
            added.profiles.len()
        ),
        serde_json::json!({"group_id": group_id, "imported": added.profiles.len()}),
    );
    Ok(())
}

pub async fn remove(g: &GlobalArgs, selectors: &[String], yes: bool) -> Result<(), CliError> {
    if !yes {
        return Err(CliError::Usage(
            "profile rm is destructive; pass --yes to confirm".into(),
        ));
    }
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    let mut ids = Vec::new();
    for sel in selectors {
        let (group, profile) = resolve::find_profile(&state.groups, sel, None)?;
        ids.push(ProfileID {
            id: profile.id,
            group_id: group.id,
        });
    }

    let id = session
        .send(
            "delete-profiles",
            &DeleteProfilesData {
                profiles: ids.clone(),
            },
        )
        .await?;
    let deleted: ProfilesDeleted = session
        .wait(
            &id,
            Reply::Id {
                expect: &["profiles-deleted"],
                fatal_warns: &[],
            },
        )
        .await?;
    if deleted.deleted_profiles.len() != ids.len() {
        return Err(CliError::Core(format!(
            "deleted {} of {} profiles",
            deleted.deleted_profiles.len(),
            ids.len()
        )));
    }
    out.ok(
        format!("Deleted {} profile(s).", ids.len()),
        serde_json::json!({"deleted": ids.len()}),
    );
    Ok(())
}

pub async fn rename(g: &GlobalArgs, selector: &str, new_name: &str) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    let (group, profile) = resolve::find_profile(&state.groups, selector, None)?;
    let updated: ProfileUpdated = session
        .request(
            "update-profile",
            &UpdateProfileData {
                profile: ProfileID {
                    id: profile.id,
                    group_id: group.id,
                },
                name: new_name.to_string(),
            },
            Reply::Id {
                expect: &["profile-updated"],
                fatal_warns: &["update-profile-failed"],
            },
        )
        .await?;
    out.ok(
        format!("Renamed to '{}'.", updated.profile.name),
        serde_json::json!({"profile": updated.profile}),
    );
    Ok(())
}

pub async fn move_to(g: &GlobalArgs, selector: &str, to: &str) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    let (from, profile) = resolve::find_profile(&state.groups, selector, None)?;
    let target = resolve::find_group(&state.groups, to)?;
    if from.id == target.id {
        return Err(CliError::Usage("profile is already in that group".into()));
    }
    let moved: ProfileMoved = session
        .request(
            "move-profile",
            &MoveProfileData {
                profile_id: profile.id,
                from_group_id: from.id,
                to_group_id: target.id,
            },
            Reply::Id {
                expect: &["profile-moved"],
                fatal_warns: &["move-profile-failed"],
            },
        )
        .await?;
    out.ok(
        format!(
            "Moved '{}' to group #{}.",
            moved.new_profile.name, target.id
        ),
        serde_json::json!({"profile": moved.new_profile, "group_id": target.id}),
    );
    Ok(())
}

pub async fn reorder(
    g: &GlobalArgs,
    group_sel: &str,
    selectors: &[String],
    order: Option<&str>,
) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    let (group, profile) = {
        let group = resolve::find_group(&state.groups, group_sel)?;
        let data = state
            .groups
            .iter()
            .find(|x| x.group.id == group.id)
            .expect("group from state");
        let profile = data.profiles.first().map(|p| p.id);
        (group, profile)
    };
    let _ = profile;

    let group_data = state
        .groups
        .iter()
        .find(|x| x.group.id == group.id)
        .expect("group from state");
    let current: Vec<i32> = group_data.profiles.iter().map(|p| p.id).collect();

    let mut desired = Vec::new();
    if let Some(list) = order {
        for part in list.split(',').filter(|s| !s.trim().is_empty()) {
            desired.push(part.trim().parse::<i32>().map_err(|_| {
                CliError::Usage(format!("invalid profile id in --order: '{part}'"))
            })?);
        }
    } else {
        for sel in selectors {
            let (_, p) = resolve::find_profile(&state.groups, sel, Some(group_sel))?;
            desired.push(p.id);
        }
    }

    let current_set: HashSet<i32> = current.iter().copied().collect();
    let desired_set: HashSet<i32> = desired.iter().copied().collect();
    if current_set != desired_set {
        return Err(CliError::Usage(
            "the new order must contain exactly the profiles of the group".into(),
        ));
    }

    let group_id = group.id;
    let ids_out = desired.clone();
    let _: ProfilesReordered = session
        .request(
            "reorder-profiles",
            &ReorderProfilesData {
                group_id,
                profile_ids: desired,
            },
            Reply::Id {
                expect: &["profiles-reordered"],
                fatal_warns: &["reorder-profiles-failed"],
            },
        )
        .await?;
    out.ok(
        "Profiles reordered.",
        serde_json::json!({"group_id": group_id, "profile_ids": ids_out}),
    );
    Ok(())
}

fn exact_group<'a>(groups: &'a [GroupWithProfiles], sel: &str) -> Option<&'a Group> {
    if let Ok(id) = sel.parse::<i32>() {
        return groups.iter().find(|g| g.group.id == id).map(|g| &g.group);
    }
    groups
        .iter()
        .find(|g| g.group.name.eq_ignore_ascii_case(sel))
        .map(|g| &g.group)
}

fn is_active(state: &ApplicationState, profile: &Profile) -> bool {
    state.connection_status.connection == "connected"
        && state
            .connection_status
            .profile
            .as_ref()
            .is_some_and(|p| p.id == profile.id && p.group_id == profile.group_id)
}

fn ping_label(ms: i32) -> String {
    match ms {
        i32::MIN..=-1 => "fail".into(),
        0 => "—".into(),
        _ => format!("{ms} ms"),
    }
}

fn profile_json(
    state: &ApplicationState,
    group: &GroupWithProfiles,
    p: &Profile,
    reveal: bool,
) -> serde_json::Value {
    json!({
        "id": p.id,
        "group_id": group.group.id,
        "group_name": group.group.name,
        "name": p.name,
        "protocol": p.protocol,
        "address": p.address,
        "host": p.host,
        "test_result": p.test_result,
        "loss_pct": p.loss_pct,
        "jitter_ms": p.jitter_ms,
        "tested_at": p.tested_at,
        "active": is_active(state, p),
        "uri": if reveal { Some(p.uri.as_str()) } else { None },
    })
}

fn print_table_owned(headers: &[&str], rows: &[Vec<String>]) {
    super::super::output::print_table(headers, rows);
}
