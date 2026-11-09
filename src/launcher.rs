//! `whoisthat run [-d|--detach|-b|--background] <app> [args...]` — launch an application
//! inside the split-tunnel cgroup so the core's nftables rules route it separately.
//!
//! With systemd we keep using a transient `systemd-run --user` scope/service
//! under `whoisthat_split.slice` (the user manager owns the cgroup tree).
//!
//! Without systemd we use the root-created native cgroup
//! `/sys/fs/cgroup/whoisthat-<uid>` and move the child into it in `pre_exec`
//! before it execs, so no socket can be opened outside the cgroup.
//!
//! In foreground mode (default) the app runs in the foreground and its exit code
//! is propagated. `-d` detaches it from the terminal.

use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const SLICE: &str = "whoisthat_split.slice";

unsafe extern "C" {
    #[link_name = "setsid"]
    fn libc_setsid() -> i32;
    #[link_name = "write"]
    fn libc_write(fd: i32, buf: *const u8, count: usize) -> isize;
}

/// Handle the `run` subcommand. `args` are everything after `run` (flags, the target
/// program and its arguments). Returns the process exit code to propagate.
pub fn run_in_split_slice(args: &[String]) -> i32 {
    let mut detach = false;
    let mut target_args = args;

    if let Some(first) = target_args.first()
        && matches!(first.as_str(), "-d" | "--detach" | "-b" | "--background")
    {
        detach = true;
        target_args = &target_args[1..];
        if target_args.first().map(String::as_str) == Some("--") {
            target_args = &target_args[1..];
        }
    }

    if target_args.is_empty() {
        eprintln!("usage: whoisthat run [-d|--detach|-b|--background] <application> [args...]");
        eprintln!();
        eprintln!("Launches <application> inside the split-tunnel cgroup:");
        eprintln!("  systemd hosts: whoisthat_split.slice (via systemd-run)");
        eprintln!("  other hosts:   /sys/fs/cgroup/whoisthat-<uid> (native cgroup)");
        eprintln!("Its traffic is routed per the current split mode:");
        eprintln!("  exclude — bypass tunnel (direct physical gateway)");
        eprintln!("  include — only these apps use the tunnel");
        eprintln!();
        eprintln!("Options:");
        eprintln!(
            "  -d, --detach, -b, --background   Run in background detached from the terminal"
        );
        return 2;
    }

    if crate::platform::started_via_privilege_wrapper() {
        eprintln!(
            "whoisthat run: must be started as the target user, not via sudo/pkexec (the cgroup uid would be wrong)"
        );
        return 1;
    }

    if crate::platform::has_systemd() {
        return run_via_systemd(target_args, detach);
    }
    run_native(target_args, detach)
}

fn run_via_systemd(target_args: &[String], detach: bool) -> i32 {
    if which("systemd-run").is_none() {
        eprintln!("whoisthat run: systemd-run not found; split-tunnel launching on systemd");
        eprintln!("needs systemd-tools (the native cgroup path is only used without systemd).");
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

pub(crate) fn split_cgroup_dir(uid: u32) -> PathBuf {
    PathBuf::from(format!("/sys/fs/cgroup/whoisthat-{uid}"))
}

/// The native cgroup is usable when the user can write `cgroup.procs`. Opening
/// it for writing does not change any state; the PID is written later in
/// `pre_exec`.
fn pick_writable_cgroup(uid: u32) -> Option<PathBuf> {
    let dir = split_cgroup_dir(uid);
    File::options()
        .write(true)
        .open(dir.join("cgroup.procs"))
        .ok()
        .map(|_| dir)
}

fn run_native(target_args: &[String], detach: bool) -> i32 {
    let uid = crate::config::uid();
    let Some(dir) = pick_writable_cgroup(uid) else {
        eprintln!(
            "whoisthat run: cgroup not found; run: sudo /usr/lib/whoisthat/cgroup-setup $USER"
        );
        return 1;
    };

    let procs = match File::options().write(true).open(dir.join("cgroup.procs")) {
        Ok(file) => file,
        Err(e) => {
            eprintln!(
                "whoisthat run: cannot open {}/cgroup.procs: {e}",
                dir.display()
            );
            return 1;
        }
    };
    let fd = procs.as_raw_fd();

    let mut cmd = Command::new(&target_args[0]);
    cmd.args(&target_args[1..]);
    if detach {
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
    }
    unsafe {
        cmd.pre_exec(move || {
            // Async-signal-safe only: no allocations, no std::fs.
            if detach && libc_setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            // "0" means "move the current process" into the cgroup.
            if libc_write(fd, b"0".as_ptr(), 1) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }

    if detach {
        match cmd.spawn() {
            Ok(_) => 0,
            Err(e) => {
                report_native_error(&e);
                1
            }
        }
    } else {
        match cmd.status() {
            Ok(status) => status.code().unwrap_or(1),
            Err(e) => {
                report_native_error(&e);
                1
            }
        }
    }
}

fn report_native_error(e: &io::Error) {
    if e.raw_os_error() == Some(1) {
        eprintln!(
            "whoisthat run: cgroup migration denied; check cgroup ownership/permissions \
             (sudo /usr/lib/whoisthat/cgroup-setup $USER)"
        );
    } else {
        eprintln!("whoisthat run: failed to start application: {e}");
    }
}

/// Minimal PATH lookup so we can give a clear error instead of a cryptic
/// spawn failure when systemd-run is absent.
pub(crate) fn which(bin: &str) -> Option<String> {
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
        // No target program → usage error, never touches systemd or cgroups.
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

    #[test]
    fn native_cgroup_path_is_per_uid() {
        assert_eq!(
            split_cgroup_dir(1000),
            PathBuf::from("/sys/fs/cgroup/whoisthat-1000")
        );
    }

    #[test]
    fn missing_native_cgroup_is_not_writable() {
        // uid 999 is never used by the helper, so this path must not exist.
        assert!(pick_writable_cgroup(999).is_none());
    }
}
