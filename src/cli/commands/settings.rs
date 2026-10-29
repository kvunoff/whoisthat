use serde_json::{json, Value};

use crate::config;
use crate::core_client::protocol::*;

use super::super::args::{GlobalArgs, SettingsArgs, SettingsCmd};
use super::super::client::{Reply, Session};
use super::super::error::CliError;
use super::super::output::Out;
use super::super::resolve;

const CORE_KEYS: &[&str] = &[
    "kill-switch",
    "split-tunnel",
    "autoconnect",
    "autoconnect-profile",
    "autoconnect-mode",
    "tun-name",
    "hwid-enabled",
    "user-agent",
    "test.concurrency",
    "test.timeout",
    "test.samples",
    "test.endpoint",
    "test.auto-on-subscribe",
];

const LOCAL_KEYS: &[&str] = &[
    "theme",
    "show-ip",
    "log-enabled",
    "log-level",
    "test-method",
    "core.host",
    "core.port",
    "core.use-tcp",
    "core.socket",
];

pub async fn dispatch(g: &GlobalArgs, args: SettingsArgs) -> Result<(), CliError> {
    let SettingsArgs { cmd, set } = args;
    match cmd {
        Some(SettingsCmd::List) => list(g).await,
        None if set.is_empty() => list(g).await,
        None => set_values(g, &set).await,
        Some(SettingsCmd::Get { key }) => get(g, &key).await,
        Some(SettingsCmd::Set { assignments }) => set_values(g, &assignments).await,
        Some(SettingsCmd::Toggle { key }) => toggle(g, &key).await,
    }
}

async fn list(g: &GlobalArgs) -> Result<(), CliError> {
    let out = Out::new(g);
    let cfg = config::load_config();
    let mut entries: Vec<(String, Value, &'static str)> = Vec::new();

    for key in LOCAL_KEYS {
        if let Some(v) = local_value(&cfg, key) {
            entries.push(((*key).to_string(), v, "local"));
        }
    }

    match Session::open(g).await {
        Ok(mut session) => {
            let state = session.state().await?;
            for key in CORE_KEYS {
                if let Some(v) = core_value(&state, &cfg, key) {
                    let source = if *key == "autoconnect-profile" {
                        "core-config"
                    } else {
                        "core"
                    };
                    entries.push(((*key).to_string(), v, source));
                }
            }
        }
        Err(_) => {
            for key in CORE_KEYS {
                entries.push(((*key).to_string(), Value::String("unknown".into()), "core"));
            }
        }
    }

    if g.json {
        let list: Vec<Value> = entries
            .iter()
            .map(|(k, v, s)| json!({"key": k, "value": v, "source": s}))
            .collect();
        out.json(&list);
        return Ok(());
    }

    for (key, value, source) in entries {
        println!("{key}={}  # {source}", value_label(&value));
    }
    Ok(())
}

async fn get(g: &GlobalArgs, key: &str) -> Result<(), CliError> {
    let out = Out::new(g);
    let key = canonical_key(key)?;
    let cfg = config::load_config();

    if LOCAL_KEYS.contains(&key.as_str()) {
        let value = local_value(&cfg, &key)
            .ok_or_else(|| CliError::Usage(format!("no value for '{key}'")))?;
        if g.json {
            out.json(&json!({"key": key, "value": value, "source": "local"}));
        } else {
            println!("{}", value_label(&value));
        }
        return Ok(());
    }

    let mut session = Session::open(g).await?;
    let state = session.state().await?;
    let value = core_value(&state, &cfg, &key)
        .ok_or_else(|| CliError::Usage(format!("no value for '{key}'")))?;
    let source = if key == "autoconnect-profile" {
        "core-config"
    } else {
        "core"
    };
    if g.json {
        out.json(&json!({"key": key, "value": value, "source": source}));
    } else {
        println!("{}", value_label(&value));
    }
    Ok(())
}

async fn toggle(g: &GlobalArgs, key: &str) -> Result<(), CliError> {
    let key = canonical_key(key)?;
    if !is_bool_key(&key) {
        return Err(CliError::Usage(format!("'{key}' is not a boolean setting")));
    }
    let current = if LOCAL_KEYS.contains(&key.as_str()) {
        let cfg = config::load_config();
        local_value(&cfg, &key)
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    } else {
        let mut session = Session::open(g).await?;
        let state = session.state().await?;
        core_value(&state, &session.cfg, &key)
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    };
    let assignment = format!("{key}={}", !current);
    set_values(g, &[assignment]).await
}

async fn set_values(g: &GlobalArgs, assignments: &[String]) -> Result<(), CliError> {
    if assignments.is_empty() {
        return Err(CliError::Usage(
            "expected KEY=VALUE (see `whoisthat settings list`)".into(),
        ));
    }

    let mut parsed: Vec<(String, String)> = Vec::new();
    for pair in assignments {
        let Some((key, value)) = pair.split_once('=') else {
            return Err(CliError::Usage(format!("expected KEY=VALUE, got '{pair}'")));
        };
        let key = canonical_key(key)?;
        validate(&key, value)?;
        parsed.push((key, value.to_string()));
    }

    let out = Out::new(g);
    let core_jobs: Vec<&(String, String)> = parsed.iter().filter(|(k, _)| is_core_key(k)).collect();
    let local_jobs: Vec<&(String, String)> =
        parsed.iter().filter(|(k, _)| !is_core_key(k)).collect();

    if !core_jobs.is_empty() {
        let mut session = Session::open_or_spawn(g).await?;
        for (key, value) in core_jobs {
            apply_core(&mut session, &out, key, value).await?;
        }
        session.flush_warnings(&out);
    }

    if !local_jobs.is_empty() {
        // Re-read just before writing so a running TUI's changes are not clobbered.
        let mut cfg = config::load_config();
        for (key, value) in &local_jobs {
            apply_local(&mut cfg, key, value)?;
        }
        config::save_config(&cfg);
        if !g.json {
            for (key, value) in local_jobs {
                out.line(format!("{key}={value}"));
            }
        }
    }

    if g.json {
        let applied: Vec<Value> = parsed
            .iter()
            .map(|(k, v)| json!({"key": k, "value": v}))
            .collect();
        out.json(&json!({"ok": true, "applied": applied}));
    }
    Ok(())
}

async fn apply_core(
    session: &mut Session,
    out: &Out,
    key: &str,
    value: &str,
) -> Result<(), CliError> {
    match key {
        "kill-switch" => {
            let want = parse_bool(value)?;
            let reply: SetKillSwitchData = session
                .request(
                    "set-kill-switch",
                    &SetKillSwitchData { enabled: want },
                    Reply::Id {
                        expect: &["kill-switch-updated"],
                        fatal_warns: &[],
                    },
                )
                .await?;
            if reply.enabled != want {
                session.flush_warnings(out);
                return Err(CliError::Core(if want {
                    "kill-switch was not armed; see core.log".into()
                } else {
                    "kill-switch was not disarmed; see core.log".into()
                }));
            }
            let mut cfg = session.cfg.clone();
            cfg.kill_switch_enabled = reply.enabled;
            config::save_config(&cfg);
            session.cfg = cfg;
            out.line(format!("kill-switch={}", reply.enabled));
        }
        "split-tunnel" => {
            let reply: SetSplitTunnelData = session
                .request(
                    "set-split-tunnel",
                    &SetSplitTunnelData {
                        mode: value.to_string(),
                    },
                    Reply::Id {
                        expect: &["split-tunnel-updated"],
                        fatal_warns: &[],
                    },
                )
                .await?;
            if reply.mode != value {
                session.flush_warnings(out);
                return Err(CliError::Core(format!(
                    "split-tunnel was not applied; current mode is '{}'",
                    reply.mode
                )));
            }
            out.line(format!("split-tunnel={}", reply.mode));
        }
        "autoconnect" | "autoconnect-profile" | "autoconnect-mode" => {
            let state = session.state().await?;
            let (mut enabled, mut group_id, mut profile_id, mut mode) =
                autoconnect_current(session, &state);

            match key {
                "autoconnect" => enabled = parse_bool(value)?,
                "autoconnect-mode" => mode = value.to_string(),
                "autoconnect-profile" => {
                    let (group, profile) = resolve::find_profile(&state.groups, value, None)?;
                    group_id = group.id;
                    profile_id = profile.id;
                }
                _ => unreachable!(),
            }

            if enabled && (group_id == 0 || profile_id == 0) {
                return Err(CliError::Usage(
                    "autoconnect needs a profile; set autoconnect-profile=<selector>".into(),
                ));
            }

            let reply: AutoconnectInfo = session
                .request(
                    "set-autoconnect",
                    &SetAutoconnectData {
                        enabled,
                        group_id,
                        profile_id,
                        mode: mode.clone(),
                    },
                    Reply::Id {
                        expect: &["autoconnect-updated"],
                        fatal_warns: &[],
                    },
                )
                .await?;
            if reply.enabled != enabled {
                return Err(CliError::Core("autoconnect was not applied".into()));
            }
            let mut cfg = session.cfg.clone();
            cfg.autoconnect = reply.enabled;
            config::save_config(&cfg);
            session.cfg = cfg;
            out.line(format!(
                "autoconnect={} profile={}:{} mode={}",
                reply.enabled, group_id, profile_id, reply.mode
            ));
        }
        "tun-name" => {
            let reply: SetTunNameData = session
                .request(
                    "set-tun-name",
                    &SetTunNameData {
                        tun_name: value.to_string(),
                    },
                    Reply::Id {
                        expect: &["tun-name-updated"],
                        fatal_warns: &["set-tun-name-failed"],
                    },
                )
                .await?;
            let mut cfg = session.cfg.clone();
            cfg.tun_name = reply.tun_name.clone();
            config::save_config(&cfg);
            session.cfg = cfg;
            out.line(format!("tun-name={}", reply.tun_name));
        }
        "hwid-enabled" => {
            let want = parse_bool(value)?;
            let reply: HwidData = session
                .request(
                    "set-hwid",
                    &SetHwidData {
                        enabled: Some(want),
                        user_agent: None,
                        reset: false,
                    },
                    Reply::Id {
                        expect: &["hwid-updated"],
                        fatal_warns: &[],
                    },
                )
                .await?;
            out.line(format!("hwid-enabled={}", reply.enabled));
        }
        "user-agent" => {
            let reply: HwidData = session
                .request(
                    "set-hwid",
                    &SetHwidData {
                        enabled: None,
                        user_agent: Some(value.to_string()),
                        reset: false,
                    },
                    Reply::Id {
                        expect: &["hwid-updated"],
                        fatal_warns: &[],
                    },
                )
                .await?;
            out.line(format!("user-agent={}", reply.user_agent));
        }
        "test.concurrency"
        | "test.timeout"
        | "test.samples"
        | "test.endpoint"
        | "test.auto-on-subscribe" => {
            let state = session.state().await?;
            let mut config = state.test_config.clone();
            match key {
                "test.concurrency" => {
                    config.concurrency = value.parse().map_err(|_| {
                        CliError::Usage("test.concurrency must be an integer".into())
                    })?
                }
                "test.timeout" => {
                    config.timeout_seconds = value
                        .parse()
                        .map_err(|_| CliError::Usage("test.timeout must be an integer".into()))?
                }
                "test.samples" => {
                    config.samples_per_test = value
                        .parse()
                        .map_err(|_| CliError::Usage("test.samples must be an integer".into()))?
                }
                "test.endpoint" => config.test_endpoint = value.to_string(),
                "test.auto-on-subscribe" => config.auto_test_on_subscribe = parse_bool(value)?,
                _ => unreachable!(),
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
            // The ack carries the requested (unsanitized) values; mirror the daemon's state.
            let state = session.state().await?;
            let mut cfg = session.cfg.clone();
            let tc = state.test_config.clone();
            cfg.test_concurrency = tc.concurrency;
            cfg.test_timeout_seconds = tc.timeout_seconds;
            cfg.test_samples = tc.samples_per_test;
            cfg.test_endpoint = tc.test_endpoint.clone();
            cfg.auto_test_on_subscribe = tc.auto_test_on_subscribe;
            config::save_config(&cfg);
            session.cfg = cfg;
            out.line(format!("{key}={value}"));
        }
        _ => return Err(CliError::Usage(format!("unknown core setting '{key}'"))),
    }
    Ok(())
}

fn apply_local(cfg: &mut config::AppConfig, key: &str, value: &str) -> Result<(), CliError> {
    match key {
        "theme" => {
            if !crate::ui::theme::THEMES
                .iter()
                .any(|t| t.id == value || t.name.eq_ignore_ascii_case(value))
            {
                return Err(CliError::Usage(format!("unknown theme '{value}'")));
            }
            cfg.theme = value.to_string();
        }
        "show-ip" => cfg.show_ip = parse_bool(value)?,
        "log-enabled" => cfg.log_enabled = parse_bool(value)?,
        "log-level" => {
            if !matches!(value, "error" | "warn" | "info" | "debug" | "trace") {
                return Err(CliError::Usage(format!("invalid log level '{value}'")));
            }
            cfg.log_level = value.to_string();
        }
        "test-method" => {
            if !matches!(value, "tcp" | "http-get" | "http-head") {
                return Err(CliError::Usage(format!("invalid test method '{value}'")));
            }
            cfg.test_method = value.to_string();
        }
        "core.host" => cfg.core_host = value.to_string(),
        "core.port" => {
            cfg.core_tcp_port = value
                .parse()
                .map_err(|_| CliError::Usage("core.port must be 1..65535".into()))?
        }
        "core.use-tcp" => cfg.use_tcp = parse_bool(value)?,
        "core.socket" => cfg.ipc_socket_path = value.to_string(),
        _ => return Err(CliError::Usage(format!("unknown local setting '{key}'"))),
    }
    Ok(())
}

fn validate(key: &str, value: &str) -> Result<(), CliError> {
    match key {
        "kill-switch"
        | "autoconnect"
        | "hwid-enabled"
        | "test.auto-on-subscribe"
        | "show-ip"
        | "log-enabled"
        | "core.use-tcp" => {
            parse_bool(value)?;
        }
        "split-tunnel" => {
            if !matches!(value, "off" | "exclude" | "include") {
                return Err(CliError::Usage(format!(
                    "split-tunnel must be off|exclude|include, got '{value}'"
                )));
            }
        }
        "autoconnect-mode" => {
            if !matches!(value, "proxy" | "tun") {
                return Err(CliError::Usage(format!(
                    "autoconnect-mode must be proxy|tun, got '{value}'"
                )));
            }
        }
        "tun-name" => {
            let ok = !value.is_empty()
                && value.len() <= 15
                && value
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic())
                && value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
            if !ok {
                return Err(CliError::Usage(
                    "tun-name must start with a letter and match [a-zA-Z0-9_-]{0,14}".into(),
                ));
            }
        }
        "test.concurrency" => validate_int(value, 1, 64, key)?,
        "test.timeout" => validate_int(value, 1, 120, key)?,
        "test.samples" => validate_int(value, 1, 10, key)?,
        "test.endpoint" => {
            if !value.starts_with("http://") && !value.starts_with("https://") {
                return Err(CliError::Usage(
                    "test.endpoint must be an http(s) URL".into(),
                ));
            }
        }
        "theme" => {
            if !crate::ui::theme::THEMES
                .iter()
                .any(|t| t.id == value || t.name.eq_ignore_ascii_case(value))
            {
                return Err(CliError::Usage(format!("unknown theme '{value}'")));
            }
        }
        "log-level" => {
            if !matches!(value, "error" | "warn" | "info" | "debug" | "trace") {
                return Err(CliError::Usage(format!("invalid log level '{value}'")));
            }
        }
        "test-method" => {
            if !matches!(value, "tcp" | "http-get" | "http-head") {
                return Err(CliError::Usage(format!("invalid test method '{value}'")));
            }
        }
        "core.port" => validate_int(value, 1, 65535, key)?,
        "autoconnect-profile" | "user-agent" | "core.host" | "core.socket" => {}
        _ => return Err(CliError::Usage(format!("unknown setting '{key}'"))),
    }
    Ok(())
}

fn validate_int(value: &str, min: i64, max: i64, key: &str) -> Result<(), CliError> {
    let n: i64 = value
        .parse()
        .map_err(|_| CliError::Usage(format!("{key} must be an integer")))?;
    if n < min || n > max {
        return Err(CliError::Usage(format!("{key} must be in {min}..{max}")));
    }
    Ok(())
}

fn parse_bool(value: &str) -> Result<bool, CliError> {
    match value.to_lowercase().as_str() {
        "true" | "on" | "1" | "yes" => Ok(true),
        "false" | "off" | "0" | "no" => Ok(false),
        _ => Err(CliError::Usage(format!(
            "expected a boolean, got '{value}'"
        ))),
    }
}

fn canonical_key(key: &str) -> Result<String, CliError> {
    let key = key.trim();
    let key = match key {
        "killswitch" | "kill_switch" => "kill-switch",
        other => other,
    };
    if is_core_key(key) || LOCAL_KEYS.contains(&key) {
        return Ok(key.to_string());
    }
    let hint = CORE_KEYS
        .iter()
        .chain(LOCAL_KEYS.iter())
        .find(|k| k.contains(key) || key.contains(*k))
        .map(|k| format!("; did you mean '{k}'?"))
        .unwrap_or_default();
    Err(CliError::Usage(format!("unknown setting '{key}'{hint}")))
}

fn is_core_key(key: &str) -> bool {
    CORE_KEYS.contains(&key)
}

fn is_bool_key(key: &str) -> bool {
    matches!(
        key,
        "kill-switch"
            | "autoconnect"
            | "hwid-enabled"
            | "test.auto-on-subscribe"
            | "show-ip"
            | "log-enabled"
            | "core.use-tcp"
    )
}

fn local_value(cfg: &config::AppConfig, key: &str) -> Option<Value> {
    Some(match key {
        "theme" => json!(cfg.theme),
        "show-ip" => json!(cfg.show_ip),
        "log-enabled" => json!(cfg.log_enabled),
        "log-level" => json!(cfg.log_level),
        "test-method" => json!(cfg.test_method),
        "core.host" => json!(cfg.core_host),
        "core.port" => json!(cfg.core_tcp_port),
        "core.use-tcp" => json!(cfg.use_tcp),
        "core.socket" => json!(cfg.ipc_socket_path),
        _ => return None,
    })
}

fn core_value(state: &ApplicationState, cfg: &config::AppConfig, key: &str) -> Option<Value> {
    let (ac_enabled, ac_group, ac_profile, ac_mode) = autoconnect_config_json();
    Some(match key {
        "kill-switch" => json!(state.kill_switch),
        "split-tunnel" => json!(state.split_tunnel),
        "autoconnect" => json!(state.autoconnect.enabled),
        "autoconnect-profile" => {
            if ac_group == 0 || ac_profile == 0 {
                Value::Null
            } else {
                json!(format!("{ac_group}:{ac_profile}"))
            }
        }
        "autoconnect-mode" => json!(state.autoconnect.mode),
        "tun-name" => json!(state.tun_name),
        "hwid-enabled" => json!(state.hwid_info.as_ref().map(|h| h.enabled).unwrap_or(false)),
        "user-agent" => json!(state
            .hwid_info
            .as_ref()
            .map(|h| h.user_agent.clone())
            .unwrap_or_default()),
        "test.concurrency" => json!(state.test_config.concurrency),
        "test.timeout" => json!(state.test_config.timeout_seconds),
        "test.samples" => json!(state.test_config.samples_per_test),
        "test.endpoint" => json!(state.test_config.test_endpoint),
        "test.auto-on-subscribe" => json!(state.test_config.auto_test_on_subscribe),
        _ => {
            let _ = (cfg, ac_enabled, ac_mode);
            return None;
        }
    })
}

fn autoconnect_current(session: &Session, state: &ApplicationState) -> (bool, i32, i32, String) {
    let (_, group, profile, mode) = autoconnect_config_json();
    let group = if group != 0 {
        group
    } else {
        session.cfg.last_group_id
    };
    let profile = if profile != 0 {
        profile
    } else {
        session.cfg.last_profile_id
    };
    let mode = if mode.is_empty() {
        if state.autoconnect.mode.is_empty() {
            "proxy".to_string()
        } else {
            state.autoconnect.mode.clone()
        }
    } else {
        mode
    };
    (state.autoconnect.enabled, group, profile, mode)
}

fn autoconnect_config_json() -> (bool, i32, i32, String) {
    let path = config::config_dir().join("config.json");
    let Ok(text) = std::fs::read_to_string(path) else {
        return (false, 0, 0, "proxy".into());
    };
    let Ok(v) = serde_json::from_str::<Value>(&text) else {
        return (false, 0, 0, "proxy".into());
    };
    (
        v.get("autoconnect-enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        v.get("autoconnect-group-id")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        v.get("autoconnect-profile-id")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        v.get("autoconnect-mode")
            .and_then(Value::as_str)
            .unwrap_or("proxy")
            .to_string(),
    )
}

fn value_label(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "—".into(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_tun_names_like_the_core() {
        assert!(validate("tun-name", "whoisthattun").is_ok());
        assert!(validate("tun-name", "1bad").is_err());
        assert!(validate("tun-name", "bad.name").is_err());
        assert!(validate("tun-name", "abcdefghijklmnop").is_err());
    }

    #[test]
    fn rejects_unknown_keys_with_hint() {
        assert!(canonical_key("kill-switch").is_ok());
        assert!(canonical_key("killswitch").is_ok());
        assert!(canonical_key("kills witch").is_err());
    }
}
