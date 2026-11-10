//! Autostart backend selection and the session-scoped lifecycle markers shared
//! by the TUI/CLI and the init wrapper (`/usr/lib/whoisthat/run-core`).
//!
//! The marker files live in the user's config dir and are always 0600: the TUI
//! and the core run as the same user, so the wrapper can read the user's
//! intent without any privilege.

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

use crate::platform::{self, InitServiceState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Backend {
    Systemd,
    Init,
    Desktop,
}

impl Backend {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Backend::Systemd => "systemd",
            Backend::Init => "init",
            Backend::Desktop => "desktop",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "systemd" => Some(Backend::Systemd),
            "init" => Some(Backend::Init),
            "desktop" => Some(Backend::Desktop),
            _ => None,
        }
    }
}

fn autostart_backend_file() -> PathBuf {
    crate::config::config_dir().join("autostart-backend")
}

/// The explicitly selected backend, if the file is present and valid.
pub(crate) fn selected_backend() -> Option<Backend> {
    fs::read_to_string(autostart_backend_file())
        .ok()
        .and_then(|value| Backend::parse(value.trim()))
}

/// The backend that actually owns the core lifecycle right now.
///
/// An installed init service is not enough: without a running wrapper the core
/// must still be spawned normally, otherwise nothing would ever start it.
pub(crate) fn effective_backend() -> Backend {
    if let Some(backend) = selected_backend() {
        return backend;
    }
    if platform::has_systemd() {
        return Backend::Systemd;
    }
    let uid = crate::config::uid();
    if matches!(
        platform::init_service_state(uid),
        InitServiceState::Configured
    ) && platform::init_service_running(uid)
    {
        return Backend::Init;
    }
    Backend::Desktop
}

pub(crate) fn marker_path(name: &str) -> PathBuf {
    crate::config::config_dir().join(name)
}

pub(crate) fn write_marker(name: &str) -> io::Result<()> {
    let dir = crate::config::config_dir();
    fs::create_dir_all(&dir)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(dir.join(name))?;
    file.write_all(b"1")
}

pub(crate) fn remove_marker(name: &str) {
    let path = marker_path(name);
    if let Err(e) = fs::remove_file(&path)
        && e.kind() != io::ErrorKind::NotFound
    {
        log::warn!("could not remove {}: {e}", path.display());
    }
}
