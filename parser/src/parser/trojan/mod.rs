pub mod data;
use crate::config_models::*;

pub fn create_outbound_settings(data: &RawData) -> OutboundSettings {
    return OutboundSettings::Trojan(TrojanOutboundSettings {
        servers: vec![TrojanServerObject {
            address: data.address.clone(),
            port: data.port,
            password: data.uuid.clone(),
            level: Some(0),
        }],
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_trojan_data() -> RawData {
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
        }
    }

    #[test]
    fn creates_trojan_settings() {
        let settings = create_outbound_settings(&sample_trojan_data());
        match settings {
            OutboundSettings::Trojan(s) => {
                assert_eq!(s.servers.len(), 1);
                assert_eq!(s.servers[0].address, Some("example.com".to_string()));
                assert_eq!(s.servers[0].port, Some(443));
                assert_eq!(s.servers[0].password, Some("my-password".to_string()));
                assert_eq!(s.servers[0].level, Some(0));
            }
            _ => panic!("Expected Trojan settings"),
        }
    }
}