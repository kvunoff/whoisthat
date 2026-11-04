use crate::cli::args::{GlobalArgs, RouteCmd};
use crate::cli::error::CliError;
use crate::cli::resolve;
use crate::core_client::protocol::ApplicationState;

mod effective;
mod presets;
mod profiles;
mod rules;

use presets::{import, preset_dispatch};
use profiles::profile_dispatch;
use rules::{Action, add, list, move_rule, mutate_indexes, set_mode};

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
    if state.connection_status.connection == "connected"
        && let Some(p) = &state.connection_status.profile
    {
        return Ok(p.group_id);
    }
    state
        .groups
        .first()
        .map(|g| g.group.id)
        .ok_or_else(|| CliError::Usage("no groups yet".into()))
}
