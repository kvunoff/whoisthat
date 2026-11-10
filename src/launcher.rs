//! `whoisthat run [-d|--detach|-b|--background] <app> [args...]` — launch an
//! application inside the split-tunnel cgroup.
//!
//! With systemd we keep using a transient `systemd-run --user` scope/service
//! under `whoisthat_split.slice`.
//!
//! Without systemd the command is sent to the per-user `whoisthat-runner`
//! (`/run/whoisthat/<uid>/whoisthat/runner.sock`). The runner lives inside
//! `/sys/fs/cgroup/whoisthat-<uid>`, so forked applications inherit the cgroup;
//! no sudo and no PID migration are involved.

use std::io::{self, IsTerminal, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::proto::{self, Control, ExitStatus, Signal, Spawn};

const SLICE: &str = "whoisthat_split.slice";
const START_TIMEOUT: Duration = Duration::from_secs(10);
const RUNNER_SOCKET: &str = "runner.sock";

static PENDING_SIGNAL: AtomicI32 = AtomicI32::new(0);

extern "C" fn on_signal(signal: libc::c_int) {
    PENDING_SIGNAL.store(signal, Ordering::SeqCst);
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
        eprintln!("  other hosts:   /sys/fs/cgroup/whoisthat-<uid> (whoisthat-runner)");
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
    run_via_runner(target_args, detach)
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

fn runner_socket() -> PathBuf {
    if std::env::var("WHOISTHAT_TEST_MODE").as_deref() == Ok("1")
        && let Ok(path) = std::env::var("WHOISTHAT_RUNNER_SOCKET")
    {
        return PathBuf::from(path);
    }
    crate::config::runtime_dir().join(RUNNER_SOCKET)
}

fn runner_start_hint() -> &'static str {
    match crate::platform::detect_init() {
        crate::platform::InitSystem::OpenRC => "sudo rc-service whoisthat-runner start",
        crate::platform::InitSystem::Dinit => "sudo dinitctl start whoisthat-runner",
        crate::platform::InitSystem::Runit => "sudo sv start whoisthat-runner",
        _ => "start the whoisthat-runner service",
    }
}

fn run_via_runner(target_args: &[String], detach: bool) -> i32 {
    let socket = runner_socket();
    let mut stream = match UnixStream::connect(&socket) {
        Ok(stream) => stream,
        Err(e) => {
            eprintln!(
                "whoisthat run: split-tunnel runner is not running ({}: {e}); {}",
                socket.display(),
                runner_start_hint()
            );
            return 1;
        }
    };

    let request = Spawn {
        protocol: proto::PROTOCOL,
        argv: target_args.to_vec(),
        env: forwarded_env(),
        cwd: current_cwd(),
        umask: current_umask(),
        rows: 0,
        cols: 0,
        tty: false,
        detach,
    };

    let mut writer_stream = match stream.try_clone() {
        Ok(stream) => stream,
        Err(e) => {
            eprintln!("whoisthat run: cannot clone runner connection: {e}");
            return 1;
        }
    };
    if writer_stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .is_err()
    {
        log::warn!("cannot set runner write timeout");
    }

    let payload = match serde_json::to_vec(&request) {
        Ok(payload) => payload,
        Err(e) => {
            eprintln!("whoisthat run: cannot encode request: {e}");
            return 1;
        }
    };
    if let Err(e) = proto::write_frame(&mut writer_stream, proto::kind::CONTROL, &payload) {
        eprintln!("whoisthat run: cannot send request to runner: {e}");
        return 1;
    }

    stream.set_read_timeout(Some(START_TIMEOUT)).ok();
    let started = match proto::read_frame(&mut stream) {
        Ok((kind, payload)) if kind == proto::kind::CONTROL => {
            match serde_json::from_slice::<Control>(&payload) {
                Ok(Control::Started { pid, .. }) => Ok(pid),
                Ok(Control::Error { message }) => Err(message),
                Err(e) => Err(format!("invalid runner response: {e}")),
            }
        }
        Ok(_) => Err("unexpected runner frame".into()),
        Err(e) => Err(format!("runner did not answer: {e}")),
    };
    let pid = match started {
        Ok(pid) => pid,
        Err(message) => {
            eprintln!("whoisthat run: {message}");
            return 1;
        }
    };

    if detach {
        return 0;
    }
    if std::io::stdin().is_terminal() || std::io::stdout().is_terminal() {
        eprintln!(
            "whoisthat run: note: full-screen/interactive programs need PTY support (phase R2)"
        );
    }

    let writer = Arc::new(Mutex::new(writer_stream));
    let (exit_tx, exit_rx) = mpsc::channel::<Option<ExitStatus>>();
    let reader = thread::spawn(move || {
        let result = read_output(&mut stream);
        let _ = exit_tx.send(result);
    });

    install_signal_handler();
    let _ = pid;

    let stdin_thread = {
        let writer = Arc::clone(&writer);
        thread::spawn(move || forward_stdin(&writer))
    };

    loop {
        match exit_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Some(status)) => {
                let _ = stdin_thread.join();
                let _ = reader.join();
                return exit_code(status);
            }
            Ok(None) => {
                let _ = stdin_thread.join();
                let _ = reader.join();
                return 1;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let signal = PENDING_SIGNAL.swap(0, Ordering::SeqCst);
                if signal != 0 {
                    let payload = serde_json::to_vec(&Signal { signal }).unwrap_or_default();
                    let _ = send_frame(&writer, proto::kind::SIGNAL, &payload);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return 1,
        }
    }
}

fn read_output(stream: &mut UnixStream) -> Option<ExitStatus> {
    loop {
        match proto::read_frame(stream) {
            Ok((kind, payload)) => match kind {
                proto::kind::STDOUT => {
                    let _ = io::stdout().write_all(&payload);
                    let _ = io::stdout().flush();
                }
                proto::kind::STDERR => {
                    let _ = io::stderr().write_all(&payload);
                    let _ = io::stderr().flush();
                }
                proto::kind::EXIT => {
                    return serde_json::from_slice::<ExitStatus>(&payload).ok();
                }
                _ => {}
            },
            Err(_) => return None,
        }
    }
}

fn forward_stdin(writer: &Arc<Mutex<UnixStream>>) {
    let mut stdin = io::stdin().lock();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        match stdin.read(&mut buffer) {
            Ok(0) => {
                let _ = send_frame(writer, proto::kind::STDIN, &[]);
                return;
            }
            Ok(n) => {
                if send_frame(writer, proto::kind::STDIN, &buffer[..n]).is_err() {
                    return;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        }
    }
}

fn send_frame(writer: &Arc<Mutex<UnixStream>>, kind: u8, payload: &[u8]) -> io::Result<()> {
    let mut stream = writer.lock().expect("writer lock");
    proto::write_frame(&mut *stream, kind, payload)
}

fn exit_code(status: ExitStatus) -> i32 {
    if let Some(code) = status.code {
        return code;
    }
    if let Some(signal) = status.signal {
        return 128 + signal;
    }
    1
}

fn forwarded_env() -> Vec<(String, String)> {
    std::env::vars_os()
        .filter_map(|(key, value)| {
            let key = key.into_string().ok()?;
            let value = match value.into_string() {
                Ok(value) => value,
                Err(value) => {
                    log::debug!("runner env {key}: non-UTF8 value passed lossy");
                    value.to_string_lossy().into_owned()
                }
            };
            Some((key, value))
        })
        .collect()
}

fn current_cwd() -> String {
    std::env::current_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "/".into())
}

fn current_umask() -> u32 {
    unsafe {
        let previous = libc::umask(0);
        libc::umask(previous);
        previous as u32
    }
}

fn install_signal_handler() {
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_signal as *const () as usize;
        libc::sigemptyset(&mut action.sa_mask);
        libc::sigaction(libc::SIGINT, &action, std::ptr::null_mut());
        libc::sigaction(libc::SIGTERM, &action, std::ptr::null_mut());
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
        assert!(which("sh").is_some());
        assert!(which("whoisthat-nonexistent-binary-xyz").is_none());
    }

    #[test]
    fn socket_defaults_to_runtime_dir() {
        assert!(runner_socket().ends_with(RUNNER_SOCKET));
    }
}
