pub mod data;
use crate::config_models::OutboundSettings;
use crate::config_models::RawData;
use serde::{Deserialize, Serialize};

// xray-style outbound settings — still produced for metadata/test paths that
// expect the xray JSON shape. The actual hysteria2 client subprocess now
// consumes `create_hysteria2_client_yaml()` output instead.
pub fn create_outbound_settings(data: &RawData) -> OutboundSettings {
    let obfs = match (&data.obfs, &data.obfs_password) {
        (Some(obs_type), Some(obs_pass)) => Some(crate::config_models::Hysteria2ObfsObject {
            r#type: Some(obs_type.clone()),
            password: Some(obs_pass.clone()),
        }),
        _ => None,
    };

    return OutboundSettings::Hysteria2(crate::config_models::Hysteria2OutboundSettings {
        servers: vec![crate::config_models::Hysteria2ServerObject {
            address: data.address.clone(),
            port: data.port,
            password: data.uuid.clone(),
            level: Some(0),
            obfs,
        }],
    });
}

// ---------------------------------------------------------------------------
// YAML config for the official hysteria2 client (apernet/hysteria2).
//
// The hysteria2 client reads a YAML config. We emit the minimal subset that
// the URI scheme can express, then let hysteria2's defaults cover the rest.
// Produced YAML example:
//
//   server: example.com:443
//   auth: my-password
//   tls:
//     sni: sni.example.com
//     insecure: false
//     alpn:
//       - h3
//   obfs:
//     type: salamander
//     salamander:
//       password: secret
//   bandwidth:
//     up: 100 mbps
//     down: 100 mbps
//   ports: 20000-30000
//   socks5:
//     listen: 127.0.0.1:3090
//   http:
//     listen: 127.0.0.1:3091
// ---------------------------------------------------------------------------

// serde_yaml's defaults on Option<T> already skip None fields when
// `skip_serializing_if = "Option::is_none"` is used. We annotate every
// optional field so the output stays clean.

fn is_none<T>(o: &Option<T>) -> bool {
    o.is_none()
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Hysteria2ClientTLS {
    #[serde(skip_serializing_if = "is_none", default)]
    pub sni: Option<String>,
    #[serde(skip_serializing_if = "is_none", default)]
    pub insecure: Option<bool>,
    #[serde(skip_serializing_if = "is_none", default)]
    pub alpn: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Hysteria2ClientSalamander {
    #[serde(skip_serializing_if = "is_none", default)]
    pub password: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Hysteria2ClientObfs {
    #[serde(rename = "type", skip_serializing_if = "is_none", default)]
    pub obfs_type: Option<String>,
    #[serde(skip_serializing_if = "is_none", default)]
    pub salamander: Option<Hysteria2ClientSalamander>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Hysteria2ClientBandwidth {
    #[serde(skip_serializing_if = "is_none", default)]
    pub up: Option<String>,
    #[serde(skip_serializing_if = "is_none", default)]
    pub down: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Hysteria2ClientListen {
    pub listen: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Hysteria2ClientConfig {
    pub server: String,
    pub auth: String,
    #[serde(skip_serializing_if = "is_none", default)]
    pub tls: Option<Hysteria2ClientTLS>,
    #[serde(skip_serializing_if = "is_none", default)]
    pub obfs: Option<Hysteria2ClientObfs>,
    #[serde(skip_serializing_if = "is_none", default)]
    pub bandwidth: Option<Hysteria2ClientBandwidth>,
    #[serde(skip_serializing_if = "is_none", default)]
    pub ports: Option<String>,
    pub socks5: Hysteria2ClientListen,
    #[serde(skip_serializing_if = "is_none", default)]
    pub http: Option<Hysteria2ClientListen>,
}

pub fn build_client_config(data: &RawData, socks_port: u16, http_port: Option<u16>) -> Hysteria2ClientConfig {
    let server = match (&data.address, data.port) {
        (Some(addr), Some(port)) => format!("{}:{}", addr, port),
        (Some(addr), None) => addr.clone(),
        _ => String::new(),
    };

    let alpn_vec = data.alpn.as_ref().map(|a| {
        a.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    });

    let allow_insecure =
        data.allowInsecure == Some(String::from("true")) || data.allowInsecure == Some(String::from("1"));

    let tls = if data.sni.is_some() || allow_insecure || alpn_vec.is_some() {
        Some(Hysteria2ClientTLS {
            sni: data.sni.clone(),
            insecure: Some(allow_insecure),
            alpn: alpn_vec,
        })
    } else {
        None
    };

    let obfs = match (&data.obfs, &data.obfs_password) {
        (Some(t), pw) => Some(Hysteria2ClientObfs {
            obfs_type: Some(t.clone()),
            salamander: if t == "salamander" {
                Some(Hysteria2ClientSalamander {
                    password: pw.clone(),
                })
            } else {
                None
            },
        }),
        _ => None,
    };

    let bandwidth = if data.up.is_some() || data.down.is_some() {
        Some(Hysteria2ClientBandwidth {
            up: data.up.clone(),
            down: data.down.clone(),
        })
    } else {
        None
    };

    Hysteria2ClientConfig {
        server,
        auth: data.uuid.clone().unwrap_or_default(),
        tls,
        obfs,
        bandwidth,
        ports: data.ports.clone(),
        socks5: Hysteria2ClientListen {
            listen: format!("127.0.0.1:{}", socks_port),
        },
        http: http_port.map(|p| Hysteria2ClientListen {
            listen: format!("127.0.0.1:{}", p),
        }),
    }
}

pub fn create_client_yaml(data: &RawData, socks_port: u16, http_port: Option<u16>) -> Result<String, String> {
    let cfg = build_client_config(data, socks_port, http_port);
    serde_yaml::to_string(&cfg).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_hysteria2_data() -> RawData {
        RawData {
            remarks: String::new(),
            uuid: Some("my-password".to_string()),
            address: Some("example.com".to_string()),
            port: Some(443),
            security: Some("tls".to_string()),
            sni: Some("sni.example.com".to_string()),
            flow: None,
            encryption: None,
            r#type: Some("tcp".to_string()),
            pbk: None,
            sid: None,
            fp: None,
            path: None,
            host: None,
            alpn: None,
            authority: None,
            header_type: None,
            allowInsecure: None,
            key: None,
            quic_security: None,
            mode: None,
            service_name: None,
            seed: None,
            slpn: None,
            spx: None,
            extra: None,
            vnext_security: None,
            server_method: None,
            username: None,
            obfs: Some("salamander".to_string()),
            obfs_password: Some("obfs-secret".to_string()),
            up: None,
            down: None,
            ports: None,
        }
    }

    #[test]
    fn creates_hysteria2_settings() {
        let settings = create_outbound_settings(&sample_hysteria2_data());
        match settings {
            OutboundSettings::Hysteria2(s) => {
                assert_eq!(s.servers.len(), 1);
                assert_eq!(s.servers[0].address, Some("example.com".to_string()));
                assert_eq!(s.servers[0].port, Some(443));
                assert_eq!(s.servers[0].password, Some("my-password".to_string()));
                assert_eq!(s.servers[0].level, Some(0));
                let obfs = s.servers[0].obfs.as_ref().unwrap();
                assert_eq!(obfs.r#type, Some("salamander".to_string()));
                assert_eq!(obfs.password, Some("obfs-secret".to_string()));
            }
            _ => panic!("Expected Hysteria2 settings"),
        }
    }

    #[test]
    fn creates_hysteria2_settings_without_obfs() {
        let mut data = sample_hysteria2_data();
        data.obfs = None;
        data.obfs_password = None;
        let settings = create_outbound_settings(&data);
        match settings {
            OutboundSettings::Hysteria2(s) => {
                assert_eq!(s.servers[0].obfs, None);
            }
            _ => panic!("Expected Hysteria2 settings"),
        }
    }

    #[test]
    fn build_client_config_minimal() {
        let mut d = sample_hysteria2_data();
        d.obfs = None;
        d.obfs_password = None;
        d.sni = None;
        d.security = None;
        let cfg = build_client_config(&d, 3090, Some(3091));
        assert_eq!(cfg.server, "example.com:443");
        assert_eq!(cfg.auth, "my-password");
        assert!(cfg.tls.is_none());
        assert!(cfg.obfs.is_none());
        assert_eq!(cfg.socks5.listen, "127.0.0.1:3090");
        assert_eq!(cfg.http.as_ref().unwrap().listen, "127.0.0.1:3091");
    }

    #[test]
    fn build_client_config_with_obfs_and_tls() {
        let d = sample_hysteria2_data();
        let cfg = build_client_config(&d, 3090, None);
        let tls = cfg.tls.unwrap();
        assert_eq!(tls.sni, Some("sni.example.com".to_string()));
        assert!(!tls.insecure.unwrap());
        let obfs = cfg.obfs.unwrap();
        assert_eq!(obfs.obfs_type, Some("salamander".to_string()));
        assert_eq!(obfs.salamander.unwrap().password, Some("obfs-secret".to_string()));
        assert!(cfg.http.is_none());
    }

    #[test]
    fn build_client_config_insecure_from_alias() {
        let mut d = sample_hysteria2_data();
        d.allowInsecure = Some("1".to_string());
        let cfg = build_client_config(&d, 3090, None);
        assert!(cfg.tls.unwrap().insecure.unwrap());
    }

    #[test]
    fn build_client_config_splits_alpn_csv() {
        let mut d = sample_hysteria2_data();
        d.alpn = Some("h2,http/1.1".to_string());
        let cfg = build_client_config(&d, 3090, None);
        assert_eq!(cfg.tls.unwrap().alpn.unwrap(), vec!["h2".to_string(), "http/1.1".to_string()]);
    }

    #[test]
    fn build_client_config_with_bandwidth_and_ports() {
        let mut d = sample_hysteria2_data();
        d.up = Some("100 mbps".to_string());
        d.down = Some("200 mbps".to_string());
        d.ports = Some("20000-30000".to_string());
        let cfg = build_client_config(&d, 3090, None);
        let bw = cfg.bandwidth.unwrap();
        assert_eq!(bw.up, Some("100 mbps".to_string()));
        assert_eq!(bw.down, Some("200 mbps".to_string()));
        assert_eq!(cfg.ports, Some("20000-30000".to_string()));
    }

    #[test]
    fn create_client_yaml_is_valid_yaml() {
        let d = sample_hysteria2_data();
        let yaml = create_client_yaml(&d, 3090, Some(3091)).unwrap();
        assert!(yaml.contains("server: example.com:443"));
        assert!(yaml.contains("auth: my-password"));
        assert!(yaml.contains("sni: sni.example.com"));
        assert!(yaml.contains("127.0.0.1:3090"));
        assert!(yaml.contains("127.0.0.1:3091"));
        assert!(yaml.contains("type: salamander"));
    }
}