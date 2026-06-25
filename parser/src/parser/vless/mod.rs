pub mod data;
use crate::config_models::*;

pub fn create_outbound_settings(data: &RawData) -> OutboundSettings {
    return OutboundSettings::Vless(VlessOutboundSettings {
        vnext: vec![VnextServerObject {
            port: data.port,
            address: data.address.clone(),
            users: Some(vec![VnextUser {
                id: data.uuid.clone(),
                flow: data.flow.clone(),
                encryption: Some(data.encryption.clone().unwrap_or(String::from("none"))),
                level: Some(0),
                security: None,
            }]),
        }],
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_vless_data() -> RawData {
        RawData {
            remarks: String::new(),
            uuid: Some("my-uuid".to_string()),
            address: Some("example.com".to_string()),
            port: Some(443),
            flow: Some("xtls-rprx-vision".to_string()),
            encryption: Some("none".to_string()),
            security: Some("tls".to_string()),
            sni: Some("sni.example.com".to_string()),
            r#type: Some("ws".to_string()),
            pbk: None,
            sid: None,
            fp: None,
            path: Some("/ws".to_string()),
            host: Some("host.example.com".to_string()),
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
    fn creates_vless_settings() {
        let settings = create_outbound_settings(&sample_vless_data());
        match settings {
            OutboundSettings::Vless(s) => {
                assert_eq!(s.vnext.len(), 1);
                assert_eq!(s.vnext[0].address, Some("example.com".to_string()));
                assert_eq!(s.vnext[0].port, Some(443));
                let user = s.vnext[0].users.as_ref().unwrap();
                assert_eq!(user[0].id, Some("my-uuid".to_string()));
                assert_eq!(user[0].flow, Some("xtls-rprx-vision".to_string()));
                assert_eq!(user[0].encryption, Some("none".to_string()));
                assert_eq!(user[0].level, Some(0));
            }
            _ => panic!("Expected Vless settings"),
        }
    }

    #[test]
    fn defaults_encryption_to_none() {
        let mut data = sample_vless_data();
        data.encryption = None;
        let settings = create_outbound_settings(&data);
        match settings {
            OutboundSettings::Vless(s) => {
                let user = s.vnext[0].users.as_ref().unwrap();
                assert_eq!(user[0].encryption, Some("none".to_string()));
            }
            _ => panic!("Expected Vless settings"),
        }
    }
}