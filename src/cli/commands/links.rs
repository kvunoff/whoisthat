use crate::deeplink;

use super::super::args::GlobalArgs;
use super::super::error::CliError;

pub async fn handle(g: &GlobalArgs, url: &str, group: Option<&str>) -> Result<(), CliError> {
    if g.json {
        eprintln!("note: progress output is textual; JSON is not supported for link handle");
    }
    let mut action = deeplink::parse_deeplink(url).map_err(CliError::Usage)?;
    if let Some(group) = group {
        match &mut action {
            deeplink::DeepLinkAction::ImportHappRouting { group: target, .. } => {
                *target = Some(group.to_string());
            }
            deeplink::DeepLinkAction::AddProfile { group_name, .. } => {
                *group_name = Some(group.to_string());
            }
            _ => {}
        }
    }
    deeplink::run_deeplink_action(action)
        .await
        .map_err(|e| CliError::Core(e.to_string()))
}

pub async fn register(g: &GlobalArgs) -> Result<(), CliError> {
    if g.json {
        eprintln!("note: progress output is textual; JSON is not supported for link register");
    }
    deeplink::register_desktop_handler().map_err(|e| CliError::Core(e.to_string()))
}
