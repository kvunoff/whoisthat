use crate::config_models::{
    self, ConfigMetaData, GRPCSettings, HttpUpgradeSettings, KCPSettings, NonHeaderObject, Outbound,
    OutboundSettings, QuicSettings, RawData, RealitySettings, StreamSettings, TCPHeader, TCPSettings,
    TlsSettings, WsSettings, XHTTPSettings,
};
use crate::utils::{inbound_generator, parse_raw_json};

mod shadow_socks;
mod socks;
mod trojan;
mod uri_identifier;
mod vless;
mod vmess;
mod hysteria2;

pub fn get_metadata(uri: &str) -> Result<String, String> {
    let (protocol, data, _) = get_uri_data(uri)?;
    let meta_data = ConfigMetaData {
        name: data.remarks,
        host: data.host.clone(),
        address: data.address.clone(),
        port: data.port.clone(),
        protocol,
    };
    let serialized = serde_json::to_string(&meta_data).map_err(|e| e.to_string())?;
    Ok(serialized)
}

pub fn create_json_config(uri: &str, socks_port: Option<u16>, http_port: Option<u16>) -> Result<String, String> {
    let config = create_config(uri, socks_port, http_port)?;
    let serialized = serde_json::to_string(&config).map_err(|e| e.to_string())?;
    Ok(serialized)
}

pub fn create_config(
    uri: &str,
    socks_port: Option<u16>,
    http_port: Option<u16>,
) -> Result<config_models::Config, String> {
    let outbound_object = create_outbound_object(uri)?;
    let inbound_config =
        inbound_generator::generate_inbound_config(inbound_generator::InboundGenerationOptions {
            socks_port,
            http_port,
        });
    let config = config_models::Config {
        outbounds: vec![outbound_object],
        inbounds: inbound_config,
    };
    Ok(config)
}

// Emits a YAML config understood by the official hysteria2 client
// (apernet/hysteria2). Only valid for `hysteria2://` / `hy2://` URIs; for
// any other protocol this returns an error so callers can fall back to the
// xray JSON path.
pub fn create_hysteria2_client_yaml(
    uri: &str,
    socks_port: u16,
    http_port: Option<u16>,
) -> Result<String, String> {
    let protocol = uri_identifier::get_uri_protocol(uri);
    if !matches!(protocol, Some(uri_identifier::Protocols::Hysteria2)) {
        return Err(format!(
            "URI is not a hysteria2/hy2 link (this command only handles hysteria2)"
        ));
    }
    let data = hysteria2::data::get_data(uri)?;
    hysteria2::create_client_yaml(&data, socks_port, http_port)
}

pub fn create_outbound_object(uri: &str) -> Result<config_models::Outbound, String> {
    let (name, data, outbound_settings) = get_uri_data(uri)?;

    // network defaults to "tcp" when absent (xray's default transport).
    // Explicitly setting it helps protocols like vless+reality+vision that
    // expect a TCP transport.
    let network_type = data.r#type.as_deref().unwrap_or("tcp");

    // Trojan is TLS-by-default in xray-core: a URI without an explicit
    // `security=` parameter must still produce tlsSettings, otherwise xray
    // falls back to plaintext trojan which the server rejects.
    // An explicit `security=none` (or any non-tls value) is still respected.
    let effective_security = if name == "trojan" {
        data.security
            .clone()
            .or_else(|| Some(String::from("tls")))
    } else {
        data.security.clone()
    };

    let allow_insecure = data.allowInsecure == Some(String::from("true"))
        || data.allowInsecure == Some(String::from("1"));

    // ALPN in URIs is frequently comma-separated (e.g. "h2,http/1.1").
    // Split on commas and drop empty segments so we emit ["h2","http/1.1"]
    // instead of ["h2,http/1.1"] (which breaks ALPN negotiation).
    let split_alpn = |alpn: &Option<String>| -> Option<Vec<String>> {
        alpn.as_ref().map(|a| {
            a.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
    };

    let outbound = Outbound {
        protocol: name,
        tag: String::from("proxy"),
        streamSettings: StreamSettings {
            network: Some(network_type.to_string()),
            security: effective_security.clone(),
            tlsSettings: match effective_security.as_deref() {
                Some("tls") => Some(TlsSettings {
                    alpn: split_alpn(&data.alpn),
                    rejectUnknownSni: None,
                    enableSessionResumption: None,
                    minVersion: None,
                    maxVersion: None,
                    cipherSuites: None,
                    disableSystemRoot: None,
                    preferServerCipherSuites: None,
                    fingerprint: data.fp.clone(),
                    serverName: data.sni.clone(),
                    allowInsecure: allow_insecure,
                }),
                _ => None,
            },
            realitySettings: match effective_security.as_deref() {
                Some("reality") => Some(RealitySettings {
                    publicKey: data.pbk,
                    serverName: data.sni.clone(),
                    shortId: data.sid,
                    spiderX: Some(String::from("")),
                    fingerprint: data.fp.clone(),
                }),
                _ => None,
            },
            wsSettings: match network_type {
                "ws" => Some(WsSettings {
                    Host: data.host.clone(),
                    path: data.path.clone(),
                    acceptProxyProtocol: None,
                }),
                _ => None,
            },
            tcpSettings: match network_type {
                "tcp" => Some(TCPSettings {
                    header: Some(TCPHeader {
                        r#type: Some(data.header_type.unwrap_or(String::from("none"))),
                    }),
                    acceptProxyProtocol: None,
                }),
                _ => None,
            },
            grpcSettings: match network_type {
                "grpc" => Some(GRPCSettings {
                    authority: data.authority,
                    // gRPC "multiMode" can be toggled via the URI param `mode=multi`
                    // (matches the convention used by other clients). Defaults to
                    // false to preserve historical behaviour.
                    multiMode: Some(data.mode.as_deref() == Some("multi")),
                    serviceName: data.service_name,
                }),
                _ => None,
            },
            quicSettings: match network_type {
                "quic" => Some(QuicSettings {
                    header: Some(NonHeaderObject {
                        r#type: Some(String::from("none")),
                    }),
                    security: Some(String::from("none")),
                    key: Some(String::from("")),
                }),
                _ => None,
            },
            kcpSettings: match network_type {
                "kcp" => Some(KCPSettings {
                    mtu: None,
                    tti: None,
                    congestion: None,
                    uplinkCapacity: None,
                    readBufferSize: None,
                    writeBufferSize: None,
                    downlinkCapacity: None,
                    seed: data.seed,
                }),
                _ => None,
            },
            xhttpSettings: match network_type {
                "xhttp" => Some(XHTTPSettings {
                    host: data.host.clone(),
                    path: data.path.clone(),
                    mode: data.mode,
                    extra: data.extra.and_then(|e| parse_raw_json(e.as_str())),
                }),
                _ => None,
            },
            httpupgradeSettings: match network_type {
                "httpupgrade" => Some(HttpUpgradeSettings {
                    host: data.host.clone(),
                    path: data.path.clone(),
                }),
                _ => None,
            },
        },
        settings: outbound_settings,
    };

    Ok(outbound)
}

fn get_uri_data(uri: &str) -> Result<(String, RawData, OutboundSettings), String> {
	let protocol = uri_identifier::get_uri_protocol(uri);
	match protocol {
		Some(uri_identifier::Protocols::Vless) => {
			let d = vless::data::get_data(uri)?;
			let s = vless::create_outbound_settings(&d);
			Ok((String::from("vless"), d, s))
		}
		Some(uri_identifier::Protocols::Vmess) => {
			let d = vmess::data::get_data(uri)?;
			let s = vmess::create_outbound_settings(&d);
			Ok((String::from("vmess"), d, s))
		}
		Some(uri_identifier::Protocols::Trojan) => {
			let d = trojan::data::get_data(uri)?;
			let s = trojan::create_outbound_settings(&d);
			Ok((String::from("trojan"), d, s))
		}
		Some(uri_identifier::Protocols::Shadowsocks) => {
			let d = shadow_socks::data::get_data(uri)?;
			let s = shadow_socks::create_outbound_settings(&d);
			Ok((String::from("shadowsocks"), d, s))
		}
		Some(uri_identifier::Protocols::Socks) => {
			let d = socks::data::get_data(uri)?;
			let s = socks::create_outbound_settings(&d);
			Ok((String::from("socks"), d, s))
		}
		Some(uri_identifier::Protocols::Hysteria2) => {
			let d = hysteria2::data::get_data(uri)?;
			let s = hysteria2::create_outbound_settings(&d);
			Ok((String::from("hysteria2"), d, s))
		}
		Some(_) => Err("The protocol was recognized but is not supported yet".to_string()),
		None => Err("The protocol is not supported".to_string()),
	}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_models::OutboundSettings;

    mod get_uri_data_tests {
        use super::*;

        #[test]
        fn dispatches_vless() {
            let result = get_uri_data("vless://uuid@example.com:443?test=1");
            assert!(result.is_ok());
            let (protocol, data, settings) = result.unwrap();
            assert_eq!(protocol, "vless");
            assert_eq!(data.uuid, Some("uuid".to_string()));
            assert!(matches!(settings, OutboundSettings::Vless(_)));
        }

        #[test]
        fn dispatches_vmess_base64() {
            let json = r#"{"add":"example.com","port":"443","id":"uuid","ps":"name","net":"tcp","tls":"","type":"none"}"#;
            let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, json);
            let uri = format!("vmess://{}", encoded);
            let result = get_uri_data(&uri);
            assert!(result.is_ok());
            let (protocol, _, settings) = result.unwrap();
            assert_eq!(protocol, "vmess");
            assert!(matches!(settings, OutboundSettings::Vmess(_)));
        }

        #[test]
        fn dispatches_trojan() {
            let result = get_uri_data("trojan://pw@example.com:443?test=1");
            assert!(result.is_ok());
            let (protocol, _, settings) = result.unwrap();
            assert_eq!(protocol, "trojan");
            assert!(matches!(settings, OutboundSettings::Trojan(_)));
        }

        #[test]
        fn dispatches_shadowsocks() {
            let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, "method:pass");
            let uri = format!("ss://{}@example.com:8388", encoded);
            let result = get_uri_data(&uri);
            assert!(result.is_ok());
            let (protocol, _, settings) = result.unwrap();
            assert_eq!(protocol, "shadowsocks");
            assert!(matches!(settings, OutboundSettings::ShadowSocks(_)));
        }

        #[test]
        fn dispatches_socks() {
            let result = get_uri_data("socks5://example.com:1080");
            assert!(result.is_ok());
            let (protocol, _, settings) = result.unwrap();
            assert_eq!(protocol, "socks");
            assert!(matches!(settings, OutboundSettings::Socks(_)));
        }

        #[test]
        fn unknown_protocol_returns_error() {
            let result = get_uri_data("invalid://something");
            assert!(result.is_err());
            assert!(result.unwrap_err().contains("not supported"));
        }

        #[test]
        fn http_protocol_returns_unsupported() {
            let result = get_uri_data("http://example.com:80");
            assert!(result.is_err());
            assert!(result.unwrap_err().contains("not supported yet"));
        }
    }

    mod get_metadata_tests {
        use super::*;

        #[test]
        fn extracts_vless_metadata() {
            let result = get_metadata("vless://uuid@example.com:443?test=1#MyName");
            assert!(result.is_ok());
            let json = result.unwrap();
            assert!(json.contains("MyName"));
            assert!(json.contains("vless"));
            assert!(json.contains("example.com"));
        }

        #[test]
        fn extracts_trojan_metadata() {
            let result = get_metadata("trojan://pw@example.com:443?test=1#TrojanName");
            assert!(result.is_ok());
            let json = result.unwrap();
            assert!(json.contains("TrojanName"));
            assert!(json.contains("trojan"));
        }

        #[test]
        fn invalid_uri_returns_error() {
            let result = get_metadata("not-a-uri");
            assert!(result.is_err());
        }
    }

    mod create_config_tests {
        use super::*;

        #[test]
        fn creates_config_with_both_inbounds() {
            let result = create_config(
                "vless://uuid@example.com:443?test=1",
                Some(3090),
                Some(3091),
            );
            assert!(result.is_ok());
            let config = result.unwrap();
            assert_eq!(config.outbounds.len(), 1);
            assert_eq!(config.inbounds.len(), 2);
            assert_eq!(config.outbounds[0].protocol, "vless");
            assert_eq!(config.outbounds[0].tag, "proxy");
        }

        #[test]
        fn creates_config_without_inbounds() {
            let result = create_config("vless://uuid@example.com:443?test=1", None, None);
            assert!(result.is_ok());
            let config = result.unwrap();
            assert_eq!(config.inbounds.len(), 0);
        }

        #[test]
        fn invalid_uri_returns_error() {
            let result = create_config("bad-uri", None, None);
            assert!(result.is_err());
        }
    }

    mod create_json_config_tests {
        use super::*;

        #[test]
        fn produces_valid_json() {
            let result = create_json_config(
                "vless://uuid@example.com:443?test=1",
                Some(3090),
                None,
            );
            assert!(result.is_ok());
            let json = result.unwrap();
            assert!(json.contains("vless"));
            assert!(json.contains("socks-in"));
        }

        #[test]
        fn invalid_uri_returns_error() {
            let result = create_json_config("bad-uri", None, None);
            assert!(result.is_err());
        }
    }

    mod create_outbound_object_tests {
        use super::*;

        #[test]
        fn creates_vless_with_tls() {
            let result = create_outbound_object(
                "vless://uuid@example.com:443?security=tls&sni=sni.com&type=tcp&fp=chrome#test"
            );
            assert!(result.is_ok());
            let outbound = result.unwrap();
            assert_eq!(outbound.protocol, "vless");
            assert_eq!(outbound.tag, "proxy");
            assert_eq!(outbound.streamSettings.security, Some("tls".to_string()));
            assert!(outbound.streamSettings.tlsSettings.is_some());
            let tls = outbound.streamSettings.tlsSettings.unwrap();
            assert_eq!(tls.serverName, Some("sni.com".to_string()));
            assert_eq!(tls.fingerprint, Some("chrome".to_string()));
        }

        #[test]
        fn creates_vless_with_reality() {
            let result = create_outbound_object(
                "vless://uuid@example.com:443?security=reality&sni=google.com&pbk=pubkey&sid=sid&fp=firefox&flow=xtls-rprx-vision"
            );
            assert!(result.is_ok());
            let outbound = result.unwrap();
            assert_eq!(outbound.streamSettings.security, Some("reality".to_string()));
            let reality = outbound.streamSettings.realitySettings.unwrap();
            assert_eq!(reality.serverName, Some("google.com".to_string()));
            assert_eq!(reality.publicKey, Some("pubkey".to_string()));
            assert_eq!(reality.shortId, Some("sid".to_string()));
            assert_eq!(reality.fingerprint, Some("firefox".to_string()));
        }

        #[test]
        fn creates_vless_with_ws() {
            let result = create_outbound_object(
                "vless://uuid@example.com:443?type=ws&host=ws.example.com&path=/ws-path"
            );
            assert!(result.is_ok());
            let outbound = result.unwrap();
            assert_eq!(outbound.streamSettings.network, Some("ws".to_string()));
            let ws = outbound.streamSettings.wsSettings.unwrap();
            assert_eq!(ws.Host, Some("ws.example.com".to_string()));
            assert_eq!(ws.path, Some("/ws-path".to_string()));
        }

        #[test]
        fn creates_vless_with_grpc() {
            let result = create_outbound_object(
                "vless://uuid@example.com:443?type=grpc&serviceName=svc&authority=auth"
            );
            assert!(result.is_ok());
            let outbound = result.unwrap();
            assert_eq!(outbound.streamSettings.network, Some("grpc".to_string()));
            let grpc = outbound.streamSettings.grpcSettings.unwrap();
            assert_eq!(grpc.serviceName, Some("svc".to_string()));
            assert_eq!(grpc.authority, Some("auth".to_string()));
        }

        #[test]
        fn creates_vless_with_allow_insecure() {
            let result = create_outbound_object(
                "vless://uuid@example.com:443?security=tls&sni=sni.com&allowInsecure=1"
            );
            assert!(result.is_ok());
            let outbound = result.unwrap();
            let tls = outbound.streamSettings.tlsSettings.unwrap();
            assert!(tls.allowInsecure);
        }

        #[test]
        fn does_not_allow_insecure_by_default() {
            let result = create_outbound_object(
                "vless://uuid@example.com:443?security=tls&sni=sni.com"
            );
            assert!(result.is_ok());
            let outbound = result.unwrap();
            let tls = outbound.streamSettings.tlsSettings.unwrap();
            assert!(!tls.allowInsecure);
        }

        #[test]
        fn creates_trojan_with_tcp_header() {
            let result = create_outbound_object(
                "trojan://pw@example.com:443?security=tls&sni=sni.com&type=tcp&headerType=http"
            );
            assert!(result.is_ok());
            let outbound = result.unwrap();
            let tcp = outbound.streamSettings.tcpSettings.unwrap();
            assert_eq!(tcp.header.unwrap().r#type, Some("http".to_string()));
        }

        #[test]
        fn creates_kcp_settings() {
            let result = create_outbound_object(
                "vless://uuid@example.com:443?type=kcp&seed=myseed"
            );
            assert!(result.is_ok());
            let outbound = result.unwrap();
            let kcp = outbound.streamSettings.kcpSettings.unwrap();
            assert_eq!(kcp.seed, Some("myseed".to_string()));
        }

        #[test]
        fn splits_alpn_csv() {
            let result = create_outbound_object(
                "vless://uuid@example.com:443?security=tls&sni=sni.com&alpn=h2,http/1.1",
            );
            assert!(result.is_ok());
            let outbound = result.unwrap();
            let tls = outbound.streamSettings.tlsSettings.unwrap();
            assert_eq!(tls.alpn, Some(vec!["h2".to_string(), "http/1.1".to_string()]));
        }

        #[test]
        fn filters_empty_alpn_segments() {
            let result = create_outbound_object(
                "vless://uuid@example.com:443?security=tls&sni=sni.com&alpn=h2,,http/1.1,",
            );
            assert!(result.is_ok());
            let tls = result.unwrap().streamSettings.tlsSettings.unwrap();
            assert_eq!(tls.alpn, Some(vec!["h2".to_string(), "http/1.1".to_string()]));
        }

        #[test]
        fn network_defaults_to_tcp_when_absent() {
            let result = create_outbound_object("vless://uuid@example.com:443?test=1");
            assert!(result.is_ok());
            let outbound = result.unwrap();
            assert_eq!(outbound.streamSettings.network, Some("tcp".to_string()));
        }

        #[test]
        fn network_defaults_to_tcp_for_trojan() {
            let result = create_outbound_object("trojan://pw@example.com:443?test=1");
            assert!(result.is_ok());
            let outbound = result.unwrap();
            assert_eq!(outbound.streamSettings.network, Some("tcp".to_string()));
        }

        #[test]
        fn trojan_defaults_to_tls_when_security_absent() {
            let result = create_outbound_object("trojan://pw@example.com:443?sni=sni.com");
            assert!(result.is_ok());
            let outbound = result.unwrap();
            assert_eq!(outbound.streamSettings.security, Some("tls".to_string()));
            assert!(outbound.streamSettings.tlsSettings.is_some());
            let tls = outbound.streamSettings.tlsSettings.unwrap();
            assert_eq!(tls.serverName, Some("sni.com".to_string()));
        }

        #[test]
        fn trojan_respects_explicit_security_none() {
            let result = create_outbound_object("trojan://pw@example.com:443?security=none");
            assert!(result.is_ok());
            let outbound = result.unwrap();
            assert_eq!(outbound.streamSettings.security, Some("none".to_string()));
            assert!(outbound.streamSettings.tlsSettings.is_none());
        }

        #[test]
        fn trojan_with_ws_and_tls() {
            let result = create_outbound_object(
                "trojan://pw@example.com:443?type=ws&host=ws.example.com&path=/ws&security=tls&sni=sni.com",
            );
            assert!(result.is_ok());
            let outbound = result.unwrap();
            assert_eq!(outbound.streamSettings.network, Some("ws".to_string()));
            assert_eq!(outbound.streamSettings.security, Some("tls".to_string()));
            let ws = outbound.streamSettings.wsSettings.unwrap();
            assert_eq!(ws.Host, Some("ws.example.com".to_string()));
            assert_eq!(ws.path, Some("/ws".to_string()));
        }

        #[test]
        fn trojan_with_grpc() {
            let result = create_outbound_object(
                "trojan://pw@example.com:443?type=grpc&serviceName=svc&authority=auth",
            );
            assert!(result.is_ok());
            let outbound = result.unwrap();
            assert_eq!(outbound.streamSettings.network, Some("grpc".to_string()));
            let grpc = outbound.streamSettings.grpcSettings.unwrap();
            assert_eq!(grpc.serviceName, Some("svc".to_string()));
            assert_eq!(grpc.authority, Some("auth".to_string()));
        }

        #[test]
        fn trojan_with_reality() {
            let result = create_outbound_object(
                "trojan://pw@example.com:443?security=reality&sni=google.com&pbk=pubkey&sid=sid&fp=firefox",
            );
            assert!(result.is_ok());
            let outbound = result.unwrap();
            assert_eq!(outbound.streamSettings.security, Some("reality".to_string()));
            let reality = outbound.streamSettings.realitySettings.unwrap();
            assert_eq!(reality.publicKey, Some("pubkey".to_string()));
            assert_eq!(reality.shortId, Some("sid".to_string()));
            assert_eq!(reality.serverName, Some("google.com".to_string()));
        }

        #[test]
        fn grpc_multimode_via_mode_param() {
            let result = create_outbound_object(
                "vless://uuid@example.com:443?type=grpc&serviceName=svc&mode=multi",
            );
            assert!(result.is_ok());
            let grpc = result.unwrap().streamSettings.grpcSettings.unwrap();
            assert_eq!(grpc.multiMode, Some(true));
        }

        #[test]
        fn grpc_multimode_default_false() {
            let result = create_outbound_object(
                "vless://uuid@example.com:443?type=grpc&serviceName=svc",
            );
            assert!(result.is_ok());
            let grpc = result.unwrap().streamSettings.grpcSettings.unwrap();
            assert_eq!(grpc.multiMode, Some(false));
        }

        #[test]
        fn vless_reality_uses_tcp_network_by_default() {
            let result = create_outbound_object(
                "vless://uuid@example.com:443?security=reality&sni=google.com&pbk=pubkey&sid=sid&flow=xtls-rprx-vision",
            );
            assert!(result.is_ok());
            let outbound = result.unwrap();
            assert_eq!(outbound.streamSettings.network, Some("tcp".to_string()));
        }
    }
}
