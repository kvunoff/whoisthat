---
title: "Split Tunneling"
description: "Run selected applications inside or outside the VPN tunnel with cgroup v2."
audience: "user"
type: "how-to"
---

# Split Tunneling (cgroup v2)

Split Tunneling allows you to route specific applications differently from the rest of your system. You can bypass the VPN for domestic banking or high-speed gaming while keeping the rest of your OS protected, or route only a specific web browser through the tunnel.

---

## Operating Modes

Configure the mode in the TUI (**Settings → Network → Split tunnel**) or via the CLI:

### `exclude` Mode *(Recommended)*
* **Entire Operating System**: Protected and routed through the VPN tunnel.
* **Launched Applications** (`whoisthat run`): **Bypass the VPN**, connecting directly through your physical gateway.
* **Ideal For**: Spotify, domestic banking portals, Russian web services (`.ru` / Госуслуги), online gaming (Steam, Discord voice), torrent clients, and local network tools.

::: tip Default is `off`
Split tunneling is disabled by default. Set it to `exclude` or `include` explicitly; with `off`, `whoisthat run` launches the app but installs no split rules.
:::

### `include` Mode
* **Entire Operating System**: Connects unprotected via your physical gateway.
* **Launched Applications** (`whoisthat run`): **Exclusively routed through the VPN**.
* **Ideal For**: Dedicated private browsing sessions, scraping scripts, or testing international endpoints without affecting system-wide network routing.

::: warning Security Consideration
In `include` mode, only processes explicitly launched inside the split slice are protected by the VPN. All other applications and background traffic connect unprotected over your physical connection.
:::

::: danger Kill-Switch incompatibility
`include` mode cannot be combined with the Kill-Switch: the kill-switch blocks everything outside the tunnel, which is the exact traffic include mode is supposed to leave on the physical interface. The core rejects enabling either setting while the other is active and normalizes a hand-edited config to `kill-switch: false`.
:::

### DNS Behaviour

DNS follows the same split boundary:

* **`exclude`** — the system-wide Happ DNS hijack stays active. Excluded applications carry `fwmark 1`, and the DNS hijack returns early for marked sockets, so they resolve directly through their normal resolver instead of the tunnel sink.
* **`include`** — the Happ DNS hijack is disabled, and FakeDNS is therefore off as well. Included applications still use the system resolver, which lives outside the split slice, so their DNS is not tunnelled. This is intentional: hijacking port 53 system-wide would break every non-included application.
* **`off`** — no split rules are installed; all DNS follows the normal TUN/Happ path.
* **Manual routing** — the legacy hijack to the configured resolver remains in place in every split mode.

When the split mode changes while TUN is active, the hijack is reconciled together with the cgroup and policy-routing rules.

---

## Launch backends

`whoisthat run` picks the backend automatically. There is no user-facing switch:
the client checks for a live systemd (`/run/systemd/system`) and uses
`systemd-run` when present; otherwise it talks to the per-user runner. In both
cases the application gets a new process inside a split cgroup, not a migrated
PID.

| Backend | Selected when | cgroup | Foreground | Detached |
| :--- | :--- | :--- | :--- | :--- |
| **systemd** | `has_systemd()` | `user.slice/user-<uid>.slice/user@<uid>.service/whoisthat_split.slice` | `systemd-run --user --collect --slice=... --scope -- <argv>` | transient service (`--same-dir`, GUI env) |
| **runner** | no systemd | `/sys/fs/cgroup/whoisthat-<uid>` | runner PTY or pipes | double-fork inside the cgroup |
| **unavailable** | runner not installed/running | — | explicit error + setup hint | explicit error + setup hint |

### systemd hosts

The client runs:

```bash
systemd-run --user --collect --slice=whoisthat_split.slice --scope -- <argv>
```

for foreground use, and a transient service with `--same-dir` plus
`DISPLAY`, `WAYLAND_DISPLAY` and `XDG_CURRENT_DESKTOP` for detached use. The
application's exit code is propagated. No sudo or PID migration is involved.

This path needs a running systemd user manager and `systemd-run`. If
`systemd-run` is missing, the command fails with code `127` and says that
systemd-tools are required; it does not silently fall back to the runner.

### Non-systemd hosts

The client connects to the per-user runner:

```text
/run/whoisthat/<uid>/whoisthat/runner.sock   (mode 0600)
```

The runner is started at boot by the init service and lives inside
`/sys/fs/cgroup/whoisthat-<uid>`. It runs as your user; the root wrapper
(`run-runner`) only places it in the cgroup and drops privileges with
`setpriv`. Applications forked by the runner inherit that cgroup.

- **Foreground with a terminal**: the runner allocates a PTY, forwards
  stdin/stdout/stderr, signals and window resizes, and returns the application's
  real exit code. `vim`, `top` and `ssh` behave like normal launches.
- **Foreground without a terminal**: the runner uses pipes; closing stdin ends
  the child's stdin, but the session stays alive until the application exits.
- **Detached (`-d`)**: the runner double-forks with `setsid`, attaches
  `/dev/null` to stdin/stdout/stderr, and returns after the process starts. The
  application keeps running after the terminal closes.

The client forwards its environment, working directory and umask. The runner
re-validates the request (argv ≤ 128 KiB, env ≤ 1 MiB, absolute cwd, no NUL) and
verifies that the peer UID matches the runner user.

### Setup and failure modes

The runner is a non-systemd backend; `install-init runner enable` refuses to
install it on a systemd host.

```bash
sudo /usr/lib/whoisthat/cgroup-setup "$USER"
sudo /usr/lib/whoisthat/install-init runner enable "$USER" "$HOME"
```

If the runner is not running, `whoisthat run` prints the socket path and the
start command for the detected init (`rc-service`, `dinitctl`, `sv`, or a
generic hint). If the cgroup directory is missing, the root wrapper logs
`run-runner: ... run: sudo /usr/lib/whoisthat/cgroup-setup <uid>` and retries;
the runner will not start until the directory exists.

`whoisthat run` refuses to run under `sudo`/`pkexec`: the cgroup and UID would
be wrong. It must be started as the target user.

Exit codes: `2` means no application was given, `127` means `systemd-run` is
missing on a systemd host, `1` means the runner/setup failed, and otherwise the
application's own exit code is propagated.

## Launching applications

### Detached mode (`-d`, `--detach`, `-b`)

Use detached mode for GUI applications that should outlive the terminal:

```bash
whoisthat run -d spotify
whoisthat run -d google-chrome-stable
whoisthat run -b telegram-desktop
```

On systemd this is a transient service under `whoisthat_split.slice` with the
graphical environment forwarded; on non-systemd it is a double-fork by the
runner. The application keeps running after the shell exits. The `-d` flag must
come before the application name; after it, it is an application argument.

### Interactive mode

Use foreground mode for tools where you need the output, input or exit code:

```bash
whoisthat run curl 2ip.io
whoisthat run mpv "https://example.com/video.mp4"
```

On systemd this is a `--scope`; on non-systemd the runner provides a PTY when
stdin is a terminal, otherwise pipes. Standard I/O, signals, terminal resize and
the exit code are preserved.

## Crucial Note: Single-Instance Applications

Many popular Linux desktop applications (such as **Spotify**, **Google Chrome**, **Chromium**, **Discord**, and **Slack**) enforce a single running instance via D-Bus IPC:

1. When you type `whoisthat run -d spotify` while Spotify is already running minimized to tray, the new process detects the existing instance and sends an IPC message to open the window.
2. The newly spawned split-tunnel process exits immediately.
3. Because the original instance was started *outside* the split slice, it **remains in its original cgroup and continues using the default system route!**

### Proper Workflow for Single-Instance Apps:

1. **Quit the existing instance completely**:
   ```bash
   pkill -9 spotify
   # or for Chrome / Discord:
   pkill -f chrome
   pkill -f discord
   ```
2. **Launch via `whoisthat run -d`**:
   ```bash
   whoisthat run -d spotify
   ```

*(Alternative for Chrome/Chromium)*: If you want to run one Chrome instance through the VPN and simultaneously run another Chrome instance bypassing the VPN, launch the split instance with a separate user data directory:

```bash
whoisthat run -d google-chrome-stable --user-data-dir="$HOME/.config/chrome-split"
```

---

## Desktop Application Integration (`.desktop`)

You can configure desktop applications to **always** launch into Split Tunneling when clicked in your application launcher (GNOME, KDE, Rofi, Wofi):

1. Copy the desktop launcher to your personal applications directory:
   ```bash
   cp /usr/share/applications/spotify.desktop ~/.local/share/applications/
   ```
2. Edit `~/.local/share/applications/spotify.desktop` and prefix the `Exec=` command with `whoisthat run -d`:
   ```ini
   # Before:
   Exec=spotify %U

   # After:
   Exec=whoisthat run -d spotify %U
   ```
3. Update your desktop database:
   ```bash
   update-desktop-database ~/.local/share/applications
   ```

---

## Gaming & Steam Integration

WhoisThat offers a seamless gaming experience on Linux: you can keep your **entire operating system connected to your VPN** (for private browsing, background services, or unblocking Discord/streaming), while routing **individual games directly through your physical ISP** with **lowest native ping**, zero VPN jitter, and no Anti-Cheat VPN bans.

### Per-Game Launch Options in Steam (`%command%`)

You can direct any specific game to bypass the VPN without touching other games or restarting Steam:

1. In your **Steam Library**, right-click the game and select **Properties...**
2. In the **General** tab, scroll down to **Launch Options**.
3. Enter:
   ```bash
   whoisthat run %command%
   ```
4. Start the game.

That's it! The game, its Proton/Wine environment, runtime containers (pressure-vessel), and anti-cheat modules will route directly via your physical gateway.

::: danger NEVER Use `-d` or `--detach` in Steam Launch Options
Always use `whoisthat run %command%` without `-d` or `-b`.

Steam tracks the game lifecycle through the launcher command. When `-d` is used, the launcher detaches and exits immediately; Steam will mistakenly assume the game closed, abort your "In-Game" status, break the Steam Overlay, and attempt to upload Cloud Saves while the game is still running!

Without `-d`, `whoisthat run` creates a transient systemd `--scope`, staying synchronously attached to the game until you exit.
:::

### Compatibility with MangoHud, GameMode & Gamescope

`whoisthat run` integrates seamlessly with popular Linux gaming wrappers. Always place `whoisthat run` at the very beginning of the pipeline:

```bash
# GameMode + MangoHud
whoisthat run gamemoderun mangohud %command%

# Gamescope + GameMode + MangoHud
whoisthat run gamescope -W 1920 -H 1080 -f -- gamemoderun mangohud %command%
```

Every tool in the chain, along with the game itself, will be grouped into the split-tunnel slice.

### Third-Party Game Launchers

* **Heroic Games Launcher (Epic Games / GOG)**:
  Open Game Settings → **Other** → **Wrapper Command** (or **Game Arguments Prefix**): enter `whoisthat run`.
* **Lutris**:
  Right-click game → **Configure** → **System options** → enable **Command prefix** and enter `whoisthat run`.
* **Bottles**:
  Go to Bottle Preferences → **System** → **Command Prefix** and set `whoisthat run`.

### Bypassing the Entire Steam Client

If you prefer to route the entire Steam client — including multi-gigabyte library downloads and Steam Voice chat — directly through your physical ISP:

```bash
whoisthat run -d steam
```

::: tip Single-Instance Reminder
If Steam is already minimized to your system tray, close it completely (`pkill -9 steam` or right-click tray icon → **Exit Steam**) before launching it via `whoisthat run -d steam`.
:::

---

## See also

- [Kill-Switch](/usage/kill-switch)
- [Platforms & Service Management](/guide/platforms)
- [Networking internals](/architecture/networking)
- [Routing](/usage/routing)
