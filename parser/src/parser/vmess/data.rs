use crate::config_models::{RawData, UserAddress};
use crate::utils::{get_parameter_value, url_decode, url_decode_str};
use base64::{engine::general_purpose, Engine};
use http::Uri;
use serde_json::Value;

pub fn get_data(uri: &str) -> Result<RawData, String> {
    let data = uri
        .split_once("vmess://")
        .ok_or_else(|| "Invalid vmess URI: missing 'vmess://'".to_string())?
        .1;

    return match general_purpose::STANDARD
        .decode(url_decode_str(data).unwrap_or(String::from(data)))
    {
        Ok(decoded) => get_raw_data_from_base64(&decoded),
        Err(_) => get_raw_data_from_uri(data),
    };
}

fn get_raw_data_from_base64(decoded_base64: &[u8]) -> Result<RawData, String> {
    let json_str = std::str::from_utf8(decoded_base64)
        .map_err(|e| format!("Invalid UTF-8 in vmess base64: {}", e))?;
    let json = serde_json::from_str::<Value>(json_str)
        .map_err(|e| format!("Invalid JSON in vmess base64: {}", e))?;

    let port = get_str_field(&json, "port")
        .and_then(|s| s.parse::<u16>().ok());

    Ok(RawData {
        remarks: url_decode(get_str_field(&json, "ps")).unwrap_or(String::from("")),
        uuid: get_str_field(&json, "id"),
        port,
        address: get_str_field(&json, "add"),
        alpn: url_decode(get_str_field(&json, "alpn")),
        path: url_decode(get_str_field(&json, "path")),
        authority: url_decode(get_str_field(&json, "host")),
        pbk: url_decode(get_str_field(&json, "pbk")),
        security: get_str_field(&json, "tls"),
        vnext_security: get_str_field(&json, "scy"),
        sid: url_decode(get_str_field(&json, "sid")),
        flow: url_decode(get_str_field(&json, "flow")),
        sni: get_str_field(&json, "sni"),
        fp: url_decode(get_str_field(&json, "fp")),
        r#type: url_decode(get_str_field(&json, "net")),
        encryption: None,
        header_type: url_decode(get_str_field(&json, "type")),
        host: url_decode(get_str_field(&json, "host")),
        seed: url_decode(get_str_field(&json, "seed")),
        quic_security: None,
        key: None,
        mode: url_decode(get_str_field(&json, "mode")),
        service_name: url_decode(get_str_field(&json, "path")),
        slpn: url_decode(get_str_field(&json, "slpn")),
        spx: url_decode(get_str_field(&json, "spx")),
        extra: url_decode(get_str_field(&json, "extra")),
        allowInsecure: None,
        server_method: None,
        username: None,
    })
}

fn get_str_field(json: &Value, field: &str) -> Option<String> {
    return json.get(field).and_then(|v| v.as_str()).map(String::from);
}

fn get_raw_data_from_uri(data: &str) -> Result<RawData, String> {
    let (before_query, query_and_name) = data
        .split_once("?")
        .ok_or_else(|| "Missing query in vmess URI".to_string())?;

    let (raw_query, name) = query_and_name
        .split_once("#")
        .unwrap_or((query_and_name, ""));
    let parsed_address = parse_vmess_address(before_query)?;
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

fn parse_vmess_address(raw_data: &str) -> Result<UserAddress, String> {
    let (uuid_raw, raw_address) = raw_data
        .split_once("@")
        .ok_or_else(|| {
            "Wrong vmess format, no `@` found in the address and it was not a valid base64"
                .to_string()
        })?;
    let uuid = String::from(uuid_raw);
    let address_wo_slash = raw_address.strip_suffix("/").unwrap_or(raw_address);

    let parsed: Uri = address_wo_slash
        .parse()
        .map_err(|e| format!("Invalid vmess address URI: {}", e))?;

    let uuid = url_decode(Some(uuid))
        .ok_or_else(|| "Failed to URL-decode vmess UUID".to_string())?;

    Ok(UserAddress {
        uuid,
        address: parsed
            .host()
            .ok_or_else(|| "Missing host in vmess address".to_string())?
            .to_string(),
        port: parsed
            .port()
            .ok_or_else(|| "Missing port in vmess address".to_string())?
            .as_u16(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    mod base64_format {
        use super::*;

        #[test]
        fn parses_valid_base64_json() {
            let uri = "vmess://eyJhZGQiOiJleGFtcGxlLmNvbSIsInBvcnQiOiI0NDMiLCJpZCI6InRlc3QtdXVpZCIsInBzIjoiTXlWTUVTUyIsIm5ldCI6InRjcCIsInRscyI6InRscyIsInNuaSI6ImV4YW1wbGUuY29tIiwidHlwZSI6Im5vbmUifQ==";
            let result = get_data(uri);
            assert!(result.is_ok());
            let data = result.unwrap();
            assert_eq!(data.address, Some("example.com".to_string()));
            assert_eq!(data.port, Some(443));
            assert_eq!(data.uuid, Some("test-uuid".to_string()));
            assert_eq!(data.remarks, "MyVMESS");
            assert_eq!(data.r#type, Some("tcp".to_string()));
            assert_eq!(data.security, Some("tls".to_string()));
            assert_eq!(data.sni, Some("example.com".to_string()));
            assert_eq!(data.header_type, Some("none".to_string()));
        }

        #[test]
        fn falls_back_to_raw_uri_on_invalid_base64() {
            let uri = "vmess://!!!invalid-base64!!!@example.com:443?test=1#Name";
            let result = get_data(uri);
            assert!(result.is_ok());
            let data = result.unwrap();
            assert_eq!(data.uuid, Some("!!!invalid-base64!!!".to_string()));
            assert_eq!(data.address, Some("example.com".to_string()));
            assert_eq!(data.port, Some(443));
            assert_eq!(data.remarks, "Name");
        }

        #[test]
        fn falls_back_to_raw_uri_on_non_utf8_base64() {
            let uri = "vmess://gA==@example.com:443?test=1";
            let result = get_data(uri);
            assert!(result.is_ok());
            let data = result.unwrap();
            assert_eq!(data.uuid, Some("gA==".to_string()));
            assert_eq!(data.address, Some("example.com".to_string()));
        }

        #[test]
        fn falls_back_to_raw_uri_on_invalid_json() {
            let uri = "vmess://bm90LWpzb24=@example.com:443?test=1";
            let result = get_data(uri);
            assert!(result.is_ok());
            let data = result.unwrap();
            assert_eq!(data.address, Some("example.com".to_string()));
        }

        #[test]
        fn json_with_numeric_port() {
            let json_str = r#"{"add":"example.com","port":"443","id":"uuid","ps":"name","net":"tcp","tls":"","type":"none"}"#;
            let uri = format!("vmess://{}", general_purpose::STANDARD.encode(json_str));
            let result = get_data(&uri);
            assert!(result.is_ok());
            let data = result.unwrap();
            assert_eq!(data.port, Some(443));
        }

        #[test]
        fn json_with_non_numeric_port_returns_none() {
            let json_str = r#"{"add":"example.com","port":"abc","id":"uuid","ps":"name","net":"tcp","tls":"","type":"none"}"#;
            let uri = format!("vmess://{}", general_purpose::STANDARD.encode(json_str));
            let result = get_data(&uri);
            assert!(result.is_ok());
            let data = result.unwrap();
            assert_eq!(data.port, None);
        }

        #[test]
        fn json_with_missing_fields() {
            let json_str = r#"{"add":"example.com","port":"443","id":"uuid","ps":"name"}"#;
            let uri = format!("vmess://{}", general_purpose::STANDARD.encode(json_str));
            let result = get_data(&uri);
            assert!(result.is_ok());
            let data = result.unwrap();
            assert_eq!(data.r#type, None);
            assert_eq!(data.security, None);
        }

        #[test]
        fn json_distinguishes_scy_from_tls() {
            let json_str = r#"{"add":"example.com","port":"443","id":"uuid","ps":"name","tls":"tls","scy":"auto","net":"tcp","type":"none"}"#;
            let uri = format!("vmess://{}", general_purpose::STANDARD.encode(json_str));
            let result = get_data(&uri);
            assert!(result.is_ok());
            let data = result.unwrap();
            assert_eq!(data.security, Some("tls".to_string()));
            assert_eq!(data.vnext_security, Some("auto".to_string()));
        }
    }

    mod raw_uri_format {
        use super::*;

        #[test]
        fn parses_minimal_raw_uri() {
            let result = get_data("vmess://uuid123@example.com:443?test=1");
            assert!(result.is_ok());
            let data = result.unwrap();
            assert_eq!(data.uuid, Some("uuid123".to_string()));
            assert_eq!(data.address, Some("example.com".to_string()));
            assert_eq!(data.port, Some(443));
        }

        #[test]
        fn raw_uri_missing_at_returns_error() {
            let result = get_data("vmess://noat.example.com:443?test=1");
            assert!(result.is_err());
        }

        #[test]
        fn raw_uri_with_remarks() {
            let result = get_data("vmess://uuid@example.com:443?test=1#Remarks%20Here");
            assert!(result.is_ok());
            let data = result.unwrap();
            assert_eq!(data.remarks, "Remarks Here");
        }

        #[test]
        fn raw_uri_with_multiple_query_params() {
            let result = get_data("vmess://uuid@example.com:443?security=tls&sni=sni.com&type=grpc&encryption=none");
            assert!(result.is_ok());
            let data = result.unwrap();
            assert_eq!(data.security, Some("tls".to_string()));
            assert_eq!(data.sni, Some("sni.com".to_string()));
            assert_eq!(data.r#type, Some("grpc".to_string()));
            assert_eq!(data.encryption, Some("none".to_string()));
        }
    }
}