use std::path::Path;
use std::time::Duration;

use crate::config;
use crate::core_client::connection::CoreConnection;
use crate::systemd;

use super::super::args::{CoreCmd, GlobalArgs};
use super::super::client::{endpoint, Reply, Session};
use super::super::error::CliError;
use super::super::output::Out;

pub async fn dispatch(g: &GlobalArgs, cmd: CoreCmd) -> Result<(), CliError> {
    match cmd {
        CoreCmd::Status => status(g).await,
        CoreCmd::Start => start(g).await,
        CoreCmd::Stop => stop(g).await,
        CoreCmd::Restart => restart(g).await,
        CoreCmd::Version => super::connection::version(g).await,
    }
}

async fn status(g: &GlobalArgs) -> Result<(), CliError> {
    let out = Out::new(g);
    let cfg = config::load_config();
    let ep = endpoint(g, &cfg)?;
    let binary = crate::core_spawn::find_core_binary();
    let caps = crate::core_spawn::has_cap_net(Path::new(&binary));

    match Session::open(g).await {
        Ok(mut session) => {
            let state = session.state().await?;
            let xray = state.xray_status.as_ref().map(|x| {
                serde_json::json!({
                    "version": x.version,
                    "target": x.target_version,
                    "status": x.status,
                })
            });
            if g.json {
                out.json(&serde_json::json!({
                    "running": true,
                    "version": state.core_version,
                    "protocol": state.protocol_version,
                    "socket": ep.describe(),
                    "binary": binary,
                    "capabilities": caps,
                    "connected": state.connection_status.connection == "connected",
                    "tun_enabled": state.tun_status,
                    "xray": xray,
                }));
            } else {
                println!("Core:        running");
                println!("Version:     {}", state.core_version);
                println!("Protocol:    {}", state.protocol_version);
                println!("Socket:      {}", ep.describe());
                println!("Binary:      {binary}");
                println!(
                    "Capabilities:{}",
                    if caps { " cap_net_admin" } else { " missing" }
                );
                println!(
                    "Connection:  {}{}",
                    state.connection_status.connection,
                    if state.tun_status {
                        " (TUN)"
                    } else {
                        " (proxy)"
                    }
                );
                if let Some(x) = state.xray_status {
                    println!("Xray:        {} ({})", x.version, x.status);
                }
            }
            Ok(())
        }
        Err(_) => {
            if g.json {
                out.json(&serde_json::json!({
                    "running": false,
                    "version": null,
                    "protocol": null,
                    "socket": ep.describe(),
                    "binary": binary,
                    "capabilities": caps,
                }));
            } else {
                println!("Core:        offline");
                println!("Socket:      {}", ep.describe());
                println!("Binary:      {binary}");
                println!(
                    "Capabilities:{}",
                    if caps { " cap_net_admin" } else { " missing" }
                );
            }
            Ok(())
        }
    }
}

async fn start(g: &GlobalArgs) -> Result<(), CliError> {
    let out = Out::new(g);
    if g.tcp.is_some() || g.socket.is_some() {
        return Err(CliError::Usage(
            "core start cannot bind a custom --tcp/--socket endpoint; start the daemon yourself"
                .into(),
        ));
    }
    if Session::open(g).await.is_ok() {
        out.ok(
            "Core is already running.",
            serde_json::json!({"running": true, "started": false}),
        );
        return Ok(());
    }
    let cfg = config::load_config();
    crate::core_spawn::spawn_core(&cfg.log_level)
        .map_err(|e| CliError::Offline(format!("failed to start whoisthat-core: {e}")))?;
    let mut session = wait_for_core(g).await?;
    let state = session.state().await?;
    out.ok(
        format!("Core started (v{}).", state.core_version),
        serde_json::json!({"running": true, "version": state.core_version}),
    );
    Ok(())
}

async fn stop(g: &GlobalArgs) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = match Session::open(g).await {
        Ok(s) => s,
        Err(_) => {
            out.ok(
                "Core is not running.",
                serde_json::json!({"running": false}),
            );
            return Ok(());
        }
    };
    session.send("die", &serde_json::json!({})).await?;
    drop(session);
    out.note("Waiting for whoisthat-core to exit...");
    let cfg = config::load_config();
    let ep = endpoint(g, &cfg)?;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if CoreConnection::connect_endpoint(&ep).await.is_err() {
            out.ok("Core stopped.", serde_json::json!({"running": false}));
            out.note("note: an open TUI may respawn the core automatically");
            return Ok(());
        }
    }
    Err(CliError::Timeout("core did not stop within 10s".into()))
}

async fn restart(g: &GlobalArgs) -> Result<(), CliError> {
    let out = Out::new(g);
    if systemd::systemd_is_enabled() {
        out.note("Restarting whoisthat-core.service via systemd...");
        let status = std::process::Command::new("systemctl")
            .args(["--user", "restart", "whoisthat-core.service"])
            .status()
            .map_err(|e| CliError::Core(format!("systemctl failed: {e}")))?;
        if !status.success() {
            return Err(CliError::Core("systemctl --user restart failed".into()));
        }
        let mut session = wait_for_core(g).await?;
        let state = session.state().await?;
        out.ok(
            format!("Core restarted (v{}).", state.core_version),
            serde_json::json!({"running": true, "version": state.core_version}),
        );
        return Ok(());
    }

    let _ = stop(g).await;
    start(g).await
}

async fn wait_for_core(g: &GlobalArgs) -> Result<Session, CliError> {
    for _ in 0..300 {
        if let Ok(session) = Session::open(g).await {
            return Ok(session);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(CliError::Timeout(
        "core did not become reachable within 30s".into(),
    ))
}

#[allow(dead_code)]
fn _reply_marker(_: Reply<'_>) {}
