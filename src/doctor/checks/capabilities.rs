use std::fs;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::Path;
use std::process::Command;

use crate::core_spawn::find_core_binary;
use crate::doctor::types::{CheckCategory, CheckItem};

pub(super) fn check_capabilities() -> CheckCategory {
    let mut items = Vec::new();

    // 1. whoisthat-core capabilities
    let core_bin = find_core_binary();
    let core_path = Path::new(&core_bin);
    if core_path.exists() {
        let caps_output = Command::new("getcap")
            .arg(&core_bin)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        let has_net_admin = caps_output.contains("cap_net_admin");
        let has_net_raw = caps_output.contains("cap_net_raw");
        let has_setpcap = caps_output.contains("cap_setpcap");

        if has_net_admin && has_net_raw && has_setpcap {
            let caps_str = caps_output.split_whitespace().last().unwrap_or("verified");
            items.push(CheckItem::pass(
                "whoisthat-core caps",
                format!("{} ({})", core_bin, caps_str),
            ));
        } else {
            let fix = format!(
                "sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep {}",
                core_bin
            );
            items.push(CheckItem::fail(
                "whoisthat-core caps",
                "missing required network capabilities for rootless TUN mode",
                Some(fix),
            ));
        }
    } else {
        items.push(CheckItem::fail(
            "whoisthat-core caps",
            "cannot check capabilities because whoisthat-core binary was not found",
            None,
        ));
    }

    // 2. /dev/net/tun
    let tun_dev = Path::new("/dev/net/tun");
    if tun_dev.exists() {
        match fs::metadata(tun_dev) {
            Ok(meta) => {
                let is_char = meta.file_type().is_char_device();
                let mode = meta.permissions().mode() & 0o777;
                if is_char {
                    items.push(CheckItem::pass(
                        "/dev/net/tun",
                        format!("available (char device, mode {:04o})", mode),
                    ));
                } else {
                    items.push(CheckItem::fail(
                        "/dev/net/tun",
                        "exists but is not a character device",
                        Some("Ensure the 'tun' kernel module is loaded: sudo modprobe tun".into()),
                    ));
                }
            }
            Err(e) => {
                items.push(CheckItem::fail(
                    "/dev/net/tun",
                    format!("cannot read metadata: {}", e),
                    None,
                ));
            }
        }
    } else {
        items.push(CheckItem::fail(
            "/dev/net/tun",
            "virtual device /dev/net/tun not found",
            Some("Load tun kernel module: sudo modprobe tun".into()),
        ));
    }

    // 3. Firewall backend
    let has_nft = Command::new("nft").arg("--version").output().is_ok();
    let has_iptables = Command::new("iptables").arg("--version").output().is_ok();

    if has_nft {
        items.push(CheckItem::pass(
            "firewall backend",
            "nftables active (recommended)",
        ));
    } else if has_iptables {
        items.push(CheckItem::pass("firewall backend", "iptables legacy mode"));
    } else {
        items.push(CheckItem::fail(
            "firewall backend",
            "neither nftables nor iptables found — TUN routing & killswitch will fail",
            Some("Install 'nftables' or 'iptables' from package manager".into()),
        ));
    }

    // 4. cgroup v2
    let cgroup_controllers = Path::new("/sys/fs/cgroup/cgroup.controllers");
    if cgroup_controllers.exists() {
        items.push(CheckItem::pass(
            "cgroup v2",
            "available — split-tunneling supported",
        ));
    } else {
        items.push(CheckItem::warn(
            "cgroup v2",
            "/sys/fs/cgroup/cgroup.controllers not found — system may use legacy cgroup v1",
            Some("Split-tunneling ('whoisthat run') requires unified cgroup v2 hierarchy".into()),
        ));
    }

    CheckCategory::new("Linux Capabilities & Kernel", items)
}
