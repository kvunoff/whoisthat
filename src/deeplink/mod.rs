mod actions;
mod desktop;
mod parse;

pub use actions::{run_deeplink, run_deeplink_action};
pub use desktop::{ensure_desktop_registered, notify_desktop, register_desktop_handler};
pub use parse::parse_deeplink;

/// Represents an action requested via a `wit://` deep link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeepLinkAction {
    /// Add a subscription URL and auto-update it.
    AddSubscription { name: String, url: String },
    /// Import one or more profile URIs (e.g. vless://, ss://, etc.).
    AddProfile {
        uri: String,
        group_name: Option<String>,
    },
    /// Connect to VPN.
    Connect,
    /// Disconnect from VPN.
    Disconnect,
    /// Toggle VPN connection.
    Toggle,
    /// Import Happ routing link (happ://routing/add/..., onadd/..., off).
    ImportHappRouting { link: String, group: Option<String> },
}
