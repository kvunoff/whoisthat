//! Distro/init detection.
//!
//! Every place that would otherwise assume systemd asks this module first, so
//! non-systemd distributions (Artix/OpenRC, dinit, runit) get an honest answer
//! instead of a silently failed `systemctl` call.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InitSystem {
    Systemd,
    OpenRC,
    Dinit,
    Runit,
    S6,
    Unknown,
}

impl InitSystem {
    pub(crate) fn name(self) -> &'static str {
        match self {
            InitSystem::Systemd => "systemd",
            InitSystem::OpenRC => "openrc",
            InitSystem::Dinit => "dinit",
            InitSystem::Runit => "runit",
            InitSystem::S6 => "s6",
            InitSystem::Unknown => "unknown",
        }
    }
}

/// Filesystem roots, overridable so tests never have to look at the real
/// /etc, /run, /sys or /usr/lib of the machine running them.
#[derive(Debug, Clone)]
pub(crate) struct PlatformPaths {
    pub(crate) etc: PathBuf,
    pub(crate) run: PathBuf,
    pub(crate) sys_cgroup: PathBuf,
    pub(crate) lib: PathBuf,
    pub(crate) bin_dirs: Vec<PathBuf>,
}

impl Default for PlatformPaths {
    fn default() -> Self {
        Self {
            etc: PathBuf::from("/etc"),
            run: PathBuf::from("/run"),
            sys_cgroup: PathBuf::from("/sys/fs/cgroup"),
            lib: PathBuf::from("/usr/lib"),
            bin_dirs: std::env::var_os("PATH")
                .map(|path| std::env::split_paths(&path).collect())
                .unwrap_or_default(),
        }
    }
}

/// `/run/systemd/system` is the canonical sign of a *running* systemd.
/// `systemctl` can be installed without it being PID 1 (chroots, Artix with
/// elogind), which is why the binary alone is not a reliable signal.
pub(crate) fn has_systemd() -> bool {
    has_systemd_in(Path::new("/run/systemd/system"))
}

pub(crate) fn has_systemd_in(root: &Path) -> bool {
    root.is_dir()
}

pub(crate) fn detect_init() -> InitSystem {
    detect_init_in(&PlatformPaths::default())
}

pub(crate) fn detect_init_in(paths: &PlatformPaths) -> InitSystem {
    if has_systemd_in(&paths.run.join("systemd/system")) {
        return InitSystem::Systemd;
    }
    if paths.run.join("openrc").exists() || bin_exists(paths, "openrc-run") {
        return InitSystem::OpenRC;
    }
    if paths.run.join("dinitctl").exists()
        || paths.run.join("dinitctl/socket").exists()
        || bin_exists(paths, "dinitctl")
    {
        return InitSystem::Dinit;
    }
    if paths.run.join("runit").exists()
        || paths.run.join("runit/service").exists()
        || paths.run.join("runit/supervise").exists()
        || bin_exists(paths, "runsvdir")
    {
        return InitSystem::Runit;
    }
    if paths.run.join("s6").exists()
        || paths.run.join("command/s6-svscan").exists()
        || bin_exists(paths, "s6-svscan")
    {
        return InitSystem::S6;
    }
    InitSystem::Unknown
}

fn bin_exists(paths: &PlatformPaths, name: &str) -> bool {
    paths.bin_dirs.iter().any(|dir| dir.join(name).is_file())
}

/// State of the per-uid init service.
///
/// `Unknown` means the check could not be performed (unreadable /etc), not
/// "not configured": callers that gate spawning must only block on
/// `Configured`, never on `Unknown`, or a locked-down /etc would stop the
/// daemon from starting at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InitServiceState {
    Configured,
    NotConfigured,
    Unknown(String),
}

pub(crate) fn init_service_state(uid: u32) -> InitServiceState {
    init_service_state_in(&PlatformPaths::default(), uid)
}

pub(crate) fn init_service_state_in(paths: &PlatformPaths, uid: u32) -> InitServiceState {
    let service_files = [
        paths.etc.join("init.d").join("whoisthat-core"),
        paths.etc.join("dinit.d").join("whoisthat-core"),
        paths.etc.join("runit/sv").join("whoisthat-core"),
    ];
    if !service_files.iter().any(|path| path.exists()) {
        return InitServiceState::NotConfigured;
    }

    let marker = paths.etc.join("whoisthat").join(format!("init-user-{uid}"));
    match std::fs::metadata(&marker) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return InitServiceState::NotConfigured;
        }
        Err(e) => {
            return InitServiceState::Unknown(format!("cannot read {}: {e}", marker.display()));
        }
    }

    match service_enabled(paths) {
        Ok(true) => InitServiceState::Configured,
        Ok(false) => InitServiceState::NotConfigured,
        Err(e) => InitServiceState::Unknown(e),
    }
}

fn service_enabled(paths: &PlatformPaths) -> Result<bool, String> {
    // OpenRC: a symlink under /etc/runlevels/<level>/whoisthat-core.
    let runlevels = paths.etc.join("runlevels");
    match std::fs::read_dir(&runlevels) {
        Ok(entries) => {
            for entry in entries.flatten() {
                if entry.path().join("whoisthat-core").exists() {
                    return Ok(true);
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("cannot read {}: {e}", runlevels.display())),
    }

    // dinit: enabled through boot.d; runit: a symlink in the active runsvdir.
    if paths.etc.join("dinit.d/boot.d/whoisthat-core").exists() {
        return Ok(true);
    }
    if paths
        .etc
        .join("runit/runsvdir/default/whoisthat-core")
        .exists()
    {
        return Ok(true);
    }
    Ok(paths.run.join("runit/service/whoisthat-core").exists())
}

/// Best-effort liveness of `/usr/lib/whoisthat/run-core` for this uid. Used to
/// decide whether the init backend actually owns the core lifecycle. A false
/// negative is safe: it only allows the normal spawn path.
pub(crate) fn init_service_running(uid: u32) -> bool {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    entries.flatten().any(|entry| {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            return false;
        };
        let Ok(cmdline) = std::fs::read(entry.path().join("cmdline")) else {
            return false;
        };
        let runs_wrapper = cmdline
            .split(|byte| *byte == 0)
            .any(|arg| arg.ends_with(b"/run-core"));
        runs_wrapper && process_uid(pid) == Some(uid)
    })
}

fn process_uid(pid: u32) -> Option<u32> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|uid| uid.parse().ok())
}

/// True when the process was started through sudo/pkexec. Such a process must
/// not write user-owned state (config, lifecycle markers): they would end up
/// root-owned and unreadable by the core, and the cgroup uid would be wrong.
pub(crate) fn started_via_privilege_wrapper() -> bool {
    crate::config::uid() != crate::config::effective_uid()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("whoisthat-platform-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn detects_systemd_by_runtime_marker() {
        let root = temp_dir("systemd");
        assert!(!has_systemd_in(&root.join("systemd/system")));
        std::fs::create_dir_all(root.join("systemd/system")).expect("create marker");
        assert!(has_systemd_in(&root.join("systemd/system")));
        let _ = std::fs::remove_dir_all(root);
    }

    fn paths(root: &Path) -> PlatformPaths {
        PlatformPaths {
            etc: root.join("etc"),
            run: root.join("run"),
            sys_cgroup: root.join("sys/fs/cgroup"),
            lib: root.join("usr/lib"),
            bin_dirs: vec![root.join("bin")],
        }
    }

    #[test]
    fn detects_init_system_in_priority_order() {
        let root = temp_dir("init");
        let p = paths(&root);
        std::fs::create_dir_all(&p.run).expect("run dir");
        std::fs::create_dir_all(&p.bin_dirs[0]).expect("bin dir");
        assert_eq!(detect_init_in(&p), InitSystem::Unknown);

        std::fs::write(p.bin_dirs[0].join("runsvdir"), "").expect("runsvdir");
        assert_eq!(detect_init_in(&p), InitSystem::Runit);

        std::fs::write(p.bin_dirs[0].join("dinitctl"), "").expect("dinitctl");
        assert_eq!(detect_init_in(&p), InitSystem::Dinit);

        std::fs::create_dir_all(p.run.join("openrc")).expect("openrc");
        assert_eq!(detect_init_in(&p), InitSystem::OpenRC);

        std::fs::create_dir_all(p.run.join("systemd/system")).expect("systemd");
        assert_eq!(detect_init_in(&p), InitSystem::Systemd);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn init_service_state_requires_marker_and_enabled_service() {
        let root = temp_dir("service");
        let p = paths(&root);
        std::fs::create_dir_all(p.etc.join("init.d")).expect("init.d");
        std::fs::create_dir_all(p.etc.join("whoisthat")).expect("marker dir");
        assert_eq!(
            init_service_state_in(&p, 1000),
            InitServiceState::NotConfigured
        );

        std::fs::write(p.etc.join("init.d/whoisthat-core"), "").expect("service");
        // Marker without an enabled symlink is still not configured.
        std::fs::write(p.etc.join("whoisthat/init-user-1000"), "1000").expect("marker");
        assert_eq!(
            init_service_state_in(&p, 1000),
            InitServiceState::NotConfigured
        );
        // Marker for a different uid must not count.
        std::fs::write(p.etc.join("whoisthat/init-user-1001"), "1001").expect("other marker");
        std::fs::create_dir_all(p.etc.join("runlevels/default")).expect("runlevels");
        std::fs::write(p.etc.join("runlevels/default/whoisthat-core"), "").expect("symlink stub");
        assert_eq!(
            init_service_state_in(&p, 1000),
            InitServiceState::Configured
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
