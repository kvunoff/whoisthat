use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::config::{self, AppConfig};
use crate::doctor::types::{CheckCategory, CheckItem};

pub(super) fn check_config_and_storage(cfg: &AppConfig) -> CheckCategory {
    let mut items = Vec::new();

    // 1. TUI config
    let tui_cfg_path = config::config_path();
    if tui_cfg_path.exists() {
        items.push(CheckItem::pass(
            "TUI config",
            format!(
                "{} (theme: {}, test: {})",
                tui_cfg_path.display(),
                cfg.theme,
                cfg.test_method
            ),
        ));
    } else {
        items.push(CheckItem::info(
            "TUI config",
            format!(
                "{} (using defaults, created on first save)",
                tui_cfg_path.display()
            ),
        ));
    }

    // 2. Core config
    let core_cfg_path = config::config_dir().join("config.json");
    if core_cfg_path.exists() {
        match fs::read_to_string(&core_cfg_path) {
            Ok(json_str) => match serde_json::from_str::<serde_json::Value>(&json_str) {
                Ok(val) => {
                    let socks = val.get("socks-port").and_then(|v| v.as_i64()).unwrap_or(0);
                    let http = val.get("http-port").and_then(|v| v.as_i64()).unwrap_or(0);
                    items.push(CheckItem::pass(
                        "core config",
                        format!(
                            "{} (SOCKS: {}, HTTP: {})",
                            core_cfg_path.display(),
                            socks,
                            http
                        ),
                    ));
                }
                Err(e) => {
                    items.push(CheckItem::warn(
                        "core config",
                        format!("invalid JSON: {}", e),
                        Some("Core will recreate default config on startup".into()),
                    ));
                }
            },
            Err(e) => {
                items.push(CheckItem::warn(
                    "core config",
                    format!("cannot read {}: {}", core_cfg_path.display(), e),
                    None,
                ));
            }
        }
    } else {
        items.push(CheckItem::info(
            "core config",
            format!(
                "{} (auto-generated on core startup)",
                core_cfg_path.display()
            ),
        ));
    }

    // 3. Encrypted DB directory
    let db_dir = config::data_dir().join("db");
    if db_dir.exists() {
        // The key moved out of the DB directory; accept the legacy location
        // during the migration window so doctor stays useful on old installs.
        let new_key = config::config_dir().join("db.key");
        let key_file = if new_key.exists() {
            new_key
        } else {
            db_dir.join(".key")
        };
        if key_file.exists() {
            let meta = fs::metadata(&key_file);
            let key_size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let mode = meta
                .as_ref()
                .map(|m| m.permissions().mode() & 0o777)
                .unwrap_or(0);
            if key_size != 32 {
                items.push(CheckItem::warn(
                    "encrypted db",
                    format!(
                        "key file {} is {} bytes (expected 32 bytes)",
                        key_file.display(),
                        key_size
                    ),
                    Some("Restore the key from a backup; do not delete it while encrypted data exists".into()),
                ));
            } else if mode & 0o077 != 0 {
                items.push(CheckItem::warn(
                    "encrypted db",
                    format!(
                        "key file {} has mode {:o} (expected 0600)",
                        key_file.display(),
                        mode
                    ),
                    Some(format!("chmod 600 {}", key_file.display())),
                ));
            } else {
                items.push(CheckItem::pass(
                    "encrypted db",
                    format!(
                        "{} (AES-256-GCM key present at {})",
                        db_dir.display(),
                        key_file.display()
                    ),
                ));
            }
        } else {
            items.push(CheckItem::warn(
                "encrypted db",
                format!(
                    "{} exists but the encryption key is missing",
                    db_dir.display()
                ),
                Some("Core will generate a new AES key on startup".into()),
            ));
        }
    } else {
        items.push(CheckItem::info(
            "encrypted db",
            format!(
                "{} (will be created on first profile/group add)",
                db_dir.display()
            ),
        ));
    }

    // 4. Geo assets (geoip.dat, geosite.dat)
    let (geoip, geosite) = find_geo_assets();
    match (geoip, geosite) {
        (Some(p_ip), Some(p_site)) => {
            let size_ip = fs::metadata(&p_ip).map(|m| m.len()).unwrap_or(0);
            let size_site = fs::metadata(&p_site).map(|m| m.len()).unwrap_or(0);
            let mb_ip = size_ip as f64 / 1_048_576.0;
            let mb_site = size_site as f64 / 1_048_576.0;

            if size_ip >= 10 * 1024 * 1024 {
                items.push(CheckItem::pass(
                    "geo assets",
                    format!(
                        "geoip.dat ({:.1} MB), geosite.dat ({:.1} MB)",
                        mb_ip, mb_site
                    ),
                ));
            } else {
                items.push(CheckItem::warn(
                    "geo assets",
                    format!(
                        "geoip.dat ({:.1} MB is under 10MB) — may trigger re-download",
                        mb_ip
                    ),
                    None,
                ));
            }
        }
        _ => {
            items.push(CheckItem::info(
                "geo assets",
                "not downloaded yet (auto-downloaded from v2fly on first xray startup)",
            ));
        }
    }

    CheckCategory::new("Configuration & Storage", items)
}

fn find_geo_assets() -> (Option<PathBuf>, Option<PathBuf>) {
    let mut search_dirs = Vec::new();

    search_dirs.push(config::config_dir().join("geo"));

    let data_dir = config::data_dir();
    let runtime_dir = data_dir.join("runtimes").join("xray");
    if let Ok(entries) = fs::read_dir(&runtime_dir) {
        for entry in entries.flatten() {
            search_dirs.push(entry.path());
        }
    }

    search_dirs.push(PathBuf::from("/usr/share/xray"));
    search_dirs.push(PathBuf::from("/usr/local/share/xray"));

    for dir in search_dirs {
        let ip = dir.join("geoip.dat");
        let site = dir.join("geosite.dat");
        if ip.exists() && site.exists() {
            return (Some(ip), Some(site));
        }
    }

    (None, None)
}
