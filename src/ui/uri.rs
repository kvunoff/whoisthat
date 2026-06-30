use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct ParsedUri {
    pub host: String,
    pub port: String,
    pub sni: String,
    pub transport: String,
    pub security: String,
    pub flow: String,
    pub name: String,
    pub protocol: String,
}

pub fn parse_uri(uri: &str) -> ParsedUri {
    let mut params = ParsedUri::default();

    let without_scheme = uri.strip_prefix("vless://").unwrap_or(uri);
    let without_scheme = without_scheme
        .strip_prefix("vmess://")
        .unwrap_or(without_scheme);
    let without_scheme = without_scheme
        .strip_prefix("trojan://")
        .unwrap_or(without_scheme);
    let without_scheme = without_scheme
        .strip_prefix("ss://")
        .unwrap_or(without_scheme);
    let without_scheme = without_scheme
        .strip_prefix("socks://")
        .unwrap_or(without_scheme);

    let (rest, fragment) = match without_scheme.rsplit_once('#') {
        Some((r, f)) => (r, f),
        None => (without_scheme, ""),
    };
    params.name = urlencoding(fragment);

    let (before_query, query_str) = match rest.split_once('?') {
        Some((b, q)) => (b, q),
        None => (rest, ""),
    };

    let query: HashMap<&str, &str> = query_str
        .split('&')
        .filter(|s| !s.is_empty())
        .filter_map(|pair| pair.split_once('='))
        .collect();

    params.sni = query.get("sni").map(|s| s.to_string()).unwrap_or_default();
    params.transport = query.get("type").map(|s| s.to_string()).unwrap_or_else(|| {
        query
            .get("network")
            .map(|s| s.to_string())
            .unwrap_or_default()
    });
    params.security = query
        .get("security")
        .map(|s| s.to_string())
        .unwrap_or_default();
    params.flow = query.get("flow").map(|s| s.to_string()).unwrap_or_default();
    params.protocol = if uri.starts_with("vless://") {
        "vless".into()
    } else if uri.starts_with("vmess://") {
        "vmess".into()
    } else if uri.starts_with("trojan://") {
        "trojan".into()
    } else if uri.starts_with("ss://") {
        "shadowsocks".into()
    } else if uri.starts_with("socks://") {
        "socks".into()
    } else {
        String::new()
    };

    let authority = if let Some(pos) = before_query.rfind('@') {
        before_query[pos + 1..].to_string()
    } else {
        before_query.to_string()
    };

    if let Some((host_part, port_part)) = authority.rsplit_once(':') {
        if !host_part.is_empty() && port_part.chars().all(|c| c.is_ascii_digit()) {
            params.host = host_part.to_string();
            params.port = port_part.to_string();
        } else {
            params.host = authority;
        }
    } else {
        params.host = authority;
    }

    params
}

/// Extract the cipher method from an ss:// URI (SIP002 format).
/// Returns None if the URI is not shadowsocks or parsing fails.
pub fn parse_ss_method(uri: &str) -> Option<String> {
    let without_scheme = uri.strip_prefix("ss://")?;
    let without_fragment = without_scheme.split('#').next()?;
    let without_query = without_fragment.split('?').next()?;
    let userinfo = without_query.split('@').next()?;
    let decoded =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, userinfo).ok()?;
    let decoded_str = String::from_utf8(decoded).ok()?;
    decoded_str.split(':').next().map(|s| s.to_string())
}

/// Returns true if the cipher method is known-insecure (non-AEAD or broken).
pub fn is_insecure_ss_cipher(method: &str) -> bool {
    matches!(
        method,
        "none"
            | "table"
            | "rc4"
            | "rc4-md5"
            | "aes-128-cfb"
            | "aes-192-cfb"
            | "aes-256-cfb"
            | "aes-128-ctr"
            | "aes-192-ctr"
            | "aes-256-ctr"
            | "des-cfb"
            | "rc2-cfb"
            | "idea-cfb"
            | "seed-cfb"
            | "camellia-128-cfb"
            | "camellia-192-cfb"
            | "camellia-256-cfb"
            | "bf-cfb"
            | "salsa20"
            | "chacha20"
    )
}

fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(decoded) =
                u8::from_str_radix(&String::from_utf8_lossy(&bytes[i + 1..i + 3]), 16)
            {
                out.push(decoded as char);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(' ');
        } else {
            out.push(bytes[i] as char);
        }
        i += 1;
    }
    out
}
