pub mod data;
use crate::config_models::*;

pub fn create_outbound_settings(data: &RawData) -> OutboundSettings {
    let obfs = match (&data.obfs, &data.obfs_password) {
        (Some(obs_type), Some(obs_pass)) => Some(Hysteria2ObfsObject {
            r#type: Some(obs_type.clone()),
            password: Some(obs_pass.clone()),
        }),
        _ => None,
    };

    return OutboundSettings::Hysteria2(Hysteria2OutboundSettings {
        servers: vec![Hysteria2ServerObject {
            address: data.address.clone(),
            port: data.port,
            password: data.uuid.clone(),
            level: Some(0),
            obfs,
        }],
    });
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
}
