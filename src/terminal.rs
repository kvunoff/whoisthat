//! Re-launch the TUI in a terminal when it is started without one (desktop entry).

use std::os::unix::process::CommandExt;
use std::process::Command;

/// Known emulators, most common first, and the arguments that precede the command.
const TERMINALS: &[(&str, &[&str])] = &[
    ("alacritty", &["-e"]),
    ("ghostty", &["-e"]),
    ("kitty", &[]),
    // -e is a no-op; foot takes the command as trailing arguments.
    ("foot", &["-e"]),
    ("wezterm", &["start", "--"]),
    ("konsole", &["-e"]),
    ("xfce4-terminal", &["-x"]),
    ("gnome-terminal", &["--"]),
    ("kgx", &["-e"]),
    ("tilix", &["-x"]),
    ("mate-terminal", &["-x"]),
    ("terminator", &["-x"]),
    ("lxterminal", &["-e"]),
    ("qterminal", &["-e"]),
    ("terminology", &["-e"]),
    ("termite", &["-e"]),
    ("rio", &["-e"]),
    ("urxvt", &["-e"]),
    ("rxvt", &["-e"]),
    ("st", &["-e"]),
    ("xterm", &["-e"]),
];

/// System default terminal launchers, tried after the known emulators.
const LAUNCHERS: &[(&str, &[&str])] = &[
    ("xdg-terminal-exec", &[]),
    ("x-terminal-emulator", &["-e"]),
    ("sensible-terminal", &["-e"]),
    ("i3-sensible-terminal", &["-e"]),
];

/// Configured terminal, then `$TERMINAL`, then the first installed emulator,
/// then the system default launcher.
fn resolve(configured: &str) -> Option<(String, Vec<String>)> {
    parse(configured)
        .or_else(|| std::env::var("TERMINAL").ok().and_then(|v| parse(&v)))
        .or_else(|| {
            TERMINALS
                .iter()
                .chain(LAUNCHERS.iter())
                .find(|(bin, _)| crate::launcher::which(bin).is_some())
                .and_then(|(bin, _)| parse(bin))
        })
}

/// Splits `terminal` into a program and its prefix arguments. A bare known
/// emulator gets its required flags; anything with explicit arguments is used
/// as written.
fn parse(value: &str) -> Option<(String, Vec<String>)> {
    let mut words = value.split_whitespace();
    let program = words.next()?;
    let args: Vec<String> = words.map(str::to_string).collect();
    if !args.is_empty() {
        return Some((program.to_string(), args));
    }
    let prefix = known_args(program).unwrap_or(&["-e"]);
    Some((
        program.to_string(),
        prefix.iter().map(|s| s.to_string()).collect(),
    ))
}

fn known_args(program: &str) -> Option<&'static [&'static str]> {
    let name = std::path::Path::new(program).file_name()?.to_str()?;
    TERMINALS
        .iter()
        .chain(LAUNCHERS.iter())
        .find(|(bin, _)| *bin == name)
        .map(|(_, args)| *args)
}

/// Replaces this process with `<terminal> <args> <current exe>`. Never returns.
pub fn relaunch(configured: &str) -> ! {
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "whoisthat".to_string());

    let Some((program, args)) = resolve(configured) else {
        crate::deeplink::notify_desktop(
            "WhoisThat",
            "No terminal emulator found. Install one or set `terminal` in config.toml.",
        );
        eprintln!("whoisthat: no terminal emulator found");
        eprintln!("Set `terminal` in ~/.config/whoisthat/config.toml or $TERMINAL, e.g.:");
        eprintln!("  terminal = \"alacritty\"");
        std::process::exit(1);
    };

    let mut cmd = Command::new(&program);
    cmd.args(&args).arg(&exe);
    let err = cmd.exec();
    crate::deeplink::notify_desktop(
        "WhoisThat",
        &format!("Could not start terminal '{program}': {err}"),
    );
    eprintln!("whoisthat: could not start terminal '{program}': {err}");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_known_terminal_gets_its_flags() {
        assert_eq!(
            parse("alacritty"),
            Some(("alacritty".into(), vec!["-e".into()]))
        );
        assert_eq!(parse("kitty"), Some(("kitty".into(), vec![])));
        assert_eq!(
            parse("gnome-terminal"),
            Some(("gnome-terminal".into(), vec!["--".into()]))
        );
        assert_eq!(
            parse("xfce4-terminal"),
            Some(("xfce4-terminal".into(), vec!["-x".into()]))
        );
        assert_eq!(
            parse("ghostty"),
            Some(("ghostty".into(), vec!["-e".into()]))
        );
        assert_eq!(parse("kgx"), Some(("kgx".into(), vec!["-e".into()])));
    }

    #[test]
    fn explicit_arguments_are_kept() {
        assert_eq!(
            parse("wezterm start --"),
            Some(("wezterm".into(), vec!["start".into(), "--".into()]))
        );
    }

    #[test]
    fn path_uses_the_basename_for_known_terminal() {
        assert_eq!(
            parse("/usr/bin/alacritty"),
            Some(("/usr/bin/alacritty".into(), vec!["-e".into()]))
        );
    }

    #[test]
    fn unknown_bare_program_defaults_to_dash_e() {
        assert_eq!(parse("myterm"), Some(("myterm".into(), vec!["-e".into()])));
    }

    #[test]
    fn empty_value_is_auto() {
        assert_eq!(parse("   "), None);
    }

    #[test]
    fn priority_order_is_stable() {
        let names: Vec<&str> = TERMINALS.iter().map(|(name, _)| *name).collect();
        let at = |name: &str| names.iter().position(|n| *n == name).unwrap();
        assert!(at("alacritty") < at("ghostty"));
        assert!(at("ghostty") < at("kitty"));
        assert!(at("wezterm") < at("konsole"));
        assert!(at("konsole") < at("gnome-terminal"));
    }

    #[test]
    fn system_defaults_come_last() {
        assert_eq!(LAUNCHERS[0].0, "xdg-terminal-exec");
        assert_eq!(known_args("xdg-terminal-exec"), Some(&[][..]));
        assert_eq!(known_args("x-terminal-emulator"), Some(&["-e"][..]));
    }
}
