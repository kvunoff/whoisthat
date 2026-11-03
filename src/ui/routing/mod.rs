mod form;
mod popups;
mod presets;
mod tab;

#[cfg(test)]
mod tests;

pub use form::{form_to_rule, rule_to_form};
pub use popups::render_routing_popup;
pub use presets::*;
pub use tab::render_routing_tab;

pub(super) const TYPE_LABELS: &[&str] = &["domain", "ip", "protocol", "port", "geoip", "geosite"];
pub(super) const OUTBOUND_LABELS: &[&str] = &["proxy", "direct", "block"];

#[derive(Debug, Clone, PartialEq)]
pub enum RoutingPopup {
    Add {
        match_type: usize,
        value: String,
        outbound: usize,
        cursor: usize,
        field: usize,
    },
    Edit {
        index: usize,
        match_type: usize,
        value: String,
        outbound: usize,
        cursor: usize,
        field: usize,
    },
    ConfirmDelete {
        index: usize,
    },
    Presets {
        cursor: usize,
    },
    HappProfiles {
        cursor: usize,
    },
}
