//! `whoisthat-runner`: per-user fork server started by the init wrapper inside
//! `/sys/fs/cgroup/whoisthat-<uid>`. Never runs as root.

#[allow(dead_code)]
#[path = "../runner/proto.rs"]
mod proto;
#[path = "../runner/pty.rs"]
mod pty;
#[path = "../runner/server.rs"]
mod server;

use std::process::ExitCode;

fn usage() -> ! {
    eprintln!("usage: whoisthat-runner --uid <uid> --socket <path>");
    std::process::exit(2);
}

fn main() -> ExitCode {
    let mut uid = None;
    let mut socket = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--uid" => uid = args.next().and_then(|value| value.parse::<u32>().ok()),
            "--socket" => socket = args.next(),
            "-h" | "--help" => usage(),
            other => {
                eprintln!("whoisthat-runner: unknown argument {other}");
                usage();
            }
        }
    }
    let Some(uid) = uid else { usage() };
    let Some(socket) = socket else { usage() };

    match server::run(uid, &socket) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("whoisthat-runner: {e}");
            ExitCode::FAILURE
        }
    }
}
