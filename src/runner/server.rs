//! `whoisthat-runner` server: a small per-user fork server that lives inside
//! `/sys/fs/cgroup/whoisthat-<uid>`. It never runs as root and never touches
//! cgroups itself: membership is inherited from the boot wrapper.

use std::ffi::CString;
use std::fs;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use std::os::unix::fs::OpenOptionsExt;

use super::proto::{self, Control, ExitStatus, Spawn};

const MAX_CONNECTIONS: usize = 64;
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(10);
const IDLE_POLL: Duration = Duration::from_millis(200);

type Writer = Arc<Mutex<UnixStream>>;

static TERMINATE: AtomicBool = AtomicBool::new(false);
static ACTIVE_PIDS: Mutex<Vec<i32>> = Mutex::new(Vec::new());

extern "C" fn on_terminate(_: libc::c_int) {
    TERMINATE.store(true, Ordering::SeqCst);
}

pub fn run(uid: u32, socket: &str) -> io::Result<()> {
    assert_cgroup(uid)?;
    validate_socket_dir(Path::new(socket), uid)?;

    let _lock = acquire_lock(Path::new(socket))?;
    remove_stale_socket(Path::new(socket))?;

    let listener = UnixListener::bind(socket)?;
    fs::set_permissions(socket, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;

    install_signal_handler();

    let active = Arc::new(Mutex::new(0usize));
    loop {
        if TERMINATE.load(Ordering::SeqCst) {
            break;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let count = {
                    let mut active = active.lock().expect("active lock");
                    if *active >= MAX_CONNECTIONS {
                        Some(*active)
                    } else {
                        *active += 1;
                        None
                    }
                };
                if count.is_some() {
                    let mut stream = stream;
                    send_error(&mut stream, "too many runner connections");
                    continue;
                }
                let active = active.clone();
                thread::spawn(move || {
                    let _ = handle(stream, uid);
                    if let Ok(mut active) = active.lock() {
                        *active = active.saturating_sub(1);
                    }
                });
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(e),
        }
    }

    let _ = fs::remove_file(socket);
    stop_active();
    Ok(())
}

fn assert_cgroup(uid: u32) -> io::Result<()> {
    let cgroup = fs::read_to_string("/proc/self/cgroup")?;
    if !cgroup.contains(&format!("/whoisthat-{uid}")) {
        return Err(io::Error::other(format!(
            "whoisthat-runner: not inside /whoisthat-{uid} ({})",
            cgroup.trim()
        )));
    }
    Ok(())
}

fn validate_socket_dir(socket: &Path, uid: u32) -> io::Result<()> {
    let dir = socket
        .parent()
        .ok_or_else(|| io::Error::other("socket has no parent directory"))?;
    let meta = fs::symlink_metadata(dir).map_err(|e| {
        io::Error::other(format!(
            "runtime dir {} is missing ({e}); run: sudo /usr/lib/whoisthat/cgroup-setup $USER",
            dir.display()
        ))
    })?;
    if meta.file_type().is_symlink() {
        return Err(io::Error::other(format!(
            "{} is a symlink; refusing to use it",
            dir.display()
        )));
    }
    if !meta.is_dir() || meta.uid() != uid || meta.permissions().mode() & 0o777 != 0o700 {
        return Err(io::Error::other(format!(
            "{} must be a directory owned by uid {uid} with mode 0700",
            dir.display()
        )));
    }
    Ok(())
}

fn acquire_lock(socket: &Path) -> io::Result<OwnedFd> {
    let path = socket.with_file_name("runner.lock");
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(&path)?;
    let fd = file.as_raw_fd();
    let rc = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
    if rc != 0 {
        return Err(io::Error::other(
            "another whoisthat-runner is already running for this user",
        ));
    }
    Ok(OwnedFd::from(file))
}

fn remove_stale_socket(socket: &Path) -> io::Result<()> {
    match fs::symlink_metadata(socket) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(io::Error::other(format!(
                    "{} is a symlink; refusing to remove it",
                    socket.display()
                )));
            }
            if UnixStream::connect(socket).is_ok() {
                return Err(io::Error::other("another runner is listening"));
            }
            fs::remove_file(socket)?;
            Ok(())
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

fn install_signal_handler() {
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_terminate as *const () as usize;
        libc::sigemptyset(&mut action.sa_mask);
        libc::sigaction(libc::SIGTERM, &action, std::ptr::null_mut());
        libc::sigaction(libc::SIGINT, &action, std::ptr::null_mut());
    }
}

fn stop_active() {
    if let Ok(pids) = ACTIVE_PIDS.lock() {
        for pid in pids.iter() {
            unsafe {
                libc::kill(-*pid, libc::SIGTERM);
            }
        }
        thread::sleep(Duration::from_secs(3));
        for pid in pids.iter() {
            unsafe {
                libc::kill(-*pid, libc::SIGKILL);
            }
        }
    }
}

fn register_pid(pid: i32, add: bool) {
    let Ok(mut pids) = ACTIVE_PIDS.lock() else {
        return;
    };
    if add {
        pids.push(pid);
    } else {
        pids.retain(|value| *value != pid);
    }
}

fn peer_uid(stream: &UnixStream) -> Option<u32> {
    let mut cred = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };
    if rc == 0 { Some(cred.uid) } else { None }
}

fn send_error(stream: &mut UnixStream, message: &str) {
    let control = Control::Error {
        message: message.to_string(),
    };
    if let Ok(payload) = serde_json::to_vec(&control) {
        let _ = proto::write_frame(stream, proto::kind::CONTROL, &payload);
    }
}

fn send_control(writer: &Writer, control: &Control) -> io::Result<()> {
    let payload = serde_json::to_vec(control)
        .map_err(|e| io::Error::other(format!("cannot encode control: {e}")))?;
    let mut stream = writer.lock().expect("writer lock");
    proto::write_frame(&mut *stream, proto::kind::CONTROL, &payload)
}

fn handle(mut stream: UnixStream, uid: u32) -> io::Result<()> {
    if peer_uid(&stream) != Some(uid) {
        send_error(&mut stream, "peer uid does not match the runner user");
        return Ok(());
    }

    stream.set_read_timeout(Some(FIRST_FRAME_TIMEOUT))?;
    let (kind, payload) = match proto::read_frame(&mut stream) {
        Ok(frame) => frame,
        Err(e) => {
            send_error(&mut stream, &format!("cannot read request: {e}"));
            return Ok(());
        }
    };
    if kind != proto::kind::CONTROL {
        send_error(&mut stream, "the first frame must be a control frame");
        return Ok(());
    }
    let spawn: Spawn = match serde_json::from_slice(&payload) {
        Ok(spawn) => spawn,
        Err(e) => {
            send_error(&mut stream, &format!("invalid request: {e}"));
            return Ok(());
        }
    };
    if let Err(message) = spawn.validate() {
        send_error(&mut stream, &message);
        return Ok(());
    }
    if spawn.tty {
        send_error(
            &mut stream,
            "interactive (tty) launches are not available in this build; PTY support is phase R2",
        );
        return Ok(());
    }

    let writer: Writer = Arc::new(Mutex::new(stream.try_clone()?));
    if spawn.detach {
        return run_detached(&mut stream, &writer, &spawn);
    }
    run_foreground(&mut stream, &writer, &spawn)
}

fn resolve_program(argv0: &str, env: &[(String, String)]) -> io::Result<PathBuf> {
    if argv0.contains('/') {
        return Ok(PathBuf::from(argv0));
    }
    let path = env
        .iter()
        .find(|(key, _)| key == "PATH")
        .map(|(_, value)| value.clone())
        .or_else(|| std::env::var("PATH").ok())
        .unwrap_or_else(|| "/usr/bin:/bin".into());
    for dir in path.split(':') {
        if dir.is_empty() {
            continue;
        }
        let candidate = Path::new(dir).join(argv0);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("program not found in PATH: {argv0}"),
    ))
}

struct Command {
    program: CString,
    argv: Vec<*const libc::c_char>,
    envp: Vec<*const libc::c_char>,
    cwd: CString,
    _strings: Vec<CString>,
}

impl Command {
    fn build(spawn: &Spawn) -> io::Result<Self> {
        let program = resolve_program(&spawn.argv[0], &spawn.env)?;
        let program = CString::new(program.as_os_str().as_encoded_bytes())
            .map_err(|_| io::Error::other("program path contains NUL"))?;
        let mut strings = Vec::new();
        let mut args = Vec::new();
        for arg in &spawn.argv {
            let value =
                CString::new(arg.as_bytes()).map_err(|_| io::Error::other("argv contains NUL"))?;
            args.push(value);
        }
        let mut argv: Vec<*const libc::c_char> = args.iter().map(|arg| arg.as_ptr()).collect();
        argv.push(std::ptr::null());
        let mut env = Vec::new();
        for (key, value) in &spawn.env {
            let entry = CString::new(format!("{key}={value}"))
                .map_err(|_| io::Error::other("env contains NUL"))?;
            env.push(entry);
        }
        let mut envp: Vec<*const libc::c_char> = env.iter().map(|entry| entry.as_ptr()).collect();
        envp.push(std::ptr::null());
        let cwd =
            CString::new(spawn.cwd.as_bytes()).map_err(|_| io::Error::other("cwd contains NUL"))?;
        strings.append(&mut args);
        strings.append(&mut env);
        Ok(Self {
            program,
            argv,
            envp,
            cwd,
            _strings: strings,
        })
    }
}

fn pipe() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0; 2];
    let rc = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    unsafe { Ok((OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1]))) }
}

fn write_errno(fd: i32) {
    let errno = io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(libc::EIO);
    unsafe {
        libc::write(fd, &errno as *const i32 as *const libc::c_void, 4);
    }
}

fn close_from(first: i32) {
    let rc = unsafe {
        libc::syscall(
            libc::SYS_close_range as libc::c_long,
            first as libc::c_uint,
            libc::c_uint::MAX,
            0 as libc::c_uint,
        )
    };
    if rc == 0 {
        return;
    }
    let max = unsafe { libc::sysconf(libc::_SC_OPEN_MAX) };
    let max = if max > 0 { max as i32 } else { 1024 };
    for fd in first..max {
        unsafe {
            libc::close(fd);
        }
    }
}

fn run_foreground(stream: &mut UnixStream, writer: &Writer, spawn: &Spawn) -> io::Result<()> {
    let command = match Command::build(spawn) {
        Ok(command) => command,
        Err(e) => {
            send_error(stream, &e.to_string());
            return Ok(());
        }
    };

    let (stdin_read, stdin_write) = pipe()?;
    let (stdout_read, stdout_write) = pipe()?;
    let (stderr_read, stderr_write) = pipe()?;
    let (status_read, status_write) = pipe()?;

    let pid = unsafe { libc::fork() };
    if pid < 0 {
        send_error(
            stream,
            &format!("fork failed: {}", io::Error::last_os_error()),
        );
        return Ok(());
    }
    if pid == 0 {
        child_exec(
            &command,
            spawn,
            stdin_read.as_raw_fd(),
            stdout_write.as_raw_fd(),
            stderr_write.as_raw_fd(),
            status_write.as_raw_fd(),
        );
    }

    drop(stdin_read);
    drop(stdout_write);
    drop(stderr_write);
    drop(status_write);
    register_pid(pid, true);

    let mut status = Vec::new();
    {
        let mut file = fs::File::from(status_read);
        let _ = file.read_to_end(&mut status);
    }
    if status.len() >= 4 {
        let errno = i32::from_ne_bytes([status[0], status[1], status[2], status[3]]);
        let message = io::Error::from_raw_os_error(errno);
        send_error(
            stream,
            &format!("cannot start {}: {message}", spawn.argv[0]),
        );
        reap(pid);
        register_pid(pid, false);
        return Ok(());
    }

    send_control(
        writer,
        &Control::Started {
            pid,
            protocol: proto::PROTOCOL,
        },
    )?;

    let out_writer = writer.clone();
    let out_thread =
        thread::spawn(move || copy_frames(stdout_read, out_writer, proto::kind::STDOUT));
    let err_writer = writer.clone();
    let err_thread =
        thread::spawn(move || copy_frames(stderr_read, err_writer, proto::kind::STDERR));

    let (stdin_tx, stdin_rx) = mpsc::channel::<Vec<u8>>();
    let stdin_thread = thread::spawn(move || {
        let mut file = fs::File::from(stdin_write);
        while let Ok(data) = stdin_rx.recv() {
            if file.write_all(&data).is_err() {
                break;
            }
        }
    });
    let mut stdin_tx = Some(stdin_tx);

    stream.set_read_timeout(Some(IDLE_POLL))?;
    loop {
        match proto::read_frame(stream) {
            Ok((kind, payload)) => match kind {
                proto::kind::STDIN => {
                    if payload.is_empty() {
                        // EOF on stdin: close the child's stdin, keep the
                        // session alive so the app can finish and report EXIT.
                        stdin_tx = None;
                    } else if let Some(tx) = &stdin_tx
                        && tx.send(payload).is_err()
                    {
                        stdin_tx = None;
                    }
                }
                proto::kind::SIGNAL => {
                    if let Ok(signal) = serde_json::from_slice::<proto::Signal>(&payload)
                        && (1..=64).contains(&signal.signal)
                    {
                        unsafe {
                            libc::kill(-pid, signal.signal);
                        }
                    }
                }
                _ => {}
            },
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut => {
            }
            Err(_) => {
                // Client vanished: do not leave an orphaned foreground app.
                unsafe {
                    libc::kill(-pid, libc::SIGHUP);
                }
                let exited = wait_for(pid, Duration::from_secs(2))?;
                if !exited {
                    unsafe {
                        libc::kill(-pid, libc::SIGKILL);
                    }
                }
                register_pid(pid, false);
                return Ok(());
            }
        }
        if let Some(status) = try_wait(pid)? {
            drop(stdin_tx);
            let _ = stdin_thread.join();
            let _ = out_thread.join();
            let _ = err_thread.join();
            let payload = serde_json::to_vec(&status).unwrap_or_default();
            let mut stream = writer.lock().expect("writer lock");
            let _ = proto::write_frame(&mut *stream, proto::kind::EXIT, &payload);
            register_pid(pid, false);
            return Ok(());
        }
    }
}

fn run_detached(stream: &mut UnixStream, writer: &Writer, spawn: &Spawn) -> io::Result<()> {
    let command = match Command::build(spawn) {
        Ok(command) => command,
        Err(e) => {
            send_error(stream, &e.to_string());
            return Ok(());
        }
    };
    let (status_read, status_write) = pipe()?;
    let devnull = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/null")?;

    let intermediate = unsafe { libc::fork() };
    if intermediate < 0 {
        send_error(
            stream,
            &format!("fork failed: {}", io::Error::last_os_error()),
        );
        return Ok(());
    }
    if intermediate == 0 {
        unsafe {
            libc::setsid();
        }
        let grandchild = unsafe { libc::fork() };
        if grandchild < 0 {
            write_errno(status_write.as_raw_fd());
            unsafe { libc::_exit(127) };
        }
        if grandchild > 0 {
            unsafe { libc::_exit(0) };
        }
        unsafe {
            libc::setpgid(0, 0);
        }
        child_exec_detached(
            &command,
            spawn,
            devnull.as_raw_fd(),
            status_write.as_raw_fd(),
        );
    }

    drop(status_write);
    drop(devnull);
    reap(intermediate);
    let mut data = Vec::new();
    {
        let mut file = fs::File::from(status_read);
        let _ = file.read_to_end(&mut data);
    }
    let mut lines = data.split(|byte| *byte == b'\n');
    let pid = lines
        .next()
        .and_then(|line| std::str::from_utf8(line).ok())
        .and_then(|line| line.parse::<i32>().ok())
        .unwrap_or(0);
    if let Some(line) = lines.next()
        && let Ok(text) = std::str::from_utf8(line)
        && let Ok(errno) = text.parse::<i32>()
    {
        send_error(
            stream,
            &format!(
                "cannot start {}: {}",
                spawn.argv[0],
                io::Error::from_raw_os_error(errno)
            ),
        );
        return Ok(());
    }
    send_control(
        writer,
        &Control::Started {
            pid,
            protocol: proto::PROTOCOL,
        },
    )
}

fn child_exec(
    command: &Command,
    spawn: &Spawn,
    stdin_fd: i32,
    stdout_fd: i32,
    stderr_fd: i32,
    status_fd: i32,
) -> ! {
    unsafe {
        libc::setpgid(0, 0);
        if libc::chdir(command.cwd.as_ptr()) != 0 {
            write_errno(status_fd);
            libc::_exit(127);
        }
        libc::umask(spawn.umask as libc::mode_t);
        if libc::dup2(stdin_fd, 0) < 0
            || libc::dup2(stdout_fd, 1) < 0
            || libc::dup2(stderr_fd, 2) < 0
        {
            write_errno(status_fd);
            libc::_exit(127);
        }
        if libc::dup2(status_fd, 3) < 0 {
            libc::_exit(127);
        }
        libc::fcntl(3, libc::F_SETFD, libc::FD_CLOEXEC);
        close_from(4);
        libc::execve(
            command.program.as_ptr(),
            command.argv.as_ptr(),
            command.envp.as_ptr(),
        );
        write_errno(3);
        libc::_exit(127);
    }
}

fn child_exec_detached(command: &Command, spawn: &Spawn, null_fd: i32, status_fd: i32) -> ! {
    unsafe {
        if libc::chdir(command.cwd.as_ptr()) != 0 {
            write_errno(status_fd);
            libc::_exit(127);
        }
        libc::umask(spawn.umask as libc::mode_t);
        if libc::dup2(null_fd, 0) < 0 || libc::dup2(null_fd, 1) < 0 || libc::dup2(null_fd, 2) < 0 {
            write_errno(status_fd);
            libc::_exit(127);
        }
        if libc::dup2(status_fd, 3) < 0 {
            libc::_exit(127);
        }
        libc::fcntl(3, libc::F_SETFD, libc::FD_CLOEXEC);
        close_from(4);
        let pid = libc::getpid();
        let line = format!("{pid}\n");
        libc::write(3, line.as_ptr() as *const libc::c_void, line.len());
        libc::execve(
            command.program.as_ptr(),
            command.argv.as_ptr(),
            command.envp.as_ptr(),
        );
        write_errno(3);
        libc::_exit(127);
    }
}

fn copy_frames(reader: OwnedFd, writer: Writer, kind: u8) {
    let mut file = fs::File::from(reader);
    let mut buffer = [0u8; 64 * 1024];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => return,
            Ok(n) => {
                let mut stream = writer.lock().expect("writer lock");
                if proto::write_frame(&mut *stream, kind, &buffer[..n]).is_err() {
                    return;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        }
    }
}

fn try_wait(pid: i32) -> io::Result<Option<ExitStatus>> {
    let mut status = 0;
    let rc = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
    if rc == 0 {
        return Ok(None);
    }
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Some(exit_status(status)))
}

fn wait_for(pid: i32, timeout: Duration) -> io::Result<bool> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if try_wait(pid)?.is_some() {
            return Ok(true);
        }
        if std::time::Instant::now() >= deadline {
            return Ok(false);
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn reap(pid: i32) {
    let mut status = 0;
    unsafe {
        libc::waitpid(pid, &mut status, 0);
    }
}

fn exit_status(status: i32) -> ExitStatus {
    if libc::WIFEXITED(status) {
        ExitStatus {
            code: Some(libc::WEXITSTATUS(status)),
            signal: None,
        }
    } else if libc::WIFSIGNALED(status) {
        ExitStatus {
            code: None,
            signal: Some(libc::WTERMSIG(status)),
        }
    } else {
        ExitStatus {
            code: Some(1),
            signal: None,
        }
    }
}
