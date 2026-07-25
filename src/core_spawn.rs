use std::io;
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::process::Command;

pub(crate) fn spawn_core(log_level: &str) -> io::Result<()> {
    let bin = find_core_binary();
    Command::new(&bin)
        .env("WHOISTHAT_LOG_LEVEL", log_level)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()?;
    log::info!("Spawned whoisthat-core from {}", bin);
    Ok(())
}

pub(crate) fn find_core_binary() -> String {
    let candidates = [
        "whoisthat-core",
        "./whoisthat-core",
        "./core/core/whoisthat-core",
        "./bin/whoisthat-core",
        "/usr/bin/whoisthat-core",
        "/usr/local/bin/whoisthat-core",
    ];
    for c in candidates {
        if std::path::Path::new(c).exists() {
            return c.to_string();
        }
    }
    "whoisthat-core".to_string()
}

pub(crate) fn has_cap_net(path: &std::path::Path) -> bool {
    if let Ok(output) = Command::new("getcap").arg(path).output() {
        String::from_utf8_lossy(&output.stdout).contains("cap_net_admin")
    } else {
        false
    }
}

pub(crate) fn ensure_core_caps(core_path: &str) {
    let core = std::fs::canonicalize(core_path)
        .or_else(|_| std::path::absolute(core_path))
        .unwrap_or_else(|_| core_path.into());

    if has_cap_net(&core) {
        return;
    }

    eprintln!("whoisthat: TUN mode needs network capabilities on core binary.");
    eprint!("Set up via pkexec? [Y/n] ");
    let _ = io::stderr().flush();

    let caps = "cap_net_admin,cap_net_raw,cap_setpcap=+ep";
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        eprintln!("\nRun manually: sudo setcap {} {}", caps, core.display());
        return;
    }
    let answer = answer.trim().to_lowercase();
    if !answer.is_empty() && answer != "y" && answer != "yes" {
        eprintln!("Run manually: sudo setcap {} {}", caps, core.display());
        return;
    }

    let s = Command::new("pkexec")
        .arg("setcap")
        .arg(caps)
        .arg(core.as_os_str())
        .status();

    match s {
        Ok(s) if s.success() => {
            if has_cap_net(&core) {
                eprintln!(
                    "whoisthat: capabilities verified on {}. TUN mode ready.",
                    core.display()
                );
            } else {
                eprintln!("whoisthat: pkexec reported success but caps missing on {}. Run: sudo setcap {} {}", core.display(), caps, core.display());
            }
        }
        _ => eprintln!(
            "whoisthat: failed. Run: sudo setcap {} {}",
            caps,
            core.display()
        ),
    }
}
