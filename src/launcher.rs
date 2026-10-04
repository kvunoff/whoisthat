//! `whoisthat run [-d|--detach|-b|--background] <app> [args...]` — launch an application
//! inside the split-tunnel cgroup slice so the core's nftables rules route it separately.
//!
//! In foreground mode (default), we drop the app into a transient systemd --user scope
//! under `whoisthat_split.slice`. The app runs in the foreground and propagates exit code.
//!
//! In background mode (`-d`, `--detach`, `-b`, `--background`), we launch the app as an
//! independent transient systemd --user service under `whoisthat_split.slice`. The command
//! returns immediately, allowing the terminal to close or continue working without terminating
//! the application.
//!
//! Using the per-user systemd manager (not the system one) keeps the app running as the
//! invoking user with their full session environment.

use std::process::Command;

const SLICE: &str = "whoisthat_split.slice";

/// Handle the `run` subcommand. `args` are everything after `run` (flags, the target
/// program and its arguments). Returns the process exit code to propagate.
pub fn run_in_split_slice(args: &[String]) -> i32 {
    let mut detach = false;
    let mut target_args = args;

    if let Some(first) = target_args.first() {
        if matches!(
            first.as_str(),
            "-d" | "--detach" | "-b" | "--background"
        ) {
            detach = true;
            target_args = &target_args[1..];
            if target_args.first().map(String::as_str) == Some("--") {
                target_args = &target_args[1..];
            }
        }
    }

    if target_args.is_empty() {
        eprintln!("usage: whoisthat run [-d|--detach|-b|--background] <application> [args...]");
        eprintln!();
        eprintln!("Launches <application> inside the split-tunnel slice (whoisthat_split.slice).");
        eprintln!("Its traffic is routed per the current split mode:");
        eprintln!("  exclude — bypass tunnel (direct physical gateway)");
        eprintln!("  include — only these apps use the tunnel");
        eprintln!();
        eprintln!("Options:");
        eprintln!("  -d, --detach, -b, --background   Run in background detached from the terminal");
        return 2;
    }

    if which("systemd-run").is_none() {
        eprintln!("whoisthat run: systemd-run not found. Split-tunnel launching requires");
        eprintln!("systemd with a per-user manager (systemctl --user).");
        return 127;
    }

    let mut cmd = Command::new("systemd-run");
    cmd.arg("--user")
        .arg("--collect")
        .arg(format!("--slice={SLICE}"));

    if detach {
        cmd.arg("--same-dir");
        for var in ["DISPLAY", "WAYLAND_DISPLAY", "XDG_CURRENT_DESKTOP"] {
            if let Ok(val) = std::env::var(var) {
                cmd.arg(format!("-E{var}={val}"));
            }
        }
    } else {
        cmd.arg("--scope");
    }

    cmd.arg("--").args(target_args);

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
    fn detach_flags_without_target_returns_usage_code() {
        assert_eq!(run_in_split_slice(&["-d".into()]), 2);
        assert_eq!(run_in_split_slice(&["--detach".into()]), 2);
        assert_eq!(run_in_split_slice(&["-b".into()]), 2);
        assert_eq!(run_in_split_slice(&["--background".into()]), 2);
        assert_eq!(run_in_split_slice(&["-d".into(), "--".into()]), 2);
    }

    #[test]
    fn which_finds_sh_but_not_bogus_binary() {
        // `sh` is present on every POSIX system the TUI runs on.
        assert!(which("sh").is_some());
        assert!(which("whoisthat-nonexistent-binary-xyz").is_none());
    }
}
