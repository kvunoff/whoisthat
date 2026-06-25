pub mod data;
mod models;
use crate::config_models::*;

pub fn create_outbound_settings(data: &RawData) -> OutboundSettings {
    return OutboundSettings::ShadowSocks(ShadowSocksOutboundSettings {
        servers: vec![ShadowSocksServerObject {
            address: data.address.clone(),
            port: data.port,
            password: data.uuid.clone(),
            level: Some(0),
            method: data.server_method.clone(),
        }],
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_ss_data() -> RawData {
        RawData {
            remarks: String::new(),
            server_method: Some("aes-256-gcm".to_string()),
            address: Some("example.com".to_string()),
            port: Some(8388),
            uuid: Some("secretpw".to_string()),
            r#type: Some("tcp".to_string()),
            security: None,
            sni: None,
            fp: None,
            pbk: None,
            sid: None,
            flow: None,
            key: None,
            spx: None,
            path: None,
            host: None,
            seed: None,
            mode: None,
            slpn: None,
            alpn: None,
            extra: None,
            authority: None,
            encryption: None,
            header_type: None,
            service_name: None,
            quic_security: None,
            allowInsecure: None,
            vnext_security: None,
            username: None,
            obfs: None,
            obfs_password: None,
        }
    }

    #[test]
    fn creates_shadowsocks_settings() {
        let settings = create_outbound_settings(&sample_ss_data());
        match settings {
            OutboundSettings::ShadowSocks(s) => {
                assert_eq!(s.servers.len(), 1);
                assert_eq!(s.servers[0].address, Some("example.com".to_string()));
                assert_eq!(s.servers[0].port, Some(8388));
                assert_eq!(s.servers[0].password, Some("secretpw".to_string()));
                assert_eq!(s.servers[0].method, Some("aes-256-gcm".to_string()));
                assert_eq!(s.servers[0].level, Some(0));
            }
            _ => panic!("Expected ShadowSocks settings"),
        }
    }
}