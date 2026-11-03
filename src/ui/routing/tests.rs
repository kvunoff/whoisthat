use super::popups::truncate_dns_label;
use super::*;
use crate::core_client::protocol::*;

fn rule(domain: &str, ip: &str, protocol: &str, port: &str, outbound_tag: &str) -> RoutingRule {
    RoutingRule {
        r#type: "field".into(),
        domain: domain.into(),
        ip: ip.into(),
        protocol: protocol.into(),
        port: port.into(),
        network: "".into(),
        outbound_tag: outbound_tag.into(),
        enabled: true,
    }
}

// --- form_to_rule ---

#[test]
fn form_to_rule_domain() {
    let r = form_to_rule(0, "example.com", 0);
    assert_eq!(r.domain, "example.com");
    assert!(r.ip.is_empty());
    assert_eq!(r.outbound_tag, "proxy");
    assert_eq!(r.r#type, "field");
    assert!(r.enabled);
}

#[test]
fn form_to_rule_ip() {
    let r = form_to_rule(1, "10.0.0.0/8", 1);
    assert_eq!(r.ip, "10.0.0.0/8");
    assert!(r.domain.is_empty());
    assert_eq!(r.outbound_tag, "direct");
}

#[test]
fn form_to_rule_protocol() {
    let r = form_to_rule(2, "bittorrent", 2);
    assert_eq!(r.protocol, "bittorrent");
    assert_eq!(r.outbound_tag, "block");
}

#[test]
fn form_to_rule_port() {
    let r = form_to_rule(3, "8000-9000", 0);
    assert_eq!(r.port, "8000-9000");
}

#[test]
fn form_to_rule_geoip() {
    let r = form_to_rule(4, "geoip:ru", 1);
    assert_eq!(r.ip, "geoip:ru");
    assert!(r.domain.is_empty());
}

#[test]
fn form_to_rule_geosite() {
    let r = form_to_rule(5, "geosite:youtube", 2);
    assert_eq!(r.domain, "geosite:youtube");
    assert!(r.ip.is_empty());
}

// --- rule_to_form ---

#[test]
fn rule_to_form_domain() {
    let (mt, val, ob) = rule_to_form(&rule("example.com", "", "", "", ""));
    assert_eq!(mt, 0);
    assert_eq!(val, "example.com");
    assert_eq!(ob, 0); // proxy
}

#[test]
fn rule_to_form_ip() {
    let (mt, val, ob) = rule_to_form(&rule("", "192.168.1.0/24", "", "", "direct"));
    assert_eq!(mt, 1);
    assert_eq!(val, "192.168.1.0/24");
    assert_eq!(ob, 1);
}

#[test]
fn rule_to_form_protocol() {
    let (mt, val, ob) = rule_to_form(&rule("", "", "http", "", "block"));
    assert_eq!(mt, 2);
    assert_eq!(val, "http");
    assert_eq!(ob, 2);
}

#[test]
fn rule_to_form_port() {
    let (mt, val, _ob) = rule_to_form(&rule("", "", "", "443", ""));
    assert_eq!(mt, 3);
    assert_eq!(val, "443");
}

#[test]
fn rule_to_form_geoip() {
    let (mt, val, _ob) = rule_to_form(&rule("", "geoip:private", "", "", ""));
    assert_eq!(mt, 4);
    assert_eq!(val, "geoip:private");
}

#[test]
fn rule_to_form_geosite() {
    let (mt, val, _ob) = rule_to_form(&rule("geosite:netflix", "", "", "", ""));
    assert_eq!(mt, 5);
    assert_eq!(val, "geosite:netflix");
}

// --- round-trip ---

#[test]
fn round_trip_domain_proxy() {
    let original = rule("example.com", "", "", "", "proxy");
    let (mt, val, ob) = rule_to_form(&original);
    let reconstructed = form_to_rule(mt, &val, ob);
    assert_eq!(reconstructed.domain, original.domain);
    assert_eq!(reconstructed.outbound_tag, original.outbound_tag);
}

#[test]
fn round_trip_geosite_block() {
    let original = rule("geosite:category-ads", "", "", "", "block");
    let (mt, val, ob) = rule_to_form(&original);
    let reconstructed = form_to_rule(mt, &val, ob);
    assert_eq!(reconstructed.domain, original.domain);
    assert_eq!(reconstructed.outbound_tag, "block");
}

// --- presets tests ---

#[test]
fn test_presets_definitions() {
    assert_eq!(ROUTING_PRESETS.len(), 11);
    for p in ROUTING_PRESETS {
        assert!(!p.id.is_empty());
        assert!(!p.title.is_empty());
        assert!(!p.desc.is_empty());
        assert!(!p.rules.is_empty());
        for r in p.rules {
            assert!(!r.value.is_empty());
            assert!(matches!(r.outbound, "direct" | "proxy" | "block"));
            assert!(matches!(
                r.match_type,
                "geosite" | "geoip" | "protocol" | "domain" | "ip" | "port"
            ));
            let routing_rule = r.to_routing_rule();
            assert_eq!(routing_rule.outbound_tag, r.outbound);
            assert!(routing_rule.enabled);
        }
    }
}

#[test]
fn test_preset_is_applied_and_toggle() {
    let preset = &ROUTING_PRESETS[0]; // bypass_ru
    let mut cfg = RoutingConfig::default();

    assert!(!preset.is_applied(&cfg));

    // Toggle on
    let added = toggle_preset(&mut cfg, preset);
    assert!(added);
    assert!(preset.is_applied(&cfg));
    assert_eq!(cfg.rules.len(), preset.rules.len());

    // Toggle off
    let added_again = toggle_preset(&mut cfg, preset);
    assert!(!added_again);
    assert!(!preset.is_applied(&cfg));
    assert!(cfg.rules.is_empty());
}

#[test]
fn test_apply_preset_idempotent() {
    let preset = &ROUTING_PRESETS[0]; // bypass_ru
    let mut cfg = RoutingConfig::default();

    apply_preset(&mut cfg, preset);
    let len_first = cfg.rules.len();

    apply_preset(&mut cfg, preset);
    assert_eq!(cfg.rules.len(), len_first);
    assert!(preset.is_applied(&cfg));
}

#[test]
fn test_apply_preset_re_enables_disabled_rule() {
    let preset = &ROUTING_PRESETS[1]; // block_ads
    let mut cfg = RoutingConfig::default();

    apply_preset(&mut cfg, preset);
    assert!(preset.is_applied(&cfg));

    // Disable rule manually
    cfg.rules[0].enabled = false;
    assert!(!preset.is_applied(&cfg));

    // Re-apply preset -> should enable existing rule without creating duplicate
    apply_preset(&mut cfg, preset);
    assert_eq!(cfg.rules.len(), 1);
    assert!(cfg.rules[0].enabled);
    assert!(preset.is_applied(&cfg));
}

#[test]
fn test_remove_preset_leaves_other_rules_intact() {
    let p_ru = &ROUTING_PRESETS[0]; // bypass_ru
    let p_ads = &ROUTING_PRESETS[1]; // block_ads
    let mut cfg = RoutingConfig::default();

    apply_preset(&mut cfg, p_ru);
    apply_preset(&mut cfg, p_ads);
    assert!(p_ru.is_applied(&cfg));
    assert!(p_ads.is_applied(&cfg));

    remove_preset(&mut cfg, p_ru);
    assert!(!p_ru.is_applied(&cfg));
    assert!(p_ads.is_applied(&cfg));
    assert_eq!(cfg.rules.len(), p_ads.rules.len());
}

#[test]
fn test_render_routing_tab_with_group_shadowed() {
    let backend = ratatui::backend::TestBackend::new(100, 30);
    let mut term = ratatui::Terminal::new(backend).unwrap();
    let cfg = RoutingConfig::default();
    let group = GroupWithProfiles {
        group: Group {
            id: 1,
            name: "Test Group".into(),
            routing_mode: "auto".into(),
            active_routing_profile_id: "happ-1".into(),
            ..Default::default()
        },
        profiles: Vec::new(),
        routing_profiles: vec![RoutingProfileInfo {
            id: "happ-1".into(),
            name: "Happ Profile 1".into(),
            state: "ready".into(),
            active: true,
            rules_count: 5,
            domain_strategy: "AsIs".into(),
            ..Default::default()
        }],
    };

    term.draw(|f| {
        render_routing_tab(f, f.area(), &cfg, 0, true, Some(&group), 1, 2, "global");
    })
    .unwrap();

    let text = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol().to_string())
        .collect::<Vec<_>>()
        .join("");
    assert!(text.contains("Group: < [1/1] Test Group >"));
    assert!(text.contains("Mode: [auto]"));
    assert!(text.contains("Happ Profiles: [H] (1 profiles)"));
    assert!(text.contains("Effective Source: Auto (Happ: Happ Profile 1)"));
    assert!(text.contains("Notice: Manual rules below are shadowed"));
}

#[test]
fn test_render_routing_tab_manual_mode_not_shadowed() {
    let backend = ratatui::backend::TestBackend::new(100, 30);
    let mut term = ratatui::Terminal::new(backend).unwrap();
    let cfg = RoutingConfig::default();
    let group = GroupWithProfiles {
        group: Group {
            id: 1,
            name: "Manual Group".into(),
            routing_mode: "manual".into(),
            ..Default::default()
        },
        profiles: Vec::new(),
        routing_profiles: Vec::new(),
    };

    term.draw(|f| {
        render_routing_tab(f, f.area(), &cfg, 0, true, Some(&group), 1, 2, "custom");
    })
    .unwrap();

    let text = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol().to_string())
        .collect::<Vec<_>>()
        .join("");
    assert!(text.contains("Group: < [1/1] Manual Group >"));
    assert!(text.contains("Mode: [manual]"));
    assert!(text.contains("Effective Source: Manual (group rules)"));
    assert!(!text.contains("Notice: Manual rules below are shadowed"));
}

#[test]
fn test_render_routing_tab_global_page() {
    let backend = ratatui::backend::TestBackend::new(100, 30);
    let mut term = ratatui::Terminal::new(backend).unwrap();
    let cfg = RoutingConfig::default();

    term.draw(|f| {
        render_routing_tab(f, f.area(), &cfg, 0, true, None, 0, 3, "global");
    })
    .unwrap();

    let text = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol().to_string())
        .collect::<Vec<_>>()
        .join("");
    assert!(text.contains("Group: < Global >"));
    assert!(text.contains("Scope: shared base for 2 group(s)"));
    assert!(text.contains("Manual: Global rules"));
    assert!(text.contains("Effective Source: Manual (Global rules)"));
}

#[test]
fn test_render_routing_tab_inherited_notice() {
    let backend = ratatui::backend::TestBackend::new(100, 30);
    let mut term = ratatui::Terminal::new(backend).unwrap();
    let cfg = RoutingConfig::default();
    let group = GroupWithProfiles {
        group: Group {
            id: 1,
            name: "Inheriting Group".into(),
            routing_mode: "manual".into(),
            ..Default::default()
        },
        profiles: Vec::new(),
        routing_profiles: Vec::new(),
    };

    term.draw(|f| {
        render_routing_tab(f, f.area(), &cfg, 0, true, Some(&group), 1, 2, "global");
    })
    .unwrap();

    let text = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol().to_string())
        .collect::<Vec<_>>()
        .join("");
    assert!(text.contains("Global rules (inherited)"));
    assert!(text.contains("Press g or edit a rule to create group-specific rules"));
    assert!(text.contains("Effective Source: Manual (Global rules)"));
}

#[test]
fn test_render_happ_profiles_popup() {
    let backend = ratatui::backend::TestBackend::new(100, 30);
    let mut term = ratatui::Terminal::new(backend).unwrap();
    let cfg = RoutingConfig::default();
    let group = GroupWithProfiles {
        group: Group {
            id: 1,
            name: "Happ Group".into(),
            routing_mode: "subscription".into(),
            active_routing_profile_id: "happ-1".into(),
            ..Default::default()
        },
        profiles: Vec::new(),
        routing_profiles: vec![RoutingProfileInfo {
            id: "happ-1".into(),
            name: "My Happ Rules".into(),
            state: "ready".into(),
            active: true,
            rules_count: 42,
            domain_strategy: "IPIfNonMatch".into(),
            remote_dns: "DoH cloudflare-dns.com".into(),
            domestic_dns: "DoU 8.8.8.8".into(),
            fake_dns_enabled: true,
            ..Default::default()
        }],
    };

    term.draw(|f| {
        render_routing_popup(
            f,
            &RoutingPopup::HappProfiles { cursor: 0 },
            f.area(),
            &cfg,
            Some(&group),
        );
    })
    .unwrap();

    let text = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol().to_string())
        .collect::<Vec<_>>()
        .join("");
    assert!(text.contains("Happ Routing Profiles"));
    assert!(text.contains("[● Active]"));
    assert!(text.contains("[ready]"));
    assert!(text.contains("My Happ Rules"));
    assert!(text.contains("rules: 42"));
    assert!(text.contains("strategy: IPIfNonMatch"));
    assert!(text.contains("DNS: DoH cloudflare-dns.com │ DoU 8.8.8.8 │ FakeDNS: on"));
}

#[test]
fn test_truncate_dns_label() {
    assert_eq!(truncate_dns_label("DoH dns.google"), "DoH dns.google");

    let long = "DoH a-very-long-doh-provider-hostname.example";
    let truncated = truncate_dns_label(long);
    assert_eq!(truncated.chars().count(), 28);
    assert!(truncated.ends_with('…'));
    assert!(truncated.starts_with("DoH a-very-long"));
}

#[test]
fn test_render_happ_profiles_popup_with_error() {
    let backend = ratatui::backend::TestBackend::new(100, 30);
    let mut term = ratatui::Terminal::new(backend).unwrap();
    let cfg = RoutingConfig::default();
    let group = GroupWithProfiles {
        group: Group {
            id: 1,
            name: "Happ Group".into(),
            routing_mode: "subscription".into(),
            active_routing_profile_id: "happ-1".into(),
            ..Default::default()
        },
        profiles: Vec::new(),
        routing_profiles: vec![RoutingProfileInfo {
            id: "happ-1".into(),
            name: "Broken Happ Rules".into(),
            state: "error".into(),
            active: true,
            rules_count: 10,
            last_error: "downloading geosite failed: connection timed out to cdn.jsdelivr.net"
                .into(),
            ..Default::default()
        }],
    };

    term.draw(|f| {
        render_routing_popup(
            f,
            &RoutingPopup::HappProfiles { cursor: 0 },
            f.area(),
            &cfg,
            Some(&group),
        );
    })
    .unwrap();

    let text = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol().to_string())
        .collect::<Vec<_>>()
        .join("");
    assert!(text.contains("Error Details"));
    assert!(text.contains("downloading geosite failed"));
    assert!(text.contains("connection timed out"));
}
