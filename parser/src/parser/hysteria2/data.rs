use crate::config_models::{RawData, UserAddress};
use crate::utils::{get_parameter_value, url_decode};
use http::Uri;

pub fn get_data(uri: &str) -> Result<RawData, String> {
    let (_prefix, data) = if uri.starts_with("hysteria2://") {
        uri.split_once("hysteria2://")
    } else if uri.starts_with("hy2://") {
        uri.split_once("hy2://")
    } else {
        return Err("Invalid hysteria2 URI: missing 'hysteria2://' or 'hy2://'".to_string());
    }
    .ok_or_else(|| "Invalid hysteria2 URI".to_string())?;

    let query_and_name = uri
        .split_once("?")
        .ok_or_else(|| "Missing query in hysteria2 URI".to_string())?
        .1;
    let (raw_query, name) = query_and_name
        .split_once("#")
        .unwrap_or((query_and_name, ""));
    let parsed_address = parse_hysteria2_address(
        data.split_once("?")
            .ok_or_else(|| "Missing '?' in hysteria2 URI".to_string())?
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
        allowInsecure: get_parameter_value(&query, "allowInsecure")
            .or_else(|| get_parameter_value(&query, "insecure")),
        server_method: None,
        username: None,
        obfs: url_decode(get_parameter_value(&query, "obfs")),
        obfs_password: url_decode(get_parameter_value(&query, "obfs-password")),
    })
}

fn parse_hysteria2_address(raw_data: &str) -> Result<UserAddress, String> {
    let (uuid_raw, raw_address) = raw_data
        .split_once("@")
        .ok_or_else(|| "Wrong hysteria2 format, no `@` found in the address".to_string())?;
    let uuid = String::from(uuid_raw);
    let address_wo_slash = raw_address.strip_suffix("/").unwrap_or(raw_address);

    let parsed: Uri = address_wo_slash
        .parse()
        .map_err(|e| format!("Invalid hysteria2 address URI: {}", e))?;

    let uuid = url_decode(Some(uuid))
        .ok_or_else(|| "Failed to URL-decode hysteria2 password".to_string())?;

    Ok(UserAddress {
        uuid,
        address: parsed
            .host()
            .ok_or_else(|| "Missing host in hysteria2 address".to_string())?
            .to_string(),
        port: parsed
            .port()
            .ok_or_else(|| "Missing port in hysteria2 address".to_string())?
            .as_u16(),
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
        let result =
            get_data("hysteria2://pw@example.com:443?insecure=1");
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

    #[test]
    fn missing_query_returns_error() {
        let result = get_data("hysteria2://pw@example.com:443");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing query"));
    }

    #[test]
    fn missing_port_returns_error() {
        let result = get_data("hysteria2://pw@example.com?test=1");
        assert!(result.is_err());
    }
}
