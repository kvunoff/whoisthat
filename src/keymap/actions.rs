#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    // Global Lifecycle & Navigation
    Quit,
    Detach,
    Help,
    TabSwitcherModal,
    SwitchTab(usize),
    NextTab,
    PrevTab,
    ToggleTun,
    Disconnect,
    Dismiss,

    // Navigation & Focus Motions
    CursorUp,
    CursorDown,
    CursorLeft,
    CursorRight,
    ToggleFocus,
    ScrollTop,
    ScrollBottom,
    PageUp,
    PageDown,

    // Profiles & Groups
    Connect,
    ToggleGroup,
    TestFocused,
    TestAll,
    CancelTests,
    AddProfile,
    AddGroup,
    UpdateSubscription,
    EditItem,
    CopyUri,
    DeleteItem,
    DeleteGroup,
    MoveItemUp,
    MoveItemDown,
    StartSearch,

    // Routing
    AddRoutingRule,
    EditRoutingRule,
    DeleteRoutingRule,
    ToggleRuleEnabled,
    OpenPresets,
    OpenHappProfiles,
    CycleRoutingMode,
    ImportHappLink,
    RoutingGroupPrev,
    RoutingGroupNext,

    // Settings
    SettingsActivate,
    SettingsPrev,
    SettingsNext,

    // Logs
    LogsCycleFilter,

    // Unmapped
    None,
}
