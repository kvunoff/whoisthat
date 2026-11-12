---
title: "Platforms & Service Management"
description: "Run WhoisThat with systemd, OpenRC, dinit, runit, or desktop autostart."
audience: "operator"
type: "how-to"
---

# Platforms & Service Management

WhoisThat runs on systemd hosts and on non-systemd systems. The TUI, CLI and
core daemon do not require systemd; only autostart, the core lifecycle, and the
split-tunnel launcher choose a backend.

Supported init systems: **systemd**, **OpenRC**, **dinit**, and **runit**. s6 is
documented below as manual setup; sysvinit/Devuan are out of scope.

## What changes by platform

| Feature | systemd hosts | non-systemd hosts |
| :--- | :--- | :--- |
| Core lifecycle | user unit + `systemctl --user` | init wrapper `run-core` + state files |
| `whoisthat run` | `systemd-run --user` scope | per-user `whoisthat-runner` (no sudo) |
| Autostart | user unit + linger | init service or a `.desktop` entry |
| Split-tunnel cgroup | `user@<uid>.service` | `/sys/fs/cgroup/whoisthat-<uid>` |

The daemon runs as your user; it never runs as root.

## Systemd user service

WhoisThat can run as a persistent systemd user service, launching at boot even
before you log in graphically via user lingering. This is one of three
autostart backends. On systems without systemd use the `init` service or a
desktop entry. The legacy `whoisthat systemd …` command still forces this
backend.

### Unit file

When you toggle **Autostart** in Settings (or run
`whoisthat autostart enable --backend systemd`; the legacy
`whoisthat systemd enable` and `whoisthat --systemd-on` aliases still work),
WhoisThat generates and enables the following unit file. The `ExecStart` path is
filled in from the resolved core binary. The log level is not pinned in the
unit: the daemon starts at `info`, and the TUI applies the current Settings
value over IPC as soon as it connects.

**Location**: `~/.config/systemd/user/whoisthat-core.service`

```ini
[Unit]
Description=WhoisThat VPN Core
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/bin/whoisthat-core
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

Enabling the unit does **not** start it immediately. Run
`systemctl --user start whoisthat-core` (or log out and back in) to launch it now.

### Boot autostart with user lingering

By default, systemd user services start when you log into a graphical or TTY
session and stop when you log out. To start the VPN engine at boot before
interactive login:

```bash
# Verify current lingering status
loginctl show-user $USER --property=Linger

# Enable user lingering
sudo loginctl enable-linger $USER
```

::: tip Automated by the TUI
When toggling **Autostart** in the TUI Settings tab, WhoisThat checks linger
status and prompts for polkit authentication to run `loginctl enable-linger` if
required.
:::

### Service management

```bash
# Inspect current service status
systemctl --user status whoisthat-core

# Start, stop, or restart the daemon
systemctl --user start whoisthat-core
systemctl --user stop whoisthat-core
systemctl --user restart whoisthat-core

# The kill-switch (when enabled) stays armed after a stop; disable it first or
# remove the table: sudo nft delete table inet whoisthat_ks

# View journal logs
journalctl --user -u whoisthat-core -f
```

### Detaching vs systemd

- **Detaching with `q`** exits the TUI while leaving the daemon running for the
  current login session. After a reboot it starts only if the service is active.
- **The systemd user service** starts the daemon during boot, reconnects
  automatically when **Autoconnect** is enabled, and restarts after a crash.

## Non-systemd systems

WhoisThat works without systemd. The TUI, CLI, core daemon and split-tunnel
launcher do not require it; only autostart, the core lifecycle, and the
split-tunnel launcher pick a different backend.

### One-time setup

Install with the universal script (`sudo ./install.sh --from-source` or a
release), or with the AUR package. The installer copies the helpers, enables
the cgroup boot hook, and starts the services.

Manual equivalent:

```sh
sudo /usr/lib/whoisthat/cgroup-setup "$USER"
sudo /usr/lib/whoisthat/install-init cgroup enable
sudo /usr/lib/whoisthat/install-init runner enable "$USER" "$HOME"
sudo /usr/lib/whoisthat/install-init core enable "$USER" "$HOME"
```

`install-init` detects the running init from `/run` and the PATH, and renders
the matching service. `install-init remove` removes everything again.

### `whoisthat run` without sudo

`whoisthat-runner` is started at boot by init inside
`/sys/fs/cgroup/whoisthat-<uid>` and runs as your user. `whoisthat run` talks to
it over `/run/whoisthat/<uid>/whoisthat/runner.sock` (0600); forked applications
inherit the cgroup. No sudo or password is involved.

If the runner is not running, `whoisthat run` fails with the command needed to
start it. The native "move the process into the cgroup" path is not used: the
kernel requires write access to the source cgroup, which a normal session does
not have.

Interactive programs work through a PTY proxy; `whoisthat run vim`, `top` and
`ssh` behave like normal launches, including Ctrl-C and window resizing.

### Autostart

```sh
whoisthat autostart status
whoisthat autostart enable --backend init
whoisthat autostart enable --backend desktop
whoisthat autostart disable
```

- `init` writes `~/.config/whoisthat/autostart`; the init service starts the
  core at boot. No root prompt is needed for the toggle.
- `desktop` writes `~/.config/autostart/whoisthat-core.desktop`; it needs a
  graphical session and uses the same runtime directory as the TUI.
- `whoisthat systemd …` is a legacy command and forces the systemd backend.

`core-stop` is session-scoped: the boot hook clears it, so `core stop` lasts
until the next reboot.

### Lifecycle

```sh
whoisthat core status
whoisthat core start
whoisthat core stop
whoisthat core restart
```

The init wrapper (`/usr/lib/whoisthat/run-core`) keeps the core alive and
restarts it after a crash. Starts, stops and restarts are requested through
user-owned marker files; the wrapper and the TUI always agree on the runtime
directory `/run/whoisthat/<uid>`.

### Limitations

- One runner and one core per user; the core still owns one network namespace
  (one TUN name and one nft table), so simultaneous VPN sessions of different
  users are not supported.
- The runner service is installed for the user who ran the installer.
- `setpriv` from util-linux is required to drop privileges in the boot wrapper.
- A custom `XDG_CONFIG_HOME`/`XDG_DATA_HOME` must be passed to the init service
  at install time (`install-init` reads it from the environment).
- Encrypted `$HOME`: the boot service may start before the home is unlocked;
  use autostart via `.desktop` or start the core after login instead.
- cgroup v2 must be mounted. On Artix/OpenRC this is done by the `cgroups`
  service; on minimal systems mount it manually or via your init.

### s6 (manual)

s6 is not packaged, but `run-core` is a plain long-running wrapper:

```sh
# /etc/s6/sv/whoisthat-core/run
#!/bin/sh
exec /usr/lib/whoisthat/run-core /home/USER
```

Run it as the target user (`s6-setuidgid USER`) after the cgroup hook created
`/run/whoisthat/<uid>` and `/sys/fs/cgroup/whoisthat-<uid>`.

## See also

- [Installation](/guide/installation)
- [Maintenance, Upgrading & Recovery](/guide/maintenance)
- [Split Tunneling](/usage/split-tunnel)
- [Systemd user service reference](/reference/configuration)
