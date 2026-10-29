use crate::config;
use crate::core_client::protocol::*;

use super::super::args::{GlobalArgs, TestCmd, TestConfigArgs, TestMethodArgs, TestTargetArgs};
use super::super::client::{Reply, Session};
use super::super::error::CliError;
use super::super::output::{print_table, Out};
use super::super::resolve;

pub async fn dispatch(g: &GlobalArgs, cmd: TestCmd) -> Result<(), CliError> {
    match cmd {
        TestCmd::Profile(a) => profile(g, a).await,
        TestCmd::Group(a) => group(g, a.selector.as_str(), a.method.as_deref(), a.wait).await,
        TestCmd::All(a) => all(g, a).await,
        TestCmd::Cancel => cancel(g).await,
        TestCmd::Config(a) => config(g, a).await,
    }
}

async fn profile(g: &GlobalArgs, args: TestTargetArgs) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    let (group, profile) = resolve::find_profile(&state.groups, &args.selector, None)?;
    let group_id = group.id;
    let profile_id = profile.id;
    let name = profile.name.clone();

    let id = session
        .send(
            "test-profile",
            &TestProfileData {
                profile: ProfileID {
                    id: profile_id,
                    group_id,
                },
                method: args.method.clone().unwrap_or_default(),
                sample_count: 0,
            },
        )
        .await?;

    if !args.wait {
        out.ok(
            format!("Testing '{name}'..."),
            serde_json::json!({"queued": true, "group_id": group_id, "profile_id": profile_id}),
        );
        return Ok(());
    }

    session.set_timeout(g.timeout.unwrap_or(120));
    let updated: ProfileUpdated = session
        .wait(
            &id,
            Reply::Broadcast {
                pred: Box::new(move |msg| {
                    msg.msg == "profile-updated"
                        && serde_json::from_value::<ProfileUpdated>(msg.data.clone())
                            .map(|u| u.profile.id == profile_id && u.profile.group_id == group_id)
                            .unwrap_or(false)
                }),
                fatal_warns: &["test-failed"],
            },
        )
        .await?;
    print_profile(&out, g, &updated.profile);
    Ok(())
}

async fn group(
    g: &GlobalArgs,
    selector: &str,
    method: Option<&str>,
    wait: bool,
) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    let group = resolve::find_group(&state.groups, selector)?;
    let group_id = group.id;
    let name = group.name.clone();

    let id = session
        .send(
            "test-group",
            &TestGroupData {
                group_id,
                method: method.unwrap_or_default().to_string(),
                sample_count: 0,
            },
        )
        .await?;

    if !wait {
        out.ok(
            format!("Testing group '{name}'..."),
            serde_json::json!({"queued": true, "group_id": group_id}),
        );
        return Ok(());
    }

    session.set_timeout(g.timeout.unwrap_or(120));
    let progress: TestProgress = session
        .wait(
            &id,
            Reply::Broadcast {
                pred: Box::new(move |msg| {
                    msg.msg == "test-progress"
                        && serde_json::from_value::<TestProgress>(msg.data.clone())
                            .map(|p| {
                                p.group_id == group_id && (p.total == 0 || p.tested >= p.total)
                            })
                            .unwrap_or(false)
                }),
                fatal_warns: &["test-group-failed", "command-ignored"],
            },
        )
        .await?;

    if progress.total == 0 {
        return Err(CliError::Core(
            "no tests were queued (all profiles are already being tested)".into(),
        ));
    }

    print_group_results(&mut session, &out, g, group_id, &name).await
}

async fn all(g: &GlobalArgs, args: TestMethodArgs) -> Result<(), CliError> {
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    drop(session);
    for gdata in state.groups {
        group(
            g,
            &gdata.group.id.to_string(),
            args.method.as_deref(),
            args.wait,
        )
        .await?;
    }
    Ok(())
}

async fn cancel(g: &GlobalArgs) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    session.send("cancel-tests", &serde_json::json!({})).await?;
    out.ok(
        "Cancelled pending and active tests.",
        serde_json::json!({"cancelled": true}),
    );
    Ok(())
}

async fn config(g: &GlobalArgs, args: TestConfigArgs) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let state = session.state().await?;
    let mut config = state.test_config.clone();

    let mut changed = false;
    if let Some(v) = args.concurrency {
        config.concurrency = v;
        changed = true;
    }
    if let Some(v) = args.test_timeout {
        config.timeout_seconds = v;
        changed = true;
    }
    if let Some(v) = args.samples {
        config.samples_per_test = v;
        changed = true;
    }
    if let Some(v) = args.endpoint {
        config.test_endpoint = v;
        changed = true;
    }
    if let Some(v) = args.auto_on_subscribe {
        config.auto_test_on_subscribe = match v.as_str() {
            "on" | "true" | "1" => true,
            "off" | "false" | "0" => false,
            _ => return Err(CliError::Usage("expected on|off".into())),
        };
        changed = true;
    }

    if changed {
        if config.concurrency < 1 || config.concurrency > 64 {
            return Err(CliError::Usage("concurrency must be 1..64".into()));
        }
        if config.timeout_seconds < 1 || config.timeout_seconds > 120 {
            return Err(CliError::Usage("timeout must be 1..120".into()));
        }
        if config.samples_per_test < 1 || config.samples_per_test > 10 {
            return Err(CliError::Usage("samples must be 1..10".into()));
        }
        let _: TestConfigUpdated = session
            .request(
                "set-test-config",
                &SetTestConfigData {
                    config: config.clone(),
                },
                Reply::Id {
                    expect: &["test-config-updated"],
                    fatal_warns: &[],
                },
            )
            .await?;
        // Re-read: the ack echoes requested values, the daemon sanitizes them.
        let state = session.state().await?;
        config = state.test_config.clone();
        let mut cfg = session.cfg.clone();
        cfg.test_concurrency = config.concurrency;
        cfg.test_timeout_seconds = config.timeout_seconds;
        cfg.test_samples = config.samples_per_test;
        cfg.test_endpoint = config.test_endpoint.clone();
        cfg.auto_test_on_subscribe = config.auto_test_on_subscribe;
        config::save_config(&cfg);
        session.cfg = cfg;
    }

    if g.json {
        out.json(&config);
    } else {
        println!("concurrency:          {}", config.concurrency);
        println!("timeout:              {} s", config.timeout_seconds);
        println!("samples:              {}", config.samples_per_test);
        println!("endpoint:             {}", config.test_endpoint);
        println!("auto-test on refresh: {}", config.auto_test_on_subscribe);
    }
    Ok(())
}

async fn print_group_results(
    session: &mut Session,
    out: &Out,
    g: &GlobalArgs,
    group_id: i32,
    group_name: &str,
) -> Result<(), CliError> {
    let state = session.state().await?;
    let group = state
        .groups
        .iter()
        .find(|x| x.group.id == group_id)
        .ok_or_else(|| CliError::Core("group disappeared".into()))?;
    if g.json {
        let rows: Vec<serde_json::Value> = group
            .profiles
            .iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.id,
                    "name": p.name,
                    "test_result": p.test_result,
                    "loss_pct": p.loss_pct,
                    "jitter_ms": p.jitter_ms,
                })
            })
            .collect();
        out.json(&rows);
        return Ok(());
    }
    let rows: Vec<Vec<String>> = group
        .profiles
        .iter()
        .map(|p| {
            vec![
                p.name.clone(),
                match p.test_result {
                    i32::MIN..=-1 => "fail".into(),
                    0 => "—".into(),
                    ms => format!("{ms} ms"),
                },
                format!("{}%", p.loss_pct),
                format!("{} ms", p.jitter_ms),
            ]
        })
        .collect();
    println!("Results for '{group_name}':");
    print_table(&["NAME", "PING", "LOSS", "JITTER"], &rows);
    Ok(())
}

fn print_profile(out: &Out, g: &GlobalArgs, p: &Profile) {
    if g.json {
        out.json(p);
        return;
    }
    println!(
        "{}: {}",
        p.name,
        match p.test_result {
            i32::MIN..=-1 => "fail".to_string(),
            0 => "—".to_string(),
            ms => format!("{ms} ms"),
        }
    );
    if p.loss_pct > 0 || p.jitter_ms > 0 {
        println!("loss: {}%  jitter: {} ms", p.loss_pct, p.jitter_ms);
    }
}
