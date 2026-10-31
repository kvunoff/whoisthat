# Troubleshooting & FAQ

Encountering an issue? Consult this troubleshooting matrix and frequently asked questions for immediate solutions.

---

## Troubleshooting Matrix

| Symptom | Likely Cause | Solution |
| :--- | :--- | :--- |
| **`TUN mode needs network capabilities`** popup appears | Core binary lacks Linux file capabilities (common after rebuilding `whoisthat-core`). | Run: `sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep $(which whoisthat-core)` and restart `whoisthat` (accept the startup `pkexec` prompt if offered). |
| **`Failed to connect to core: Connection refused`** on startup | Socket path is locked by a stale process or port 4897 is occupied. | Terminate any lingering instances: `pkill -9 -f whoisthat-core` and restart the client. |
| **VPN drops when closing terminal** | Client was exited with <kbd>q</kbd> while daemon was running as a child process of the shell session. | Enable **Systemd Autostart** in Settings, or run the core daemon as a systemd user service (`systemctl --user start whoisthat-core`). |
| **Profile tests consistently show `err`** | Firewall blocking outbound test port range, or server credentials are invalid. | Inspect `~/.config/whoisthat/core.log` for the exact dialer error; verify the `test-port-range` (default `3095–30120`) is open. |
| **Routing rule doesn't match or trigger** | Rule is disabled (grayed out) or geo databases are missing. | Press <kbd>Space</kbd> on the rule to enable it. Verify `~/.config/whoisthat/geo/geoip.dat` is at least 10 MiB and `geosite.dat` at least 1 MiB. |
| **Kill-Switch left dangling rules after crash** | System rebooted abruptly while Kill-Switch was active. | Automatically self-heals on next core startup. To clear manually: `sudo nft delete table inet whoisthat_ks 2>/dev/null`. |
| **"Could not enable lingering" error in Settings** | Polkit authorization failed or was dismissed. | In a terminal, run: `sudo loginctl enable-linger $USER`, then re-toggle the setting. |
| **Settings or profiles reset after an update** | `config.json` or the encrypted database could not be read, so the daemon fell back to in-memory defaults. Nothing is overwritten automatically. | On the next start the app offers **Repair WhoisThat** — press <kbd>Enter</kbd> to restore settings from `config.json.bak` and profiles from the newest daily snapshot. Dismissing (<kbd>Esc</kbd>) keeps the damaged files untouched. |
| **`Cannot decrypt DB file` error in core log** | Master key file `~/.config/whoisthat/db.key` was deleted while encrypted DB files remain. | Try **Repair WhoisThat** first (restores the newest daily snapshot under `~/.local/share/whoisthat/backups/`). If that is not enough: restore your `.key` backup. If unsalvageable: stop the core, purge `~/.local/share/whoisthat/db/`, and restart to generate a fresh key. |
| **Hysteria2 fails with syntax error** | System has an obsolete Xray-core (< v26.10.29) installed. | Upgrade Xray-core to v26.10.29 or later, or delete any obsolete `/usr/bin/xray` to let WhoisThat manage its pinned runtime. |
| **App still uses VPN when run via `whoisthat run`** | Split tunnel is `off`, or the target is a single-instance app already running in the background (Spotify, Chrome, Discord). | Set **Split tunnel** to `exclude` (or `include`) in Settings, terminate all active instances (`pkill -9 spotify` or `pkill -f chrome`), then launch via `whoisthat run -d`. |
| **TUN toggle says "Checking root…" then nothing** | Kernel or security module (e.g. AppArmor/SELinux) blocks ambient capability elevation. | Launch the daemon manually: `whoisthat-core` and inspect `~/.config/whoisthat/core.log`. `CanTun()` exercises the capability test device `wt-capcheck`. |
| **Logs pane in TUI is completely empty** | Minimum severity filter is hiding messages or core hasn't written events yet. | Press <kbd>f</kbd> in the Logs tab to cycle level filter to `ALL`, or raise **Log level** in Settings. |
| **App closes when closing terminal** | Application was started in foreground interactive mode without the `-d` flag. | Use `whoisthat run -d <app>` (or `-b`, `--detach`) to spawn as an independent transient systemd user service. |

::: tip First Step When Debugging Any Issue
Tail both logs in a separate terminal (or use the Logs tab, <kbd>s</kbd> switches core/tui):
```bash
tail -f ~/.config/whoisthat/core.log ~/.local/share/whoisthat/tui.log
whoisthat logs --level error
whoisthat logs --tui --follow
```
The core forwards exact dial errors, DNS lookup failures, and firewall command exit codes directly to `core.log`. Xray's own stderr is inlined as a single `[ERRO]` line when it exits unexpectedly. A daemon panic lands in `~/.local/share/whoisthat/core-crash.log` (or journald when run as a service).
:::

---

## Frequently Asked Questions (FAQ)

### Can I run WhoisThat on a headless server without a GUI?
**Yes.** You can install `whoisthat-core`, manage it via `systemctl --user`, and control connections entirely through non-interactive CLI commands (`whoisthat connect`, `whoisthat toggle`, `whoisthat status -s`, `whoisthat profile list`, `whoisthat settings set ...`). The legacy `-c`/`-t` aliases still work.

### Does WhoisThat prevent DNS leaks?
**Yes.** In TUN mode, kernel-level DNAT rules intercept all outbound DNS requests destined for port 53. With a Happ DNS profile, queries go to the reserved sink (`198.18.0.53` / `fd00::53`) and then through Xray's tagged domestic/remote resolvers. Split-excluded apps deliberately keep resolving directly.

### Why does `ping <fake-ip>` sometimes get a reply?
FakeDNS replaces the address with a virtual one from `198.19.0.0/16` or `fc00::/18`. ICMP has no domain name to sniff, so it is never forwarded to the real host. The TUN stack may answer the echo locally instead (typically `ttl 64` and sub-millisecond latency); this synthetic reply proves nothing about remote reachability. Use TCP-based tools (`curl`, browser) to verify fake-IP routing.

### Why is FakeDNS not active even though the profile enables it?
FakeDNS only runs in TUN mode with `GlobalProxy=true` and split mode `off` or `exclude`. It is intentionally disabled in proxy mode, with `GlobalProxy=false`, and in split `include` mode.

### Why does HTTP/3/QUIC sometimes bypass domain routing?
`quic` is deliberately not in the TUN sniffing list to avoid the screen-share latency regression. HTTP/3 traffic may therefore not be mapped back from a fake IP to its domain; force HTTP/2 or use a TCP-based test.

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
