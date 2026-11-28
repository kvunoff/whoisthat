#[path = "../launcher.rs"]
mod launcher;
#[allow(dead_code)]
#[path = "../platform.rs"]
mod platform;
#[allow(dead_code)]
#[path = "../runner/proto.rs"]
mod proto;

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let uid = platform::login_uid();
    let runtime = format!("/run/whoisthat/{uid}/whoisthat").into();
    std::process::exit(launcher::run_in_split_slice(
        &args,
        runtime,
        "whoisthat-run",
    ));
}
