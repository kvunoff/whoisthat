use crate::core_client::protocol::RoutingRule;

use super::OUTBOUND_LABELS;

pub(super) fn rule_type_label(rule: &RoutingRule) -> &'static str {
    if rule.domain.starts_with("geosite:") {
        "geosite"
    } else if !rule.domain.is_empty() {
        "domain"
    } else if rule.ip.starts_with("geoip:") {
        "geoip"
    } else if !rule.ip.is_empty() {
        "ip"
    } else if !rule.protocol.is_empty() {
        "protocol"
    } else if !rule.port.is_empty() {
        "port"
    } else {
        ""
    }
}

pub(super) fn rule_value(rule: &RoutingRule) -> &str {
    if rule.domain.starts_with("geosite:") {
        &rule.domain[8..]
    } else if !rule.domain.is_empty() {
        &rule.domain
    } else if rule.ip.starts_with("geoip:") {
        &rule.ip[6..]
    } else if !rule.ip.is_empty() {
        &rule.ip
    } else if !rule.protocol.is_empty() {
        &rule.protocol
    } else if !rule.port.is_empty() {
        &rule.port
    } else {
        ""
    }
}

pub(super) fn rule_outbound_label(rule: &RoutingRule) -> &str {
    if rule.outbound_tag.is_empty() {
        "proxy"
    } else {
        &rule.outbound_tag
    }
}

pub fn rule_to_form(rule: &RoutingRule) -> (usize, String, usize) {
    let (match_type, value) = if rule.domain.starts_with("geosite:") {
        (5, rule.domain.clone())
    } else if rule.ip.starts_with("geoip:") {
        (4, rule.ip.clone())
    } else if !rule.domain.is_empty() {
        (0, rule.domain.clone())
    } else if !rule.ip.is_empty() {
        (1, rule.ip.clone())
    } else if !rule.protocol.is_empty() {
        (2, rule.protocol.clone())
    } else {
        (3, rule.port.clone())
    };
    let outbound = match rule.outbound_tag.as_str() {
        "direct" => 1,
        "block" => 2,
        _ => 0,
    };
    (match_type, value, outbound)
}

pub fn form_to_rule(match_type: usize, value: &str, outbound: usize) -> RoutingRule {
    let mut rule = RoutingRule {
        r#type: "field".into(),
        outbound_tag: OUTBOUND_LABELS[outbound].into(),
        enabled: true,
        ..Default::default()
    };
    match match_type {
        0 => rule.domain = value.to_string(),
        1 => rule.ip = value.to_string(),
        2 => rule.protocol = value.to_string(),
        3 => rule.port = value.to_string(),
        4 => rule.ip = value.to_string(),
        5 => rule.domain = value.to_string(),
        _ => {}
    }
    rule
}
