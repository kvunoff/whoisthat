pub mod data;
use crate::config_models::*;

pub fn create_outbound_settings(data: &RawData) -> OutboundSettings {
    return OutboundSettings::Vmess(VmessOutboundSettings {
        vnext: vec![VnextServerObject {
            port: data.port,
            address: data.address.clone(),
            users: Some(vec![VnextUser {
                id: data.uuid.clone(),
                flow: data.flow.clone(),
                encryption: Some(data.encryption.clone().unwrap_or(String::from("none"))),
                level: Some(0),
                security: data.vnext_security.clone(),
            }]),
        }],
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_vmess_data() -> RawData {
        RawData {
            remarks: String::new(),
            uuid: Some("my-uuid".to_string()),
            address: Some("example.com".to_string()),
            port: Some(443),
            flow: None,
            encryption: None,
            vnext_security: Some("auto".to_string()),
            security: None,
            sni: None,
            r#type: None,
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
            server_method: None,
            username: None,
        }
    }

    #[test]
    fn creates_vmess_settings() {
        let settings = create_outbound_settings(&sample_vmess_data());
        match settings {
            OutboundSettings::Vmess(s) => {
                assert_eq!(s.vnext.len(), 1);
                assert_eq!(s.vnext[0].address, Some("example.com".to_string()));
                assert_eq!(s.vnext[0].port, Some(443));
                let user = s.vnext[0].users.as_ref().unwrap();
                assert_eq!(user[0].id, Some("my-uuid".to_string()));
                assert_eq!(user[0].security, Some("auto".to_string()));
            }
            _ => panic!("Expected Vmess settings"),
        }
    }

    #[test]
    fn preserves_vnext_security() {
        let settings = create_outbound_settings(&sample_vmess_data());
        match settings {
            OutboundSettings::Vmess(s) => {
                let user = s.vnext[0].users.as_ref().unwrap();
                assert_eq!(user[0].security, Some("auto".to_string()));
            }
            _ => panic!("Expected Vmess settings"),
        }
    }

    #[test]
    fn defaults_encryption_to_none() {
        let data = sample_vmess_data();
        let settings = create_outbound_settings(&data);
        match settings {
            OutboundSettings::Vmess(s) => {
                let user = s.vnext[0].users.as_ref().unwrap();
                assert_eq!(user[0].encryption, Some("none".to_string()));
            }
            _ => panic!("Expected Vmess settings"),
        }
    }
}