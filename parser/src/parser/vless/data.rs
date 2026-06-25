use crate::config_models::{RawData, UserAddress};
use crate::utils::{get_parameter_value, url_decode};
use http::Uri;

pub fn get_data(uri: &str) -> Result<RawData, String> {
    let data = uri
        .split_once("vless://")
        .ok_or_else(|| "Invalid vless URI: missing 'vless://'".to_string())?
        .1;
    let query_and_name = uri
        .split_once("?")
        .ok_or_else(|| "Missing query in vless URI".to_string())?
        .1;
    let (raw_query, name) = query_and_name
        .split_once("#")
        .unwrap_or((query_and_name, ""));
    let parsed_address = parse_vless_address(
        data.split_once("?")
            .ok_or_else(|| "Missing '?' in vless URI".to_string())?
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

fn parse_vless_address(raw_data: &str) -> Result<UserAddress, String> {
    let (uuid_raw, raw_address) = raw_data.split_once("@").ok_or_else(|| {
        "Wrong vless format, no `@` found in the address".to_string()
    })?;
    let uuid = String::from(uuid_raw);
    let address_wo_slash = raw_address.strip_suffix("/").unwrap_or(raw_address);

    let parsed: Uri = address_wo_slash
        .parse()
        .map_err(|e| format!("Invalid vless address URI: {}", e))?;

    let uuid = url_decode(Some(uuid))
        .ok_or_else(|| "Failed to URL-decode vless UUID".to_string())?;

    Ok(UserAddress {
        uuid,
        address: parsed
            .host()
            .ok_or_else(|| "Missing host in vless address".to_string())?
            .to_string(),
        port: parsed
            .port()
            .ok_or_else(|| "Missing port in vless address".to_string())?
            .as_u16(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_vless_uri() {
        let result = get_data("vless://uuid123@example.com:443?test=1");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.uuid, Some("uuid123".to_string()));
        assert_eq!(data.address, Some("example.com".to_string()));
        assert_eq!(data.port, Some(443));
        assert_eq!(data.remarks, "");
    }

    #[test]
    fn parses_with_remarks() {
        let result = get_data("vless://uuid@example.com:443?test=1#MyVless");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.remarks, "MyVless");
    }

    #[test]
    fn parses_with_all_query_params() {
        let uri = "vless://uuid@example.com:443?security=tls&sni=mysni.com&type=ws&path=/ws&host=myhost.com&fp=chrome&flow=xtls-rprx-vision&alpn=h2&allowInsecure=true&encryption=none&headerType=none&key=mykey&quicSecurity=none&mode=auto&serviceName=svc&authority=auth&seed=123&sid=sidval&pbk=pubkey&slpn=slpnval&spx=spxval&extra=extraval";
        let result = get_data(uri);
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.security, Some("tls".to_string()));
        assert_eq!(data.sni, Some("mysni.com".to_string()));
        assert_eq!(data.r#type, Some("ws".to_string()));
        assert_eq!(data.path, Some("/ws".to_string()));
        assert_eq!(data.host, Some("myhost.com".to_string()));
        assert_eq!(data.fp, Some("chrome".to_string()));
        assert_eq!(data.flow, Some("xtls-rprx-vision".to_string()));
        assert_eq!(data.alpn, Some("h2".to_string()));
        assert_eq!(data.allowInsecure, Some("true".to_string()));
        assert_eq!(data.encryption, Some("none".to_string()));
        assert_eq!(data.header_type, Some("none".to_string()));
        assert_eq!(data.key, Some("mykey".to_string()));
        assert_eq!(data.quic_security, Some("none".to_string()));
        assert_eq!(data.mode, Some("auto".to_string()));
        assert_eq!(data.service_name, Some("svc".to_string()));
        assert_eq!(data.authority, Some("auth".to_string()));
        assert_eq!(data.seed, Some("123".to_string()));
        assert_eq!(data.sid, Some("sidval".to_string()));
        assert_eq!(data.pbk, Some("pubkey".to_string()));
        assert_eq!(data.slpn, Some("slpnval".to_string()));
        assert_eq!(data.spx, Some("spxval".to_string()));
        assert_eq!(data.extra, Some("extraval".to_string()));
    }

    #[test]
    fn parses_reality_config() {
        let uri = "vless://uuid@example.com:443?security=reality&sni=google.com&pbk=pubkey&sid=shortid&fp=firefox&flow=xtls-rprx-vision#Reality";
        let result = get_data(uri);
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.security, Some("reality".to_string()));
        assert_eq!(data.pbk, Some("pubkey".to_string()));
        assert_eq!(data.sid, Some("shortid".to_string()));
        assert_eq!(data.fp, Some("firefox".to_string()));
        assert_eq!(data.remarks, "Reality");
    }

    #[test]
    fn url_decodes_uuid() {
        let result = get_data("vless://my%20uuid@example.com:443?test=1");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.uuid, Some("my uuid".to_string()));
    }

    #[test]
    fn url_decodes_remarks() {
        let result = get_data("vless://uuid@example.com:443?test=1#My%20Vless");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.remarks, "My Vless");
    }

    #[test]
    fn missing_at_returns_error() {
        let result = get_data("vless://noat.example.com:443?test=1");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no `@` found"));
    }

    #[test]
    fn missing_query_returns_error() {
        let result = get_data("vless://uuid@example.com:443");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing query"));
    }

    #[test]
    fn missing_port_returns_error() {
        let result = get_data("vless://uuid@example.com?test=1");
        assert!(result.is_err());
    }

    #[test]
    fn encrypt_defaults_to_none_when_absent() {
        let result = get_data("vless://uuid@example.com:443?test=1");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.encryption, None);
    }

    #[test]
    fn allow_insecure_with_value_1() {
        let result = get_data("vless://uuid@example.com:443?test=1&allowInsecure=1");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.allowInsecure, Some("1".to_string()));
    }

    #[test]
    fn empty_query_string() {
        let result = get_data("vless://uuid@example.com:443?");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.uuid, Some("uuid".to_string()));
    }
}