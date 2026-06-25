use http::Uri;

use crate::{
    config_models::RawData,
    parser::shadow_socks::models,
    utils::{url_decode, url_decode_str},
};
use base64::{engine::general_purpose, Engine};

pub fn get_data(uri: &str) -> Result<RawData, String> {
    let data = uri
        .split_once("ss://")
        .ok_or_else(|| "Invalid shadowsocks URI: missing 'ss://'".to_string())?
        .1;
    let (raw_data, name) = data.split_once("#").unwrap_or((data, ""));
    let (raw_uri, _) = raw_data.split_once("?").unwrap_or((raw_data, ""));
    let parsed_address = parse_ss_address(raw_uri)?;
    Ok(RawData {
        remarks: url_decode(Some(String::from(name))).unwrap_or(String::from("")),
        server_method: url_decode(Some(parsed_address.method)),
        address: Some(parsed_address.address),
        port: Some(parsed_address.port),
        uuid: url_decode(Some(parsed_address.password)),
        r#type: Some(String::from("tcp")),
        header_type: Some(String::from("none")),
        security: None,
        fp: None,
        sni: None,
        pbk: None,
        sid: None,
        key: None,
        spx: None,
        flow: None,
        path: None,
        host: None,
        seed: None,
        mode: None,
        slpn: None,
        alpn: None,
        extra: None,
        authority: None,
        encryption: None,
        service_name: None,
        quic_security: None,
        allowInsecure: None,
        vnext_security: None,
        username: None,
    })
}

fn parse_ss_address(raw_data: &str) -> Result<models::ShadowSocksAddress, String> {
    let (userinfo_raw, raw_address) = raw_data.split_once("@").ok_or_else(|| {
        "Wrong shadowsocks format, no `@` found in the address".to_string()
    })?;
    let userinfo = String::from(userinfo_raw);
    let address_wo_slash = raw_address.strip_suffix("/").unwrap_or(raw_address);

    let parsed: Uri = address_wo_slash
        .parse()
        .map_err(|e| format!("Invalid shadowsocks address URI: {}", e))?;

    let method_and_password = general_purpose::STANDARD
        .decode(url_decode_str(&userinfo).unwrap_or(userinfo))
        .map_err(|e| format!("Shadowsocks user info is not valid base64: {}", e))?;

    let decoded = std::str::from_utf8(&method_and_password)
        .map_err(|e| format!("Shadowsocks base64 is not valid UTF-8: {}", e))?;

    let (method, password) = decoded
        .split_once(":")
        .ok_or_else(|| "No `:` found in decoded shadowsocks data".to_string())?;

    Ok(models::ShadowSocksAddress {
        method: String::from(method),
        password: String::from(password),
        address: parsed
            .host()
            .ok_or_else(|| "Missing host in shadowsocks address".to_string())?
            .to_string(),
        port: parsed
            .port()
            .ok_or_else(|| "Missing port in shadowsocks address".to_string())?
            .as_u16(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_ss_uri() {
        let method_password = base64::engine::general_purpose::STANDARD
            .encode("chacha20-ietf-poly1305:secretpw");
        let uri = format!("ss://{}@example.com:8388#MySS", method_password);
        let result = get_data(&uri);
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.server_method, Some("chacha20-ietf-poly1305".to_string()));
        assert_eq!(data.uuid, Some("secretpw".to_string()));
        assert_eq!(data.address, Some("example.com".to_string()));
        assert_eq!(data.port, Some(8388));
        assert_eq!(data.remarks, "MySS");
        assert_eq!(data.r#type, Some("tcp".to_string()));
        assert_eq!(data.header_type, Some("none".to_string()));
    }

    #[test]
    fn parses_without_remarks() {
        let method_password = base64::engine::general_purpose::STANDARD
            .encode("aes-256-gcm:password");
        let uri = format!("ss://{}@example.com:8388", method_password);
        let result = get_data(&uri);
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.remarks, "");
    }

    #[test]
    fn invalid_base64_returns_error() {
        let result = get_data("ss://!!!invalid!!!@example.com:8388");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not valid base64"));
    }

    #[test]
    fn no_colon_in_decoded_returns_error() {
        let method_password = base64::engine::general_purpose::STANDARD
            .encode("no-colon-here");
        let uri = format!("ss://{}@example.com:8388", method_password);
        let result = get_data(&uri);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No `:` found"));
    }

    #[test]
    fn missing_at_returns_error() {
        let method_password = base64::engine::general_purpose::STANDARD
            .encode("method:pass");
        let uri = format!("ss://{}-no-at", method_password);
        let result = get_data(&uri);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no `@` found"));
    }

    #[test]
    fn sip002_query_params_are_silently_ignored() {
        let method_password = base64::engine::general_purpose::STANDARD
            .encode("method:pass");
        let uri = format!("ss://{}@example.com:8388?plugin=obfs-local;obfs=http#Name", method_password);
        let result = get_data(&uri);
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.remarks, "Name");
        assert_eq!(data.address, Some("example.com".to_string()));
    }

    #[test]
    fn empty_password_allowed() {
        let method_password = base64::engine::general_purpose::STANDARD
            .encode("method:");
        let uri = format!("ss://{}@example.com:8388", method_password);
        let result = get_data(&uri);
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.server_method, Some("method".to_string()));
        assert_eq!(data.uuid, Some("".to_string()));
    }

    #[test]
    fn url_encodes_remarks() {
        let method_password = base64::engine::general_purpose::STANDARD
            .encode("method:pass");
        let uri = format!("ss://{}@example.com:8388#SS%20Name", method_password);
        let result = get_data(&uri);
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.remarks, "SS Name");
    }
}