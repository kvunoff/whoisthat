use std::path::Path;
use std::time::Duration;

use crate::config;
use crate::core_client::connection::CoreConnection;
use crate::lifecycle::{self, Backend};
use crate::{platform, systemd};

use super::super::args::{CoreCmd, GlobalArgs};
use super::super::client::{Reply, Session, endpoint};
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
    let backend = lifecycle::effective_backend();
    if backend == Backend::Init && !platform::init_service_running(config::uid()) {
        return Err(CliError::Offline(
            "init backend is selected but the whoisthat-core service is not running; run: \
             sudo /usr/lib/whoisthat/install-init core enable $USER $HOME"
                .into(),
        ));
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
    let init = lifecycle::effective_backend() == Backend::Init;
    if init {
        // The wrapper must not restart the core after this exit; the boot hook
        // clears core-stop on the next boot.
        lifecycle::remove_marker("core-run");
        lifecycle::write_marker("core-stop")
            .map_err(|e| CliError::Core(format!("cannot write core-stop: {e}")))?;
    } else {
        lifecycle::remove_marker("core-stop");
    }

    let mut session = match Session::open(g).await {
        Ok(s) => s,
        Err(_) => {
            if init {
                terminate_core_pidfile().await;
                out.ok(
                    "Core is not running; init wrapper will not start it.",
                    serde_json::json!({"running": false}),
                );
                return Ok(());
            }
            out.ok(
                "Core is not running.",
                serde_json::json!({"running": false}),
            );
            return Ok(());
        }
    };
    let kill_switch_enabled = session
        .state()
        .await
        .map(|state| state.kill_switch)
        .unwrap_or(false);
    session.send("die", &serde_json::json!({})).await?;
    if kill_switch_enabled {
        out.note(
            "Kill-switch rules stay armed while the core is stopped. \
             Remove them with `sudo nft delete table inet whoisthat_ks`, \
             or start the core and disable the kill-switch.",
        );
    }
    drop(session);
    out.note("Waiting for whoisthat-core to exit...");
    let cfg = config::load_config();
    let ep = endpoint(g, &cfg)?;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if CoreConnection::connect_endpoint(&ep).await.is_err() {
            out.ok("Core stopped.", serde_json::json!({"running": false}));
            if !init {
                out.note("note: an open TUI may respawn the core automatically");
            }
            return Ok(());
        }
    }
    if init {
        // Hunched core: the wrapper waits on the child, so TERM/KILL by pidfile
        // is the only way out.
        terminate_core_pidfile().await;
        return Ok(());
    }
    Err(CliError::Timeout("core did not stop within 10s".into()))
}

async fn restart(g: &GlobalArgs) -> Result<(), CliError> {
    let out = Out::new(g);
    if lifecycle::effective_backend() == Backend::Init {
        if !platform::init_service_running(config::uid()) {
            return Err(CliError::Offline(
                "init backend is selected but the whoisthat-core service is not running; run: \
                 sudo /usr/lib/whoisthat/install-init core enable $USER $HOME"
                    .into(),
            ));
        }
        lifecycle::remove_marker("core-stop");
        lifecycle::write_marker("core-restart")
            .map_err(|e| CliError::Core(format!("cannot write core-restart: {e}")))?;
        if let Ok(mut session) = Session::open(g).await {
            let _ = session.send("die", &serde_json::json!({})).await;
        }
        let mut session = wait_for_core(g).await?;
        let state = session.state().await?;
        out.ok(
            format!("Core restarted (v{}).", state.core_version),
            serde_json::json!({"running": true, "version": state.core_version}),
        );
        return Ok(());
    }

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

/// TERM (then KILL) the pid recorded by the init wrapper. The comm check keeps
/// a recycled PID from killing an unrelated process.
async fn terminate_core_pidfile() {
    let path = lifecycle::marker_path("core.pid");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(pid) = text.trim().parse::<u32>() else {
        return;
    };
    let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
    if comm.trim() != "whoisthat-core" {
        return;
    }
    let _ = std::process::Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status();
    for _ in 0..50 {
        if !Path::new(&format!("/proc/{pid}")).exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let _ = std::process::Command::new("kill")
        .arg("-KILL")
        .arg(pid.to_string())
        .status();
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
