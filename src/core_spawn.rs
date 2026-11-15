use std::io;
use std::io::IsTerminal;
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

const AUTHORIZE_CORE: &str = "/usr/lib/whoisthat/authorize-core";

pub(crate) fn spawn_core(log_level: &str) -> io::Result<()> {
    if let Some(err) = crate::config::runtime_base_error() {
        return Err(io::Error::other(err));
    }
    ensure_core_authorized()?;
    match crate::lifecycle::effective_backend() {
        crate::lifecycle::Backend::Init => {
            // The init wrapper owns the core lifecycle: ask it to start and let
            // the callers wait for the socket. Spawning here would bypass the
            // wrapper and race it.
            crate::lifecycle::remove_marker("core-stop");
            crate::lifecycle::write_marker("core-start")?;
            log::info!("init backend: requested core start via core-start marker");
            return Ok(());
        }
        crate::lifecycle::Backend::Systemd if crate::systemd::systemd_is_enabled() => {
            let status = Command::new("systemctl")
                .args(["--user", "start", "whoisthat-core.service"])
                .status()?;
            if !status.success() {
                return Err(io::Error::other("systemctl --user start failed"));
            }
            log::info!("Started whoisthat-core.service via systemd");
            return Ok(());
        }
        _ => {}
    }

    let cfg = crate::config::load_config();
    let bin = find_core_binary();
    let mut cmd = Command::new(&bin);
    cmd.env("WHOISTHAT_LOG_LEVEL", log_level)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0);
    // The core appends /whoisthat itself, so pass the base, not our own dir.
    // An explicit socket override already carries its own directory.
    if cfg.ipc_socket_path.is_empty() {
        cmd.env("XDG_RUNTIME_DIR", crate::config::runtime_base());
    }
    cmd.spawn()?;
    log::info!("Spawned whoisthat-core from {}", bin);
    Ok(())
}

pub(crate) fn find_core_binary() -> String {
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join("whoisthat-core");
            if candidate.is_file() {
                return candidate.to_string_lossy().into_owned();
            }
        }
    }

    let candidates = [
        "./whoisthat-core",
        "./core/core/whoisthat-core",
        "./bin/whoisthat-core",
        "/usr/bin/whoisthat-core",
        // Legacy install location; kept last so a stale copy cannot shadow
        // the canonical /usr/bin binary.
        "/usr/local/bin/whoisthat-core",
    ];
    for c in candidates {
        if Path::new(c).exists() {
            return c.to_string();
        }
    }
    "whoisthat-core".to_string()
}

pub(crate) fn has_cap_net(path: &Path) -> bool {
    match Command::new("getcap").arg(path).output() {
        Ok(output) => String::from_utf8_lossy(&output.stdout).contains("cap_net_admin"),
        _ => false,
    }
}

fn authorize_hint() -> String {
    format!("sudo {AUTHORIZE_CORE} --add $USER")
}

/// Refuse to spawn a core that would run with network capabilities without a
/// root-written allowlist entry. Capless proxy/dev builds need no entry: they
/// cannot change global network state.
pub(crate) fn ensure_core_authorized() -> io::Result<()> {
    if crate::config::authorized() {
        return Ok(());
    }
    let bin = find_core_binary();
    if !has_cap_net(Path::new(&bin)) {
        // A capless core cannot change global network state.
        return Ok(());
    }
    if !io::stdin().is_terminal() {
        return Err(io::Error::other(format!(
            "whoisthat-core is not authorized for this user; run: {}",
            authorize_hint()
        )));
    }

    eprintln!(
        "whoisthat: this user is not authorized to run whoisthat-core with network capabilities."
    );
    eprint!("Authorize now via pkexec? [Y/n] ");
    let _ = io::stderr().flush();
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        return Err(io::Error::other(format!("run: {}", authorize_hint())));
    }
    let answer = answer.trim().to_lowercase();
    if !answer.is_empty() && answer != "y" && answer != "yes" {
        return Err(io::Error::other(format!("run: {}", authorize_hint())));
    }

    let status = Command::new("pkexec")
        .arg(AUTHORIZE_CORE)
        .arg("--add")
        .status();
    if matches!(status, Ok(s) if s.success())
        && crate::config::authorized()
        && crate::config::run_lock_present()
    {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "authorization failed; run: {}",
        authorize_hint()
    )))
}
