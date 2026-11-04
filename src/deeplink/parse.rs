use super::DeepLinkAction;

/// Percent-decodes a URL-encoded UTF-8 string.
pub fn url_decode(s: &str) -> String {
    let mut out_bytes = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex_str = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(decoded) = u8::from_str_radix(hex_str, 16) {
                out_bytes.push(decoded);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out_bytes.push(b' ');
        } else {
            out_bytes.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out_bytes).to_string()
}

/// Helper to derive a friendly group name from a subscription URL (e.g. "example.com").
fn derive_name_from_url(url: &str) -> String {
    let trimmed = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let host = trimmed
        .split(['/', '?', '#', ':'])
        .next()
        .unwrap_or("Subscription");
    if host.is_empty() {
        "Subscription".to_string()
    } else {
        host.to_string()
    }
}

/// Parses a `wit://` deep link into a `DeepLinkAction`.
///
/// Supported formats:
/// 1. Subscription query format:
///    `wit://sub?url=https%3A%2F%2F...&name=MySub`
///    `wit://add-sub?url=...&name=...`
///    `wit://install-sub?url=...&name=...`
///    `wit://subscription?url=...&name=...`
///
/// 2. Direct subscription URL:
///    `wit://sub/https://example.com/sub`
///    `wit://https://example.com/sub`
///    `wit://http://example.com/sub`
///
/// 3. Direct profile URIs:
///    `wit://vless://...`
///    `wit://vmess://...`
///    `wit://trojan://...`
///    `wit://ss://...`
///    `wit://hysteria2://...`
///    `wit://hy2://...`
///
/// 4. Profile query format:
///    `wit://import?uri=vless%3A%2F%2F...&group=MyGroup`
///    `wit://add?uri=...`
///
/// 5. Commands:
///    `wit://connect`, `wit://disconnect`, `wit://toggle`
pub fn parse_deeplink(raw: &str) -> Result<DeepLinkAction, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("Empty deep link".to_string());
    }

    // Direct Happ routing link
    let raw_lower = raw.to_lowercase();
    if raw_lower.starts_with("happ://routing/") || raw_lower.starts_with("happ:routing/") {
        return Ok(DeepLinkAction::ImportHappRouting {
            link: raw.to_string(),
            group: None,
        });
    }

    // Strip "wit://" or "wit:"
    let after_scheme = if let Some(stripped) = raw.strip_prefix("wit://") {
        stripped
    } else if let Some(stripped) = raw.strip_prefix("wit:") {
        stripped
    } else {
        return Err(format!(
            "Invalid scheme in deep link (expected wit:// or happ://routing/...): {raw}"
        ));
    };

    let without_leading_slashes = after_scheme.trim_start_matches('/');

    // Check embedded happ:// routing link in wit://
    let without_lower = without_leading_slashes.to_lowercase();
    if without_lower.starts_with("happ://routing/") || without_lower.starts_with("happ:routing/") {
        return Ok(DeepLinkAction::ImportHappRouting {
            link: without_leading_slashes.to_string(),
            group: None,
        });
    }

    // Check direct profile schemes
    for proto in &[
        "vless://",
        "vmess://",
        "trojan://",
        "ss://",
        "socks://",
        "socks5://",
        "hysteria2://",
        "hy2://",
    ] {
        if without_leading_slashes.starts_with(proto) {
            return Ok(DeepLinkAction::AddProfile {
                uri: without_leading_slashes.to_string(),
                group_name: None,
            });
        }
    }

    // Check direct http/https subscription URLs
    if without_leading_slashes.starts_with("https://")
        || without_leading_slashes.starts_with("http://")
    {
        let url = without_leading_slashes.to_string();
        let name = derive_name_from_url(&url);
        return Ok(DeepLinkAction::AddSubscription { name, url });
    }

    // Check prefix commands like "sub/https://..."
    if let Some(rest) = without_leading_slashes
        .strip_prefix("sub/")
        .or_else(|| without_leading_slashes.strip_prefix("subscription/"))
        .or_else(|| without_leading_slashes.strip_prefix("add-sub/"))
    {
        let url = if rest.starts_with("https://") || rest.starts_with("http://") {
            rest.to_string()
        } else if rest.starts_with("https%3A%2F%2F") || rest.starts_with("http%3A%2F%2F") {
            url_decode(rest)
        } else {
            format!("https://{rest}")
        };
        let name = derive_name_from_url(&url);
        return Ok(DeepLinkAction::AddSubscription { name, url });
    }

    // Split action and query string
    let (action_part, query_str) = match without_leading_slashes.split_once('?') {
        Some((a, q)) => (a.trim_end_matches('/'), q),
        None => (without_leading_slashes.trim_end_matches('/'), ""),
    };

    // Parse query params
    let mut query_params: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for pair in query_str.split('&').filter(|s| !s.is_empty()) {
        if let Some((k, v)) = pair.split_once('=') {
            query_params.insert(k.to_lowercase(), url_decode(v));
        }
    }

    match action_part {
        "connect" => Ok(DeepLinkAction::Connect),
        "disconnect" => Ok(DeepLinkAction::Disconnect),
        "toggle" => Ok(DeepLinkAction::Toggle),

        "routing" | "happ" | "happ-routing" | "import-routing" => {
            let link = query_params
                .get("link")
                .or_else(|| query_params.get("url"))
                .or_else(|| query_params.get("uri"))
                .cloned()
                .ok_or_else(|| "Missing 'link' parameter in routing deep link".to_string())?;
            let group = query_params.get("group").cloned();
            Ok(DeepLinkAction::ImportHappRouting { link, group })
        }

        "sub" | "subscription" | "add-sub" | "install-sub" | "import-sub" => {
            let url = query_params
                .get("url")
                .or_else(|| query_params.get("link"))
                .or_else(|| query_params.get("sub"))
                .cloned()
                .ok_or_else(|| "Missing 'url' parameter in subscription deep link".to_string())?;

            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Err(format!(
                    "Subscription URL must use http:// or https://: {url}"
                ));
            }

            let name = query_params
                .get("name")
                .or_else(|| query_params.get("title"))
                .cloned()
                .unwrap_or_else(|| derive_name_from_url(&url));

            Ok(DeepLinkAction::AddSubscription { name, url })
        }

        "import" | "add" | "profile" | "node" => {
            let uri = query_params
                .get("uri")
                .or_else(|| query_params.get("link"))
                .or_else(|| query_params.get("url"))
                .cloned()
                .ok_or_else(|| "Missing 'uri' parameter in profile deep link".to_string())?;

            let group_name = query_params.get("group").cloned();
            Ok(DeepLinkAction::AddProfile { uri, group_name })
        }

        _ => {
            // Fallback: If query contains link, check if it's happ routing
            if let Some(link) = query_params.get("link") {
                let link_lower = link.to_lowercase();
                if link_lower.starts_with("happ://routing/")
                    || link_lower.starts_with("happ:routing/")
                {
                    let group = query_params.get("group").cloned();
                    return Ok(DeepLinkAction::ImportHappRouting {
                        link: link.clone(),
                        group,
                    });
                }
            }

            // Fallback: If query contains url, treat as subscription
            if let Some(url) = query_params.get("url").or_else(|| query_params.get("sub")) {
                let name = query_params
                    .get("name")
                    .cloned()
                    .unwrap_or_else(|| derive_name_from_url(url));
                return Ok(DeepLinkAction::AddSubscription {
                    name,
                    url: url.clone(),
                });
            }

            // Fallback: If query contains uri, treat as profile
            if let Some(uri) = query_params.get("uri") {
                let group_name = query_params.get("group").cloned();
                return Ok(DeepLinkAction::AddProfile {
                    uri: uri.clone(),
                    group_name,
                });
            }

            Err(format!("Unknown deep link action: '{action_part}'"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_subscription_query() {
        let link = "wit://sub?url=https%3A%2F%2Fexample.com%2Fsub%2Ftoken&name=My%20VPN";
        let action = parse_deeplink(link).expect("must parse");
        assert_eq!(
            action,
            DeepLinkAction::AddSubscription {
                name: "My VPN".to_string(),
                url: "https://example.com/sub/token".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_install_sub() {
        let link = "wit://install-sub?url=https://sub.provider.org/clash.yaml";
        let action = parse_deeplink(link).expect("must parse");
        assert_eq!(
            action,
            DeepLinkAction::AddSubscription {
                name: "sub.provider.org".to_string(),
                url: "https://sub.provider.org/clash.yaml".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_direct_https_url() {
        let link = "wit://https://example.com/api/v1/client/subscribe?token=123";
        let action = parse_deeplink(link).expect("must parse");
        assert_eq!(
            action,
            DeepLinkAction::AddSubscription {
                name: "example.com".to_string(),
                url: "https://example.com/api/v1/client/subscribe?token=123".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_sub_path_url() {
        let link = "wit://sub/https://server.net/sub";
        let action = parse_deeplink(link).expect("must parse");
        assert_eq!(
            action,
            DeepLinkAction::AddSubscription {
                name: "server.net".to_string(),
                url: "https://server.net/sub".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_direct_vless_profile() {
        let link =
            "wit://vless://11111111-2222-3333-4444-555555555555@example.com:443?type=tcp#Node1";
        let action = parse_deeplink(link).expect("must parse");
        assert_eq!(
            action,
            DeepLinkAction::AddProfile {
                uri: "vless://11111111-2222-3333-4444-555555555555@example.com:443?type=tcp#Node1"
                    .to_string(),
                group_name: None,
            }
        );
    }

    #[test]
    fn test_parse_import_profile_with_group() {
        let link = "wit://import?uri=ss%3A%2F%2FY2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzQDEuMi4zLjQ6NDQz#Node&group=Special";
        let action = parse_deeplink(link).expect("must parse");
        assert_eq!(
            action,
            DeepLinkAction::AddProfile {
                uri: "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzQDEuMi4zLjQ6NDQz#Node".to_string(),
                group_name: Some("Special".to_string()),
            }
        );
    }

    #[test]
    fn test_parse_commands() {
        assert_eq!(
            parse_deeplink("wit://connect").unwrap(),
            DeepLinkAction::Connect
        );
        assert_eq!(
            parse_deeplink("wit://disconnect").unwrap(),
            DeepLinkAction::Disconnect
        );
        assert_eq!(
            parse_deeplink("wit://toggle").unwrap(),
            DeepLinkAction::Toggle
        );
    }

    #[test]
    fn test_parse_invalid() {
        assert!(parse_deeplink("").is_err());
        assert!(parse_deeplink("http://example.com").is_err());
        assert!(parse_deeplink("wit://unknown-action").is_err());
    }

    #[test]
    fn test_parse_happ_routing_direct() {
        let link = "happ://routing/add/eyJNYW1lIjoiVGVzdCJ9";
        let action = parse_deeplink(link).expect("must parse direct happ link");
        assert_eq!(
            action,
            DeepLinkAction::ImportHappRouting {
                link: link.to_string(),
                group: None,
            }
        );

        let off_link = "happ://routing/off";
        let off_action = parse_deeplink(off_link).expect("must parse off link");
        assert_eq!(
            off_action,
            DeepLinkAction::ImportHappRouting {
                link: off_link.to_string(),
                group: None,
            }
        );
    }

    #[test]
    fn test_parse_happ_routing_with_wit_scheme() {
        let link = "wit://happ://routing/onadd/eyJNYW1lIjoiT25BZGQifQ==";
        let action = parse_deeplink(link).expect("must parse embedded happ link");
        assert_eq!(
            action,
            DeepLinkAction::ImportHappRouting {
                link: "happ://routing/onadd/eyJNYW1lIjoiT25BZGQifQ==".to_string(),
                group: None,
            }
        );
    }

    #[test]
    fn test_parse_happ_routing_query() {
        let link = "wit://routing?link=happ%3A%2F%2Frouting%2Fadd%2Fabc&group=Work";
        let action = parse_deeplink(link).expect("must parse routing query");
        assert_eq!(
            action,
            DeepLinkAction::ImportHappRouting {
                link: "happ://routing/add/abc".to_string(),
                group: Some("Work".to_string()),
            }
        );
    }
}
