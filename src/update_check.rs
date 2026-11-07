use std::path::Path;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::mpsc;
use ureq::ResponseExt;

use crate::event_loop::AppEvent;

/// Redirects to `/releases/tag/vX.Y.Z`, so no GitHub API or token is needed.
const LATEST_URL: &str = "https://github.com/kvunoff/whoisthat/releases/latest";
const RELEASES_URL: &str = "https://github.com/kvunoff/whoisthat/releases";
const INSTALL_URL: &str = "https://raw.githubusercontent.com/kvunoff/whoisthat/main/install.sh";

const CHECK_INTERVAL_SECS: i64 = 86_400;

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateInfo {
    pub current: String,
    /// Latest stable release without the `v` prefix.
    pub latest: String,
    pub source: InstallSource,
    pub command: Option<String>,
    pub release_url: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InstallSource {
    DevBuild,
    Pacman { package: String },
    Dpkg { package: String },
    Rpm { package: String },
    InstallScript,
    Manual,
    Other,
}

impl InstallSource {
    pub fn label(&self) -> String {
        match self {
            Self::DevBuild => "development build".into(),
            Self::Pacman { package } => format!("pacman ({package})"),
            Self::Dpkg { package } => format!("dpkg ({package})"),
            Self::Rpm { package } => format!("rpm ({package})"),
            Self::InstallScript => "install.sh".into(),
            Self::Manual => "manual install".into(),
            Self::Other => "unknown".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PkgManager {
    Pacman,
    Dpkg,
    Rpm,
}

#[derive(Debug, Clone, Copy, Default)]
struct Tools {
    paru: bool,
    yay: bool,
    dnf: bool,
    zypper: bool,
    rpm: bool,
}

pub(crate) fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Parse `X.Y.Z` or `vX.Y.Z` into a numeric triple.
fn parse_version(value: &str) -> Option<(u64, u64, u64)> {
    let value = value.strip_prefix('v').unwrap_or(value);
    let mut parts = value.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// Extract `X.Y.Z` (without `v`) from a GitHub release tag URL.
fn parse_release_tag(url: &str) -> Option<String> {
    let url = url.split(['?', '#']).next()?.trim_end_matches('/');
    let tag = url.strip_prefix("https://github.com/kvunoff/whoisthat/releases/tag/")?;
    if tag.contains('/') {
        return None;
    }
    let version = tag.strip_prefix('v')?;
    parse_version(version)?;
    Some(version.to_string())
}

fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

pub(crate) fn should_check_cheap(
    enabled: bool,
    now: i64,
    last_check: i64,
    no_update_check_env: bool,
    test_mode_env: bool,
) -> bool {
    if !enabled || no_update_check_env || test_mode_env {
        return false;
    }
    // A last-check value in the future means the clock moved backwards; treat
    // it as due instead of blocking checks until wall time catches up.
    last_check == 0 || last_check > now || now.saturating_sub(last_check) >= CHECK_INTERVAL_SECS
}

pub(crate) fn should_prompt(latest: &str, current: &str, skipped: &str) -> bool {
    is_newer(latest, current) && latest != skipped
}

/// A `cargo run`/`make build` binary must not check for updates.
pub(crate) fn is_dev_build(exe: &Path) -> bool {
    let path = exe.to_string_lossy();
    if path.contains("/target/debug/") || path.contains("/target/release/") {
        return true;
    }
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cargo")));
    cargo_home.is_some_and(|home| exe.starts_with(home.join("bin")))
}

fn command_exists(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).is_file()))
        .unwrap_or(false)
}

fn detect_tools() -> Tools {
    Tools {
        paru: command_exists("paru"),
        yay: command_exists("yay"),
        dnf: command_exists("dnf"),
        zypper: command_exists("zypper"),
        rpm: command_exists("rpm"),
    }
}

fn run_owned(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

fn package_owner(path: &Path) -> Option<(PkgManager, String)> {
    let path = path.to_string_lossy().into_owned();

    if let Some(text) = run_owned("pacman", &["-Qoq", &path])
        && let Some(package) = text.lines().next()
    {
        return Some((PkgManager::Pacman, package.trim().to_string()));
    }
    if let Some(text) = run_owned("dpkg-query", &["-S", &path])
        && let Some(package) = text.split(':').next()
    {
        return Some((PkgManager::Dpkg, package.trim().to_string()));
    }
    if let Some(text) = run_owned("rpm", &["-q", "-f", &path, "--qf", "%{NAME}\n"])
        && let Some(package) = text.lines().next()
    {
        return Some((PkgManager::Rpm, package.trim().to_string()));
    }
    None
}

fn classify(path: &Path, owner: Option<(PkgManager, String)>) -> InstallSource {
    match owner {
        Some((PkgManager::Pacman, package)) => return InstallSource::Pacman { package },
        Some((PkgManager::Dpkg, package)) => return InstallSource::Dpkg { package },
        Some((PkgManager::Rpm, package)) => return InstallSource::Rpm { package },
        None => {}
    }
    let path_text = path.to_string_lossy();
    if path_text.starts_with("/nix/store/")
        || path_text.starts_with("/app/")
        || path_text.starts_with("/snap/")
    {
        return InstallSource::Other;
    }
    if path == Path::new("/usr/bin/whoisthat") {
        return InstallSource::InstallScript;
    }
    InstallSource::Manual
}

fn detect_install_source() -> InstallSource {
    let exe = std::env::current_exe().unwrap_or_default();
    let canonical = exe.canonicalize().unwrap_or_else(|_| exe.clone());
    if is_dev_build(&canonical) {
        return InstallSource::DevBuild;
    }
    for candidate in [&exe, &canonical] {
        if let Some(owner) = package_owner(candidate) {
            return classify(candidate, Some(owner));
        }
    }
    classify(&canonical, None)
}

fn release_url(latest: &str) -> String {
    format!("{RELEASES_URL}/tag/v{latest}")
}

fn update_command(
    source: &InstallSource,
    latest: &str,
    arch: &str,
    tools: &Tools,
) -> Option<String> {
    let deb_arch = match arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        _ => return None,
    };
    let rpm_arch = match arch {
        "x86_64" | "aarch64" => arch,
        _ => return None,
    };
    let base = format!("{RELEASES_URL}/download/v{latest}");

    match source {
        InstallSource::Pacman { package } => {
            if tools.paru {
                Some(format!("paru -S {package}"))
            } else if tools.yay {
                Some(format!("yay -S {package}"))
            } else {
                None
            }
        }
        InstallSource::Dpkg { .. } => {
            let asset = format!("whoisthat_{latest}-1_{deb_arch}.deb");
            Some(format!(
                "curl -fsSLO {base}/SHA256SUMS && curl -fsSLO {base}/{asset} && sha256sum -c --ignore-missing SHA256SUMS && sudo apt install ./{asset}"
            ))
        }
        InstallSource::Rpm { .. } => {
            let asset = format!("whoisthat-{latest}-1.{rpm_arch}.rpm");
            let install = if tools.dnf {
                format!("sudo dnf install ./{asset}")
            } else if tools.zypper {
                format!("sudo zypper --non-interactive install ./{asset}")
            } else if tools.rpm {
                format!("sudo rpm -Uvh ./{asset}")
            } else {
                return None;
            };
            Some(format!(
                "curl -fsSLO {base}/SHA256SUMS && curl -fsSLO {base}/{asset} && sha256sum -c --ignore-missing SHA256SUMS && {install}"
            ))
        }
        InstallSource::InstallScript | InstallSource::Manual => {
            Some(format!("curl -fsSL {INSTALL_URL} | bash"))
        }
        InstallSource::DevBuild | InstallSource::Other => None,
    }
}

fn valid_cached(value: &str) -> Option<String> {
    parse_version(value).map(|_| value.to_string())
}

fn fetch_latest_tag() -> Option<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(3)))
        .timeout_per_read(Some(Duration::from_secs(5)))
        .timeout_per_write(Some(Duration::from_secs(5)))
        .build()
        .into();
    let response = agent
        .get(LATEST_URL)
        .header("User-Agent", "whoisthat-update-check")
        .call()
        .ok()?;
    parse_release_tag(&response.get_uri().to_string())
}

/// Run a release check in the background. `fetch = false` only re-uses the
/// cached version and detects the install source (for the every-launch popup).
pub fn spawn(
    fetch: bool,
    cached_latest: String,
    skipped: String,
    tx: mpsc::UnboundedSender<AppEvent>,
) {
    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            let source = detect_install_source();
            if matches!(source, InstallSource::DevBuild) {
                log::debug!("update check: development build, skipped");
                return None;
            }
            let latest = if fetch {
                let fetched = fetch_latest_tag();
                if fetched.is_none() {
                    log::debug!("update check: GitHub request failed, using cached version");
                }
                fetched.or_else(|| valid_cached(&cached_latest))
            } else {
                valid_cached(&cached_latest)
            }?;
            let current = env!("CARGO_PKG_VERSION").to_string();
            log::debug!("update check: current {current}, latest {latest}, source {source:?}");
            if !should_prompt(&latest, &current, &skipped) {
                return None;
            }
            Some(UpdateInfo {
                release_url: release_url(&latest),
                command: update_command(&source, &latest, std::env::consts::ARCH, &detect_tools()),
                current,
                latest,
                source,
            })
        })
        .await;

        match result {
            Ok(Some(info)) => {
                let _ = tx.send(AppEvent::UpdateAvailable(info));
            }
            Ok(None) => {}
            Err(e) => log::debug!("update check task failed: {e}"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions_with_and_without_prefix() {
        assert_eq!(parse_version("0.18.1"), Some((0, 18, 1)));
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("0.9.9"), Some((0, 9, 9)));
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert_eq!(parse_version("1.2.x"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn parses_release_tag_urls() {
        assert_eq!(
            parse_release_tag("https://github.com/kvunoff/whoisthat/releases/tag/v0.18.1"),
            Some("0.18.1".into())
        );
        assert_eq!(
            parse_release_tag("https://github.com/kvunoff/whoisthat/releases/tag/v0.18.1/"),
            Some("0.18.1".into())
        );
        assert_eq!(
            parse_release_tag("https://github.com/kvunoff/whoisthat/releases/tag/v0.18.1?x=1"),
            Some("0.18.1".into())
        );
        assert_eq!(
            parse_release_tag("https://github.com/evil/whoisthat/releases/tag/v0.18.1"),
            None
        );
        assert_eq!(
            parse_release_tag("https://github.com/kvunoff/whoisthat/releases/tag/core-v0.5.1"),
            None
        );
        assert_eq!(
            parse_release_tag("https://github.com/kvunoff/whoisthat/releases/tag/v0.18.1-rc1"),
            None
        );
        assert_eq!(
            parse_release_tag("https://github.com/kvunoff/whoisthat/releases"),
            None
        );
    }

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("0.18.2", "0.18.1"));
        assert!(is_newer("0.19.0", "0.18.9"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.18.1", "0.18.1"));
        assert!(!is_newer("0.18.0", "0.18.1"));
        assert!(!is_newer("garbage", "0.18.1"));
    }

    #[test]
    fn gates_checks_on_setting_time_and_env() {
        let now = 1_000_000i64;
        assert!(should_check_cheap(true, now, 0, false, false));
        assert!(should_check_cheap(
            true,
            now,
            now - CHECK_INTERVAL_SECS,
            false,
            false
        ));
        assert!(!should_check_cheap(
            true,
            now,
            now - CHECK_INTERVAL_SECS + 1,
            false,
            false
        ));
        assert!(should_check_cheap(true, now, now + 10, false, false)); // clock moved back
        assert!(!should_check_cheap(false, now, 0, false, false));
        assert!(!should_check_cheap(true, now, 0, true, false));
        assert!(!should_check_cheap(true, now, 0, false, true));
    }

    #[test]
    fn prompts_only_for_a_new_unskipped_version() {
        assert!(should_prompt("0.18.2", "0.18.1", ""));
        assert!(!should_prompt("0.18.1", "0.18.1", ""));
        assert!(!should_prompt("0.18.0", "0.18.1", ""));
        assert!(!should_prompt("0.18.2", "0.18.1", "0.18.2"));
    }

    #[test]
    fn detects_dev_build_paths() {
        assert!(is_dev_build(Path::new("/src/target/debug/whoisthat")));
        assert!(is_dev_build(Path::new("/src/target/release/whoisthat")));
        assert!(!is_dev_build(Path::new("/usr/bin/whoisthat")));
    }

    #[test]
    fn classifies_install_sources() {
        assert_eq!(
            classify(
                Path::new("/usr/bin/whoisthat"),
                Some((PkgManager::Pacman, "whoisthat-bin".into()))
            ),
            InstallSource::Pacman {
                package: "whoisthat-bin".into()
            }
        );
        assert_eq!(
            classify(
                Path::new("/usr/bin/whoisthat"),
                Some((PkgManager::Dpkg, "whoisthat".into()))
            ),
            InstallSource::Dpkg {
                package: "whoisthat".into()
            }
        );
        assert_eq!(
            classify(
                Path::new("/usr/bin/whoisthat"),
                Some((PkgManager::Rpm, "whoisthat".into()))
            ),
            InstallSource::Rpm {
                package: "whoisthat".into()
            }
        );
        assert_eq!(
            classify(Path::new("/usr/bin/whoisthat"), None),
            InstallSource::InstallScript
        );
        assert_eq!(
            classify(Path::new("/nix/store/abc-whoisthat/bin/whoisthat"), None),
            InstallSource::Other
        );
        assert_eq!(
            classify(Path::new("/opt/whoisthat"), None),
            InstallSource::Manual
        );
    }

    fn tools(paru: bool, yay: bool, dnf: bool, zypper: bool, rpm: bool) -> Tools {
        Tools {
            paru,
            yay,
            dnf,
            zypper,
            rpm,
        }
    }

    #[test]
    fn builds_package_commands_with_checksums() {
        let pacman = InstallSource::Pacman {
            package: "whoisthat-bin".into(),
        };
        assert_eq!(
            update_command(
                &pacman,
                "0.18.2",
                "x86_64",
                &tools(true, false, false, false, false)
            ),
            Some("paru -S whoisthat-bin".into())
        );
        assert_eq!(
            update_command(
                &pacman,
                "0.18.2",
                "x86_64",
                &tools(false, true, false, false, false)
            ),
            Some("yay -S whoisthat-bin".into())
        );
        assert_eq!(
            update_command(
                &pacman,
                "0.18.2",
                "x86_64",
                &tools(false, false, false, false, false)
            ),
            None
        );

        let deb = update_command(
            &InstallSource::Dpkg {
                package: "whoisthat".into(),
            },
            "0.18.2",
            "x86_64",
            &tools(false, false, false, false, false),
        )
        .unwrap();
        assert!(deb.contains("/download/v0.18.2/whoisthat_0.18.2-1_amd64.deb"));
        assert!(deb.contains("sha256sum -c --ignore-missing SHA256SUMS"));
        assert!(deb.contains("sudo apt install ./whoisthat_0.18.2-1_amd64.deb"));

        let rpm = update_command(
            &InstallSource::Rpm {
                package: "whoisthat".into(),
            },
            "0.18.2",
            "aarch64",
            &tools(false, false, true, false, false),
        )
        .unwrap();
        assert!(rpm.contains("/download/v0.18.2/whoisthat-0.18.2-1.aarch64.rpm"));
        assert!(rpm.contains("sudo dnf install ./whoisthat-0.18.2-1.aarch64.rpm"));

        let zypper = update_command(
            &InstallSource::Rpm {
                package: "whoisthat".into(),
            },
            "0.18.2",
            "x86_64",
            &tools(false, false, false, true, false),
        )
        .unwrap();
        assert!(zypper.contains("sudo zypper --non-interactive install"));

        let raw_rpm = update_command(
            &InstallSource::Rpm {
                package: "whoisthat".into(),
            },
            "0.18.2",
            "x86_64",
            &tools(false, false, false, false, true),
        )
        .unwrap();
        assert!(raw_rpm.contains("sudo rpm -Uvh"));
    }

    #[test]
    fn skips_commands_for_unknown_or_unsupported_targets() {
        assert_eq!(
            update_command(
                &InstallSource::Other,
                "0.18.2",
                "x86_64",
                &tools(true, true, true, true, true)
            ),
            None
        );
        assert_eq!(
            update_command(
                &InstallSource::Dpkg {
                    package: "whoisthat".into()
                },
                "0.18.2",
                "i686",
                &tools(false, false, false, false, false)
            ),
            None
        );
        assert_eq!(
            update_command(
                &InstallSource::InstallScript,
                "0.18.2",
                "x86_64",
                &tools(false, false, false, false, false)
            ),
            Some(format!("curl -fsSL {INSTALL_URL} | bash"))
        );
    }
}
