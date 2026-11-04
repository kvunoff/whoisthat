use std::process::Command;

/// Sends a desktop notification using notify-send if available.
pub fn notify_desktop(title: &str, message: &str) {
    let _ = Command::new("notify-send")
        .arg("-a")
        .arg("WhoisThat")
        .arg("-i")
        .arg("preferences-system-network")
        .arg(title)
        .arg(message)
        .spawn();
}

/// The project logo, embedded so user-level integration has an icon even when
/// WhoisThat was not installed by a package manager.
const ICON_SVG: &[u8] = include_bytes!("../../docs/public/logo.svg");

const HANDLER_DESKTOP_FILE: &str = "whoisthat-url.desktop";

/// Quotes a path for the desktop entry `Exec` key, which is not a shell.
fn exec_quote(path: &str) -> String {
    format!("\"{}\"", path.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Launcher entry: clicking the icon opens the TUI in the configured terminal.
fn launcher_entry(exe: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Name=WhoisThat\n\
         GenericName=VPN Client\n\
         Comment=Modern terminal-based VPN client with Xray-core backend\n\
         Exec={}\n\
         Icon=whoisthat\n\
         Terminal=false\n\
         Type=Application\n\
         Categories=Network;X-VPN;ConsoleOnly;\n\
         Keywords=vpn;proxy;xray;vless;vmess;hysteria;\n",
        exec_quote(exe)
    )
}

/// Hidden entry that only handles `wit://` links; it must not open a terminal.
fn url_handler_entry(exe: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Name=WhoisThat URL Handler\n\
         Comment=Handle wit:// deep links for WhoisThat\n\
         Exec={} link handle %u\n\
         Icon=whoisthat\n\
         Terminal=false\n\
         NoDisplay=true\n\
         Type=Application\n\
         MimeType=x-scheme-handler/wit;\n",
        exec_quote(exe)
    )
}

struct DesktopFiles {
    apps_dir: std::path::PathBuf,
    launcher: std::path::PathBuf,
    handler: std::path::PathBuf,
    icon: std::path::PathBuf,
}

fn desktop_files(home: &std::path::Path) -> DesktopFiles {
    let apps_dir = home.join(".local/share/applications");
    DesktopFiles {
        launcher: apps_dir.join("whoisthat.desktop"),
        handler: apps_dir.join(HANDLER_DESKTOP_FILE),
        icon: home.join(".local/share/icons/hicolor/scalable/apps/whoisthat.svg"),
        apps_dir,
    }
}

fn write_desktop_files(files: &DesktopFiles, exe: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(&files.apps_dir)?;
    if let Some(dir) = files.icon.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&files.launcher, launcher_entry(exe))?;
    std::fs::write(&files.handler, url_handler_entry(exe))?;
    std::fs::write(&files.icon, ICON_SVG)?;
    Ok(())
}

fn current_exe_string() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "whoisthat".to_string())
}

/// Points `x-scheme-handler/wit` at the hidden URL handler entry.
fn register_url_handler(files: &DesktopFiles) -> bool {
    let ok = Command::new("xdg-mime")
        .args(["default", HANDLER_DESKTOP_FILE, "x-scheme-handler/wit"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let _ = Command::new("update-desktop-database")
        .arg(&files.apps_dir)
        .status();
    ok
}

/// Registers the desktop integration in ~/.local/share/applications/
pub fn register_desktop_handler() -> Result<(), Box<dyn std::error::Error>> {
    let home = std::env::var("HOME").map_err(|_| "HOME environment variable not set")?;
    let files = desktop_files(std::path::Path::new(&home));
    let exe = current_exe_string();

    write_desktop_files(&files, &exe)?;
    println!("✓ Written desktop entries: {}", files.launcher.display());
    println!("                          {}", files.handler.display());

    if register_url_handler(&files) {
        println!("✓ Registered {HANDLER_DESKTOP_FILE} as default handler for x-scheme-handler/wit");
    } else {
        println!(
            "⚠ Note: could not register {HANDLER_DESKTOP_FILE} for x-scheme-handler/wit (xdg-utils might not be installed)"
        );
    }

    Ok(())
}

/// Silently ensures the desktop integration is registered in the background.
/// Only writes and calls xdg-mime if an entry is missing, points to a stale
/// binary, or is not registered as the default scheme handler.
pub fn ensure_desktop_registered() {
    std::thread::spawn(|| {
        let home = match std::env::var("HOME") {
            Ok(h) => h,
            Err(_) => return,
        };
        let files = desktop_files(std::path::Path::new(&home));
        let exe = current_exe_string();

        let needs_write = std::fs::read_to_string(&files.launcher)
            .map(|c| c != launcher_entry(&exe))
            .unwrap_or(true)
            || std::fs::read_to_string(&files.handler)
                .map(|c| c != url_handler_entry(&exe))
                .unwrap_or(true)
            || !files.icon.is_file();

        if needs_write {
            let _ = write_desktop_files(&files, &exe);
            log::info!(
                "Wrote desktop integration entries to {}",
                files.apps_dir.display()
            );
        }

        // Verify if default handler is registered
        let registered = Command::new("xdg-mime")
            .args(["query", "default", "x-scheme-handler/wit"])
            .output()
            .map(|out| String::from_utf8_lossy(&out.stdout).trim() == HANDLER_DESKTOP_FILE)
            .unwrap_or(false);

        if !registered || needs_write {
            register_url_handler(&files);
            log::info!("Registered {HANDLER_DESKTOP_FILE} as default x-scheme-handler/wit handler");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_entry_opens_tui() {
        let entry = launcher_entry("/usr/bin/whoisthat");
        assert!(entry.contains("Exec=\"/usr/bin/whoisthat\"\n"));
        assert!(entry.contains("Icon=whoisthat\n"));
        assert!(entry.contains("Terminal=false\n"));
        assert!(!entry.contains("MimeType="));
        assert!(!entry.contains("%u"));
    }

    #[test]
    fn url_handler_entry_is_hidden_and_silent() {
        let entry = url_handler_entry("/usr/bin/whoisthat");
        assert!(entry.contains("Exec=\"/usr/bin/whoisthat\" link handle %u\n"));
        assert!(entry.contains("Icon=whoisthat\n"));
        assert!(entry.contains("Terminal=false\n"));
        assert!(entry.contains("NoDisplay=true\n"));
        assert!(entry.contains("MimeType=x-scheme-handler/wit;\n"));
    }

    #[test]
    fn exec_quote_escapes_paths() {
        assert_eq!(
            exec_quote("/opt/my app/whoisthat"),
            "\"/opt/my app/whoisthat\""
        );
        assert_eq!(exec_quote("/opt/a\"b\\c"), "\"/opt/a\\\"b\\\\c\"");
    }

    #[test]
    fn embedded_icon_is_svg() {
        assert!(String::from_utf8_lossy(ICON_SVG).contains("<svg"));
    }
}
