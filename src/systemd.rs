use std::process::Command;

const SYSTEMD_SERVICE_NAME: &str = "whoisthat-core.service";

pub(crate) fn systemd_service_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    std::path::PathBuf::from(home)
        .join(".config")
        .join("systemd")
        .join("user")
        .join(SYSTEMD_SERVICE_NAME)
}

pub(crate) fn systemd_is_enabled() -> bool {
    Command::new("systemctl")
        .arg("--user")
        .arg("is-enabled")
        .arg(SYSTEMD_SERVICE_NAME)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub(crate) fn linger_is_enabled() -> bool {
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
         Restart=on-failure\n\
         RestartSec=5\n\
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
        // The log level is applied over IPC, so the unit must not pin a stale
        // env value.
        assert!(!unit.contains("WHOISTHAT_LOG_LEVEL"));
        // A stray space after the executable breaks systemd's argument parsing.
        assert!(!unit.contains("\" \n"));
    }
}
