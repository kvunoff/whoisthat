mod binaries;
mod capabilities;
mod dns;
mod ipc;
mod logs;
mod polkit;
mod storage;

#[cfg(test)]
mod tests;

mod util;

use crate::config;
use crate::doctor::types::DoctorReport;

use binaries::check_binaries;
use capabilities::check_capabilities;
use dns::check_dns_and_routing;
use ipc::check_ipc_and_daemon;
use logs::check_logs;
use polkit::check_polkit;
use storage::check_config_and_storage;

/// Must match `xraymgr.PinnedVersion` in core/core/lib/xraymgr/version.go.
const PINNED_XRAY_VERSION: &str = "26.10.31";

pub async fn run_doctor(include_logs: bool) -> DoctorReport {
    let cfg = config::load_config();

    let cat_binaries = check_binaries();
    let cat_capabilities = check_capabilities();
    let cat_polkit = check_polkit();
    let cat_ipc = check_ipc_and_daemon(&cfg).await;
    let cat_dns = check_dns_and_routing(&cfg);
    let cat_config = check_config_and_storage(&cfg);

    let mut categories = vec![
        cat_binaries,
        cat_capabilities,
        cat_polkit,
        cat_ipc,
        cat_dns,
        cat_config,
    ];
    if include_logs {
        categories.push(check_logs());
    }
    let mut report = DoctorReport::new(categories);
    if let Some((cv, proto)) = crate::core_client::query::fetch_core_version_ipc().await {
        report.core_version = Some(cv);
        report.core_protocol_version = Some(proto);
    } else {
        // Offline: never execute the core binary (old binaries ignore
        // --version and boot a daemon). Use last-seen version from config.
        let last_seen = cfg.core_version.clone();
        if !last_seen.is_empty() {
            report.core_version = Some(last_seen);
        }
    }
    report
}
