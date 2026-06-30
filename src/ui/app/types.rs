#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActiveTab {
    Profiles,
    Logs,
    Settings,
    Routing,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    LeftPanel,
    RightPanel,
    Popup,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Popup {
    Import {
        input: String,
        cursor: usize,
    },
    ConfirmDelete {
        gid: i32,
        pid: i32,
        name: String,
    },
    ConfirmDeleteGroup {
        gid: i32,
        name: String,
    },
    AddGroup {
        name: String,
        url: String,
        cursor: usize,
        field: usize,
    },
    EditSubscription {
        name: String,
        url: String,
        group_id: i32,
        cursor: usize,
        field: usize,
    },
    EditUserAgent {
        input: String,
        cursor: usize,
    },
    EditTunName {
        input: String,
        cursor: usize,
    },
    EditProfileName {
        input: String,
        cursor: usize,
        group_id: i32,
        profile_id: i32,
    },
    Help,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum TreeNode {
    Group(usize),
    Profile(usize, usize),
}
