use crate::config_models::{RawData, UserAddress};
use crate::utils::{get_parameter_value, url_decode};
use http::Uri;

pub fn get_data(uri: &str) -> Result<RawData, String> {
    let data = uri
        .split_once("trojan://")
        .ok_or_else(|| "Invalid trojan URI: missing 'trojan://'".to_string())?
        .1;
    let query_and_name = uri
        .split_once("?")
        .ok_or_else(|| "Missing query in trojan URI".to_string())?
        .1;
    let (raw_query, name) = query_and_name
        .split_once("#")
        .unwrap_or((query_and_name, ""));
    let parsed_address = parse_trojan_address(
        data.split_once("?")
            .ok_or_else(|| "Missing '?' in trojan URI".to_string())?
            .0,
    )?;
    let query: Vec<(&str, &str)> = querystring::querify(raw_query);

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
        allowInsecure: get_parameter_value(&query, "allowInsecure"),
        server_method: None,
        username: None,
    })
}

fn parse_trojan_address(raw_data: &str) -> Result<UserAddress, String> {
    let (uuid_raw, raw_address) = raw_data
        .split_once("@")
        .ok_or_else(|| "Wrong trojan format, no `@` found in the address".to_string())?;
    let uuid = String::from(uuid_raw);
    let address_wo_slash = raw_address.strip_suffix("/").unwrap_or(raw_address);

    let parsed: Uri = address_wo_slash
        .parse()
        .map_err(|e| format!("Invalid trojan address URI: {}", e))?;

    let uuid = url_decode(Some(uuid))
        .ok_or_else(|| "Failed to URL-decode trojan password".to_string())?;

    Ok(UserAddress {
        uuid,
        address: parsed
            .host()
            .ok_or_else(|| "Missing host in trojan address".to_string())?
            .to_string(),
        port: parsed
            .port()
            .ok_or_else(|| "Missing port in trojan address".to_string())?
            .as_u16(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_trojan_uri() {
        let result = get_data("trojan://mypassword@example.com:443?test=1");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.uuid, Some("mypassword".to_string()));
        assert_eq!(data.address, Some("example.com".to_string()));
        assert_eq!(data.port, Some(443));
    }

    #[test]
    fn parses_with_remarks() {
        let result = get_data("trojan://pw@example.com:443?test=1#MyTrojan");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.remarks, "MyTrojan");
    }

    #[test]
    fn parses_with_tls_and_sni() {
        let result = get_data("trojan://pw@example.com:443?security=tls&sni=sni.example.com&allowInsecure=true");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.security, Some("tls".to_string()));
        assert_eq!(data.sni, Some("sni.example.com".to_string()));
        assert_eq!(data.allowInsecure, Some("true".to_string()));
    }

    #[test]
    fn url_decodes_password() {
        let result = get_data("trojan://my%20password@example.com:443?test=1");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.uuid, Some("my password".to_string()));
    }

    #[test]
    fn missing_at_returns_error() {
        let result = get_data("trojan://noat.example.com:443?test=1");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no `@` found"));
    }

    #[test]
    fn missing_query_returns_error() {
        let result = get_data("trojan://pw@example.com:443");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing query"));
    }

    #[test]
    fn missing_port_returns_error() {
        let result = get_data("trojan://pw@example.com?test=1");
        assert!(result.is_err());
    }
}