use std::path::{Path, PathBuf};

use clap::CommandFactory;

use crate::cli::Cli;

fn completion_script(shell: clap_complete::Shell) -> Vec<u8> {
    let mut cmd = Cli::command();
    let mut out = Vec::new();
    clap_complete::generate(shell, &mut cmd, "whoisthat", &mut out);
    out
}

/// Where a given shell auto-loads user completions.
fn target_for(
    shell: &str,
    home: &Path,
    data: &Path,
    config: &Path,
) -> Option<(PathBuf, clap_complete::Shell)> {
    match shell {
        "bash" => Some((
            data.join("bash-completion/completions/whoisthat"),
            clap_complete::Shell::Bash,
        )),
        // zsh has no auto-loaded per-user directory; ~/.zfunc works once it is
        // in fpath. We still write the script and log a hint.
        "zsh" => Some((home.join(".zfunc/_whoisthat"), clap_complete::Shell::Zsh)),
        "fish" => Some((
            config.join("fish/completions/whoisthat.fish"),
            clap_complete::Shell::Fish,
        )),
        _ => None,
    }
}

/// System-wide paths written by packages and `install.sh`. When one exists the
/// TUI must not also create a per-user copy.
fn system_path(shell: &str) -> Option<&'static str> {
    match shell {
        "bash" => Some("/usr/share/bash-completion/completions/whoisthat"),
        "zsh" => Some("/usr/share/zsh/site-functions/_whoisthat"),
        "fish" => Some("/usr/share/fish/vendor_completions.d/whoisthat.fish"),
        _ => None,
    }
}

/// Writes the script only when it differs, so a running TUI does not rewrite
/// the file on every start. Returns whether anything was written.
fn install_at(path: &Path, script: &[u8]) -> std::io::Result<bool> {
    if std::fs::read(path)
        .map(|old| old == script)
        .unwrap_or(false)
    {
        return Ok(false);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, script)?;
    Ok(true)
}

/// Silently keeps user completions in sync with the installed binary, the same
/// way `ensure_desktop_registered` keeps the `wit://` handler current.
pub fn ensure_completions_installed() {
    std::thread::spawn(|| {
        let Some(shell) = std::env::var("SHELL").ok().and_then(|s| {
            Path::new(&s)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        }) else {
            return;
        };
        if system_path(&shell).is_some_and(|path| Path::new(path).exists()) {
            return;
        }
        let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
            return;
        };
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"));
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));

        let Some((path, kind)) = target_for(&shell, &home, &data, &config) else {
            return;
        };
        match install_at(&path, &completion_script(kind)) {
            Ok(true) => {
                log::info!("Wrote {} completions to {}", shell, path.display());
                if shell == "zsh" {
                    log::info!(
                        "Add `fpath+={}` before compinit to enable them",
                        path.parent().unwrap_or(Path::new("")).display()
                    );
                }
            }
            Ok(false) => {}
            Err(e) => log::warn!(
                "Cannot write {} completions to {}: {e}",
                shell,
                path.display()
            ),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_shells_only() {
        let home = Path::new("/home/u");
        let data = Path::new("/data");
        let config = Path::new("/config");
        assert_eq!(
            target_for("bash", home, data, config).unwrap().0,
            Path::new("/data/bash-completion/completions/whoisthat")
        );
        assert_eq!(
            target_for("zsh", home, data, config).unwrap().0,
            Path::new("/home/u/.zfunc/_whoisthat")
        );
        assert_eq!(
            target_for("fish", home, data, config).unwrap().0,
            Path::new("/config/fish/completions/whoisthat.fish")
        );
        assert!(target_for("nushell", home, data, config).is_none());
    }

    #[test]
    fn system_paths_are_absolute() {
        for shell in ["bash", "zsh", "fish"] {
            assert!(system_path(shell).unwrap().starts_with('/'));
        }
        assert!(system_path("nushell").is_none());
    }

    #[test]
    fn install_is_idempotent() {
        let dir =
            std::env::temp_dir().join(format!("whoisthat-completions-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("whoisthat");
        assert!(install_at(&path, b"one").unwrap());
        assert!(!install_at(&path, b"one").unwrap());
        assert!(install_at(&path, b"two").unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn scripts_are_not_empty() {
        for shell in [clap_complete::Shell::Bash, clap_complete::Shell::Zsh] {
            assert!(!completion_script(shell).is_empty());
        }
    }
}
