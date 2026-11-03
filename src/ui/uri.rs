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
    /// Decoded query parameters in their original order. Values are not
    /// filtered here: secrets must be excluded by the consumer.
    pub params: Vec<(String, String)>,
}

impl ParsedUri {
    /// First non-empty value for any of the given keys.
    pub fn param(&self, keys: &[&str]) -> Option<&str> {
        for key in keys {
            if let Some((_, value)) = self.params.iter().find(|(k, _)| k == key)
                && !value.is_empty()
            {
                return Some(value);
            }
        }
        None
    }
}

pub fn parse_uri(uri: &str) -> ParsedUri {
    if let Some(vmess) = parse_vmess(uri) {
        return vmess;
    }

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
    let without_scheme = without_scheme
        .strip_prefix("hysteria2://")
        .unwrap_or(without_scheme);
    let without_scheme = without_scheme
        .strip_prefix("hy2://")
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

    let query_params: Vec<(String, String)> = query_str
        .split('&')
        .filter(|s| !s.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (urlencoding(key), urlencoding(value))
        })
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
    } else if uri.starts_with("hysteria2://") || uri.starts_with("hy2://") {
        "hysteria2".into()
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

    params.params = query_params;
    params
}

/// VMess links carry a base64-encoded JSON object instead of URI query
/// parameters. Decode it so Details can show the real endpoint; the `id`
/// (credential) is deliberately not copied anywhere.
fn parse_vmess(uri: &str) -> Option<ParsedUri> {
    let payload = uri.strip_prefix("vmess://")?;
    let (raw, fragment) = match payload.rsplit_once('#') {
        Some((raw, fragment)) => (raw, fragment),
        None => (payload, ""),
    };
    let raw = raw.split('?').next().unwrap_or(raw);
    let bytes = decode_base64(raw.trim())?;
    let json: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let obj = json.as_object()?;

    let get = |key: &str| {
        obj.get(key).and_then(|value| match value {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
    };

    let mut parsed = ParsedUri {
        protocol: "vmess".into(),
        name: urlencoding(fragment),
        ..Default::default()
    };
    if let Some(add) = get("add") {
        parsed.host = add;
    }
    if let Some(port) = get("port") {
        parsed.port = port;
    }
    parsed.sni = get("sni")
        .filter(|s| !s.is_empty())
        .or_else(|| get("host"))
        .unwrap_or_default();
    if let Some(net) = get("net") {
        parsed.transport = net;
    }
    if let Some(tls) = get("tls") {
        parsed.security = tls;
    }
    for key in [
        "path", "host", "alpn", "fp", "type", "scy", "pbk", "sid", "spx",
    ] {
        if let Some(value) = get(key)
            && !value.is_empty()
        {
            parsed.params.push((key.to_string(), value));
        }
    }

    Some(parsed)
}

fn decode_base64(input: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
    [&STANDARD, &URL_SAFE, &STANDARD_NO_PAD, &URL_SAFE_NO_PAD]
        .iter()
        .find_map(|engine| engine.decode(input).ok())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urlencoding_utf8() {
        // "%D0%9F%D1%80%D0%B8%D0%B2%D0%B5%D1%82" -> "Привет"
        let decoded = urlencoding("%D0%9F%D1%80%D0%B8%D0%B2%D0%B5%D1%82");
        assert_eq!(decoded, "Привет");

        let decoded_spaces = urlencoding("Hello+World%20%21");
        assert_eq!(decoded_spaces, "Hello World !");
    }

    #[test]
    fn test_parse_vless_exposes_technical_params() {
        let uri = "vless://uuid@example.com:443?security=reality&sni=example.com&fp=chrome&pbk=abc&sid=1a2b&spx=%2F&alpn=h2%2Chttp%2F1.1&flow=xtls-rprx-vision#Node";
        let parsed = parse_uri(uri);
        assert_eq!(parsed.param(&["pbk"]), Some("abc"));
        assert_eq!(parsed.param(&["sid"]), Some("1a2b"));
        assert_eq!(parsed.param(&["spx"]), Some("/"));
        assert_eq!(parsed.param(&["alpn"]), Some("h2,http/1.1"));
        assert_eq!(parsed.param(&["missing"]), None);
    }

    #[test]
    fn test_parse_vmess_json() {
        use base64::Engine;
        let json = r#"{"add":"example.com","port":"443","id":"secret-id","net":"ws","tls":"tls","path":"/ws","host":"cdn.example.com","sni":"sni.example.com","alpn":"h2","fp":"chrome","scy":"auto"}"#;
        let payload = base64::engine::general_purpose::STANDARD.encode(json);
        let parsed = parse_uri(&format!("vmess://{payload}#Node"));
        assert_eq!(parsed.protocol, "vmess");
        assert_eq!(parsed.host, "example.com");
        assert_eq!(parsed.port, "443");
        assert_eq!(parsed.transport, "ws");
        assert_eq!(parsed.security, "tls");
        assert_eq!(parsed.sni, "sni.example.com");
        assert_eq!(parsed.param(&["path"]), Some("/ws"));
        assert_eq!(parsed.param(&["host"]), Some("cdn.example.com"));
        // The credential stays out of the display payload.
        assert!(parsed.params.iter().all(|(k, _)| k != "id"));
        assert!(parsed.params.iter().all(|(_, v)| v != "secret-id"));
    }

    #[test]
    fn test_parse_uri_hysteria2() {
        let uri = "hysteria2://user:pass@example.com:443?sni=example.com#%D0%A1%D0%B5%D1%80%D0%B2%D0%B5%D1%80";
        let parsed = parse_uri(uri);
        assert_eq!(parsed.protocol, "hysteria2");
        assert_eq!(parsed.host, "example.com");
        assert_eq!(parsed.port, "443");
        assert_eq!(parsed.sni, "example.com");
        assert_eq!(parsed.name, "Сервер");

        let hy2_uri = "hy2://user:pass@1.2.3.4:8443#TestNode";
        let parsed_hy2 = parse_uri(hy2_uri);
        assert_eq!(parsed_hy2.protocol, "hysteria2");
        assert_eq!(parsed_hy2.host, "1.2.3.4");
        assert_eq!(parsed_hy2.port, "8443");
        assert_eq!(parsed_hy2.name, "TestNode");
    }
}
