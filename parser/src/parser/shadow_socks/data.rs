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
