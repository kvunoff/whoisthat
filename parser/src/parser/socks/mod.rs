pub mod data;
mod models;
use crate::config_models::*;

pub fn create_outbound_settings(data: &RawData) -> OutboundSettings {
    return OutboundSettings::Socks(SocksOutboundSettings {
        servers: vec![SocksServerObject {
            users: match (&data.username, &data.uuid) {
                (Some(username), Some(uuid)) => Some(vec![SocksUser {
                    user: Some(username.clone()),
                    pass: Some(uuid.clone()),
                }]),
                _ => None,
            },
            address: data.address.clone(),
            port: data.port,
            level: Some(0),
        }],
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_socks_data_with_auth() -> RawData {
        RawData {
            remarks: String::new(),
            username: Some("user".to_string()),
            uuid: Some("pass".to_string()),
            address: Some("example.com".to_string()),
            port: Some(1080),
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
            server_method: None,
            obfs: None,
            obfs_password: None,
        }
    }

    fn sample_socks_data_no_auth() -> RawData {
        RawData {
            username: None,
            uuid: None,
            ..sample_socks_data_with_auth()
        }
    }

    #[test]
    fn creates_socks_settings_with_auth() {
        let settings = create_outbound_settings(&sample_socks_data_with_auth());
        match settings {
            OutboundSettings::Socks(s) => {
                assert_eq!(s.servers.len(), 1);
                assert_eq!(s.servers[0].address, Some("example.com".to_string()));
                assert_eq!(s.servers[0].port, Some(1080));
                let users = s.servers[0].users.as_ref().unwrap();
                assert_eq!(users[0].user, Some("user".to_string()));
                assert_eq!(users[0].pass, Some("pass".to_string()));
            }
            _ => panic!("Expected Socks settings"),
        }
    }

    #[test]
    fn creates_socks_settings_without_auth() {
        let settings = create_outbound_settings(&sample_socks_data_no_auth());
        match settings {
            OutboundSettings::Socks(s) => {
                assert_eq!(s.servers[0].users, None);
            }
            _ => panic!("Expected Socks settings"),
        }
    }

    #[test]
    fn no_users_when_only_username_present() {
        let mut data = sample_socks_data_with_auth();
        data.uuid = None;
        let settings = create_outbound_settings(&data);
        match settings {
            OutboundSettings::Socks(s) => {
                assert_eq!(s.servers[0].users, None);
            }
            _ => panic!("Expected Socks settings"),
        }
    }
}