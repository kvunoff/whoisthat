use clap::{Args, Parser, Subcommand};

/// Top-level CLI definition. Every command that needs the daemon talks to it
/// through `Session`; local-only commands never do.
#[derive(Debug, Parser)]
#[command(
    name = "whoisthat",
    about = "Terminal VPN client",
    disable_version_flag = true,
    arg_required_else_help = false,
    subcommand_required = false
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,
    #[command(subcommand)]
    pub cmd: Option<Command>,
}

#[derive(Debug, Clone, Args, Default)]
pub struct GlobalArgs {
    /// Machine-readable output on stdout (one object, or NDJSON where documented)
    #[arg(long, short = 'j', global = true)]
    pub json: bool,
    /// Suppress human progress notes on stderr
    #[arg(long, short = 'q', global = true)]
    pub quiet: bool,
    /// Disable ANSI colours
    #[arg(long, global = true)]
    pub no_color: bool,
    /// Override the core Unix socket path
    #[arg(long, global = true, value_name = "PATH", conflicts_with = "tcp")]
    pub socket: Option<std::path::PathBuf>,
    /// Reach the core over loopback TCP (requires tcp-enabled in config.json)
    #[arg(long, global = true, value_name = "HOST:PORT")]
    pub tcp: Option<String>,
    /// Timeout for core replies, seconds (command defaults: 10, 60, 120)
    #[arg(long, global = true, value_name = "SECS")]
    pub timeout: Option<u64>,
    /// Never start a core daemon automatically
    #[arg(long, global = true)]
    pub no_spawn: bool,
    /// Print version and exit
    #[arg(long, short = 'v', global = true, action = clap::ArgAction::SetTrue)]
    pub version: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Connection status (one-shot, short, or streaming)
    Status(StatusArgs),
    /// Connect to a profile (id, group:id, or name)
    Connect(ConnectArgs),
    /// Disconnect the active session
    Disconnect(WaitArgs),
    /// Toggle connection
    Toggle(WaitArgs),
    /// TUN/proxy mode control
    Mode(ModeArgs),
    /// Core daemon lifecycle
    Core(CoreArgs),
    /// Profiles inside groups
    Profile(ProfileArgs),
    /// Subscription groups (alias: group)
    #[command(alias = "group")]
    Sub(SubArgs),
    /// Routing rules, Happ profiles and presets
    Route(RouteArgs),
    /// Latency tests
    Test(TestArgs),
    /// Settings (core-authoritative and local)
    Settings(SettingsArgs),
    /// Hardware ID exposed to subscription providers
    Hwid(HwidArgs),
    /// Core and TUI logs
    Logs(LogsArgs),
    /// systemd user service
    Systemd(SystemdArgs),
    /// Deep links and desktop registration
    Link(LinkArgs),
    /// System diagnostics
    Doctor,
    /// Public IPv4/IPv6
    Ip,
    /// Print version
    Version,
    /// Launch an application inside the split-tunnel slice
    Run(RunArgs),
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    /// Compact single-line status
    #[arg(long, short = 's')]
    pub short: bool,
    /// Stream updates (NDJSON with --json)
    #[arg(long, short = 'w')]
    pub watch: bool,
}

#[derive(Debug, Args)]
pub struct ConnectArgs {
    /// Profile id, group:id, or name substring
    pub target: Option<String>,
    /// Restrict selection to a group
    #[arg(long, value_name = "GROUP")]
    pub group: Option<String>,
    /// Wait for the connection result
    #[arg(long)]
    pub wait: bool,
}

#[derive(Debug, Args)]
#[command(trailing_var_arg = true)]
pub struct RunArgs {
    /// Run detached as a transient systemd user service
    #[arg(short = 'd', long = "detach", short_alias = 'b', alias = "background")]
    pub detach: bool,
    /// Application and its arguments (passed through unchanged)
    #[arg(required = true, allow_hyphen_values = true)]
    pub argv: Vec<String>,
}

#[derive(Debug, Args)]
pub struct WaitArgs {
    /// Wait for the daemon to confirm the change
    #[arg(long)]
    pub wait: bool,
}

#[derive(Debug, Args)]
pub struct ModeArgs {
    #[command(subcommand)]
    pub cmd: Option<ModeCmd>,
}

#[derive(Debug, Subcommand)]
pub enum ModeCmd {
    /// Show current mode
    Status,
    /// Switch to system-wide TUN mode
    Tun,
    /// Switch to proxy mode (TUN off)
    Proxy,
    /// Toggle between TUN and proxy
    Toggle,
}

#[derive(Debug, Args)]
pub struct CoreArgs {
    #[command(subcommand)]
    pub cmd: CoreCmd,
}

#[derive(Debug, Subcommand)]
pub enum CoreCmd {
    Status,
    Start,
    Stop,
    Restart,
    Version,
}

#[derive(Debug, Args)]
pub struct ProfileArgs {
    #[command(subcommand)]
    pub cmd: ProfileCmd,
}

#[derive(Debug, Subcommand)]
pub enum ProfileCmd {
    List(ProfileListArgs),
    Show(ProfileShowArgs),
    Add(ProfileAddArgs),
    Rm(ProfileRmArgs),
    Rename(ProfileRenameArgs),
    Move(ProfileMoveArgs),
    Reorder(ProfileReorderArgs),
}

#[derive(Debug, Args)]
pub struct ProfileListArgs {
    #[arg(long, value_name = "GROUP")]
    pub group: Option<String>,
    /// Show raw URIs (contains credentials)
    #[arg(long)]
    pub reveal: bool,
}

#[derive(Debug, Args)]
pub struct ProfileShowArgs {
    pub selector: String,
    #[arg(long, value_name = "GROUP")]
    pub group: Option<String>,
    #[arg(long)]
    pub reveal: bool,
}

#[derive(Debug, Args)]
pub struct ProfileAddArgs {
    /// URI(s), or - to read one per line from stdin
    #[arg(required = true, allow_hyphen_values = true)]
    pub uris: Vec<String>,
    #[arg(long, value_name = "GROUP")]
    pub group: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProfileRmArgs {
    #[arg(required = true)]
    pub selectors: Vec<String>,
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct ProfileRenameArgs {
    pub selector: String,
    pub new_name: String,
}

#[derive(Debug, Args)]
pub struct ProfileMoveArgs {
    pub selector: String,
    #[arg(long, value_name = "GROUP")]
    pub to: String,
}

#[derive(Debug, Args)]
pub struct ProfileReorderArgs {
    pub group: String,
    /// Profile selectors in the new order
    pub selectors: Vec<String>,
    /// Comma-separated profile ids (alternative to positional selectors)
    #[arg(long, value_name = "ID,ID,...")]
    pub order: Option<String>,
}

#[derive(Debug, Args)]
pub struct SubArgs {
    #[command(subcommand)]
    pub cmd: SubCmd,
}

#[derive(Debug, Subcommand)]
pub enum SubCmd {
    List(SubListArgs),
    Show(SubShowArgs),
    Add(SubAddArgs),
    Rm(SubRmArgs),
    Refresh(SubRefreshArgs),
    Rename(SubRenameArgs),
    Url(SubUrlArgs),
    Reorder(SubReorderArgs),
}

#[derive(Debug, Args)]
pub struct SubListArgs {
    #[arg(long)]
    pub reveal: bool,
}

#[derive(Debug, Args)]
pub struct SubShowArgs {
    pub selector: String,
    #[arg(long)]
    pub reveal: bool,
}

#[derive(Debug, Args)]
pub struct SubAddArgs {
    pub url: String,
    /// Optional name (legacy positional form)
    pub name: Option<String>,
    #[arg(long = "name", value_name = "NAME")]
    pub name_flag: Option<String>,
    #[arg(long)]
    pub no_wait: bool,
}

#[derive(Debug, Args)]
pub struct SubRmArgs {
    pub selector: String,
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct SubRefreshArgs {
    pub selector: Option<String>,
    #[arg(long)]
    pub all: bool,
    #[arg(long)]
    pub no_wait: bool,
}

#[derive(Debug, Args)]
pub struct SubRenameArgs {
    pub selector: String,
    /// New name; omit to reset to the provider name from the subscription
    pub new_name: Option<String>,
}

#[derive(Debug, Args)]
pub struct SubUrlArgs {
    pub selector: String,
    #[arg(long, value_name = "URL")]
    pub set: Option<String>,
    #[arg(long)]
    pub reveal: bool,
}

#[derive(Debug, Args)]
pub struct SubReorderArgs {
    pub selector: String,
    pub new_index: usize,
}

#[derive(Debug, Args)]
pub struct RouteArgs {
    #[command(subcommand)]
    pub cmd: RouteCmd,
}

#[derive(Debug, Subcommand)]
pub enum RouteCmd {
    List(RouteListArgs),
    Add(RouteAddArgs),
    Rm(RouteIndexArgs),
    Enable(RouteIndexArgs),
    Disable(RouteIndexArgs),
    Move(RouteMoveArgs),
    Mode(RouteModeArgs),
    Profile(RouteProfileArgs),
    Import(RouteImportArgs),
    Preset(RoutePresetArgs),
}

#[derive(Debug, Args)]
pub struct RouteListArgs {
    #[arg(long, value_name = "GROUP")]
    pub group: Option<String>,
}

#[derive(Debug, Args)]
pub struct RouteAddArgs {
    #[arg(long = "type", value_name = "TYPE")]
    pub match_type: String,
    #[arg(long, value_name = "VALUE")]
    pub value: String,
    #[arg(long, value_name = "OUTBOUND")]
    pub outbound: String,
    #[arg(long, value_name = "GROUP")]
    pub group: Option<String>,
}

#[derive(Debug, Args)]
pub struct RouteIndexArgs {
    #[arg(required = true)]
    pub indexes: Vec<usize>,
    #[arg(long, value_name = "GROUP")]
    pub group: Option<String>,
}

#[derive(Debug, Args)]
pub struct RouteMoveArgs {
    pub index: usize,
    pub new_index: usize,
    #[arg(long, value_name = "GROUP")]
    pub group: Option<String>,
}

#[derive(Debug, Args)]
pub struct RouteModeArgs {
    pub selector: String,
    pub mode: String,
}

#[derive(Debug, Args)]
pub struct RouteProfileArgs {
    #[command(subcommand)]
    pub cmd: RouteProfileCmd,
}

#[derive(Debug, Subcommand)]
pub enum RouteProfileCmd {
    List(RouteListArgs),
    Activate(RouteProfileActivateArgs),
    Off(RouteListArgs),
    Rm(RouteProfileRmArgs),
}

#[derive(Debug, Args)]
pub struct RouteProfileActivateArgs {
    pub profile: String,
    #[arg(long, value_name = "GROUP")]
    pub group: Option<String>,
}

#[derive(Debug, Args)]
pub struct RouteProfileRmArgs {
    pub profile: String,
    #[arg(long, value_name = "GROUP")]
    pub group: Option<String>,
}

#[derive(Debug, Args)]
pub struct RouteImportArgs {
    pub link: String,
    #[arg(long, value_name = "GROUP")]
    pub group: Option<String>,
}

#[derive(Debug, Args)]
pub struct RoutePresetArgs {
    #[command(subcommand)]
    pub cmd: RoutePresetCmd,
}

#[derive(Debug, Subcommand)]
pub enum RoutePresetCmd {
    List,
    Apply(RoutePresetApplyArgs),
    Remove(RoutePresetApplyArgs),
}

#[derive(Debug, Args)]
pub struct RoutePresetApplyArgs {
    pub id: String,
    #[arg(long, value_name = "GROUP")]
    pub group: Option<String>,
}

#[derive(Debug, Args)]
pub struct TestArgs {
    #[command(subcommand)]
    pub cmd: TestCmd,
}

#[derive(Debug, Subcommand)]
pub enum TestCmd {
    Profile(TestTargetArgs),
    Group(TestTargetArgs),
    All(TestMethodArgs),
    Cancel,
    Config(TestConfigArgs),
}

#[derive(Debug, Args)]
pub struct TestTargetArgs {
    pub selector: String,
    #[arg(long, value_name = "tcp|http-get|http-head")]
    pub method: Option<String>,
    #[arg(long)]
    pub wait: bool,
}

#[derive(Debug, Args)]
pub struct TestMethodArgs {
    #[arg(long, value_name = "tcp|http-get|http-head")]
    pub method: Option<String>,
    #[arg(long)]
    pub wait: bool,
}

#[derive(Debug, Args)]
pub struct TestConfigArgs {
    #[arg(long)]
    pub concurrency: Option<i32>,
    #[arg(long = "test-timeout")]
    pub test_timeout: Option<i32>,
    #[arg(long)]
    pub samples: Option<i32>,
    #[arg(long)]
    pub endpoint: Option<String>,
    #[arg(long, value_name = "on|off")]
    pub auto_on_subscribe: Option<String>,
}

#[derive(Debug, Args)]
pub struct SettingsArgs {
    #[command(subcommand)]
    pub cmd: Option<SettingsCmd>,
    /// Set one or more key=value pairs (alternative to `settings set`)
    #[arg(long = "set", value_name = "KEY=VALUE", num_args = 1..)]
    pub set: Vec<String>,
}

#[derive(Debug, Subcommand)]
pub enum SettingsCmd {
    List,
    Get { key: String },
    Set { assignments: Vec<String> },
    Toggle { key: String },
}

#[derive(Debug, Args)]
pub struct HwidArgs {
    #[command(subcommand)]
    pub cmd: HwidCmd,
}

#[derive(Debug, Subcommand)]
pub enum HwidCmd {
    Status,
    Enable,
    Disable,
    Reset,
    UserAgent { value: String },
}

#[derive(Debug, Args)]
pub struct LogsArgs {
    #[arg(long, default_value_t = 100)]
    pub lines: usize,
    #[arg(long, value_name = "error|warn|info|debug|trace")]
    pub level: Option<String>,
    #[arg(long, short = 'f')]
    pub follow: bool,
    /// Read the TUI log instead of the core log
    #[arg(long)]
    pub tui: bool,
}

#[derive(Debug, Args)]
pub struct SystemdArgs {
    #[command(subcommand)]
    pub cmd: SystemdCmd,
}

#[derive(Debug, Subcommand)]
pub enum SystemdCmd {
    Status,
    Enable,
    Disable,
    Toggle,
}

#[derive(Debug, Args)]
pub struct LinkArgs {
    #[command(subcommand)]
    pub cmd: LinkCmd,
}

#[derive(Debug, Subcommand)]
pub enum LinkCmd {
    Handle(HandleUrlArgs),
    Register,
}

#[derive(Debug, Args)]
pub struct HandleUrlArgs {
    pub url: String,
    #[arg(long, short = 'g', value_name = "GROUP")]
    pub group: Option<String>,
}
