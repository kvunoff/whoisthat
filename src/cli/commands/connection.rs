use crate::config;
use crate::core_client::protocol::*;

use super::super::args::{GlobalArgs, ModeCmd};
use super::super::client::{Reply, Session};
use super::super::error::CliError;
use super::super::output::Out;
use super::super::resolve;

pub async fn connect(
    g: &GlobalArgs,
    target: Option<&str>,
    group: Option<&str>,
    wait: bool,
) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    let (group_id, profile_id, profile_name) = select_target(&state, &session.cfg, target, group)?;

    let id = session
        .send(
            "connect",
            &ConnectData {
                profile: ProfileID {
                    id: profile_id,
                    group_id,
                },
            },
        )
        .await?;

    let mut cfg = session.cfg.clone();
    cfg.last_group_id = group_id;
    cfg.last_profile_id = profile_id;
    config::save_config(&cfg);
    session.cfg = cfg;

    if wait {
        session.set_timeout(g.timeout.unwrap_or(30));
        let _: ProxyStatus = session
            .wait(
                &id,
                Reply::Broadcast {
                    pred: Box::new(move |msg| {
                        if msg.msg != "status-changed" {
                            return false;
                        }
                        serde_json::from_value::<ProxyStatus>(msg.data.clone())
                            .map(|st| {
                                st.connection == "connected"
                                    && st.profile.as_ref().is_some_and(|p| {
                                        p.group_id == group_id && p.id == profile_id
                                    })
                            })
                            .unwrap_or(false)
                    }),
                    fatal_warns: &["connect-failed"],
                },
            )
            .await?;
        out.ok(
            format!("Connected to '{profile_name}'."),
            serde_json::json!({
                "group_id": group_id,
                "profile_id": profile_id,
                "profile_name": profile_name,
                "connected": true,
            }),
        );
    } else {
        out.ok(
            format!("Connecting to '{profile_name}'..."),
            serde_json::json!({
                "group_id": group_id,
                "profile_id": profile_id,
                "profile_name": profile_name,
                "connected": false,
            }),
        );
    }
    session.flush_warnings(&out);
    Ok(())
}

pub async fn disconnect(g: &GlobalArgs, wait: bool) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    if state.connection_status.connection != "connected" {
        out.ok("Disconnected.", serde_json::json!({"connected": false}));
        return Ok(());
    }

    let id = session.send("disconnect", &DisconnectData {}).await?;
    if wait {
        session.set_timeout(g.timeout.unwrap_or(10));
        let _: ProxyStatus = session
            .wait(
                &id,
                Reply::Broadcast {
                    pred: Box::new(|msg| {
                        msg.msg == "status-changed"
                            && serde_json::from_value::<ProxyStatus>(msg.data.clone())
                                .map(|st| st.connection == "disconnected")
                                .unwrap_or(false)
                    }),
                    fatal_warns: &[],
                },
            )
            .await?;
    }
    out.ok("Disconnected.", serde_json::json!({"connected": false}));
    session.flush_warnings(&out);
    Ok(())
}

pub async fn toggle(g: &GlobalArgs, wait: bool) -> Result<(), CliError> {
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    let connected = state.connection_status.connection == "connected";
    drop(session);

    if connected {
        disconnect(g, wait).await
    } else {
        connect(g, None, None, wait).await
    }
}

pub async fn mode(g: &GlobalArgs, cmd: ModeCmd) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;

    if matches!(cmd, ModeCmd::Status) {
        let mode = if state.connection_status.connection == "connected" {
            if state.tun_status {
                "tun"
            } else {
                "proxy"
            }
        } else {
            "off"
        };
        if g.json {
            out.json(&serde_json::json!({
                "connected": state.connection_status.connection == "connected",
                "mode": mode,
                "tun_enabled": state.tun_status,
                "split_tunnel": state.split_tunnel,
            }));
        } else {
            out.line(format!(
                "Mode: {} (split: {})",
                mode.to_uppercase(),
                if state.split_tunnel.is_empty() {
                    "off"
                } else {
                    state.split_tunnel.as_str()
                }
            ));
        }
        return Ok(());
    }

    let want = match cmd {
        ModeCmd::Tun => true,
        ModeCmd::Proxy => false,
        ModeCmd::Toggle => !state.tun_status,
        ModeCmd::Status => unreachable!(),
    };

    if want == state.tun_status {
        out.ok(
            if want {
                "TUN mode is already enabled."
            } else {
                "Proxy mode is already active."
            },
            serde_json::json!({"tun_enabled": want}),
        );
        return Ok(());
    }

    let id = session
        .send(
            if want { "enable-tun" } else { "disable-tun" },
            &serde_json::json!({}),
        )
        .await?;
    session.set_timeout(g.timeout.unwrap_or(30));
    let status: TunStatus = session
        .wait(
            &id,
            Reply::Broadcast {
                pred: Box::new(move |msg| {
                    msg.msg == "tun-status-changed"
                        && serde_json::from_value::<TunStatus>(msg.data.clone())
                            .map(|s| s.is_enabled == want)
                            .unwrap_or(false)
                }),
                fatal_warns: &["enable-tun-failed"],
            },
        )
        .await?;
    out.ok(
        if status.is_enabled {
            "TUN mode enabled."
        } else {
            "Proxy mode enabled (TUN disabled)."
        },
        serde_json::json!({"tun_enabled": status.is_enabled}),
    );
    session.flush_warnings(&out);
    Ok(())
}

pub async fn version(g: &GlobalArgs) -> Result<(), CliError> {
    let out = Out::new(g);
    let tui = env!("CARGO_PKG_VERSION");
    let cfg = config::load_config();
    let endpoint = super::super::client::endpoint(g, &cfg)?;
    let ipc = crate::core_client::query::fetch_core_version_at(&endpoint).await;
    if g.json {
        out.json(&serde_json::json!({
            "tui": tui,
            "core": ipc.as_ref().map(|(v, _)| v.clone()),
            "protocol": ipc.as_ref().map(|(_, p)| *p),
            "last_seen": if cfg.core_version.is_empty() { None } else { Some(cfg.core_version.clone()) },
        }));
        return Ok(());
    }
    println!("whoisthat (tui) v{tui}");
    match ipc {
        Some((ver, proto)) => println!("whoisthat-core v{ver} (protocol {proto})"),
        None if cfg.core_version.is_empty() => println!("whoisthat-core offline (version unknown)"),
        None => println!("whoisthat-core v{} (offline, last seen)", cfg.core_version),
    }
    Ok(())
}

pub async fn ip(g: &GlobalArgs) -> Result<(), CliError> {
    let out = Out::new(g);
    out.note("Fetching public IP...");
    let ipv4 = tokio::task::spawn_blocking(crate::net_info::fetch_public_ip)
        .await
        .unwrap_or(None);
    let ipv6 = tokio::task::spawn_blocking(crate::net_info::fetch_public_ipv6)
        .await
        .unwrap_or(None);
    if g.json {
        out.json(&serde_json::json!({"ipv4": ipv4, "ipv6": ipv6}));
    } else {
        println!("IPv4: {}", ipv4.as_deref().unwrap_or("unavailable"));
        println!("IPv6: {}", ipv6.as_deref().unwrap_or("unavailable"));
    }
    Ok(())
}

fn select_target(
    state: &ApplicationState,
    cfg: &config::AppConfig,
    target: Option<&str>,
    group: Option<&str>,
) -> Result<(i32, i32, String), CliError> {
    if let Some(selector) = target {
        let (group, profile) = resolve::find_profile(&state.groups, selector, group)?;
        return Ok((group.id, profile.id, profile.name.clone()));
    }

    if let Some(group_sel) = group {
        let group = resolve::find_group(&state.groups, group_sel)?;
        if let Some(profile) = state
            .groups
            .iter()
            .find(|g| g.group.id == group.id)
            .and_then(|g| g.profiles.first())
        {
            return Ok((group.id, profile.id, profile.name.clone()));
        }
        return Err(CliError::Usage(format!(
            "group '{}' has no profiles",
            group.name
        )));
    }

    if cfg.last_profile_id != 0 {
        for g in &state.groups {
            if g.group.id == cfg.last_group_id {
                if let Some(p) = g.profiles.iter().find(|p| p.id == cfg.last_profile_id) {
                    return Ok((g.group.id, p.id, p.name.clone()));
                }
            }
        }
    }

    for g in &state.groups {
        if let Some(p) = g.profiles.first() {
            return Ok((g.group.id, p.id, p.name.clone()));
        }
    }

    Err(CliError::Usage(
        "no profiles available; import a subscription first".into(),
    ))
}
