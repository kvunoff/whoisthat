use std::fs;
use std::path::Path;
use std::process::Command;

use crate::core_spawn::find_core_binary;
use crate::doctor::types::{CheckCategory, CheckItem};

use super::PINNED_XRAY_VERSION;

pub(super) fn check_binaries() -> CheckCategory {
    let mut items = Vec::new();

    // 1. whoisthat (client)
    let client_path = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "whoisthat".to_string());
    let version = env!("CARGO_PKG_VERSION");
    items.push(CheckItem::pass(
        "whoisthat",
        format!("{} (v{})", client_path, version),
    ));

    // 2. whoisthat-core (daemon)
    // NOTE: existence check only — never execute the binary here, old
    // binaries ignore --version and boot a daemon instead.
    let core_bin = find_core_binary();
    let core_path = Path::new(&core_bin);
    if core_path.exists() {
        items.push(CheckItem::pass("whoisthat-core", core_bin.clone()));
    } else {
        items.push(CheckItem::fail(
            "whoisthat-core",
            "daemon binary not found in PATH or standard directories",
            Some("sudo install -Dm755 core/core/whoisthat-core /usr/bin/whoisthat-core".into()),
        ));
    }

    // 3. xray (Xray-core)
    let xray_info = probe_xray();
    match xray_info {
        Some((path, ver)) => {
            let msg = format!("{} ({})", path, ver);
            if ver.contains(PINNED_XRAY_VERSION) {
                items.push(CheckItem::pass("xray-core", msg));
            } else {
                items.push(CheckItem::warn(
                    "xray-core",
                    format!("{} (recommended pinned version: v{PINNED_XRAY_VERSION})", msg),
                    Some(format!("WhoisThat will auto-download pinned v{PINNED_XRAY_VERSION} into ~/.local/share/whoisthat/runtimes/xray/")),
                ));
            }
        }
        None => {
            items.push(CheckItem::warn(
                "xray-core",
                "xray binary not found in PATH or managed runtimes",
                Some(format!(
                    "Install 'xray' or run WhoisThat to trigger automatic download of v{PINNED_XRAY_VERSION}"
                )),
            ));
        }
    }

    // 5. System CLI tools
    let mut missing_tools = Vec::new();
    let mut found_tools = Vec::new();
    for tool in ["ip", "getcap", "setcap", "pkexec"] {
        if find_binary(tool, &[]).is_some() {
            found_tools.push(tool);
        } else {
            missing_tools.push(tool);
        }
    }

    if missing_tools.is_empty() {
        items.push(CheckItem::pass(
            "system tools",
            format!("{} available", found_tools.join(", ")),
        ));
    } else {
        items.push(CheckItem::warn(
            "system tools",
            format!("missing: {}", missing_tools.join(", ")),
            Some("Install iproute2, libcap, and polkit packages".into()),
        ));
    }

    CheckCategory::new("Binaries & Runtimes", items)
}

pub(super) fn find_binary(name: &str, candidates: &[&str]) -> Option<String> {
    if let Ok(output) = Command::new("which").arg(name).output()
        && output.status.success()
    {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !s.is_empty() && Path::new(&s).exists() {
            return Some(s);
        }
    }

    for c in candidates {
        let p = Path::new(c);
        if p.exists() && p.is_file() {
            return Some(c.to_string());
        }
    }

    None
}

pub(super) fn probe_xray() -> Option<(String, String)> {
    let mut candidates = Vec::new();

    // Check managed runtime directories
    let data_dir = crate::config::data_dir();
    let runtime_dir = data_dir.join("runtimes").join("xray");
    if runtime_dir.exists()
        && let Ok(entries) = fs::read_dir(&runtime_dir)
    {
        let mut dirs: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        dirs.sort_by(|a, b| b.cmp(a)); // newest-looking first; version match is checked per candidate
        for dir in dirs {
            let bin = dir.join("xray");
            if bin.exists() {
                candidates.push(bin.to_string_lossy().to_string());
            }
        }
    }

    // Check system binaries
    if let Some(sys) = find_binary("xray", &["/usr/bin/xray", "/usr/local/bin/xray"]) {
        candidates.push(sys);
    }

    let mut first_working = None;

    for bin in candidates {
        if let Ok(output) = Command::new(&bin).arg("version").output()
            && output.status.success()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut found_ver = "version unknown".to_string();
            for line in text.lines() {
                if line.starts_with("Xray ") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        found_ver = parts[1].to_string();
                        break;
                    }
                }
            }

            if found_ver.contains(PINNED_XRAY_VERSION) {
                return Some((bin, found_ver));
            }

            if first_working.is_none() {
                first_working = Some((bin, found_ver));
            }
        }
    }

    first_working
}
