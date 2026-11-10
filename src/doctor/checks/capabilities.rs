use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::core_spawn::find_core_binary;
use crate::doctor::types::{CheckCategory, CheckItem};
use crate::platform::{self, PlatformPaths};

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

    // 4. cgroup v2 / split-tunnel prerequisites.
    let cgroup_controllers = Path::new("/sys/fs/cgroup/cgroup.controllers");
    if !cgroup_controllers.exists() {
        items.push(CheckItem::warn(
            "cgroup v2",
            "/sys/fs/cgroup/cgroup.controllers not found — system may use legacy cgroup v1",
            Some("Split-tunneling ('whoisthat run') requires unified cgroup v2 hierarchy".into()),
        ));
    } else {
        let uid = crate::config::uid();
        let paths = PlatformPaths::default();
        let setup_hint = format!("sudo {}/whoisthat/cgroup-setup $USER", paths.lib.display());
        if platform::has_systemd() {
            let manager_ok = Command::new("systemctl")
                .args(["--user", "show-environment"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            let leaf = PathBuf::from(format!(
                "/sys/fs/cgroup/user.slice/user-{uid}.slice/user@{uid}.service"
            ));
            if !manager_ok {
                items.push(CheckItem::warn(
                    "split cgroup",
                    "systemd user manager is not reachable — split tunnelling will not work",
                    Some(format!(
                        "Log in as user {uid} in a session, or enable lingering: sudo loginctl enable-linger $USER"
                    )),
                ));
            } else if owner_writable_dir(&leaf, uid) {
                items.push(CheckItem::pass(
                    "split cgroup",
                    format!("{} (delegated to uid {uid})", leaf.display()),
                ));
            } else {
                items.push(CheckItem::warn(
                    "split cgroup",
                    format!(
                        "{} is missing or not writable by uid {uid}",
                        leaf.display()
                    ),
                    Some(
                        "Check systemd cgroup delegation (systemd.unified_cgroup_hierarchy=1) and lingering"
                            .into(),
                    ),
                ));
            }
        } else {
            check_native_cgroup(&mut items, &paths, uid, &setup_hint);
            check_runner(&mut items, uid);
        }
    }

    // 5. nftables socket cgroupv2 support: without it nft cannot match split
    // apps by cgroup and the TUN start would fail after the split check above.
    if has_nft {
        let text = Command::new("nft")
            .args(["describe", "socket"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase())
            .unwrap_or_default();
        if text.contains("cgroupv2") {
            items.push(CheckItem::pass(
                "nft socket cgroupv2",
                "supported by nftables",
            ));
        } else if text.is_empty() {
            items.push(CheckItem::info(
                "nft socket cgroupv2",
                "cannot probe without root; split tunnelling needs kernel >= 5.13 and an nftables build with cgroupv2",
            ));
        } else {
            items.push(CheckItem::warn(
                "nft socket cgroupv2",
                "this nftables/kernel does not list cgroupv2 socket matching — split-tunnel TUN start will fail",
                Some("Upgrade the kernel (>= 5.13) and nftables".into()),
            ));
        }
    }

    CheckCategory::new("Linux Capabilities & Kernel", items)
}

fn owner_writable_dir(path: &Path, uid: u32) -> bool {
    fs::metadata(path)
        .map(|meta| meta.is_dir() && meta.uid() == uid && meta.permissions().mode() & 0o200 != 0)
        .unwrap_or(false)
}

fn check_runner(items: &mut Vec<CheckItem>, uid: u32) {
    let socket = crate::config::runtime_dir().join("runner.sock");
    let healthy = std::os::unix::net::UnixStream::connect(&socket).is_ok();
    if healthy {
        items.push(CheckItem::pass(
            "split runner",
            format!("{} (uid {uid})", socket.display()),
        ));
        return;
    }
    let hint = match platform::detect_init() {
        platform::InitSystem::OpenRC => "sudo rc-service whoisthat-runner start",
        platform::InitSystem::Dinit => "sudo dinitctl start whoisthat-runner",
        platform::InitSystem::Runit => "sudo sv start whoisthat-runner",
        _ => "start the whoisthat-runner service",
    };
    items.push(CheckItem::warn(
        "split runner",
        format!(
            "{} is not reachable — 'whoisthat run' will not work",
            socket.display()
        ),
        Some(hint.to_string()),
    ));
}

/// Native (non-systemd) split path: a root-created per-uid cgroup plus the
/// canonical runtime directory tree.
fn check_native_cgroup(
    items: &mut Vec<CheckItem>,
    paths: &PlatformPaths,
    uid: u32,
    setup_hint: &str,
) {
    let cgroup = paths.sys_cgroup.join(format!("whoisthat-{uid}"));
    let procs = cgroup.join("cgroup.procs");
    let mut issues = Vec::new();

    if !owner_writable_dir(&cgroup, uid) {
        issues.push(format!("{} missing or not owned by you", cgroup.display()));
    }
    match fs::metadata(&procs) {
        Ok(meta) if meta.uid() == uid => {}
        Ok(_) => issues.push(format!("{} is root-owned", procs.display())),
        Err(_) => issues.push(format!("{} missing", procs.display())),
    }

    if issues.is_empty() {
        items.push(CheckItem::pass(
            "split cgroup",
            format!("{} (writable by uid {uid})", cgroup.display()),
        ));
    } else {
        items.push(CheckItem::warn(
            "split cgroup",
            format!("split tunnelling is unavailable: {}", issues.join("; ")),
            Some(format!(
                "{setup_hint} (available after installing the helper)"
            )),
        ));
    }

    let run_root = paths.run.join("whoisthat");
    let user_run = run_root.join(uid.to_string());
    let run_root_ok = fs::metadata(&run_root)
        .map(|m| m.is_dir() && matches!(m.permissions().mode() & 0o777, 0o755 | 0o711))
        .unwrap_or(false);
    let user_run_ok = fs::metadata(&user_run)
        .map(|m| m.is_dir() && m.uid() == uid && m.permissions().mode() & 0o777 == 0o700)
        .unwrap_or(false);
    if run_root_ok && user_run_ok {
        items.push(CheckItem::pass(
            "runtime dir",
            format!("{} (0700, uid {uid})", user_run.display()),
        ));
    } else {
        items.push(CheckItem::warn(
            "runtime dir",
            format!(
                "{} or {} has the wrong owner/mode",
                run_root.display(),
                user_run.display()
            ),
            Some(setup_hint.to_string()),
        ));
    }
}
