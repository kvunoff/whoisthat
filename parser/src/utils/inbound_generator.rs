use crate::config_models;

pub struct InboundGenerationOptions {
    pub socks_port: Option<u16>,
    pub http_port: Option<u16>,
}

pub fn generate_inbound_config(options: InboundGenerationOptions) -> Vec<config_models::Inbound> {
    let mut inbounds: Vec<config_models::Inbound> = vec![];
    match options.socks_port {
        Some(port) => {
            inbounds.push(generate_socks_inbound(port));
        }
        None => {}
    }

    match options.http_port {
        Some(port) => {
            inbounds.push(generate_http_inbound(port));
        }
        None => {}
    }

    return inbounds;
}

pub fn generate_http_inbound(http_port: u16) -> config_models::Inbound {
    return config_models::Inbound {
        protocol: String::from("http"),
        port: http_port,
        tag: String::from("http-in"),
        settings: None,
        listen: String::from("127.0.0.1"),
        sniffing: Some(config_models::SniffingSettings {
            enabled: Some(true),
            routeOnly: Some(true),
            metadataOnly: Some(false),
            domainsExcluded: None,
            destOverride: Some(vec![
                String::from("http"),
                String::from("tls"),
                String::from("quic"),
            ]),
        }),
    };
}

pub fn generate_socks_inbound(socks_port: u16) -> config_models::Inbound {
    return config_models::Inbound {
        protocol: String::from("socks"),
        port: socks_port,
        tag: String::from("socks-in"),
        listen: String::from("127.0.0.1"),
        settings: Some(config_models::InboundSettings { udp: true }),
        sniffing: Some(config_models::SniffingSettings {
            enabled: Some(true),
            routeOnly: Some(true),
            metadataOnly: Some(false),
            domainsExcluded: None,
            destOverride: Some(vec![
                String::from("http"),
                String::from("tls"),
                String::from("quic"),
            ]),
        }),
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    mod generate_socks_inbound_tests {
        use super::*;

        #[test]
        fn sets_correct_port() {
            let inbound = generate_socks_inbound(3090);
            assert_eq!(inbound.port, 3090);
        }

        #[test]
        fn has_socks_protocol() {
            let inbound = generate_socks_inbound(3090);
            assert_eq!(inbound.protocol, "socks");
        }

        #[test]
        fn has_socks_tag() {
            let inbound = generate_socks_inbound(3090);
            assert_eq!(inbound.tag, "socks-in");
        }

        #[test]
        fn has_sniffing_enabled() {
            let inbound = generate_socks_inbound(3090);
            let sniffing = inbound.sniffing.unwrap();
            assert_eq!(sniffing.enabled, Some(true));
            assert_eq!(sniffing.routeOnly, Some(true));
            assert_eq!(sniffing.metadataOnly, Some(false));
        }

        #[test]
        fn has_udp_enabled() {
            let inbound = generate_socks_inbound(3090);
            let settings = inbound.settings.unwrap();
            assert!(settings.udp);
        }
    }

    mod generate_http_inbound_tests {
        use super::*;

        #[test]
        fn sets_correct_port() {
            let inbound = generate_http_inbound(3091);
            assert_eq!(inbound.port, 3091);
        }

        #[test]
        fn has_http_protocol_and_tag() {
            let inbound = generate_http_inbound(3091);
            assert_eq!(inbound.protocol, "http");
            assert_eq!(inbound.tag, "http-in");
        }

        #[test]
        fn has_sniffing_without_udp() {
            let inbound = generate_http_inbound(3091);
            assert!(inbound.sniffing.is_some());
            assert!(inbound.settings.is_none());
        }
    }

    mod generate_inbound_config_tests {
        use super::*;

        #[test]
        fn both_ports_returns_two_inbounds() {
            let config = generate_inbound_config(InboundGenerationOptions {
                socks_port: Some(3090),
                http_port: Some(3091),
            });
            assert_eq!(config.len(), 2);
            assert_eq!(config[0].protocol, "socks");
            assert_eq!(config[1].protocol, "http");
        }

        #[test]
        fn only_socks_returns_one() {
            let config = generate_inbound_config(InboundGenerationOptions {
                socks_port: Some(3090),
                http_port: None,
            });
            assert_eq!(config.len(), 1);
            assert_eq!(config[0].protocol, "socks");
        }

        #[test]
        fn only_http_returns_one() {
            let config = generate_inbound_config(InboundGenerationOptions {
                socks_port: None,
                http_port: Some(3091),
            });
            assert_eq!(config.len(), 1);
            assert_eq!(config[0].protocol, "http");
        }

        #[test]
        fn neither_returns_empty() {
            let config = generate_inbound_config(InboundGenerationOptions {
                socks_port: None,
                http_port: None,
            });
            assert_eq!(config.len(), 0);
        }
    }
}