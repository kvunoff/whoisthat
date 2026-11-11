use std::process::Command;

use crate::doctor::types::{CheckCategory, CheckItem};

use super::binaries::find_binary;

pub(super) fn check_polkit() -> CheckCategory {
    let mut items = Vec::new();

    // 1. Polkit daemon. systemctl is systemd-only; elsewhere look for the
    // daemon binary/process instead of reporting a false failure.
    if crate::platform::has_systemd() {
        let polkit_active = Command::new("systemctl")
            .arg("is-active")
            .arg("polkit.service")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
            .unwrap_or(false);

        if polkit_active {
            items.push(CheckItem::pass("polkit daemon", "active (polkit.service)"));
        } else {
            items.push(CheckItem::warn(
                "polkit daemon",
                "polkit.service is not active",
                Some("Start polkit daemon: sudo systemctl start polkit".into()),
            ));
        }
    } else {
        let running = Command::new("pgrep")
            .args(["-x", "polkitd"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        let installed = find_binary(
            "polkitd",
            &["/usr/lib/polkit-1/polkitd", "/usr/libexec/polkit-1/polkitd"],
        );
        if running {
            items.push(CheckItem::pass("polkit daemon", "running (polkitd)"));
        } else if let Some(path) = installed {
            items.push(CheckItem::info(
                "polkit daemon",
                format!("installed but not running: {path}"),
            ));
        } else {
            items.push(CheckItem::info(
                "polkit daemon",
                "not found (optional: needed for pkexec-based capability setup)",
            ));
        }
    }

    // 2. pkexec
    if let Some(path) = find_binary("pkexec", &["/usr/bin/pkexec"]) {
        items.push(CheckItem::pass("pkexec", path));
    } else {
        items.push(CheckItem::warn(
            "pkexec",
            "pkexec binary not found",
            Some("Install 'polkit' package to allow graphical/prompt capability setup".into()),
        ));
    }

    // 3. Systemd linger (systemd only)
    if crate::platform::has_systemd() {
        let user = std::env::var("USER").unwrap_or_default();
        if !user.is_empty() {
            let linger_res = Command::new("loginctl")
                .arg("show-user")
                .arg(&user)
                .arg("--property=Linger")
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).contains("Linger=yes"))
                .unwrap_or(false);

            if linger_res {
                items.push(CheckItem::pass(
                    "systemd linger",
                    format!("enabled for user '{}'", user),
                ));
            } else {
                items.push(CheckItem::info(
                    "systemd linger",
                    format!("disabled for user '{}' (enable with: loginctl enable-linger {} to start VPN on boot)", user, user),
                ));
            }
        }
    }

    CheckCategory::new("Polkit & Permissions", items)
}
