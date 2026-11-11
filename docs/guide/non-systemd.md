# Non-systemd systems (Artix, OpenRC, dinit, runit)

WhoisThat works without systemd. The TUI, CLI, core daemon and split-tunnel
launcher do not require it; only autostart and the split-tunnel launcher pick a
different backend.

Supported init systems: **OpenRC**, **dinit**, **runit**. s6 is documented below
as manual setup; sysvinit/Devuan are out of scope.

## What changes without systemd

| Feature | systemd hosts | non-systemd hosts |
| :--- | :--- | :--- |
| Core lifecycle | user unit + `systemctl --user` | init wrapper `run-core` + state files |
| `whoisthat run` | `systemd-run --user` scope | per-user `whoisthat-runner` (no sudo) |
| Autostart | user unit + linger | init service, or a `.desktop` entry |
| Split-tunnel cgroup | `user@<uid>.service` |

The daemon still runs as your user; it never runs as root.

## One-time setup

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

## `whoisthat run` without sudo

`whoisthat-runner` is started at boot by init, inside
`/sys/fs/cgroup/whoisthat-<uid>`, and runs as your user. `whoisthat run` talks to
it over `/run/whoisthat/<uid>/whoisthat/runner.sock` (0600); forked applications
inherit the cgroup. No sudo or password is involved.

If the runner is not running, `whoisthat run` fails with the command needed to
start it. The native "move the process into the cgroup" path is not used: the
kernel requires write access to the source cgroup, which a normal session does
not have.

Interactive programs work through a PTY proxy; `whoisthat run vim`, `top` and
`ssh` behave like normal launches, including Ctrl-C and window resizing.

## Autostart

```sh
whoisthat autostart status
whoisthat autostart enable --backend init
whoisthat autostart enable --backend desktop
whoisthat autostart disable
```

* `init` writes `~/.config/whoisthat/autostart`; the init service starts the
  core at boot. No root prompt is needed for the toggle.
* `desktop` writes `~/.config/autostart/whoisthat-core.desktop`; it needs a
  graphical session and uses the same runtime directory as the TUI.
* `whoisthat systemd …` is a legacy command and forces the systemd backend.

`core-stop` is session-scoped: the boot hook clears it, so `core stop` lasts
until the next reboot.

## Lifecycle

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

## Limitations

* One runner and one core per user; the core still owns one network namespace
  (one TUN name and one nft table), so simultaneous VPN sessions of different
  users are not supported.
* The runner service is installed for the user who ran the installer.
* `setpriv` from util-linux is required to drop privileges in the boot wrapper.
* A custom `XDG_CONFIG_HOME`/`XDG_DATA_HOME` must be passed to the init service
  at install time (`install-init` reads it from the environment).
* Encrypted `$HOME`: the boot service may start before the home is unlocked;
  use autostart via `.desktop` or start the core after login instead.
* cgroup v2 must be mounted. On Artix/OpenRC this is done by the `cgroups`
  service; on minimal systems mount it manually or via your init.

## s6 (manual)

s6 is not packaged, but `run-core` is a plain long-running wrapper:

```sh
# /etc/s6/sv/whoisthat-core/run
#!/bin/sh
exec /usr/lib/whoisthat/run-core /home/USER
```

Run it as the target user (`s6-setuidgid USER`) after the cgroup hook created
`/run/whoisthat/<uid>` and `/sys/fs/cgroup/whoisthat-<uid>`.
