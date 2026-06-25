use http::Uri;

use crate::{
    config_models::RawData,
    parser::socks::models,
    utils::{url_decode, url_decode_str},
};
use base64::{engine::general_purpose, Engine};

pub fn get_data(uri: &str) -> Result<RawData, String> {
    let data = uri
        .split_once("://")
        .ok_or_else(|| "Invalid socks URI".to_string())?
        .1;
    let (raw_data, name) = data.split_once("#").unwrap_or((data, ""));
    let (raw_uri, _) = raw_data.split_once("?").unwrap_or((raw_data, ""));
    let parsed_address = parse_socks_address(raw_uri)?;
    Ok(RawData {
        remarks: url_decode(Some(String::from(name))).unwrap_or(String::from("")),
        username: url_decode(parsed_address.username),
        address: Some(parsed_address.address),
        port: Some(parsed_address.port),
        uuid: url_decode(parsed_address.password),
        r#type: Some(String::from("tcp")),
        header_type: None,
        server_method: None,
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
        obfs: None,
        obfs_password: None,
    })
}

fn parse_socks_address(raw_data: &str) -> Result<models::SocksAddress, String> {
    let (maybe_userinfo, raw_address): (Option<String>, &str) = match raw_data.split_once("@") {
        Some(data) => (Some(String::from(data.0)), data.1),
        None => (None, raw_data),
    };
    let address_wo_slash = raw_address.strip_suffix("/").unwrap_or(raw_address);

    let parsed: Uri = address_wo_slash
        .parse()
        .map_err(|e| format!("Invalid socks address URI: {}", e))?;

    return match maybe_userinfo {
        Some(userinfo) => {
            let url_decoded = url_decode_str(&userinfo).unwrap_or(userinfo);
            let username_and_password = general_purpose::STANDARD
                .decode(url_decoded.clone())
                .map(|a| {
                    String::from(
                        std::str::from_utf8(&a)
                            .unwrap_or("")
                    )
                })
                .unwrap_or(String::from(url_decoded.clone()));

            let (username, password) = username_and_password
                .split_once(":")
                .unwrap_or((&username_and_password, ""));

            Ok(models::SocksAddress {
                username: Some(String::from(username)),
                password: if password.is_empty() { None } else { Some(String::from(password)) },
                address: parsed
                    .host()
                    .ok_or_else(|| "Missing host in socks address".to_string())?
                    .to_string(),
                port: parsed
                    .port()
                    .ok_or_else(|| "Missing port in socks address".to_string())?
                    .as_u16(),
            })
        }
        None => Ok(models::SocksAddress {
            username: None,
            password: None,
            address: parsed
                .host()
                .ok_or_else(|| "Missing host in socks address".to_string())?
                .to_string(),
            port: parsed
                .port()
                .ok_or_else(|| "Missing port in socks address".to_string())?
                .as_u16(),
        }),
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_with_user_and_password() {
        let result = get_data("socks5://user:pass@example.com:1080#MySocks");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.username, Some("user".to_string()));
        assert_eq!(data.uuid, Some("pass".to_string()));
        assert_eq!(data.address, Some("example.com".to_string()));
        assert_eq!(data.port, Some(1080));
        assert_eq!(data.remarks, "MySocks");
    }

    #[test]
    fn parses_without_userinfo() {
        let result = get_data("socks5://example.com:1080");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.username, None);
        assert_eq!(data.uuid, None);
        assert_eq!(data.address, Some("example.com".to_string()));
        assert_eq!(data.port, Some(1080));
    }

    #[test]
    fn parses_base64_credentials() {
        let encoded = base64::engine::general_purpose::STANDARD
            .encode("b64user:b64pass");
        let uri = format!("socks5://{}@example.com:1080", encoded);
        let result = get_data(&uri);
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.username, Some("b64user".to_string()));
        assert_eq!(data.uuid, Some("b64pass".to_string()));
    }

    #[test]
    fn non_base64_credentials_fall_through_to_raw() {
        let result = get_data("socks5://rawuser:rawpass@example.com:1080");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.username, Some("rawuser".to_string()));
        assert_eq!(data.uuid, Some("rawpass".to_string()));
    }

    #[test]
    fn empty_password_becomes_none() {
        let result = get_data("socks5://user:@example.com:1080");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.username, Some("user".to_string()));
        assert_eq!(data.uuid, None);
    }

    #[test]
    fn missing_port_returns_error() {
        let result = get_data("socks5://example.com");
        assert!(result.is_err());
    }

    #[test]
    fn stripped_trailing_slash() {
        let result = get_data("socks5://example.com:1080/");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.address, Some("example.com".to_string()));
        assert_eq!(data.port, Some(1080));
    }

    #[test]
    fn url_decodes_remarks() {
        let result = get_data("socks5://example.com:1080#Socks%20Name");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.remarks, "Socks Name");
    }

    #[test]
    fn url_decodes_credentials() {
        let result = get_data("socks5://user%20name:pass%20word@example.com:1080");
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.username, Some("user name".to_string()));
        assert_eq!(data.uuid, Some("pass word".to_string()));
    }
}