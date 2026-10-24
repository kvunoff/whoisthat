# Troubleshooting & FAQ

Encountering an issue? Consult this troubleshooting matrix and frequently asked questions for immediate solutions.

---

## Troubleshooting Matrix

| Symptom | Likely Cause | Solution |
| :--- | :--- | :--- |
| **`TUN mode needs network capabilities`** popup appears | Core binary lacks Linux file capabilities (common after rebuilding `whoisthat-core`). | Run: `sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep $(which whoisthat-core)` (or accept the TUI's one-time `pkexec` prompt). |
| **`Failed to connect to core: Connection refused`** on startup | Socket path is locked by a stale process or port 4897 is occupied. | Terminate any lingering instances: `pkill -9 -f whoisthat-core` and restart the client. |
| **VPN drops when closing terminal** | Client was exited with <kbd>q</kbd> while daemon was running as a child process of the shell session. | Enable **Systemd Autostart** in Settings, or run the core daemon as a systemd user service (`systemctl --user start whoisthat-core`). |
| **Profile tests consistently show `err`** | Firewall blocking outbound test port range, or server credentials are invalid. | Inspect `~/.config/whoisthat/core.log` for the exact dialer error; verify the `test-port-range` (default `3095–30120`) is open. |
| **Routing rule doesn't match or trigger** | Rule is disabled (grayed out) or geo databases are missing. | Press <kbd>Space</kbd> on the rule to enable it. Verify `~/.config/whoisthat/geo/*.dat` files exist and exceed 10 MB in size. |
| **Kill-Switch left dangling rules after crash** | System rebooted abruptly while Kill-Switch was active. | Automatically self-heals on next core startup. To clear manually: `sudo nft delete table inet whoisthat_ks 2>/dev/null`. |
| **"Could not enable lingering" error in Settings** | Polkit authorization failed or was dismissed. | In a terminal, run: `sudo loginctl enable-linger $USER`, then re-toggle the setting. |
| **`Cannot decrypt DB file` error in core log** | Master key file `~/.local/share/whoisthat/db/.key` was deleted while encrypted DB files remain. | Restore your `.key` backup. If unsalvageable: stop the core, purge `~/.local/share/whoisthat/db/`, and restart to generate a fresh key. |
| **Hysteria2 fails with syntax error** | System has an obsolete Xray-core (< v26.9.9) installed. | Upgrade Xray-core to v26.9.9 or later, or delete any obsolete `/usr/bin/xray` to let WhoisThat manage its pinned runtime. |
| **App still uses VPN when run via `whoisthat run`** | Target application is a single-instance app already running in the background (Spotify, Chrome, Discord). | Terminate all active instances (`pkill -9 spotify` or `pkill -f chrome`) before launching via `whoisthat run -d`. |
| **TUN toggle says "Checking root…" then nothing** | Kernel or security module (e.g. AppArmor/SELinux) blocks ambient capability elevation. | Launch the daemon manually: `whoisthat-core` and inspect `~/.config/whoisthat/core.log`. Verify `CanTun()` capability test device `wt-capcheck`. |
| **Logs pane in TUI is completely empty** | Minimum severity filter is hiding messages or core hasn't written events yet. | Press <kbd>f</kbd> in the Logs tab to cycle level filter to `ALL`, or raise **Log level** in Settings. |
| **App closes when closing terminal** | Application was started in foreground interactive mode without the `-d` flag. | Use `whoisthat run -d <app>` (or `-b`, `--detach`) to spawn as an independent transient systemd user service. |

::: tip First Step When Debugging Any Issue
Before digging into source code, always tail the two real-time log files side-by-side in a separate terminal:
```bash
tail -f ~/.config/whoisthat/core.log whoisthat.log
```
The core forwards exact dial errors, DNS lookup failures, and firewall command exit codes directly to `core.log`.
:::

---

## Frequently Asked Questions (FAQ)

### Can I run WhoisThat on a headless server without a GUI?
**Yes.** You can install `whoisthat-core`, manage it via `systemctl --user`, and control connections entirely through non-interactive CLI commands (`whoisthat -c`, `whoisthat -t`, `whoisthat status -s`).

### Does WhoisThat prevent DNS leaks?
**Yes.** In TUN mode, kernel-level DNAT rules intercept all outbound DNS requests destined for port 53 and redirect them through Xray's remote DNS resolver over the encrypted tunnel.

### How do I completely uninstall WhoisThat?
```bash
# 1. Stop and disable service
systemctl --user stop whoisthat-core 2>/dev/null
systemctl --user disable whoisthat-core 2>/dev/null

# 2. Remove binaries
sudo rm -f /usr/local/bin/whoisthat /usr/local/bin/whoisthat-core /usr/bin/whoisthat /usr/bin/whoisthat-core

# 3. Remove configurations and database (WARNING: destroys saved profiles)
rm -rf ~/.config/whoisthat ~/.local/share/whoisthat ~/.config/systemd/user/whoisthat-core.service
```
