use crate::config_models::{
    self, ConfigMetaData, GRPCSettings, KCPSettings, NonHeaderObject, Outbound, OutboundSettings,
    QuicSettings, RawData, RealitySettings, StreamSettings, TCPHeader, TCPSettings, TlsSettings,
    WsSettings, XHTTPSettings,
};
use crate::utils::{inbound_generator, parse_raw_json};

mod shadow_socks;
mod socks;
mod trojan;
mod uri_identifier;
mod vless;
mod vmess;

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

pub fn create_outbound_object(uri: &str) -> Result<config_models::Outbound, String> {
    let (name, data, outbound_settings) = get_uri_data(uri)?;

    let network_type = data.r#type.as_deref().unwrap_or("");
    let allow_insecure = data.allowInsecure == Some(String::from("true"))
        || data.allowInsecure == Some(String::from("1"));

    let outbound = Outbound {
        protocol: name,
        tag: String::from("proxy"),
        streamSettings: StreamSettings {
            network: data.r#type.clone(),
            security: data.security.clone(),
            tlsSettings: match data.security.as_deref() {
                Some("tls") => Some(TlsSettings {
                    alpn: data.alpn.map(|alpn| vec![alpn]),
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
            realitySettings: match data.security.as_deref() {
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
                    multiMode: Some(false),
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
		Some(_) => Err("The protocol was recognized but is not supported yet".to_string()),
		None => Err("The protocol is not supported".to_string()),
	}
}
