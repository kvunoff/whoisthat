use crate::core_client::protocol::{RoutingConfig, RoutingRule};

#[derive(Debug, Clone)]
pub struct PresetRule {
    pub match_type: &'static str,
    pub value: &'static str,
    pub outbound: &'static str,
}

impl PresetRule {
    pub fn to_routing_rule(&self) -> RoutingRule {
        let mut rule = RoutingRule {
            r#type: "field".into(),
            outbound_tag: self.outbound.into(),
            enabled: true,
            ..Default::default()
        };
        match self.match_type {
            "geoip" | "ip" => rule.ip = self.value.into(),
            "geosite" | "domain" => rule.domain = self.value.into(),
            "protocol" => rule.protocol = self.value.into(),
            "port" => rule.port = self.value.into(),
            _ => {}
        }
        rule
    }

    pub fn matches_rule(&self, r: &RoutingRule) -> bool {
        if r.outbound_tag != self.outbound {
            return false;
        }
        match self.match_type {
            "geoip" | "ip" => r.ip == self.value,
            "geosite" | "domain" => r.domain == self.value,
            "protocol" => r.protocol == self.value,
            "port" => r.port == self.value,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RoutingPreset {
    #[allow(dead_code)]
    pub id: &'static str,
    pub title: &'static str,
    pub desc: &'static str,
    pub rules: &'static [PresetRule],
}

impl RoutingPreset {
    pub fn is_applied(&self, config: &RoutingConfig) -> bool {
        self.rules
            .iter()
            .all(|pr| config.rules.iter().any(|r| r.enabled && pr.matches_rule(r)))
    }
}

pub static ROUTING_PRESETS: &[RoutingPreset] = &[
    RoutingPreset {
        id: "bypass_ru",
        title: "🇷🇺 Bypass Russia",
        desc: "Direct access to Russian domains, banks, government & services bypassing VPN",
        rules: &[
            PresetRule {
                match_type: "geosite",
                value: "geosite:category-ru",
                outbound: "direct",
            },
            PresetRule {
                match_type: "geoip",
                value: "geoip:ru",
                outbound: "direct",
            },
        ],
    },
    RoutingPreset {
        id: "block_ads",
        title: "🛡️ Block Ads & Trackers",
        desc: "Global ad, tracker and telemetry blocking via Xray blackhole",
        rules: &[PresetRule {
            match_type: "geosite",
            value: "geosite:category-ads-all",
            outbound: "block",
        }],
    },
    RoutingPreset {
        id: "bypass_lan",
        title: "🏠 Bypass LAN / Local",
        desc: "Direct access to local network, router and RFC1918 subnets without proxying",
        rules: &[PresetRule {
            match_type: "geoip",
            value: "geoip:private",
            outbound: "direct",
        }],
    },
    RoutingPreset {
        id: "proxy_media",
        title: "📺 Proxy Media (YouTube, Discord, Twitch)",
        desc: "Route video streaming and voice communication platforms through proxy",
        rules: &[
            PresetRule {
                match_type: "geosite",
                value: "geosite:youtube",
                outbound: "proxy",
            },
            PresetRule {
                match_type: "geosite",
                value: "geosite:discord",
                outbound: "proxy",
            },
            PresetRule {
                match_type: "geosite",
                value: "geosite:twitch",
                outbound: "proxy",
            },
        ],
    },
    RoutingPreset {
        id: "proxy_telegram",
        title: "💬 Proxy Telegram",
        desc: "Route all Telegram domains and official IP CIDR ranges through proxy",
        rules: &[
            PresetRule {
                match_type: "geosite",
                value: "geosite:telegram",
                outbound: "proxy",
            },
            PresetRule {
                match_type: "geoip",
                value: "geoip:telegram",
                outbound: "proxy",
            },
        ],
    },
    RoutingPreset {
        id: "proxy_ai",
        title: "🤖 Proxy AI Services (OpenAI, Claude)",
        desc: "Route ChatGPT, Claude, and major AI platforms through proxy",
        rules: &[
            PresetRule {
                match_type: "geosite",
                value: "geosite:openai",
                outbound: "proxy",
            },
            PresetRule {
                match_type: "geosite",
                value: "geosite:anthropic",
                outbound: "proxy",
            },
        ],
    },
    RoutingPreset {
        id: "proxy_socials",
        title: "📸 Proxy Blocked Socials (X/Twitter, Instagram, Meta)",
        desc: "Route blocked social media platforms through proxy",
        rules: &[
            PresetRule {
                match_type: "geosite",
                value: "geosite:twitter",
                outbound: "proxy",
            },
            PresetRule {
                match_type: "geosite",
                value: "geosite:instagram",
                outbound: "proxy",
            },
            PresetRule {
                match_type: "geosite",
                value: "geosite:facebook",
                outbound: "proxy",
            },
        ],
    },
    RoutingPreset {
        id: "block_torrent",
        title: "🛑 Block BitTorrent / P2P",
        desc: "Block P2P torrent protocol to protect VPS from DMCA/ISP abuse complaints",
        rules: &[PresetRule {
            match_type: "protocol",
            value: "bittorrent",
            outbound: "block",
        }],
    },
    RoutingPreset {
        id: "direct_torrent",
        title: "⚡ Direct BitTorrent / P2P",
        desc: "Download torrents directly at maximum ISP speed bypassing VPN",
        rules: &[PresetRule {
            match_type: "protocol",
            value: "bittorrent",
            outbound: "direct",
        }],
    },
    RoutingPreset {
        id: "bypass_cn",
        title: "🇨🇳 Bypass China",
        desc: "Direct access to Chinese websites and services (geosite:cn + geoip:cn)",
        rules: &[
            PresetRule {
                match_type: "geosite",
                value: "geosite:cn",
                outbound: "direct",
            },
            PresetRule {
                match_type: "geoip",
                value: "geoip:cn",
                outbound: "direct",
            },
        ],
    },
    RoutingPreset {
        id: "proxy_steam",
        title: "🎮 Proxy Gaming (Steam Community)",
        desc: "Route Steam Community and gaming services through proxy",
        rules: &[PresetRule {
            match_type: "geosite",
            value: "geosite:steam",
            outbound: "proxy",
        }],
    },
];

pub fn apply_preset(config: &mut RoutingConfig, preset: &RoutingPreset) {
    for pr in preset.rules {
        if let Some(existing) = config.rules.iter_mut().find(|r| pr.matches_rule(r)) {
            existing.enabled = true;
        } else {
            config.rules.push(pr.to_routing_rule());
        }
    }
}

pub fn remove_preset(config: &mut RoutingConfig, preset: &RoutingPreset) {
    config
        .rules
        .retain(|r| !preset.rules.iter().any(|pr| pr.matches_rule(r)));
}

pub fn toggle_preset(config: &mut RoutingConfig, preset: &RoutingPreset) -> bool {
    if preset.is_applied(config) {
        remove_preset(config, preset);
        false
    } else {
        apply_preset(config, preset);
        true
    }
}
