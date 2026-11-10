//! Autostart backends: systemd user unit where systemd runs, the init service
//! on other systems, or a per-user XDG `.desktop` entry.

use std::fs;
use std::io::{self, Write};
use std::net::TcpStream;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::time::Duration;

use crate::lifecycle;
pub(crate) use crate::lifecycle::Backend;
use crate::platform::{self, InitServiceState};

#[derive(Debug, Clone)]
pub struct Status {
    pub available: Vec<Backend>,
    pub backend: Option<Backend>,
    pub enabled: bool,
    pub running: bool,
    pub wrapper_running: bool,
    pub path: Option<PathBuf>,
    pub warning: Option<String>,
}

fn backend_file() -> PathBuf {
    crate::config::config_dir().join("autostart-backend")
}

fn autostart_marker() -> PathBuf {
    crate::config::config_dir().join("autostart")
}

fn desktop_path() -> PathBuf {
    crate::config::config_dir()
        .parent()
        .map(|parent| parent.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("autostart")
        .join("whoisthat-core.desktop")
}

/// Pure availability matrix so tests do not need a real init system.
pub(crate) fn available_in(systemd: bool, init_configured: bool) -> Vec<Backend> {
    if systemd {
        vec![Backend::Systemd, Backend::Desktop]
    } else if init_configured {
        vec![Backend::Init, Backend::Desktop]
    } else {
        vec![Backend::Desktop]
    }
}

pub fn available() -> Vec<Backend> {
    let uid = crate::config::uid();
    let init_configured = matches!(
        platform::init_service_state(uid),
        InitServiceState::Configured
    );
    available_in(platform::has_systemd(), init_configured)
}

fn default_backend(systemd: bool, init_configured: bool, init_running: bool) -> Backend {
    if systemd {
        Backend::Systemd
    } else if init_configured && init_running {
        Backend::Init
    } else {
        Backend::Desktop
    }
}

pub fn artifact_path(backend: Backend) -> PathBuf {
    match backend {
        Backend::Systemd => crate::systemd::systemd_service_path(),
        Backend::Init => autostart_marker(),
        Backend::Desktop => desktop_path(),
    }
}

pub fn is_enabled(backend: Backend) -> bool {
    match backend {
        Backend::Systemd => platform::has_systemd() && crate::systemd::systemd_is_enabled(),
        Backend::Init => autostart_marker().exists(),
        Backend::Desktop => desktop_path().exists(),
    }
}

pub fn status() -> Status {
    let uid = crate::config::uid();
    let systemd = platform::has_systemd();
    let init_state = platform::init_service_state(uid);
    let init_configured = matches!(init_state, InitServiceState::Configured);
    let init_running = platform::init_service_running(uid);
    let available = available_in(systemd, init_configured);
    let selected = lifecycle::selected_backend();
    let backend =
        selected.or_else(|| Some(default_backend(systemd, init_configured, init_running)));
    let enabled = backend.is_some_and(is_enabled);
    let path = backend.map(artifact_path);
    let mut warning = None;
    if matches!(init_state, InitServiceState::Unknown(_)) && !systemd {
        warning = Some(
            "cannot verify the init service (/etc/whoisthat is unreadable); \
             autostart may need manual setup"
                .to_string(),
        );
    }
    if backend == Some(Backend::Desktop) && !graphical_session() {
        warning = Some("desktop autostart needs a graphical session".to_string());
    }
    if let Some(selected) = selected
        && !available.contains(&selected)
    {
        warning = Some(format!(
            "selected backend '{}' is not available on this system",
            selected.name()
        ));
    }
    if backend == Some(Backend::Init) && lifecycle::marker_path("core-stop").exists() {
        warning = Some("core is stopped until 'whoisthat core start' (or reboot)".to_string());
    }
    Status {
        available,
        backend,
        enabled,
        running: core_running(),
        wrapper_running: init_running,
        path,
        warning,
    }
}

fn graphical_session() -> bool {
    ["DISPLAY", "WAYLAND_DISPLAY", "XDG_CURRENT_DESKTOP"]
        .iter()
        .any(|key| std::env::var_os(key).is_some())
}

fn core_running() -> bool {
    use crate::core_client::connection::Endpoint;
    let endpoint = crate::config::load_config().endpoint();
    match endpoint {
        Endpoint::Unix(path) => std::os::unix::net::UnixStream::connect(path).is_ok(),
        Endpoint::Tcp { host, port } => {
            let address = format!("{host}:{port}");
            address
                .parse()
                .ok()
                .and_then(|address| {
                    TcpStream::connect_timeout(&address, Duration::from_millis(300)).ok()
                })
                .is_some()
        }
    }
}

fn write_backend(backend: Backend) -> Result<(), String> {
    let dir = crate::config::config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let path = backend_file();
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    file.write_all(backend.name().as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn remove_backend() {
    if let Err(e) = fs::remove_file(backend_file())
        && e.kind() != io::ErrorKind::NotFound
    {
        log::warn!("cannot remove autostart-backend: {e}");
    }
}

pub fn enable(backend: Backend) -> Result<(), String> {
    if !available().contains(&backend) {
        return Err(match backend {
            Backend::Systemd => "systemd is not running on this system".into(),
            Backend::Init => init_hint(),
            Backend::Desktop => "desktop autostart is not available".into(),
        });
    }
    disable_artifacts()?;
    match backend {
        Backend::Systemd => {
            let core = crate::core_spawn::find_core_binary();
            crate::systemd::setup_systemd_service(&core)?;
        }
        Backend::Init => {
            let uid = crate::config::uid();
            if !platform::init_service_running(uid) {
                return Err(format!(
                    "init autostart needs the whoisthat-core service to be running; {}",
                    init_hint()
                ));
            }
            lifecycle::remove_marker("core-stop");
            lifecycle::remove_marker("core-restart");
            lifecycle::remove_marker("core-start");
            lifecycle::write_marker("autostart")
                .map_err(|e| format!("cannot write autostart marker: {e}"))?;
            if !core_running() {
                lifecycle::write_marker("core-start")
                    .map_err(|e| format!("cannot request core start: {e}"))?;
            }
        }
        Backend::Desktop => write_desktop_entry()?,
    }
    write_backend(backend)
}

fn init_hint() -> String {
    "init autostart is not configured; run: sudo /usr/lib/whoisthat/install-init core enable $USER $HOME"
        .into()
}

pub fn disable() -> Result<(), String> {
    disable_artifacts()?;
    remove_backend();
    Ok(())
}

fn disable_artifacts() -> Result<(), String> {
    let backend = lifecycle::selected_backend().or_else(|| {
        let uid = crate::config::uid();
        let configured = matches!(
            platform::init_service_state(uid),
            InitServiceState::Configured
        );
        Some(default_backend(
            platform::has_systemd(),
            configured,
            platform::init_service_running(uid),
        ))
    });
    match backend {
        Some(Backend::Systemd) => crate::systemd::teardown_systemd_service()?,
        Some(Backend::Init) => {
            lifecycle::remove_marker("autostart");
            lifecycle::remove_marker("core-run");
            lifecycle::remove_marker("core-start");
            lifecycle::remove_marker("core-restart");
            lifecycle::write_marker("core-stop")
                .map_err(|e| format!("cannot write core-stop: {e}"))?;
        }
        Some(Backend::Desktop) => {
            let path = desktop_path();
            if let Err(e) = fs::remove_file(&path)
                && e.kind() != io::ErrorKind::NotFound
            {
                return Err(format!("cannot remove {}: {e}", path.display()));
            }
        }
        None => {}
    }
    Ok(())
}

fn desktop_exec_quote(path: &str) -> String {
    format!(
        "\"{}\"",
        path.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%")
    )
}

fn write_desktop_entry() -> Result<(), String> {
    let core = crate::core_spawn::find_core_binary();
    let core = std::fs::canonicalize(&core)
        .map_err(|e| format!("core binary '{core}' is not usable: {e}"))?;
    if !core.is_file() {
        return Err(format!("core binary is not a file: {}", core.display()));
    }
    let uid = crate::config::uid();
    let exec = format!(
        "/usr/bin/env XDG_RUNTIME_DIR=/run/whoisthat/{uid} {}",
        desktop_exec_quote(&core.to_string_lossy())
    );
    let entry = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=WhoisThat Core\n\
         Exec={exec}\n\
         Terminal=false\n\
         NoDisplay=true\n\
         X-GNOME-Autostart-enabled=true\n"
    );
    let path = desktop_path();
    let dir = path.parent().expect("desktop path has a parent");
    fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    fs::write(&path, entry).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644))
        .map_err(|e| format!("cannot chmod {}: {e}", path.display()))
}

pub fn toggle(backend: Option<Backend>) -> Result<Status, String> {
    let current = status();
    if current.enabled {
        disable()?;
    } else {
        let target = backend
            .or_else(|| current.available.first().copied())
            .ok_or_else(|| "no autostart backend is available".to_string())?;
        enable(target)?;
    }
    Ok(status())
}

pub fn parse_backend(value: Option<&str>) -> Result<Option<Backend>, String> {
    match value {
        None => Ok(None),
        Some(value) => Backend::parse(value)
            .map(Some)
            .ok_or_else(|| format!("unknown backend '{value}' (use systemd, init or desktop)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn availability_matrix() {
        assert_eq!(
            available_in(true, true),
            vec![Backend::Systemd, Backend::Desktop]
        );
        assert_eq!(
            available_in(false, true),
            vec![Backend::Init, Backend::Desktop]
        );
        assert_eq!(available_in(false, false), vec![Backend::Desktop]);
    }

    #[test]
    fn desktop_exec_escapes_percent_and_quotes() {
        assert_eq!(
            desktop_exec_quote("/opt/a%b/whoisthat"),
            "\"/opt/a%%b/whoisthat\""
        );
        assert_eq!(desktop_exec_quote("/opt/a\"b\\c"), "\"/opt/a\\\"b\\\\c\"");
    }

    #[test]
    fn backend_parse_rejects_unknown() {
        assert_eq!(parse_backend(Some("init")), Ok(Some(Backend::Init)));
        assert!(parse_backend(Some("bogus")).is_err());
        assert_eq!(parse_backend(None), Ok(None));
    }
}
