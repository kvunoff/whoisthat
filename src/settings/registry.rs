//! Single source of truth for settings metadata.
//!
//! The Go core knows nothing about sections, groups, labels or ordering: another
//! client can group the same settings differently. Values and writes stay in the
//! TUI/CLI layers; this module only describes the settings surface.

use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingsId {
    Autoconnect,
    AutoconnectMode,
    Autostart,
    Theme,
    ShowIp,
    VerboseTuiLog,
    LogLevel,
    TunName,
    KillSwitch,
    SplitTunnel,
    TestMethod,
    TestSamples,
    TestConcurrency,
    TestTimeout,
    TestEndpoint,
    AutoTestOnRefresh,
    HwidEnabled,
    Hwid,
    ResetHwid,
    UserAgent,
    CheckUpdates,
    FragmentEnabled,
    FragmentPackets,
    FragmentLength,
    FragmentInterval,
    NoiseEnabled,
    NoiseType,
    NoisePacket,
    NoiseDelay,
    TunMtu,
    DnsServers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SectionId {
    Connection,
    Appearance,
    Logging,
    ProfileTests,
    SubscriptionHwid,
    StartupUpdates,
    Network,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GroupId {
    Autoconnect,
    Display,
    Levels,
    Engine,
    Automation,
    Hwid,
    Headers,
    Service,
    Updates,
    Interface,
    LeakProtection,
    Dns,
    AntiDpiMasks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Toggle,
    Cycle,
    Editable,
    Action,
    Display,
}

/// `Local` = config.toml, `Core` = IPC/config.json, `CoreOptional` = a newer
/// core's optional block (absent means "unsupported (core)"), `System` = runtime
/// OS state and is always paired with a `Hook`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Local,
    Core,
    CoreOptional,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hook {
    None,
    Theme,
    Autostart,
    ResetHwid,
    NoiseType,
    ShowIp,
    Logging,
    TestConfig,
    Autoconnect,
    CheckUpdates,
    TunNameWait,
}

#[derive(Debug, Clone, Copy)]
pub struct SettingDef {
    pub id: SettingsId,
    pub section: SectionId,
    pub group: GroupId,
    pub label: &'static str,
    pub kind: Kind,
    pub source: Source,
    /// Empty for rows that have no CLI key (`Display`/`Action`/`System`).
    pub cli_key: &'static str,
    /// Cycled values; empty for non-`Cycle` rows.
    pub options: &'static [&'static str],
    pub hook: Hook,
}

#[derive(Debug, Clone, Copy)]
pub struct SectionDef {
    pub id: SectionId,
    pub title: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct GroupDef {
    pub id: GroupId,
    pub title: &'static str,
}

const AUTOCONNECT_MODES: &[&str] = &["proxy", "tun"];
const THEMES: &[&str] = &["graphite-dark", "graphite-light", "tokyo-night", "dracula"];
const LOG_LEVELS: &[&str] = &["error", "warn", "info", "debug", "trace"];
const SPLIT_MODES: &[&str] = &["off", "exclude", "include"];
const TEST_METHODS: &[&str] = &["tcp", "http-get", "http-head"];
const TEST_SAMPLES: &[&str] = &["1", "3", "5", "10"];
const TEST_CONCURRENCY: &[&str] = &["4", "8", "16", "32", "64"];
const TEST_TIMEOUT: &[&str] = &["3", "5", "10", "15"];
const TEST_ENDPOINTS: &[&str] = &["cloudflare", "gstatic", "bing"];
const FRAGMENT_PACKETS: &[&str] = &["tlshello", "1-1", "1-2", "1-3"];
const NOISE_TYPES: &[&str] = &["rand", "str", "base64", "hex"];

/// Commit 1 keeps the current flat order. Section/group are the target tree
/// layout but are not displayed until the tree lands.
pub static REGISTRY: &[SettingDef] = &[
    SettingDef {
        id: SettingsId::Autoconnect,
        section: SectionId::Connection,
        group: GroupId::Autoconnect,
        label: "Autoconnect",
        kind: Kind::Toggle,
        source: Source::Core,
        cli_key: "autoconnect",
        options: &[],
        hook: Hook::Autoconnect,
    },
    SettingDef {
        id: SettingsId::AutoconnectMode,
        section: SectionId::Connection,
        group: GroupId::Autoconnect,
        label: "Autoconnect mode",
        kind: Kind::Cycle,
        source: Source::Core,
        cli_key: "autoconnect-mode",
        options: AUTOCONNECT_MODES,
        hook: Hook::Autoconnect,
    },
    SettingDef {
        id: SettingsId::Autostart,
        section: SectionId::StartupUpdates,
        group: GroupId::Service,
        label: "Autostart",
        kind: Kind::Action,
        source: Source::System,
        cli_key: "",
        options: &[],
        hook: Hook::Autostart,
    },
    SettingDef {
        id: SettingsId::Theme,
        section: SectionId::Appearance,
        group: GroupId::Display,
        label: "Theme",
        kind: Kind::Cycle,
        source: Source::Local,
        cli_key: "theme",
        options: THEMES,
        hook: Hook::Theme,
    },
    SettingDef {
        id: SettingsId::ShowIp,
        section: SectionId::Appearance,
        group: GroupId::Display,
        label: "Show IP",
        kind: Kind::Toggle,
        source: Source::Local,
        cli_key: "show-ip",
        options: &[],
        hook: Hook::ShowIp,
    },
    SettingDef {
        id: SettingsId::VerboseTuiLog,
        section: SectionId::Logging,
        group: GroupId::Levels,
        label: "Verbose TUI log",
        kind: Kind::Toggle,
        source: Source::Local,
        cli_key: "log-enabled",
        options: &[],
        hook: Hook::Logging,
    },
    SettingDef {
        id: SettingsId::LogLevel,
        section: SectionId::Logging,
        group: GroupId::Levels,
        label: "Log level",
        kind: Kind::Cycle,
        source: Source::Local,
        cli_key: "log-level",
        options: LOG_LEVELS,
        hook: Hook::Logging,
    },
    SettingDef {
        id: SettingsId::TunName,
        section: SectionId::Network,
        group: GroupId::Interface,
        label: "TUN name",
        kind: Kind::Editable,
        source: Source::Core,
        cli_key: "tun-name",
        options: &[],
        hook: Hook::TunNameWait,
    },
    SettingDef {
        id: SettingsId::KillSwitch,
        section: SectionId::Network,
        group: GroupId::LeakProtection,
        label: "Kill Switch",
        kind: Kind::Toggle,
        source: Source::Core,
        cli_key: "kill-switch",
        options: &[],
        hook: Hook::None,
    },
    SettingDef {
        id: SettingsId::SplitTunnel,
        section: SectionId::Network,
        group: GroupId::LeakProtection,
        label: "Split tunnel",
        kind: Kind::Cycle,
        source: Source::Core,
        cli_key: "split-tunnel",
        options: SPLIT_MODES,
        hook: Hook::None,
    },
    SettingDef {
        id: SettingsId::TestMethod,
        section: SectionId::ProfileTests,
        group: GroupId::Engine,
        label: "Test method",
        kind: Kind::Cycle,
        source: Source::Local,
        cli_key: "test-method",
        options: TEST_METHODS,
        hook: Hook::None,
    },
    SettingDef {
        id: SettingsId::TestSamples,
        section: SectionId::ProfileTests,
        group: GroupId::Engine,
        label: "Test samples",
        kind: Kind::Cycle,
        source: Source::Core,
        cli_key: "test.samples",
        options: TEST_SAMPLES,
        hook: Hook::TestConfig,
    },
    SettingDef {
        id: SettingsId::TestConcurrency,
        section: SectionId::ProfileTests,
        group: GroupId::Engine,
        label: "Test concurrency",
        kind: Kind::Cycle,
        source: Source::Core,
        cli_key: "test.concurrency",
        options: TEST_CONCURRENCY,
        hook: Hook::TestConfig,
    },
    SettingDef {
        id: SettingsId::TestTimeout,
        section: SectionId::ProfileTests,
        group: GroupId::Engine,
        label: "Test timeout",
        kind: Kind::Cycle,
        source: Source::Core,
        cli_key: "test.timeout",
        options: TEST_TIMEOUT,
        hook: Hook::TestConfig,
    },
    SettingDef {
        id: SettingsId::TestEndpoint,
        section: SectionId::ProfileTests,
        group: GroupId::Engine,
        label: "Test endpoint",
        kind: Kind::Cycle,
        source: Source::Core,
        cli_key: "test.endpoint",
        options: TEST_ENDPOINTS,
        hook: Hook::TestConfig,
    },
    SettingDef {
        id: SettingsId::AutoTestOnRefresh,
        section: SectionId::ProfileTests,
        group: GroupId::Automation,
        label: "Auto-test on refresh",
        kind: Kind::Toggle,
        source: Source::Core,
        cli_key: "test.auto-on-subscribe",
        options: &[],
        hook: Hook::TestConfig,
    },
    SettingDef {
        id: SettingsId::HwidEnabled,
        section: SectionId::SubscriptionHwid,
        group: GroupId::Hwid,
        label: "HWID: Enabled",
        kind: Kind::Toggle,
        source: Source::Core,
        cli_key: "hwid-enabled",
        options: &[],
        hook: Hook::None,
    },
    SettingDef {
        id: SettingsId::Hwid,
        section: SectionId::SubscriptionHwid,
        group: GroupId::Hwid,
        label: "HWID",
        kind: Kind::Display,
        source: Source::Core,
        cli_key: "",
        options: &[],
        hook: Hook::None,
    },
    SettingDef {
        id: SettingsId::ResetHwid,
        section: SectionId::SubscriptionHwid,
        group: GroupId::Hwid,
        label: "Reset HWID",
        kind: Kind::Action,
        source: Source::Core,
        cli_key: "",
        options: &[],
        hook: Hook::ResetHwid,
    },
    SettingDef {
        id: SettingsId::UserAgent,
        section: SectionId::SubscriptionHwid,
        group: GroupId::Headers,
        label: "User-Agent",
        kind: Kind::Editable,
        source: Source::Core,
        cli_key: "user-agent",
        options: &[],
        hook: Hook::None,
    },
    SettingDef {
        id: SettingsId::CheckUpdates,
        section: SectionId::StartupUpdates,
        group: GroupId::Updates,
        label: "Check for updates",
        kind: Kind::Toggle,
        source: Source::Local,
        cli_key: "check-updates",
        options: &[],
        hook: Hook::CheckUpdates,
    },
    SettingDef {
        id: SettingsId::FragmentEnabled,
        section: SectionId::Network,
        group: GroupId::AntiDpiMasks,
        label: "Fragment: enabled",
        kind: Kind::Toggle,
        source: Source::CoreOptional,
        cli_key: "anti-dpi.fragment.enabled",
        options: &[],
        hook: Hook::None,
    },
    SettingDef {
        id: SettingsId::FragmentPackets,
        section: SectionId::Network,
        group: GroupId::AntiDpiMasks,
        label: "Fragment: packets",
        kind: Kind::Cycle,
        source: Source::CoreOptional,
        cli_key: "anti-dpi.fragment.packets",
        options: FRAGMENT_PACKETS,
        hook: Hook::None,
    },
    SettingDef {
        id: SettingsId::FragmentLength,
        section: SectionId::Network,
        group: GroupId::AntiDpiMasks,
        label: "Fragment: length",
        kind: Kind::Editable,
        source: Source::CoreOptional,
        cli_key: "anti-dpi.fragment.length",
        options: &[],
        hook: Hook::None,
    },
    SettingDef {
        id: SettingsId::FragmentInterval,
        section: SectionId::Network,
        group: GroupId::AntiDpiMasks,
        label: "Fragment: interval",
        kind: Kind::Editable,
        source: Source::CoreOptional,
        cli_key: "anti-dpi.fragment.interval",
        options: &[],
        hook: Hook::None,
    },
    SettingDef {
        id: SettingsId::NoiseEnabled,
        section: SectionId::Network,
        group: GroupId::AntiDpiMasks,
        label: "Noise: enabled",
        kind: Kind::Toggle,
        source: Source::CoreOptional,
        cli_key: "anti-dpi.noise.enabled",
        options: &[],
        hook: Hook::None,
    },
    SettingDef {
        id: SettingsId::NoiseType,
        section: SectionId::Network,
        group: GroupId::AntiDpiMasks,
        label: "Noise: type",
        kind: Kind::Cycle,
        source: Source::CoreOptional,
        cli_key: "anti-dpi.noise.type",
        options: NOISE_TYPES,
        hook: Hook::NoiseType,
    },
    SettingDef {
        id: SettingsId::NoisePacket,
        section: SectionId::Network,
        group: GroupId::AntiDpiMasks,
        label: "Noise: packet",
        kind: Kind::Editable,
        source: Source::CoreOptional,
        cli_key: "anti-dpi.noise.packet",
        options: &[],
        hook: Hook::None,
    },
    SettingDef {
        id: SettingsId::NoiseDelay,
        section: SectionId::Network,
        group: GroupId::AntiDpiMasks,
        label: "Noise: delay",
        kind: Kind::Editable,
        source: Source::CoreOptional,
        cli_key: "anti-dpi.noise.delay",
        options: &[],
        hook: Hook::None,
    },
    SettingDef {
        id: SettingsId::TunMtu,
        section: SectionId::Network,
        group: GroupId::Interface,
        label: "TUN MTU",
        kind: Kind::Editable,
        source: Source::CoreOptional,
        cli_key: "tun-mtu",
        options: &[],
        hook: Hook::None,
    },
    SettingDef {
        id: SettingsId::DnsServers,
        section: SectionId::Network,
        group: GroupId::Dns,
        label: "DNS servers",
        kind: Kind::Editable,
        source: Source::CoreOptional,
        cli_key: "dns-servers",
        options: &[],
        hook: Hook::None,
    },
];

pub static SECTIONS: &[SectionDef] = &[
    SectionDef {
        id: SectionId::Connection,
        title: "Connection",
    },
    SectionDef {
        id: SectionId::Appearance,
        title: "Appearance",
    },
    SectionDef {
        id: SectionId::Logging,
        title: "Logging",
    },
    SectionDef {
        id: SectionId::ProfileTests,
        title: "Profile tests",
    },
    SectionDef {
        id: SectionId::SubscriptionHwid,
        title: "Subscription & HWID",
    },
    SectionDef {
        id: SectionId::StartupUpdates,
        title: "Startup & updates",
    },
    SectionDef {
        id: SectionId::Network,
        title: "Network",
    },
];

static GROUPS: &[GroupDef] = &[
    GroupDef {
        id: GroupId::Autoconnect,
        title: "Autoconnect",
    },
    GroupDef {
        id: GroupId::Display,
        title: "Display",
    },
    GroupDef {
        id: GroupId::Levels,
        title: "Levels",
    },
    GroupDef {
        id: GroupId::Engine,
        title: "Engine",
    },
    GroupDef {
        id: GroupId::Automation,
        title: "Automation",
    },
    GroupDef {
        id: GroupId::Hwid,
        title: "HWID",
    },
    GroupDef {
        id: GroupId::Headers,
        title: "Headers",
    },
    GroupDef {
        id: GroupId::Service,
        title: "Service",
    },
    GroupDef {
        id: GroupId::Updates,
        title: "Updates",
    },
    GroupDef {
        id: GroupId::Interface,
        title: "Interface",
    },
    GroupDef {
        id: GroupId::LeakProtection,
        title: "Leak protection",
    },
    GroupDef {
        id: GroupId::Dns,
        title: "DNS",
    },
    GroupDef {
        id: GroupId::AntiDpiMasks,
        title: "Anti-DPI masks",
    },
];

pub fn all() -> impl Iterator<Item = &'static SettingDef> {
    REGISTRY.iter()
}

pub fn by_id(id: SettingsId) -> &'static SettingDef {
    REGISTRY
        .iter()
        .find(|d| d.id == id)
        .expect("every SettingsId has a registry entry")
}

pub fn by_cli_key(key: &str) -> Option<&'static SettingDef> {
    REGISTRY
        .iter()
        .find(|d| d.cli_key == key && !key.is_empty())
}

pub fn sections() -> &'static [SectionDef] {
    SECTIONS
}

/// Only declared groups are returned. `GroupId::Dns` is declared in PR 2.
pub fn groups(section: SectionId) -> Vec<&'static GroupDef> {
    let ids: &[GroupId] = match section {
        SectionId::Connection => &[GroupId::Autoconnect],
        SectionId::Appearance => &[GroupId::Display],
        SectionId::Logging => &[GroupId::Levels],
        SectionId::ProfileTests => &[GroupId::Engine, GroupId::Automation],
        SectionId::SubscriptionHwid => &[GroupId::Hwid, GroupId::Headers],
        SectionId::StartupUpdates => &[GroupId::Service, GroupId::Updates],
        SectionId::Network => &[
            GroupId::Interface,
            GroupId::LeakProtection,
            GroupId::Dns,
            GroupId::AntiDpiMasks,
        ],
    };
    ids.iter()
        .map(|id| {
            GROUPS
                .iter()
                .find(|g| g.id == *id)
                .expect("every declared group has a definition")
        })
        .collect()
}

pub fn items(section: SectionId, group: GroupId) -> impl Iterator<Item = &'static SettingDef> {
    REGISTRY
        .iter()
        .filter(move |d| d.section == section && d.group == group)
}

pub fn validate_registry() -> Result<(), String> {
    let mut ids = HashSet::new();
    let mut keys = HashSet::new();
    for d in REGISTRY {
        if !ids.insert(d.id) {
            return Err(format!("duplicate SettingsId {:?}", d.id));
        }
        if d.cli_key.is_empty() {
            if !matches!(d.kind, Kind::Action | Kind::Display) && d.source != Source::System {
                return Err(format!("{:?} has no CLI key", d.id));
            }
        } else if !keys.insert(d.cli_key) {
            return Err(format!("duplicate CLI key {}", d.cli_key));
        }
        if d.kind == Kind::Cycle && d.options.is_empty() {
            return Err(format!("{:?} is a Cycle without options", d.id));
        }
        if d.kind != Kind::Cycle && !d.options.is_empty() {
            return Err(format!("{:?} is not a Cycle but has options", d.id));
        }
        if d.source == Source::System && d.hook == Hook::None {
            return Err(format!("{:?} is System without a hook", d.id));
        }
        if d.source == Source::CoreOptional && d.cli_key.is_empty() {
            return Err(format!("{:?} cannot degrade without a CLI key", d.id));
        }
    }

    for section in sections() {
        if section.title.is_empty() {
            return Err(format!("{:?} has no title", section.id));
        }
        let declared = groups(section.id);
        if declared.is_empty() {
            return Err(format!("{:?} has no groups", section.id));
        }
        for group in &declared {
            if group.title.is_empty() {
                return Err(format!("{:?} has no title", group.id));
            }
            if items(section.id, group.id).next().is_none() {
                return Err(format!("{:?}/{:?} has no items", section.id, group.id));
            }
        }
    }

    for d in REGISTRY {
        if !groups(d.section).iter().any(|g| g.id == d.group) {
            return Err(format!(
                "{:?} belongs to an undeclared {:?}/{:?} group",
                d.id, d.section, d.group
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_valid() {
        validate_registry().unwrap();
    }

    #[test]
    fn registry_has_all_rows() {
        assert_eq!(REGISTRY.len(), 31);
        assert_eq!(ids().len(), 31);
    }

    fn ids() -> Vec<SettingsId> {
        REGISTRY.iter().map(|d| d.id).collect()
    }

    #[test]
    fn every_id_and_cli_key_resolves() {
        for d in REGISTRY {
            assert_eq!(by_id(d.id).id, d.id);
            if !d.cli_key.is_empty() {
                assert_eq!(by_cli_key(d.cli_key).unwrap().id, d.id);
            }
        }
    }

    #[test]
    fn every_row_is_reachable_from_the_tree() {
        let reachable: HashSet<SettingsId> = sections()
            .iter()
            .flat_map(|s| {
                groups(s.id)
                    .into_iter()
                    .flat_map(move |g| items(s.id, g.id).map(|d| d.id))
            })
            .collect();
        assert_eq!(reachable.len(), REGISTRY.len());
    }
}
