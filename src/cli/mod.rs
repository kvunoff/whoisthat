use clap::Parser;

mod args;
mod client;
mod commands;
mod error;
mod output;
mod resolve;

pub use args::Cli;
pub use error::CliError;

use args::*;

/// Deep-link entry points. They use the same connection logic as the CLI.
pub(crate) async fn connect_default() -> Result<(), CliError> {
    commands::connection::connect(&GlobalArgs::default(), None, None, false).await
}

pub(crate) async fn disconnect_default() -> Result<(), CliError> {
    commands::connection::disconnect(&GlobalArgs::default(), false).await
}

pub(crate) async fn toggle_default() -> Result<(), CliError> {
    commands::connection::toggle(&GlobalArgs::default(), false).await
}

/// What the process should do after CLI parsing.
pub enum Outcome {
    /// No arguments: run the interactive TUI.
    Tui,
    /// Exit with this code.
    Exit(i32),
}

pub async fn run(raw_args: &[String]) -> Outcome {
    if raw_args.is_empty() {
        return Outcome::Tui;
    }

    let normalized = normalize_legacy(raw_args);
    let argv = std::iter::once("whoisthat".to_string()).chain(normalized);

    let cli = match Cli::try_parse_from(argv) {
        Ok(cli) => cli,
        Err(e) => return clap_outcome(e),
    };

    if cli.global.version {
        return finish(commands::connection::version(&cli.global).await);
    }

    let Some(cmd) = cli.cmd else {
        eprintln!("whoisthat: no command given (try --help)");
        return Outcome::Exit(2);
    };

    if let Command::Run(args) = cmd {
        let mut argv = Vec::new();
        if args.detach {
            argv.push("-d".to_string());
        }
        argv.extend(args.argv);
        return Outcome::Exit(crate::launcher::run_in_split_slice(&argv));
    }

    finish(dispatch(cmd, &cli.global).await)
}

async fn dispatch(cmd: Command, g: &GlobalArgs) -> Result<(), CliError> {
    match cmd {
        Command::Status(a) => commands::status::run(g, a.short, a.watch).await,
        Command::Connect(a) => {
            commands::connection::connect(g, a.target.as_deref(), a.group.as_deref(), a.wait).await
        }
        Command::Disconnect(a) => commands::connection::disconnect(g, a.wait).await,
        Command::Toggle(a) => commands::connection::toggle(g, a.wait).await,
        Command::Mode(a) => commands::connection::mode(g, a.cmd.unwrap_or(ModeCmd::Status)).await,
        Command::Core(a) => commands::core::dispatch(g, a.cmd).await,
        Command::Profile(a) => match a.cmd {
            ProfileCmd::List(x) => commands::profiles::list(g, x.group.as_deref(), x.reveal).await,
            ProfileCmd::Show(x) => {
                commands::profiles::show(g, &x.selector, x.group.as_deref(), x.reveal).await
            }
            ProfileCmd::Add(x) => commands::profiles::add(g, &x.uris, x.group.as_deref()).await,
            ProfileCmd::Rm(x) => commands::profiles::remove(g, &x.selectors, x.yes).await,
            ProfileCmd::Rename(x) => commands::profiles::rename(g, &x.selector, &x.new_name).await,
            ProfileCmd::Move(x) => commands::profiles::move_to(g, &x.selector, &x.to).await,
            ProfileCmd::Reorder(x) => {
                commands::profiles::reorder(g, &x.group, &x.selectors, x.order.as_deref()).await
            }
        },
        Command::Sub(a) => match a.cmd {
            SubCmd::List(x) => commands::subs::list(g, x.reveal).await,
            SubCmd::Show(x) => commands::subs::show(g, &x.selector, x.reveal).await,
            SubCmd::Add(x) => {
                let name = x.name_flag.or(x.name);
                commands::subs::add(g, &x.url, name.as_deref(), x.no_wait).await
            }
            SubCmd::Rm(x) => commands::subs::remove(g, &x.selector, x.yes).await,
            SubCmd::Refresh(x) => {
                commands::subs::refresh(g, x.selector.as_deref(), x.all, x.no_wait).await
            }
            SubCmd::Rename(x) => {
                commands::subs::rename(g, &x.selector, x.new_name.as_deref()).await
            }
            SubCmd::Url(x) => commands::subs::url(g, &x.selector, x.set.as_deref(), x.reveal).await,
            SubCmd::Reorder(x) => commands::subs::reorder(g, &x.selector, x.new_index).await,
        },
        Command::Route(a) => commands::routing::dispatch(g, a.cmd).await,
        Command::Test(a) => commands::tests::dispatch(g, a.cmd).await,
        Command::Settings(a) => commands::settings::dispatch(g, a).await,
        Command::Hwid(a) => commands::hwid::dispatch(g, a.cmd).await,
        Command::Logs(a) => commands::logs::run(g, a).await,
        Command::Systemd(a) => commands::systemd::dispatch(g, a.cmd).await,
        Command::Link(a) => match a.cmd {
            LinkCmd::Handle(x) => commands::links::handle(g, &x.url, x.group.as_deref()).await,
            LinkCmd::Register => commands::links::register(g).await,
        },
        Command::Doctor => commands::doctor::run(g).await,
        Command::Ip => commands::connection::ip(g).await,
        Command::Version => commands::connection::version(g).await,
        Command::Run(_) => unreachable!("run is handled in cli::run"),
    }
}

fn finish(result: Result<(), CliError>) -> Outcome {
    match result {
        Ok(()) => Outcome::Exit(0),
        Err(e) => {
            eprintln!("Error: {e}");
            Outcome::Exit(e.exit_code())
        }
    }
}

fn clap_outcome(e: clap::Error) -> Outcome {
    use clap::error::ErrorKind;
    let code = match e.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => 0,
        _ => 2,
    };
    let _ = e.print();
    Outcome::Exit(code)
}

/// Rewrite the old flag-style invocations into the new command grammar.
/// Only the first argument is considered; `run`/`wit://` are handled by main
/// before this function is reached.
fn normalize_legacy(args: &[String]) -> Vec<String> {
    let Some(first) = args.first().map(String::as_str) else {
        return Vec::new();
    };

    // `-s`, `-w`, and a lone `-j` used to be status flags.
    if matches!(first, "-s" | "-w")
        || (first == "-j" && (args.len() == 1 || args[1].starts_with('-')))
    {
        let mut out = vec!["status".to_string()];
        out.extend_from_slice(args);
        return out;
    }

    let mapped: Option<&[&str]> = match first {
        "-t" | "--toggle" => Some(&["toggle"]),
        "-c" | "--start" | "start" => Some(&["connect"]),
        "-d" | "--stop" | "stop" => Some(&["disconnect"]),
        "-mt" | "--mode-toggle" | "toggle-tun" | "toggle-mode" => Some(&["mode", "toggle"]),
        "--mode-tun" | "--tun" => Some(&["mode", "tun"]),
        "--mode-proxy" | "--proxy" => Some(&["mode", "proxy"]),
        "-st" | "--systemd-toggle" => Some(&["systemd", "toggle"]),
        "--systemd-on" | "--systemd-enable" | "--enable" => Some(&["systemd", "enable"]),
        "--systemd-off" | "--systemd-disable" | "--disable" => Some(&["systemd", "disable"]),
        "--systemd-status" => Some(&["systemd", "status"]),
        "-kt" | "--killswitch-toggle" => Some(&["settings", "toggle", "kill-switch"]),
        "--killswitch-on" => Some(&["settings", "set", "kill-switch=true"]),
        "--killswitch-off" => Some(&["settings", "set", "kill-switch=false"]),
        "-p" | "--profiles" | "profiles" => Some(&["sub", "list"]),
        "add-sub" => Some(&["sub", "add"]),
        "handle-url" | "url" => Some(&["link", "handle"]),
        "register-deeplink" | "setup-desktop" => Some(&["link", "register"]),
        "--status" => Some(&["status"]),
        "--ip" => Some(&["ip"]),
        "--doctor" => Some(&["doctor"]),
        _ => None,
    };

    match mapped {
        Some(prefix) => {
            let mut out: Vec<String> = prefix.iter().map(|s| s.to_string()).collect();
            out.extend_from_slice(&args[1..]);
            out
        }
        None => args.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn norm(args: &[&str]) -> Vec<String> {
        normalize_legacy(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn clap_definition_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn legacy_flags_map_to_commands() {
        assert_eq!(norm(&["-t"]), vec!["toggle"]);
        assert_eq!(norm(&["-mt"]), vec!["mode", "toggle"]);
        assert_eq!(norm(&["--mode-tun"]), vec!["mode", "tun"]);
        assert_eq!(norm(&["-p", "--json"]), vec!["sub", "list", "--json"]);
        assert_eq!(norm(&["-kt"]), vec!["settings", "toggle", "kill-switch"]);
        assert_eq!(
            norm(&["--killswitch-on"]),
            vec!["settings", "set", "kill-switch=true"]
        );
        assert_eq!(norm(&["--start", "2"]), vec!["connect", "2"]);
        assert_eq!(norm(&["stop"]), vec!["disconnect"]);
    }

    #[test]
    fn status_shorthands_work_alone_and_combined() {
        assert_eq!(norm(&["-s"]), vec!["status", "-s"]);
        assert_eq!(norm(&["-w"]), vec!["status", "-w"]);
        assert_eq!(norm(&["-j"]), vec!["status", "-j"]);
        assert_eq!(norm(&["-s", "-j"]), vec!["status", "-s", "-j"]);
        assert_eq!(norm(&["-j", "status"]), vec!["-j", "status"]);
    }

    #[test]
    fn add_sub_positional_name_is_preserved() {
        assert_eq!(
            norm(&["add-sub", "https://x", "My VPN"]),
            vec!["sub", "add", "https://x", "My VPN"]
        );
        assert_eq!(
            norm(&["handle-url", "wit://toggle", "--group", "Work"]),
            vec!["link", "handle", "wit://toggle", "--group", "Work"]
        );
        assert_eq!(norm(&["register-deeplink"]), vec!["link", "register"]);
    }

    #[test]
    fn parses_canonical_commands() {
        assert!(Cli::try_parse_from(["whoisthat", "status", "-s"]).is_ok());
        assert!(Cli::try_parse_from(["whoisthat", "status", "-w", "-j"]).is_ok());
        assert!(Cli::try_parse_from(["whoisthat", "-j", "status"]).is_ok());
        assert!(Cli::try_parse_from(["whoisthat", "mode", "tun"]).is_ok());
        assert!(Cli::try_parse_from(["whoisthat", "sub", "list"]).is_ok());
        assert!(
            Cli::try_parse_from(["whoisthat", "settings", "--set", "kill-switch=true"]).is_ok()
        );
        // Global --timeout and `test config --timeout` must not share an id.
        let parsed = Cli::try_parse_from(["whoisthat", "test", "config", "--test-timeout", "5"])
            .expect("test config timeout");
        match parsed.cmd {
            Some(Command::Test(TestArgs {
                cmd: TestCmd::Config(args),
            })) => assert_eq!(args.test_timeout, Some(5)),
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn sub_rename_name_is_optional() {
        let parsed = Cli::try_parse_from(["whoisthat", "sub", "rename", "12"]).expect("reset form");
        match parsed.cmd {
            Some(Command::Sub(a)) => match a.cmd {
                SubCmd::Rename(r) => assert_eq!(r.new_name, None),
                _ => panic!("expected sub rename"),
            },
            _ => panic!("expected sub"),
        }

        let parsed =
            Cli::try_parse_from(["whoisthat", "sub", "rename", "12", "Home"]).expect("named form");
        match parsed.cmd {
            Some(Command::Sub(a)) => match a.cmd {
                SubCmd::Rename(r) => assert_eq!(r.new_name.as_deref(), Some("Home")),
                _ => panic!("expected sub rename"),
            },
            _ => panic!("expected sub"),
        }
    }

    fn run_args(args: &[&str]) -> RunArgs {
        match Cli::try_parse_from(args).expect("run args").cmd {
            Some(Command::Run(a)) => a,
            _ => panic!("expected run"),
        }
    }

    #[test]
    fn run_parses_detach_and_app_arguments() {
        let a = run_args(&["whoisthat", "run", "-d", "spotify", "--foo", "-x"]);
        assert!(a.detach);
        assert_eq!(a.argv, vec!["spotify", "--foo", "-x"]);

        let a = run_args(&["whoisthat", "run", "spotify", "-d"]);
        assert!(!a.detach);
        assert_eq!(a.argv, vec!["spotify", "-d"]);
    }

    #[test]
    fn run_help_and_missing_target() {
        let err = Cli::try_parse_from(["whoisthat", "run", "--help"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);

        let err = Cli::try_parse_from(["whoisthat", "run"]).unwrap_err();
        assert_ne!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }
}
