pub mod actions;

pub use actions::AppAction;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyContext {
    Global,
    ProfilesTree,
    ProfilesDetails,
    Routing,
    Settings,
    Logs,
    Traffic,
}

pub struct HelpEntry {
    pub key: &'static str,
    pub desc: &'static str,
}

pub fn resolve_action(ctx: KeyContext, key: &KeyEvent) -> AppAction {
    // 1. Direct Critical Exits (checked unconditionally across non-text contexts)
    if (key.code == KeyCode::Char('Q')
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)))
        && !key.modifiers.contains(KeyModifiers::ALT)
    {
        return AppAction::Quit;
    }

    // 2. Context-specific mappings (highest priority to avoid shadowing)
    let ctx_action = match ctx {
        KeyContext::ProfilesTree => resolve_profiles_tree(key),
        KeyContext::ProfilesDetails => resolve_profiles_details(key),
        KeyContext::Routing => resolve_routing(key),
        KeyContext::Settings => resolve_settings(key),
        KeyContext::Logs => resolve_logs(key),
        KeyContext::Traffic => resolve_traffic(key),
        KeyContext::Global => AppAction::None,
    };

    if ctx_action != AppAction::None {
        return ctx_action;
    }

    // 3. Global mappings fallback (only if not handled by the active panel)
    resolve_global(key)
}

fn resolve_profiles_tree(key: &KeyEvent) -> AppAction {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    // Fast paging
    if ctrl {
        return match key.code {
            KeyCode::Char('d') => AppAction::PageDown,
            KeyCode::Char('u') => AppAction::PageUp,
            _ => AppAction::None,
        };
    }

    // Item reordering motions
    if (key.code == KeyCode::Char('J') && !alt && !ctrl)
        || ((key.code == KeyCode::Down || key.code == KeyCode::Char('j')) && (alt || shift))
    {
        return AppAction::MoveItemDown;
    }
    if (key.code == KeyCode::Char('K') && !alt && !ctrl)
        || ((key.code == KeyCode::Up || key.code == KeyCode::Char('k')) && (alt || shift))
    {
        return AppAction::MoveItemUp;
    }

    // Single keys
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => AppAction::CursorDown,
        KeyCode::Char('k') | KeyCode::Up => AppAction::CursorUp,
        KeyCode::Char('h') | KeyCode::Left => AppAction::CursorLeft,
        KeyCode::Char('l') | KeyCode::Right => AppAction::CursorRight,
        KeyCode::Tab | KeyCode::BackTab => AppAction::ToggleFocus,
        KeyCode::PageDown => AppAction::PageDown,
        KeyCode::PageUp => AppAction::PageUp,
        KeyCode::Char('g') => AppAction::ScrollTop,
        KeyCode::Char('G') => AppAction::ScrollBottom,
        KeyCode::Char('/') => AppAction::StartSearch,
        KeyCode::Esc => AppAction::Dismiss,
        KeyCode::Enter => AppAction::Connect,
        KeyCode::Char('c') => AppAction::Connect,
        KeyCode::Char(' ') => AppAction::ToggleGroup,
        KeyCode::Char('t') => AppAction::TestFocused,
        KeyCode::Char('T') => AppAction::TestAll,
        KeyCode::Char('C') => AppAction::CancelTests,
        KeyCode::Char('u') | KeyCode::Char('r') => AppAction::UpdateSubscription,
        KeyCode::Char('a') => AppAction::AddProfile,
        KeyCode::Char('A') | KeyCode::Char('n') => AppAction::AddGroup,
        KeyCode::Char('e') => AppAction::EditItem,
        KeyCode::Char('y') => AppAction::CopyUri,
        KeyCode::Char('x') | KeyCode::Delete => AppAction::DeleteItem,
        KeyCode::Char('X') => AppAction::DeleteGroup,
        _ => AppAction::None,
    }
}

fn resolve_profiles_details(key: &KeyEvent) -> AppAction {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    if ctrl {
        return match key.code {
            KeyCode::Char('d') => AppAction::PageDown,
            KeyCode::Char('u') => AppAction::PageUp,
            _ => AppAction::None,
        };
    }

    match key.code {
        KeyCode::Char('h') | KeyCode::Left | KeyCode::Esc => AppAction::CursorLeft,
        KeyCode::Tab | KeyCode::BackTab => AppAction::ToggleFocus,
        KeyCode::Char('j') | KeyCode::Down => AppAction::CursorDown,
        KeyCode::Char('k') | KeyCode::Up => AppAction::CursorUp,
        KeyCode::PageDown => AppAction::PageDown,
        KeyCode::PageUp => AppAction::PageUp,
        KeyCode::Char('g') => AppAction::ScrollTop,
        KeyCode::Char('G') => AppAction::ScrollBottom,
        KeyCode::Enter | KeyCode::Char('c') => AppAction::Connect,
        KeyCode::Char('t') => AppAction::TestFocused,
        KeyCode::Char('T') => AppAction::TestAll,
        KeyCode::Char('y') => AppAction::CopyUri,
        _ => AppAction::None,
    }
}

fn resolve_routing(key: &KeyEvent) -> AppAction {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => AppAction::CursorDown,
        KeyCode::Char('k') | KeyCode::Up => AppAction::CursorUp,
        KeyCode::Char('h') | KeyCode::Left => AppAction::RoutingGroupPrev,
        KeyCode::Char('l') | KeyCode::Right => AppAction::RoutingGroupNext,
        KeyCode::Char('a') => AppAction::AddRoutingRule,
        KeyCode::Char('e') | KeyCode::Enter => AppAction::EditRoutingRule,
        KeyCode::Char('x') | KeyCode::Delete => AppAction::DeleteRoutingRule,
        KeyCode::Char(' ') => AppAction::ToggleRuleEnabled,
        KeyCode::Char('p') | KeyCode::Char('P') => AppAction::OpenPresets,
        KeyCode::Char('H') => AppAction::OpenHappProfiles,
        KeyCode::Char('m') | KeyCode::Char('M') => AppAction::CycleRoutingMode,
        KeyCode::Char('g') | KeyCode::Char('G') => AppAction::ToggleManualRoutingSource,
        KeyCode::Char('i') | KeyCode::Char('I') => AppAction::ImportHappLink,
        KeyCode::Esc => AppAction::Dismiss,
        _ => AppAction::None,
    }
}

fn resolve_settings(key: &KeyEvent) -> AppAction {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => AppAction::CursorDown,
        KeyCode::Char('k') | KeyCode::Up => AppAction::CursorUp,
        KeyCode::Char('h') | KeyCode::Left => AppAction::SettingsPrev,
        KeyCode::Char('l') | KeyCode::Right => AppAction::SettingsNext,
        KeyCode::Enter | KeyCode::Char(' ') => AppAction::SettingsActivate,
        KeyCode::Esc => AppAction::Dismiss,
        _ => AppAction::None,
    }
}

fn resolve_logs(key: &KeyEvent) -> AppAction {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    if ctrl {
        return match key.code {
            KeyCode::Char('d') => AppAction::PageDown,
            KeyCode::Char('u') => AppAction::PageUp,
            _ => AppAction::None,
        };
    }

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => AppAction::CursorDown,
        KeyCode::Char('k') | KeyCode::Up => AppAction::CursorUp,
        KeyCode::PageDown => AppAction::PageDown,
        KeyCode::PageUp => AppAction::PageUp,
        KeyCode::Char('g') => AppAction::ScrollTop,
        KeyCode::Char('G') => AppAction::ScrollBottom,
        KeyCode::Char('f') | KeyCode::Char('F') => AppAction::LogsCycleFilter,
        KeyCode::Esc => AppAction::Dismiss,
        _ => AppAction::None,
    }
}

fn resolve_traffic(key: &KeyEvent) -> AppAction {
    match key.code {
        KeyCode::Char('d') => AppAction::Disconnect,
        KeyCode::Char('v') => AppAction::ToggleTun,
        KeyCode::Esc => AppAction::Dismiss,
        _ => AppAction::None,
    }
}

fn resolve_global(key: &KeyEvent) -> AppAction {
    if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) {
        return AppAction::None;
    }

    match key.code {
        KeyCode::Char('q') => AppAction::Detach,
        KeyCode::Char('?') => AppAction::Help,
        KeyCode::Char('1') => AppAction::SwitchTab(0),
        KeyCode::Char('2') => AppAction::SwitchTab(1),
        KeyCode::Char('3') => AppAction::SwitchTab(2),
        KeyCode::Char('4') => AppAction::SwitchTab(3),
        KeyCode::Char('5') => AppAction::SwitchTab(4),
        KeyCode::Char('[') => AppAction::PrevTab,
        KeyCode::Char(']') => AppAction::NextTab,
        KeyCode::Char('w') => AppAction::TabSwitcherModal,
        KeyCode::Char('v') => AppAction::ToggleTun,
        KeyCode::Char('d') => AppAction::Disconnect,
        _ => AppAction::None,
    }
}

pub const GLOBAL_HELP: &[HelpEntry] = &[
    HelpEntry {
        key: "[ / ]",
        desc: "Prev / Next tab",
    },
    HelpEntry {
        key: "1 – 5",
        desc: "Jump to tab 1..5",
    },
    HelpEntry {
        key: "Tab",
        desc: "Toggle Tree ↔ Details focus",
    },
    HelpEntry {
        key: "w",
        desc: "Open Tab Switcher modal",
    },
    HelpEntry {
        key: "v",
        desc: "Toggle TUN mode",
    },
    HelpEntry {
        key: "d",
        desc: "Disconnect active VPN",
    },
    HelpEntry {
        key: "?",
        desc: "This help dialog",
    },
    HelpEntry {
        key: "q",
        desc: "Detach (VPN stays active)",
    },
    HelpEntry {
        key: "Q / C-c",
        desc: "Quit + stop daemon",
    },
];

pub const PROFILES_HELP: &[HelpEntry] = &[
    HelpEntry {
        key: "j/k, ↓/↑",
        desc: "Move cursor",
    },
    HelpEntry {
        key: "h/l, ←/→",
        desc: "Collapse/expand / to Details",
    },
    HelpEntry {
        key: "Tab",
        desc: "Switch focus: Tree ↔ Details",
    },
    HelpEntry {
        key: "Ctrl+d/u",
        desc: "Page down / page up",
    },
    HelpEntry {
        key: "g / G",
        desc: "Jump to top / bottom",
    },
    HelpEntry {
        key: "Enter / c",
        desc: "Connect to selected profile",
    },
    HelpEntry {
        key: "Space",
        desc: "Fold / unfold group",
    },
    HelpEntry {
        key: "t / T",
        desc: "Test focused / test all",
    },
    HelpEntry {
        key: "C",
        desc: "Cancel active tests",
    },
    HelpEntry {
        key: "a",
        desc: "Import profile from clipboard/input",
    },
    HelpEntry {
        key: "A / n",
        desc: "Add new group / subscription",
    },
    HelpEntry {
        key: "u / r",
        desc: "Update active subscription",
    },
    HelpEntry {
        key: "e",
        desc: "Edit name / subscription",
    },
    HelpEntry {
        key: "y",
        desc: "Copy profile URI",
    },
    HelpEntry {
        key: "x / X",
        desc: "Delete profile / delete group",
    },
    HelpEntry {
        key: "J/K, Alt+↓/↑",
        desc: "Reorder profile / group up/down",
    },
    HelpEntry {
        key: "/",
        desc: "Search & filter profiles",
    },
];

pub const ROUTING_HELP: &[HelpEntry] = &[
    HelpEntry {
        key: "j/k, ↓/↑",
        desc: "Navigate rules",
    },
    HelpEntry {
        key: "h/l, ←/→",
        desc: "Switch subscription group",
    },
    HelpEntry {
        key: "Space",
        desc: "Toggle rule enabled/disabled",
    },
    HelpEntry {
        key: "a",
        desc: "Add manual rule",
    },
    HelpEntry {
        key: "e / Enter",
        desc: "Edit selected rule",
    },
    HelpEntry {
        key: "x / Del",
        desc: "Delete rule",
    },
    HelpEntry {
        key: "p",
        desc: "Routing presets (GeoIP/Ads/AI)",
    },
    HelpEntry {
        key: "H",
        desc: "Happ routing profiles modal",
    },
    HelpEntry {
        key: "m",
        desc: "Cycle mode (auto/manual/sub)",
    },
    HelpEntry {
        key: "g",
        desc: "Toggle global / group-specific rules",
    },
    HelpEntry {
        key: "i",
        desc: "Import Happ link from clipboard",
    },
];

pub const SETTINGS_HELP: &[HelpEntry] = &[
    HelpEntry {
        key: "j/k, ↓/↑",
        desc: "Navigate setting rows",
    },
    HelpEntry {
        key: "h/l, ←/→",
        desc: "Cycle value (theme, log, etc.)",
    },
    HelpEntry {
        key: "Enter/Space",
        desc: "Toggle, cycle, or edit field",
    },
];

pub const LOGS_HELP: &[HelpEntry] = &[
    HelpEntry {
        key: "j/k, ↓/↑",
        desc: "Scroll line by line",
    },
    HelpEntry {
        key: "Ctrl+d/u",
        desc: "Scroll half page down/up",
    },
    HelpEntry {
        key: "g / G",
        desc: "Jump to start / end of logs",
    },
    HelpEntry {
        key: "f",
        desc: "Cycle log level filter",
    },
];

pub const TRAFFIC_HELP: &[HelpEntry] = &[
    HelpEntry {
        key: "d",
        desc: "Disconnect active VPN",
    },
    HelpEntry {
        key: "v",
        desc: "Toggle TUN mode",
    },
    HelpEntry {
        key: "[ / ]",
        desc: "Switch tabs",
    },
];

pub fn get_tab_help(ctx: KeyContext) -> (&'static str, &'static [HelpEntry]) {
    match ctx {
        KeyContext::ProfilesTree | KeyContext::ProfilesDetails => ("Profiles", PROFILES_HELP),
        KeyContext::Routing => ("Routing", ROUTING_HELP),
        KeyContext::Settings => ("Settings", SETTINGS_HELP),
        KeyContext::Logs => ("Logs", LOGS_HELP),
        KeyContext::Traffic => ("Traffic", TRAFFIC_HELP),
        KeyContext::Global => ("Global", GLOBAL_HELP),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn test_quit_precedence() {
        assert_eq!(
            resolve_action(KeyContext::ProfilesTree, &key(KeyCode::Char('Q'))),
            AppAction::Quit
        );
        assert_eq!(
            resolve_action(KeyContext::Settings, &ctrl_key(KeyCode::Char('c'))),
            AppAction::Quit
        );
    }

    #[test]
    fn test_profiles_tree_vim_navigation_not_shadowed() {
        // 'l' must be CursorRight, NOT SwitchTab to Logs!
        assert_eq!(
            resolve_action(KeyContext::ProfilesTree, &key(KeyCode::Char('l'))),
            AppAction::CursorRight
        );
        // 'h' must be CursorLeft, NOT Help!
        assert_eq!(
            resolve_action(KeyContext::ProfilesTree, &key(KeyCode::Char('h'))),
            AppAction::CursorLeft
        );
        // 'j' and 'k'
        assert_eq!(
            resolve_action(KeyContext::ProfilesTree, &key(KeyCode::Char('j'))),
            AppAction::CursorDown
        );
        assert_eq!(
            resolve_action(KeyContext::ProfilesTree, &key(KeyCode::Char('k'))),
            AppAction::CursorUp
        );
    }

    #[test]
    fn test_settings_navigation_not_shadowed() {
        // In settings, 'h' and 'l' cycle values left and right
        assert_eq!(
            resolve_action(KeyContext::Settings, &key(KeyCode::Char('h'))),
            AppAction::SettingsPrev
        );
        assert_eq!(
            resolve_action(KeyContext::Settings, &key(KeyCode::Char('l'))),
            AppAction::SettingsNext
        );
    }

    #[test]
    fn test_tab_toggle_focus() {
        assert_eq!(
            resolve_action(KeyContext::ProfilesTree, &key(KeyCode::Tab)),
            AppAction::ToggleFocus
        );
        assert_eq!(
            resolve_action(KeyContext::ProfilesDetails, &key(KeyCode::BackTab)),
            AppAction::ToggleFocus
        );
    }

    #[test]
    fn test_bracket_tab_switching() {
        assert_eq!(
            resolve_action(KeyContext::ProfilesTree, &key(KeyCode::Char('['))),
            AppAction::PrevTab
        );
        assert_eq!(
            resolve_action(KeyContext::ProfilesTree, &key(KeyCode::Char(']'))),
            AppAction::NextTab
        );
        assert_eq!(
            resolve_action(KeyContext::Logs, &key(KeyCode::Char('['))),
            AppAction::PrevTab
        );
    }

    #[test]
    fn test_direct_numeric_tabs() {
        assert_eq!(
            resolve_action(KeyContext::ProfilesTree, &key(KeyCode::Char('1'))),
            AppAction::SwitchTab(0)
        );
        assert_eq!(
            resolve_action(KeyContext::Settings, &key(KeyCode::Char('3'))),
            AppAction::SwitchTab(2)
        );
    }

    #[test]
    fn test_ctrl_d_and_u_paging() {
        assert_eq!(
            resolve_action(KeyContext::ProfilesTree, &ctrl_key(KeyCode::Char('d'))),
            AppAction::PageDown
        );
        assert_eq!(
            resolve_action(KeyContext::ProfilesTree, &ctrl_key(KeyCode::Char('u'))),
            AppAction::PageUp
        );
        assert_eq!(
            resolve_action(KeyContext::Logs, &ctrl_key(KeyCode::Char('d'))),
            AppAction::PageDown
        );
    }

    #[test]
    fn test_routing_source_toggle_key() {
        assert_eq!(
            resolve_action(KeyContext::Routing, &key(KeyCode::Char('g'))),
            AppAction::ToggleManualRoutingSource
        );
        // In the profiles tree 'g' must still jump to the top.
        assert_eq!(
            resolve_action(KeyContext::ProfilesTree, &key(KeyCode::Char('g'))),
            AppAction::ScrollTop
        );
    }

    #[test]
    fn test_group_add_and_update() {
        assert_eq!(
            resolve_action(KeyContext::ProfilesTree, &key(KeyCode::Char('A'))),
            AppAction::AddGroup
        );
        assert_eq!(
            resolve_action(KeyContext::ProfilesTree, &key(KeyCode::Char('n'))),
            AppAction::AddGroup
        );
        assert_eq!(
            resolve_action(KeyContext::ProfilesTree, &key(KeyCode::Char('u'))),
            AppAction::UpdateSubscription
        );
        assert_eq!(
            resolve_action(KeyContext::ProfilesTree, &key(KeyCode::Char('r'))),
            AppAction::UpdateSubscription
        );
    }
}
