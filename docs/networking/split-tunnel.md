# Split Tunneling (cgroup v2)

Split Tunneling allows you to route specific applications differently from the rest of your system. You can bypass the VPN for domestic banking or high-speed gaming while keeping the rest of your OS protected, or route only a specific web browser through the tunnel.

---

## Operating Modes

Configure the mode in the TUI (**Settings → Network → Split tunnel**) or via the CLI:

### 1. `exclude` Mode *(Default & Recommended)*
* **Entire Operating System**: Protected and routed through the VPN tunnel.
* **Launched Applications** (`whoisthat run`): **Bypass the VPN**, connecting directly through your physical gateway.
* **Ideal For**: Spotify, domestic banking portals, Russian web services (`.ru` / Госуслуги), online gaming (Steam, Discord voice), torrent clients, and local network tools.

### 2. `include` Mode
* **Entire Operating System**: Connects unprotected via your physical gateway.
* **Launched Applications** (`whoisthat run`): **Exclusively routed through the VPN**.
* **Ideal For**: Dedicated private browsing sessions, scraping scripts, or testing international endpoints without affecting system-wide network routing.

::: warning Security Consideration
In `include` mode, only processes explicitly launched inside the split slice are protected by the VPN. All other applications and background traffic connect unprotected over your physical connection.
:::

---

## Launching Applications (`whoisthat run`)

The `whoisthat run` CLI command supports two execution styles:

### A. Background / Detached Mode (`-d`, `--detach`, `-b`)

Use detached mode when starting GUI applications (Spotify, Chrome, Telegram, Steam) so they run independently of your terminal:

```bash
# Launch Spotify bypassing the VPN
whoisthat run -d spotify

# Launch Google Chrome in background
whoisthat run -d google-chrome-stable

# Launch Telegram Desktop in background
whoisthat run -b telegram-desktop
```

#### How Detached Mode Works:
* Spawns the process inside a transient systemd user service (`systemd-run --user --slice=whoisthat_split.slice --same-dir`).
* Automatically forwards your graphical session variables (`DISPLAY`, `WAYLAND_DISPLAY`, `XDG_CURRENT_DESKTOP`).
* **The application keeps running even after you close your terminal or log out of the shell.**

### B. Interactive / Foreground Mode

Use interactive mode for terminal utilities, CLI tools, or shell scripts where you need to inspect standard output or pass arguments:

```bash
# Verify your external IP directly from the terminal
whoisthat run curl 2ip.io

# Play a stream in mpv through the designated route
whoisthat run mpv "https://example.com/video.mp4"
```

* Runs inside a transient systemd user scope (`systemd-run --user --scope --slice=whoisthat_split.slice`).
* Attaches directly to your terminal standard I/O (stdin, stdout, stderr) and returns the program's real exit code.

---

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

Now clicking the Spotify icon from your desktop dock will automatically launch it into the split slice!

---

## Technical Deep-Dive: Kernel Implementation

WhoisThat uses pure kernel primitives for split tunneling without network namespaces or root privileges:

```
[Target Process (spotify)]
       │
       ▼
systemd-run placed in cgroup slice:
`user.slice/user-<uid>.slice/user@<uid>.service/whoisthat_split.slice`
       │
       ▼
nftables table `inet whoisthat_split` (output hook priority -150)
Matches: socket cgroupv2 level 4 "whoisthat_split.slice"
       │
       ├─► In `exclude` mode: Assigns `fwmark 1`
       └─► In `include` mode: Assigns `fwmark 2`
       │
       ▼
Kernel Policy Routing:
       ├─► fwmark 1 ──► ip route table 100 (Physical Gateway Bypass)
       └─► fwmark 2 ──► ip route table 200 (TUN Interface)
       │
       ▼
Postrouting NAT Masquerade:
Rewrites socket source IP on physical egress to prevent LAN dropouts
       │
       ▼
IPv6 Leak Prevention:
Table 100 sets `unreachable default` for IPv6 to prevent Happy Eyeballs leaks
```

### Live Dynamic Reconciliation

Changing the split-tunnel mode in Settings (cycling `off` → `exclude` → `include`) while TUN mode is active immediately reapplies all routing tables, cgroup socket filters, and nftables rules on the fly without needing to disconnect or restart your active session.
