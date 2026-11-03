use crate::doctor::types::{CheckCategory, CheckItem, DoctorReport};

use super::dns::{
    FAKE_POOL4, FAKE_POOL6, dns_mode_from_rules, fake_pool_overlap, has_default_route,
    has_mark_return, has_mark1_policy_rule, has_sink_return, sink_assigned,
};
use super::logs::recent_errors;
use super::util::{human_bytes, truncate_line};

#[test]
fn test_doctor_report_counts() {
    let cat1 = CheckCategory::new(
        "Test Cat",
        vec![
            CheckItem::pass("item1", "ok"),
            CheckItem::warn("item2", "warning", Some("fix me".into())),
            CheckItem::fail("item3", "error", Some("critical fix".into())),
            CheckItem::info("item4", "info notice"),
        ],
    );

    let report = DoctorReport::new(vec![cat1]);
    assert_eq!(report.summary.passed, 1);
    assert_eq!(report.summary.warnings, 1);
    assert_eq!(report.summary.failed, 1);
    assert_eq!(report.summary.info, 1);
    assert!(!report.summary.is_healthy);
}

#[test]
fn test_doctor_report_json_serialization() {
    let cat = CheckCategory::new("Sample", vec![CheckItem::pass("test", "working properly")]);
    let report = DoctorReport::new(vec![cat]);
    let json = serde_json::to_string(&report).expect("must serialize");
    assert!(json.contains("\"passed\":1"));
    assert!(json.contains("\"is_healthy\":true"));
    assert!(json.contains("\"test\""));
}

#[test]
fn recent_errors_keeps_the_last_five_error_lines() {
    let dir = std::env::temp_dir().join(format!(
        "whoisthat-doctor-logs-{}-{}",
        std::process::id(),
        "recent"
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("core.log");

    let mut text = String::new();
    for i in 0..7 {
        text.push_str(&format!("2026/10/31 10:00:0{i} [ERRO] boom {i}\n"));
        text.push_str("2026/10/31 10:00:00 [INFO] ok\n");
    }
    text.push_str("2026/10/31 10:00:09 [FATA] fatal\n");
    std::fs::write(&path, text).unwrap();

    let errors = recent_errors(&path);
    assert_eq!(errors.len(), 5);
    assert!(errors[4].contains("fatal"));
    assert!(!errors[0].contains("boom 2"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn long_error_lines_are_truncated() {
    let line = "x".repeat(200);
    let out = truncate_line(&line, 20);
    assert_eq!(out.chars().count(), 20);
    assert!(out.ends_with("..."));
}

#[test]
fn formats_log_sizes() {
    assert_eq!(human_bytes(512), "512 B");
    assert_eq!(human_bytes(2048), "2.0 KiB");
    assert_eq!(human_bytes(2 * 1024 * 1024), "2.0 MiB");
}

#[test]
fn classifies_dns_hijack_mode() {
    assert_eq!(
        dns_mode_from_rules("udp dport 53 dnat to 198.18.0.53:53"),
        Some("happ")
    );
    assert_eq!(
        dns_mode_from_rules("udp dport 53 dnat to 1.1.1.1:53"),
        Some("legacy")
    );
    assert_eq!(dns_mode_from_rules("table ip whoisthat {}"), None);
}

#[test]
fn detects_sink_assignment() {
    assert!(sink_assigned(
        "2: wlan0    inet 198.18.0.53/32 scope global wlan0"
    ));
    assert!(sink_assigned("2: wlan0    inet6 fd00::53/128 scope global"));
    assert!(!sink_assigned(
        "2: wlan0    inet 192.168.3.3/24 scope global wlan0"
    ));
    assert!(!sink_assigned(
        "2: wlan0    inet 198.18.0.1/32 scope global wlan0"
    ));
}

#[test]
fn detects_fake_pool_overlap() {
    assert_eq!(
        fake_pool_overlap("1: tun inet 198.18.0.1/32 scope global tun"),
        None
    );
    assert_eq!(
        fake_pool_overlap("1: tun inet 198.19.7.7/32 scope global tun"),
        Some(FAKE_POOL4)
    );
    assert_eq!(
        fake_pool_overlap("1: tun inet6 fc00::1/128 scope global tun"),
        Some(FAKE_POOL6)
    );
    assert_eq!(
        fake_pool_overlap("1: tun inet6 fd00::1/128 scope global tun"),
        None
    );
}

#[test]
fn detects_split_policy_route() {
    assert!(has_mark1_policy_rule(
        "0:\tfrom all lookup local\n32765:\tfrom all fwmark 0x1 lookup 100"
    ));
    assert!(!has_mark1_policy_rule(
        "0:\tfrom all lookup local\n32766:\tfrom all lookup main"
    ));
    assert!(!has_mark1_policy_rule(
        "32765:\tfrom all fwmark 0x2 lookup 200"
    ));

    assert!(has_default_route(
        "default via 192.168.3.1 dev wlan0 \n192.168.3.1 dev wlan0 scope link"
    ));
    assert!(!has_default_route("192.168.3.1 dev wlan0 scope link"));
}

#[test]
fn detects_happ_safety_returns() {
    let nft = "meta mark 0x00000001 return\nip daddr 198.18.0.53 return";
    assert!(has_mark_return(nft));
    assert!(has_sink_return(nft));

    let iptables = "-A WHOISTHAT_DNS_OUT -m mark --mark 0x1 -j RETURN";
    assert!(has_mark_return(iptables));
    assert!(!has_sink_return(iptables));

    let iptables_sink = "-A WHOISTHAT_DNS_OUT -d 198.18.0.53/32 -j RETURN";
    assert!(has_sink_return(iptables_sink));
}
