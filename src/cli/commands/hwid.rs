use crate::core_client::protocol::*;

use super::super::args::{GlobalArgs, HwidCmd};
use super::super::client::{Reply, Session};
use super::super::error::CliError;
use super::super::output::Out;

pub async fn dispatch(g: &GlobalArgs, cmd: HwidCmd) -> Result<(), CliError> {
    let out = Out::new(g);
    match cmd {
        HwidCmd::Status => {
            let mut session = Session::open(g).await?;
            let state = session.state().await?;
            let hwid = state
                .hwid_info
                .ok_or_else(|| CliError::Core("core did not return HWID info".into()))?;
            if g.json {
                out.json(&hwid);
            } else {
                println!("enabled:    {}", hwid.enabled);
                println!("hwid:       {}", hwid.hwid);
                println!("user-agent: {}", hwid.user_agent);
                println!("platform:   {}", hwid.platform);
                println!("kernel:     {}", hwid.kernel);
                println!("model:      {}", hwid.model);
            }
            Ok(())
        }
        HwidCmd::Enable => set(g, Some(true), None, false).await,
        HwidCmd::Disable => set(g, Some(false), None, false).await,
        HwidCmd::Reset => set(g, None, None, true).await,
        HwidCmd::UserAgent { value } => set(g, None, Some(value), false).await,
    }
}

async fn set(
    g: &GlobalArgs,
    enabled: Option<bool>,
    user_agent: Option<String>,
    reset: bool,
) -> Result<(), CliError> {
    let out = Out::new(g);
    let mut session = Session::open_or_spawn(g).await?;
    let reply: HwidData = session
        .request(
            "set-hwid",
            &SetHwidData {
                enabled,
                user_agent,
                reset,
            },
            Reply::Id {
                expect: &["hwid-updated"],
                fatal_warns: &[],
            },
        )
        .await?;
    session.flush_warnings(&out);
    if g.json {
        out.json(&reply);
    } else {
        println!("hwid-enabled: {}", reply.enabled);
        println!("hwid:         {}", reply.hwid);
        println!("user-agent:   {}", reply.user_agent);
    }
    Ok(())
}
