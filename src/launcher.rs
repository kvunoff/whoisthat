//! `whoisthat run <app> [args...]` — launch an application inside the
//! split-tunnel cgroup slice so the core's nftables rules route it separately.
//!
//! We drop the app into a transient systemd --user scope under
//! `whoisthat-split.slice`. The core matches sockets by that cgroup path and
//! applies the fwmark that selects the exclude/include routing table. Using the
//! per-user systemd manager (not the system one) keeps the app running as the
//! invoking user with their full session environment.

use std::process::Command;

const SLICE: &str = "whoisthat-split.slice";

/// Handle the `run` subcommand. `args` are everything after `run` (the target
/// program and its arguments). Returns the process exit code to propagate.
pub fn run_in_split_slice(args: &[String]) -> i32 {
    if args.is_empty() {
        eprintln!("usage: whoisthat run <application> [args...]");
        eprintln!();
        eprintln!("Launches <application> inside the split-tunnel slice. Its traffic is");
        eprintln!("routed per the current split mode (exclude = bypass tunnel,");
        eprintln!("include = only these apps use the tunnel). Set the mode in the TUI.");
        return 2;
    }

    if which("systemd-run").is_none() {
        eprintln!("whoisthat run: systemd-run not found. Split-tunnel launching requires");
        eprintln!("systemd with a per-user manager (systemctl --user).");
        return 127;
    }

    // --scope runs the command in the foreground as a child; --slice places its
    // cgroup under whoisthat-split.slice; --user targets the calling user's
    // manager. --collect reaps the transient unit once it exits.
    let mut cmd = Command::new("systemd-run");
    cmd.arg("--user")
        .arg("--scope")
        .arg("--collect")
        .arg(format!("--slice={SLICE}"))
        .arg("--")
        .args(args);

    match cmd.status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            eprintln!("whoisthat run: failed to launch via systemd-run: {e}");
            1
        }
    }
}

/// Minimal PATH lookup so we can give a clear error instead of a cryptic
/// spawn failure when systemd-run is absent.
fn which(bin: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(bin);
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_args_returns_usage_code() {
        // No target program → usage error, never touches systemd.
        assert_eq!(run_in_split_slice(&[]), 2);
    }

    #[test]
    fn which_finds_sh_but_not_bogus_binary() {
        // `sh` is present on every POSIX system the TUI runs on.
        assert!(which("sh").is_some());
        assert!(which("whoisthat-nonexistent-binary-xyz").is_none());
    }
}
