use super::super::args::{GlobalArgs, SystemdCmd};
use super::super::error::CliError;
use super::super::output::Out;

pub async fn dispatch(g: &GlobalArgs, cmd: SystemdCmd) -> Result<(), CliError> {
    let out = Out::new(g);
    match cmd {
        SystemdCmd::Status => {
            let enabled = crate::systemd::systemd_is_enabled();
            let linger = crate::systemd::linger_is_enabled();
            let unit = crate::systemd::systemd_service_path();
            if g.json {
                out.json(&serde_json::json!({
                    "unit": "whoisthat-core.service",
                    "path": unit.to_string_lossy(),
                    "enabled": enabled,
                    "linger": linger,
                }));
            } else {
                println!(
                    "Systemd user service (whoisthat-core.service): {}",
                    if enabled { "ENABLED" } else { "DISABLED" }
                );
                println!("Unit file: {}", unit.display());
                println!("Linger:    {}", if linger { "enabled" } else { "disabled" });
            }
            Ok(())
        }
        SystemdCmd::Enable => set(g, true).await,
        SystemdCmd::Disable => set(g, false).await,
        SystemdCmd::Toggle => set(g, !crate::systemd::systemd_is_enabled()).await,
    }
}

async fn set(g: &GlobalArgs, enable: bool) -> Result<(), CliError> {
    let out = Out::new(g);
    if enable {
        if crate::systemd::systemd_is_enabled() {
            out.ok(
                "Systemd user service is already enabled.",
                serde_json::json!({"enabled": true}),
            );
            return Ok(());
        }
        let core = crate::core_spawn::find_core_binary();
        crate::systemd::setup_systemd_service(&core).map_err(CliError::Core)?;
        out.ok(
            "Systemd user service enabled (whoisthat-core.service).",
            serde_json::json!({"enabled": true}),
        );
    } else {
        if !crate::systemd::systemd_is_enabled() {
            out.ok(
                "Systemd user service is already disabled.",
                serde_json::json!({"enabled": false}),
            );
            return Ok(());
        }
        crate::systemd::teardown_systemd_service().map_err(CliError::Core)?;
        out.ok(
            "Systemd user service disabled.",
            serde_json::json!({"enabled": false}),
        );
    }
    Ok(())
}
