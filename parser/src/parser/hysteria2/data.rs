use crate::config_models::{RawData, UserAddress};
use crate::utils::{get_parameter_value, url_decode};

pub fn get_data(uri: &str) -> Result<RawData, String> {
    let (_prefix, data) = if uri.starts_with("hysteria2://") {
        uri.split_once("hysteria2://")
    } else if uri.starts_with("hy2://") {
        uri.split_once("hy2://")
    } else {
        return Err("Invalid hysteria2 URI: missing 'hysteria2://' or 'hy2://'".to_string());
    }
    .ok_or_else(|| "Invalid hysteria2 URI".to_string())?;

    // Split fragment (#name) if present
    let (rest, name) = match data.split_once('#') {
        Some((r, n)) => (r, n),
        None => (data, ""),
    };

    // Split query string (?query) if present
    let (raw_address_part, raw_query) = match rest.split_once('?') {
        Some((a, q)) => (a, q),
        None => (rest, ""),
    };

    let parsed_address = parse_hysteria2_address(raw_address_part)?;
    let query: Vec<(&str, &str)> = querystring::querify(raw_query);

    let mut obfs = url_decode(
        get_parameter_value(&query, "obfs")
            .or_else(|| get_parameter_value(&query, "obfs-type"))
            .or_else(|| get_parameter_value(&query, "obfs_type")),
    );
    let mut obfs_password = url_decode(
        get_parameter_value(&query, "obfs-password")
            .or_else(|| get_parameter_value(&query, "obfs_password"))
            .or_else(|| get_parameter_value(&query, "obfs-param"))
            .or_else(|| get_parameter_value(&query, "obfs_param")),
    );

    // If obfs or password is not provided via direct query params, check `fm` (Finalmask JSON format)
    if obfs.is_none() || obfs_password.is_none() {
        if let Some(fm_raw) = get_parameter_value(&query, "fm") {
            if let Some(decoded_fm) = url_decode(Some(fm_raw.to_string())) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&decoded_fm) {
                    if let Some(udp_arr) = val
                        .get("finalmask")
                        .and_then(|f| f.get("udp"))
                        .and_then(|u| u.as_array())
                    {
                        for item in udp_arr {
                            if let Some(t) = item.get("type").and_then(|t| t.as_str()) {
                                if obfs.is_none() {
                                    obfs = Some(t.to_string());
                                }
                                if let Some(pw) = item
                                    .get("settings")
                                    .and_then(|s| s.get("password"))
                                    .and_then(|p| p.as_str())
                                {
                                    if obfs_password.is_none() {
                                        obfs_password = Some(pw.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(RawData {
        remarks: url_decode(Some(String::from(name))).unwrap_or(String::from("")),
        uuid: Some(parsed_address.uuid),
        port: Some(parsed_address.port),
        address: Some(parsed_address.address),
        alpn: url_decode(get_parameter_value(&query, "alpn")),
        path: url_decode(get_parameter_value(&query, "path")),
        authority: url_decode(get_parameter_value(&query, "authority")),
        pbk: url_decode(get_parameter_value(&query, "pbk")),
        security: get_parameter_value(&query, "security"),
        sid: url_decode(get_parameter_value(&query, "sid")),
        flow: get_parameter_value(&query, "flow"),
        sni: get_parameter_value(&query, "sni"),
        fp: url_decode(get_parameter_value(&query, "fp")),
        r#type: get_parameter_value(&query, "type"),
        encryption: get_parameter_value(&query, "encryption"),
        header_type: get_parameter_value(&query, "headerType"),
        host: url_decode(get_parameter_value(&query, "host")),
        seed: url_decode(get_parameter_value(&query, "seed")),
        quic_security: get_parameter_value(&query, "quicSecurity"),
        key: get_parameter_value(&query, "key"),
        mode: url_decode(get_parameter_value(&query, "mode")),
        service_name: url_decode(get_parameter_value(&query, "serviceName")),
        vnext_security: None,
        slpn: get_parameter_value(&query, "slpn"),
        spx: url_decode(get_parameter_value(&query, "spx")),
        extra: url_decode(get_parameter_value(&query, "extra")),
        allowInsecure: get_parameter_value(&query, "allowInsecure")
            .or_else(|| get_parameter_value(&query, "insecure")),
        server_method: None,
        username: None,
        obfs,
        obfs_password,
        up: url_decode(get_parameter_value(&query, "up")),
        down: url_decode(get_parameter_value(&query, "down")),
        // Some hysteria2 URIs advertise the multi-port range via `mport`,
        // others via `ports`. Accept either.
        ports: url_decode(
            get_parameter_value(&query, "ports").or_else(|| get_parameter_value(&query, "mport")),
        ),
    })
}

fn parse_hysteria2_address(raw_data: &str) -> Result<UserAddress, String> {
    let (uuid_raw, raw_address) = raw_data
        .split_once('@')
        .ok_or_else(|| "Wrong hysteria2 format, no `@` found in the address".to_string())?;
    let uuid = url_decode(Some(String::from(uuid_raw)))
        .ok_or_else(|| "Failed to URL-decode hysteria2 password".to_string())?;
    let address_wo_slash = raw_address.strip_suffix('/').unwrap_or(raw_address);

    if address_wo_slash.is_empty() {
        return Err("Missing host in hysteria2 address".to_string());
    }

    let (host, port) = if address_wo_slash.starts_with('[') {
        if let Some(bracket_end) = address_wo_slash.find(']') {
            let host_part = &address_wo_slash[1..bracket_end];
            let after = &address_wo_slash[bracket_end + 1..];
            let port = if let Some(port_str) = after.strip_prefix(':') {
                port_str
                    .parse::<u16>()
                    .map_err(|e| format!("Invalid hysteria2 port: {}", e))?
            } else {
                443
            };
            (host_part.to_string(), port)
        } else {
            return Err("Invalid IPv6 address: missing closing bracket".to_string());
        }
    } else if let Some((h, p)) = address_wo_slash.rsplit_once(':') {
        if let Ok(port) = p.parse::<u16>() {
            (h.to_string(), port)
        } else {
            (address_wo_slash.to_string(), 443)
        }
    } else {
        (address_wo_slash.to_string(), 443)
    };

    if host.is_empty() {
        return Err("Missing host in hysteria2 address".to_string());
    }

    Ok(UserAddress {
        uuid,
        address: host,
        port,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_hysteria2_uri() {
        let result = get_data("hysteria2://mypassword@example.com:443?test=1");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.uuid, Some("mypassword".to_string()));
        assert_eq!(data.address, Some("example.com".to_string()));
        assert_eq!(data.port, Some(443));
    }

    #[test]
    fn parses_hy2_prefix() {
        let result = get_data("hy2://mypassword@example.com:443?test=1");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.uuid, Some("mypassword".to_string()));
    }

    #[test]
    fn parses_with_remarks() {
        let result = get_data("hysteria2://pw@example.com:443?test=1#MyHysteria");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.remarks, "MyHysteria");
    }

    #[test]
    fn parses_with_tls_and_sni() {
        let result = get_data(
            "hysteria2://pw@example.com:443?security=tls&sni=sni.example.com&allowInsecure=true",
        );
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.security, Some("tls".to_string()));
        assert_eq!(data.sni, Some("sni.example.com".to_string()));
        assert_eq!(data.allowInsecure, Some("true".to_string()));
    }

    #[test]
    fn parses_with_insecure_param() {
        let result = get_data("hysteria2://pw@example.com:443?insecure=1");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.allowInsecure, Some("1".to_string()));
    }

    #[test]
    fn parses_with_obfs() {
        let result = get_data(
            "hysteria2://pw@example.com:443?obfs=salamander&obfs-password=obfs-secret&sni=example.com",
        );
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.obfs, Some("salamander".to_string()));
        assert_eq!(data.obfs_password, Some("obfs-secret".to_string()));
    }

    #[test]
    fn parses_with_obfs_aliases() {
        let result =
            get_data("hy2://pw@example.com:443?obfs_type=salamander&obfs_password=secret2");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.obfs, Some("salamander".to_string()));
        assert_eq!(data.obfs_password, Some("secret2".to_string()));

        let result2 = get_data("hy2://pw@example.com:443?obfs=salamander&obfs-param=secret3");
        assert!(result2.is_ok());
        let data2 = result2.unwrap();
        assert_eq!(data2.obfs, Some("salamander".to_string()));
        assert_eq!(data2.obfs_password, Some("secret3".to_string()));
    }

    #[test]
    fn parses_with_fm_finalmask_json() {
        let uri = "hysteria2://f46f8ebc@example.com:8443/?sni=example.com&fm=%7B%22finalmask%22%3A%7B%22udp%22%3A%5B%7B%22type%22%3A%22salamander%22%2C%22settings%22%3A%7B%22password%22%3A%22x93vL5mP4nC72H3w%22%2C%22packetSize%22%3A%22512-1200%22%7D%7D%5D%7D%7D#Node";
        let result = get_data(uri);
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.uuid, Some("f46f8ebc".to_string()));
        assert_eq!(data.address, Some("example.com".to_string()));
        assert_eq!(data.port, Some(8443));
        assert_eq!(data.sni, Some("example.com".to_string()));
        assert_eq!(data.obfs, Some("salamander".to_string()));
        assert_eq!(data.obfs_password, Some("x93vL5mP4nC72H3w".to_string()));
        assert_eq!(data.remarks, "Node");
    }

    #[test]
    fn parses_without_query_string() {
        let result = get_data("hysteria2://mypassword@example.com:443#MyServer");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.uuid, Some("mypassword".to_string()));
        assert_eq!(data.address, Some("example.com".to_string()));
        assert_eq!(data.port, Some(443));
        assert_eq!(data.remarks, "MyServer");

        let bare = get_data("hy2://mypassword@example.com:8443");
        assert!(bare.is_ok());
        let data_bare = bare.unwrap();
        assert_eq!(data_bare.uuid, Some("mypassword".to_string()));
        assert_eq!(data_bare.address, Some("example.com".to_string()));
        assert_eq!(data_bare.port, Some(8443));
    }

    #[test]
    fn parses_without_port_defaults_to_443() {
        let result = get_data("hysteria2://mypassword@example.com?sni=example.com#Test");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.address, Some("example.com".to_string()));
        assert_eq!(data.port, Some(443));
    }

    #[test]
    fn parses_ipv6_address() {
        let result = get_data("hysteria2://mypassword@[2001:db8::1]:8443?test=1");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.address, Some("2001:db8::1".to_string()));
        assert_eq!(data.port, Some(8443));

        let result_default_port = get_data("hysteria2://mypassword@[2001:db8::1]");
        assert!(result_default_port.is_ok());
        let data_def = result_default_port.unwrap();
        assert_eq!(data_def.address, Some("2001:db8::1".to_string()));
        assert_eq!(data_def.port, Some(443));
    }

    #[test]
    fn url_decodes_password() {
        let result = get_data("hysteria2://my%20password@example.com:443?test=1");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.uuid, Some("my password".to_string()));
    }

    #[test]
    fn missing_at_returns_error() {
        let result = get_data("hysteria2://noat.example.com:443?test=1");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no `@` found"));
    }
}
