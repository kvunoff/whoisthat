use std::fs;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::process::Command;
use std::str::FromStr;

use crate::config::{self, AppConfig};
use crate::doctor::types::{CheckCategory, CheckItem};

use super::binaries::find_binary;
use super::util::permission_denied;

const DNS_SINK4: &str = "198.18.0.53";
const DNS_SINK6: &str = "fd00::53";
pub(super) const FAKE_POOL4: &str = "198.19.0.0/16";
pub(super) const FAKE_POOL6: &str = "fc00::/18";

#[derive(Default)]
struct CoreRuntimeConfig {
    split_mode: String,
    kill_switch: bool,
    tun_name: String,
}

enum DnsRules {
    Happ { text: String, ipv6: bool },
    Legacy,
    None,
    PermissionDenied,
    Unavailable,
}

pub(super) fn check_dns_and_routing(cfg: &AppConfig) -> CheckCategory {
    let mut items = Vec::new();
    let core_cfg = read_core_runtime_config();
    let tun_name = if core_cfg.tun_name.is_empty() {
        cfg.tun_name.clone()
    } else {
        core_cfg.tun_name.clone()
    };

    let tun_up = Command::new("ip")
        .args(["link", "show", "dev", &tun_name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    // The sink must never be assigned to an interface: kernel local delivery
    // would bypass the TUN inbound and Xray would never see the query.
    match Command::new("ip").args(["-o", "addr", "show"]).output() {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            if sink_assigned(&text) {
                items.push(CheckItem::fail(
                    "DNS sink address",
                    format!("{} or {} is assigned to an interface", DNS_SINK4, DNS_SINK6),
                    Some(
                        "Remove the address from the interface; the sink must stay unassigned"
                            .into(),
                    ),
                ));
            } else {
                items.push(CheckItem::pass(
                    "DNS sink address",
                    format!(
                        "{} / {} are not assigned to any interface",
                        DNS_SINK4, DNS_SINK6
                    ),
                ));
            }
        }
        _ => items.push(CheckItem::info(
            "DNS sink address",
            "could not inspect interface addresses",
        )),
    }

    if tun_up {
        match Command::new("ip")
            .args(["-o", "addr", "show", "dev", &tun_name])
            .output()
        {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout);
                match fake_pool_overlap(&text) {
                    Some(pool) => items.push(CheckItem::fail(
                        "FakeDNS pools",
                        format!("TUN address overlaps the {} fake pool", pool),
                        Some("Change the TUN address or the fake pool constants".into()),
                    )),
                    None => items.push(CheckItem::pass(
                        "FakeDNS pools",
                        format!(
                            "TUN address does not overlap {} / {}",
                            FAKE_POOL4, FAKE_POOL6
                        ),
                    )),
                }
            }
            _ => items.push(CheckItem::info(
                "FakeDNS pools",
                format!("could not inspect addresses of {}", tun_name),
            )),
        }
    } else {
        items.push(CheckItem::info(
            "FakeDNS pools",
            "TUN is inactive — pool overlap check skipped",
        ));
    }

    items.push(split_policy_route_item(tun_up, &core_cfg.split_mode));

    let dns_rules = probe_dns_rules();
    match &dns_rules {
        DnsRules::Happ { text, ipv6 } => {
            items.push(CheckItem::pass(
                "DNS hijack rules",
                format!("Happ sink ({} / {})", DNS_SINK4, DNS_SINK6),
            ));
            if !has_mark_return(text) {
                items.push(CheckItem::fail(
                    "DNS hijack safety returns",
                    "missing mark-1 return; Xray direct DNS or split-excluded apps may loop",
                    Some("Restart TUN so the scoped hijack is reinstalled".into()),
                ));
            } else if !has_sink_return(text) {
                items.push(CheckItem::fail(
                    "DNS hijack safety returns",
                    "missing sink return rule",
                    Some("Restart TUN so the scoped hijack is reinstalled".into()),
                ));
            } else {
                items.push(CheckItem::pass(
                    "DNS hijack safety returns",
                    "mark 1 and sink returns are present",
                ));
            }
            if text.to_lowercase().contains("masquerade") {
                items.push(CheckItem::warn(
                    "DNS hijack mode",
                    "Happ sink rules also contain masquerade (legacy leftovers?)",
                    Some("Restart TUN to rebuild a clean whoisthat table".into()),
                ));
            }
            if !ipv6 {
                items.push(CheckItem::warn(
                    "IPv6 DNS hijack",
                    "IPv6 hijack table is missing",
                    Some("Enable IPv6 in the kernel or restart TUN".into()),
                ));
            } else {
                items.push(CheckItem::pass(
                    "IPv6 DNS hijack",
                    format!("sink {} is installed", DNS_SINK6),
                ));
            }
        }
        DnsRules::Legacy => {
            items.push(CheckItem::pass(
                "DNS hijack rules",
                "legacy resolver DNAT (manual routing or Happ profile without DNS)",
            ));
        }
        DnsRules::None => {
            if !tun_up {
                items.push(CheckItem::info(
                    "DNS hijack rules",
                    "TUN is inactive — no hijack expected",
                ));
            } else if core_cfg.split_mode == "include" {
                items.push(CheckItem::info(
                    "DNS hijack rules",
                    "split include — hijack intentionally disabled",
                ));
            } else {
                items.push(CheckItem::warn(
                    "DNS hijack rules",
                    "TUN is active but no DNS hijack table was found",
                    Some(
                        "Restart TUN; if it persists, check core.log and firewall permissions"
                            .into(),
                    ),
                ));
            }
        }
        DnsRules::PermissionDenied => items.push(CheckItem::info(
            "DNS hijack rules",
            "cannot inspect firewall state without root (try: sudo nft list table ip whoisthat)",
        )),
        DnsRules::Unavailable => items.push(CheckItem::info(
            "DNS hijack rules",
            "neither nftables nor iptables is available for inspection",
        )),
    }

    let happ_mode = matches!(dns_rules, DnsRules::Happ { .. });
    if find_binary(
        "resolvectl",
        &["/usr/bin/resolvectl", "/usr/local/bin/resolvectl"],
    )
    .is_none()
    {
        items.push(CheckItem::info(
            "systemd-resolved",
            "resolvectl not installed — system resolver is left untouched",
        ));
    } else if !tun_up {
        items.push(CheckItem::info(
            "systemd-resolved",
            "TUN is inactive — resolvectl is not configured",
        ));
    } else if core_cfg.split_mode == "include" {
        items.push(CheckItem::info(
            "systemd-resolved",
            "split include — resolvectl is left untouched",
        ));
    } else {
        match Command::new("resolvectl").args(["dns", &tun_name]).output() {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout);
                if happ_mode && !text.contains(DNS_SINK4) {
                    items.push(CheckItem::warn(
                        "systemd-resolved",
                        format!("{} DNS is not pointed at the Happ sink", tun_name),
                        Some(format!("resolvectl dns {} {}", tun_name, DNS_SINK4)),
                    ));
                } else {
                    items.push(CheckItem::pass(
                        "systemd-resolved",
                        "system resolver configuration is consistent with the active DNS mode",
                    ));
                }
            }
            _ => items.push(CheckItem::info(
                "systemd-resolved",
                "resolvectl is installed but systemd-resolved is inactive; system resolver is left untouched (kernel DNAT still works)",
            )),
        }
    }

    if !core_cfg.kill_switch {
        items.push(CheckItem::info(
            "kill-switch DNS allowlist",
            "kill-switch is disabled",
        ));
    } else if !tun_up {
        items.push(CheckItem::info(
            "kill-switch DNS allowlist",
            "kill-switch is enabled, TUN is inactive",
        ));
    } else {
        match Command::new("nft")
            .args(["list", "table", "inet", "whoisthat_ks"])
            .output()
        {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout);
                let mut missing = Vec::new();
                if !text.contains(&tun_name) {
                    missing.push("TUN interface");
                }
                if !text.contains(DNS_SINK4) {
                    missing.push("DNS sink");
                }
                if !text.contains("mark 0x00000001") && !text.contains("mark 1") {
                    missing.push("mark 1");
                }
                if missing.is_empty() {
                    items.push(CheckItem::pass(
                        "kill-switch DNS allowlist",
                        "TUN, DNS sink, and mark 1 are allowed",
                    ));
                } else {
                    items.push(CheckItem::warn(
                        "kill-switch DNS allowlist",
                        format!("missing allows: {}", missing.join(", ")),
                        Some("Re-apply the kill-switch so the whitelist is regenerated".into()),
                    ));
                }
            }
            Ok(out) if permission_denied(&out.stderr) => items.push(CheckItem::info(
                "kill-switch DNS allowlist",
                "cannot inspect the kill-switch table without root (try: sudo nft list table inet whoisthat_ks)",
            )),
            _ => items.push(CheckItem::warn(
                "kill-switch DNS allowlist",
                "kill-switch is enabled but its nftables table was not found",
                Some("Restart the kill-switch or check core.log".into()),
            )),
        }
    }

    CheckCategory::new("DNS & Routing", items)
}

fn read_core_runtime_config() -> CoreRuntimeConfig {
    let path = config::config_dir().join("config.json");
    let Ok(text) = fs::read_to_string(path) else {
        return CoreRuntimeConfig::default();
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) else {
        return CoreRuntimeConfig::default();
    };
    CoreRuntimeConfig {
        split_mode: val
            .get("split-tunnel")
            .and_then(|v| v.get("mode"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        kill_switch: val
            .get("kill-switch-enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        tun_name: val
            .get("tun-name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    }
}

/// Exclude-mode split relies on `fwmark 1 -> table 100` (physical gateway).
/// TUN start installs it, but a startup reconcile or a network reset can drop
/// it while the TUN stays up — split apps then silently use the tunnel. These
/// ip commands are readable without root, unlike the nftables marking rules.
fn split_policy_route_item(tun_up: bool, split_mode: &str) -> CheckItem {
    const NAME: &str = "Split-tunnel routing";

    if !tun_up {
        return CheckItem::info(NAME, "TUN is inactive — no split policy route expected");
    }
    if split_mode != "exclude" {
        let mode = if split_mode.is_empty() {
            "off"
        } else {
            split_mode
        };
        return CheckItem::info(
            NAME,
            format!("split mode {mode} — mark 1/table 100 is not used"),
        );
    }

    let has_rule = Command::new("ip")
        .args(["rule", "show"])
        .output()
        .map(|o| has_mark1_policy_rule(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or(false);
    let has_route = Command::new("ip")
        .args(["route", "show", "table", "100"])
        .output()
        .map(|o| has_default_route(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or(false);

    if has_rule && has_route {
        CheckItem::pass(
            NAME,
            "exclude mode: mark 1 is routed via table 100 (physical link)",
        )
    } else {
        CheckItem::fail(
            NAME,
            "split exclude is on, but the mark 1 → table 100 policy route is missing; excluded apps are tunnelled",
            Some(
                "Re-apply the mode: whoisthat settings set split-tunnel=exclude (or restart TUN)"
                    .into(),
            ),
        )
    }
}

pub(super) fn has_mark1_policy_rule(rules: &str) -> bool {
    rules
        .lines()
        .any(|l| l.contains("fwmark 0x1") && (l.contains("lookup 100") || l.contains("table 100")))
}

pub(super) fn has_default_route(routes: &str) -> bool {
    routes
        .lines()
        .any(|l| l.trim_start().starts_with("default "))
}

fn probe_dns_rules() -> DnsRules {
    if find_binary("nft", &["/usr/bin/nft", "/usr/local/bin/nft"]).is_some() {
        let mut text = String::new();
        let mut any = false;
        let mut ipv6 = false;
        let mut denied = false;
        for (family, is_v6) in [("ip", false), ("ip6", true)] {
            match Command::new("nft")
                .args(["list", "table", family, "whoisthat"])
                .output()
            {
                Ok(out) if out.status.success() => {
                    any = true;
                    ipv6 |= is_v6;
                    text.push_str(&String::from_utf8_lossy(&out.stdout));
                }
                Ok(out) if permission_denied(&out.stderr) => denied = true,
                _ => {}
            }
        }
        if any {
            return match dns_mode_from_rules(&text) {
                Some("happ") => DnsRules::Happ { text, ipv6 },
                Some("legacy") => DnsRules::Legacy,
                _ => DnsRules::None,
            };
        }
        if denied {
            return DnsRules::PermissionDenied;
        }
        return DnsRules::None;
    }

    let mut text = String::new();
    let mut any = false;
    let mut ipv6 = false;
    let mut denied = false;
    for (cmd, is_v6) in [("iptables", false), ("ip6tables", true)] {
        if find_binary(cmd, &[]).is_none() {
            continue;
        }
        match Command::new(cmd).args(["-t", "nat", "-S"]).output() {
            Ok(out) if out.status.success() => {
                any = true;
                ipv6 |= is_v6;
                text.push_str(&String::from_utf8_lossy(&out.stdout));
            }
            Ok(out) if permission_denied(&out.stderr) => denied = true,
            _ => {}
        }
    }
    if any {
        return match dns_mode_from_rules(&text) {
            Some("happ") => DnsRules::Happ { text, ipv6 },
            Some("legacy") => DnsRules::Legacy,
            _ => DnsRules::None,
        };
    }
    if denied {
        return DnsRules::PermissionDenied;
    }
    DnsRules::Unavailable
}

pub(super) fn dns_mode_from_rules(text: &str) -> Option<&'static str> {
    if text.contains(DNS_SINK4) || text.contains(DNS_SINK6) {
        Some("happ")
    } else if text.contains("dnat to") || text.contains("DNAT --to-destination") {
        Some("legacy")
    } else {
        None
    }
}

pub(super) fn has_mark_return(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("meta mark 0x00000001 return")
        || lower.contains("mark 1 return")
        || lower.contains("--mark 0x1 -j return")
        || lower.contains("--mark 1 -j return")
}

pub(super) fn has_sink_return(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains(&format!("daddr {} return", DNS_SINK4))
        || lower.contains(&format!("daddr {} return", DNS_SINK6))
        || lower.contains(&format!("-d {}/32 -j return", DNS_SINK4))
        || lower.contains(&format!("-d {}/128 -j return", DNS_SINK6))
        || lower.contains(&format!("-d {} -j return", DNS_SINK4))
        || lower.contains(&format!("-d {} -j return", DNS_SINK6))
}

pub(super) fn sink_assigned(ip_output: &str) -> bool {
    ip_output.lines().any(|line| {
        line.contains(&format!("inet {}/", DNS_SINK4))
            || line.contains(&format!("inet6 {}/", DNS_SINK6))
    })
}

pub(super) fn fake_pool_overlap(ip_output: &str) -> Option<&'static str> {
    let pool4 = Ipv4Addr::new(198, 19, 0, 0);
    let pool6 = Ipv6Addr::from_str("fc00::").ok()?;

    for line in ip_output.lines() {
        let mut tokens = line.split_whitespace();
        while let Some(kind) = tokens.next() {
            if kind != "inet" && kind != "inet6" {
                continue;
            }
            let Some(cidr) = tokens.next() else {
                continue;
            };
            let Some((addr, _prefix)) = cidr.split_once('/') else {
                continue;
            };
            if kind == "inet" {
                if let Ok(ip) = Ipv4Addr::from_str(addr)
                    && ipv4_in_cidr(ip, pool4, 16)
                {
                    return Some(FAKE_POOL4);
                }
            } else if let Ok(ip) = Ipv6Addr::from_str(addr)
                && ipv6_in_cidr(ip, pool6, 18)
            {
                return Some(FAKE_POOL6);
            }
        }
    }
    None
}

fn ipv4_in_cidr(ip: Ipv4Addr, network: Ipv4Addr, prefix: u32) -> bool {
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    (u32::from(ip) & mask) == (u32::from(network) & mask)
}

fn ipv6_in_cidr(ip: Ipv6Addr, network: Ipv6Addr, prefix: u32) -> bool {
    let mask = if prefix == 0 {
        0
    } else {
        u128::MAX << (128 - prefix)
    };
    (u128::from(ip) & mask) == (u128::from(network) & mask)
}
