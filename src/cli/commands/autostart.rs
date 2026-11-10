use super::super::args::{AutostartCmd, GlobalArgs, SystemdCmd};
use super::super::error::CliError;
use super::super::output::Out;
use crate::autostart::{self, Backend};

pub async fn dispatch(g: &GlobalArgs, cmd: AutostartCmd) -> Result<(), CliError> {
    match cmd {
        AutostartCmd::Status { backend } => status(g, parse(backend.as_deref())?).await,
        AutostartCmd::Enable { backend } => {
            let backend = parse(backend.as_deref())?;
            let target = backend
                .or_else(|| autostart::available().first().copied())
                .ok_or_else(|| CliError::Core("no autostart backend is available".into()))?;
            autostart::enable(target).map_err(CliError::Core)?;
            report(&Out::new(g), "enabled", target);
            Ok(())
        }
        AutostartCmd::Disable { backend } => {
            if let Some(backend) = parse(backend.as_deref())?
                && !autostart::available().contains(&backend)
            {
                return Err(CliError::Core(format!(
                    "backend '{}' is not available",
                    backend.name()
                )));
            }
            autostart::disable().map_err(CliError::Core)?;
            Out::new(g).ok("Autostart disabled.", serde_json::json!({"enabled": false}));
            Ok(())
        }
        AutostartCmd::Toggle { backend } => {
            let backend = parse(backend.as_deref())?;
            let status = autostart::toggle(backend).map_err(CliError::Core)?;
            let enabled = status.enabled;
            Out::new(g).ok(
                if enabled {
                    "Autostart enabled."
                } else {
                    "Autostart disabled."
                },
                serde_json::json!({
                    "enabled": enabled,
                    "backend": status.backend.map(|b| b.name()),
                }),
            );
            Ok(())
        }
    }
}

/// `whoisthat systemd ...` keeps working and forces the systemd backend.
pub async fn dispatch_legacy(g: &GlobalArgs, cmd: SystemdCmd) -> Result<(), CliError> {
    match cmd {
        SystemdCmd::Status => status(g, Some(Backend::Systemd)).await,
        SystemdCmd::Enable => {
            autostart::enable(Backend::Systemd).map_err(CliError::Core)?;
            report(&Out::new(g), "enabled", Backend::Systemd);
            Ok(())
        }
        SystemdCmd::Disable => {
            autostart::disable().map_err(CliError::Core)?;
            Out::new(g).ok("Autostart disabled.", serde_json::json!({"enabled": false}));
            Ok(())
        }
        SystemdCmd::Toggle => {
            let status = autostart::toggle(Some(Backend::Systemd)).map_err(CliError::Core)?;
            Out::new(g).ok(
                if status.enabled {
                    "Autostart enabled."
                } else {
                    "Autostart disabled."
                },
                serde_json::json!({"enabled": status.enabled}),
            );
            Ok(())
        }
    }
}

fn parse(value: Option<&str>) -> Result<Option<Backend>, CliError> {
    autostart::parse_backend(value).map_err(CliError::Usage)
}

fn report(out: &Out, action: &str, backend: Backend) {
    out.ok(
        format!(
            "Autostart {action} via {} ({})",
            backend.name(),
            autostart::artifact_path(backend).display()
        ),
        serde_json::json!({
            "enabled": true,
            "backend": backend.name(),
        }),
    );
}

async fn status(g: &GlobalArgs, forced: Option<Backend>) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut status = autostart::status();
    let backend_available = forced
        .map(|backend| status.available.contains(&backend))
        .unwrap_or_else(|| {
            status
                .backend
                .is_some_and(|b| status.available.contains(&b))
        });
    if let Some(backend) = forced {
        status.backend = Some(backend);
        status.enabled = autostart::is_enabled(backend);
        status.path = Some(autostart::artifact_path(backend));
    }
    let systemd = crate::platform::has_systemd();
    let linger = systemd && crate::systemd::linger_is_enabled();
    let available: Vec<&str> = status.available.iter().map(|b| b.name()).collect();
    let backend = status.backend.map(|b| b.name());
    if out.json {
        out.json(&serde_json::json!({
            "backend": backend,
            "backend_available": backend_available,
            "enabled": status.enabled,
            "running": status.running,
            "wrapper_running": status.wrapper_running,
            "available": available,
            "path": status.path.as_ref().map(|p| p.to_string_lossy()),
            "warning": status.warning,
            "unit": if systemd { "whoisthat-core.service" } else { "" },
            "linger": linger,
        }));
        return Ok(());
    }
    out.line(format!("Autostart backend: {}", backend.unwrap_or("none")));
    out.line(format!(
        "Available:         {}",
        if available.is_empty() {
            "none".to_string()
        } else {
            available.join(", ")
        }
    ));
    out.line(format!(
        "State:             {}",
        if status.enabled {
            "ENABLED"
        } else {
            "DISABLED"
        }
    ));
    out.line(format!(
        "Core:              {}",
        if status.running { "running" } else { "offline" }
    ));
    if status.wrapper_running {
        out.line("Init wrapper:      running");
    }
    if let Some(path) = &status.path {
        out.line(format!("Path:              {}", path.display()));
    }
    if let Some(warning) = &status.warning {
        out.line(format!("Warning:           {warning}"));
    }
    Ok(())
}
