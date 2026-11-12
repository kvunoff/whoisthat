use std::process::Command;

const SYSTEMD_SERVICE_NAME: &str = "whoisthat-core.service";

pub(crate) fn systemd_service_path() -> std::path::PathBuf {
    crate::config::config_dir()
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("systemd")
        .join("user")
        .join(SYSTEMD_SERVICE_NAME)
}

pub(crate) fn systemd_is_enabled() -> bool {
    if !crate::platform::has_systemd() {
        return false;
    }
    Command::new("systemctl")
        .arg("--user")
        .arg("is-enabled")
        .arg(SYSTEMD_SERVICE_NAME)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub(crate) fn linger_is_enabled() -> bool {
    if !crate::platform::has_systemd() {
        return false;
    }
    let user = std::env::var("USER").unwrap_or_default();
    if user.is_empty() {
        return false;
    }
    Command::new("loginctl")
        .arg("show-user")
        .arg(&user)
        .arg("--property=Linger")
        .output()
        .map(|o| {
            let s = String::from_utf8_lossy(&o.stdout);
            s.contains("Linger=yes")
        })
        .unwrap_or(false)
}

/// Escape a value for a systemd unit: `%` starts a specifier and must be
/// doubled, and `ExecStart`/`Environment` values are quoted.
fn unit_escape(value: &str) -> String {
    value
        .replace('%', "%%")
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn generate_unit_file(core_path: &str) -> String {
    let core = unit_escape(core_path);
    format!(
        "[Unit]\n\
         Description=WhoisThat VPN Core\n\
         After=network-online.target\n\
         Wants=network-online.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart=\"{core}\"\n\
         Environment=\"PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\"\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         RestartPreventExitStatus=78\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n"
    )
}

fn enable_linger_via_pkexec() -> bool {
    let user = std::env::var("USER").unwrap_or_default();
    if user.is_empty() {
        return false;
    }
    Command::new("pkexec")
        .arg("loginctl")
        .arg("enable-linger")
        .arg(&user)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub(crate) fn setup_systemd_service(core_path: &str) -> Result<(), String> {
    // The unit starts the core at login, outside the TUI, so authorization must
    // be in place before it is enabled.
    crate::core_spawn::ensure_core_authorized().map_err(|e| e.to_string())?;
    let core =
        std::path::absolute(core_path).map_err(|e| format!("cannot resolve core path: {e}"))?;
    let core_str = core.to_string_lossy().to_string();

    let unit = generate_unit_file(&core_str);
    let service_path = systemd_service_path();
    if let Some(parent) = service_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("cannot create systemd dir: {e}"))?;
    }
    std::fs::write(&service_path, &unit).map_err(|e| format!("cannot write service file: {e}"))?;

    let status = Command::new("systemctl")
        .arg("--user")
        .arg("daemon-reload")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("daemon-reload failed: {e}"))?;
    if !status.success() {
        return Err("systemctl --user daemon-reload failed".into());
    }

    let status = Command::new("systemctl")
        .arg("--user")
        .arg("enable")
        .arg(SYSTEMD_SERVICE_NAME)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("enable failed: {e}"))?;
    if !status.success() {
        return Err("systemctl --user enable failed".into());
    }

    if !linger_is_enabled() && !enable_linger_via_pkexec() {
        return Err(
            "Could not enable lingering. Run manually: sudo loginctl enable-linger $USER".into(),
        );
    }

    Ok(())
}

/// Add RestartPreventExitStatus to units written by older releases. A missing
/// authorization or a busy global lock exits with 78; without the directive
/// Restart=on-failure would restart the core forever.
pub(crate) fn migrate_systemd_unit() {
    let path = systemd_service_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let Some(patched) = with_restart_guard(&text) else {
        return;
    };
    if std::fs::write(&path, patched).is_ok() {
        let _ = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

fn with_restart_guard(text: &str) -> Option<String> {
    if text.contains("RestartPreventExitStatus=78") {
        return None;
    }
    let marker = "[Service]\n";
    let at = text.find(marker)? + marker.len();
    Some(format!(
        "{}RestartPreventExitStatus=78\n{}",
        &text[..at],
        &text[at..]
    ))
}

pub(crate) fn teardown_systemd_service() -> Result<(), String> {
    let _ = Command::new("systemctl")
        .arg("--user")
        .arg("disable")
        .arg(SYSTEMD_SERVICE_NAME)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    let service_path = systemd_service_path();
    let _ = std::fs::remove_file(&service_path);

    let status = Command::new("systemctl")
        .arg("--user")
        .arg("daemon-reload")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("daemon-reload failed: {e}"))?;
    if !status.success() {
        return Err("systemctl --user daemon-reload failed".into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_file_escapes_paths() {
        let unit = generate_unit_file("/tmp/with space/100%/whoisthat-core");

        assert!(unit.contains("ExecStart=\"/tmp/with space/100%%/whoisthat-core\""));
        assert!(unit.contains("RestartPreventExitStatus=78"));
        // The log level is applied over IPC, so the unit must not pin a stale
        // env value.
        assert!(!unit.contains("WHOISTHAT_LOG_LEVEL"));
        // A stray space after the executable breaks systemd's argument parsing.
        assert!(!unit.contains("\" \n"));
    }

    // F-03: the user manager's PATH includes user-writable dirs; the core must
    // not resolve nft/ip/xray through it.
    #[test]
    fn unit_file_pins_path_to_system_directories() {
        let unit = generate_unit_file("/usr/bin/whoisthat-core");
        assert!(unit.contains(
            "Environment=\"PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\"\n"
        ));
    }
}
