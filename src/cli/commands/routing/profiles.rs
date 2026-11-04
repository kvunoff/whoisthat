use crate::cli::args::*;
use crate::cli::client::{Reply, Session};
use crate::cli::error::CliError;
use crate::cli::output::Out;
use crate::cli::resolve;
use crate::core_client::protocol::*;

use super::profile_group_id_for;

pub(super) async fn profile_dispatch(
    g: &GlobalArgs,
    args: RouteProfileArgs,
) -> Result<(), CliError> {
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
