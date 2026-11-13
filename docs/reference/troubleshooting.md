---
title: "Troubleshooting & FAQ"
description: "Symptom-to-fix matrix and FAQ for common WhoisThat problems."
audience: "user"
type: "how-to"
---

# Troubleshooting & FAQ

Encountering an issue? Consult this troubleshooting matrix and frequently asked questions for immediate solutions.

---

## Troubleshooting Matrix

| Symptom | Likely Cause | Solution |
| :--- | :--- | :--- |
| **`TUN mode needs network capabilities`** popup appears | Core binary lacks Linux file capabilities (common after rebuilding `whoisthat-core`). | Run: `sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep $(which whoisthat-core)` and restart `whoisthat` (accept the startup `pkexec` prompt if offered). |
| **`whoisthat-core: Operation not permitted`** inside a container | The binary carries file capabilities, but the container's bounding set does not include them. | Run the container with `--cap-add=NET_ADMIN --cap-add=NET_RAW --cap-add=SETPCAP`, or use proxy mode. This is expected in unprivileged containers and does not affect a normal host install. |
| **`whoisthat` is not found after adding the PPA** | Package index was not refreshed, or the requested Ubuntu series is not enabled in the PPA. | Run `sudo apt update`, then `apt-cache policy whoisthat`. For Ubuntu 26.04, enable the Resolute series in the PPA settings first. |
| **`Failed to connect to core: Connection refused`** on startup | Socket path is locked by a stale process or port 4897 is occupied. | Terminate any lingering instances: `pkill -9 -f whoisthat-core` and restart the client. |
| **VPN drops when closing terminal** | Client was exited with <kbd>q</kbd> while daemon was running as a child process of the shell session. | Enable **Systemd Autostart** in Settings, or run the core daemon as a systemd user service (`systemctl --user start whoisthat-core`). |
| **Profile tests consistently show `err`** | Firewall blocking outbound test port range, or server credentials are invalid. | Inspect `~/.config/whoisthat/core.log` for the exact dialer error; verify the `test-port-range` (default `3095–30120`) is open. |
| **Routing rule doesn't match or trigger** | Rule is disabled (grayed out) or geo databases are missing. | Press <kbd>Space</kbd> on the rule to enable it. Verify `~/.config/whoisthat/geo/geoip.dat` is at least 10 MiB and `geosite.dat` at least 1 MiB. |
| **Kill-Switch left dangling rules after crash** | System rebooted abruptly while Kill-Switch was active. | Automatically self-heals on next core startup. To clear manually: `sudo nft delete table inet whoisthat_ks 2>/dev/null`. |
| **"Could not enable lingering" error in Settings** | Polkit authorization failed or was dismissed. | In a terminal, run: `sudo loginctl enable-linger $USER`, then re-toggle the setting. |
| **Settings or profiles reset after an update** | `config.json` or the encrypted database could not be read, so the daemon fell back to in-memory defaults. Nothing is overwritten automatically. | On the next start the app offers **Repair WhoisThat** — press <kbd>Enter</kbd> to restore settings from `config.json.bak` and profiles from the newest daily snapshot. Dismissing (<kbd>Esc</kbd>) keeps the damaged files untouched. |
| **`Cannot decrypt DB file` error in core log** | Master key file `~/.config/whoisthat/db.key` was deleted while encrypted DB files remain. | Try **Repair WhoisThat** first (restores the newest daily snapshot under `~/.local/share/whoisthat/backups/`). If that is not enough: restore your `.key` backup. If unsalvageable: stop the core, purge `~/.local/share/whoisthat/db/`, and restart to generate a fresh key. |
| **Hysteria2 fails with syntax error** | System has an obsolete Xray-core (< v26.10.31) installed. | Upgrade Xray-core to v26.10.31 or later, or delete any obsolete `/usr/bin/xray` to let WhoisThat manage its pinned runtime. |
| **App still uses VPN when run via `whoisthat run`** | Split tunnel is `off`, the target is a single-instance app already running in the background (Spotify, Chrome, Discord), or the `fwmark 1 → table 100` policy route was dropped (e.g. by a startup reconcile from another core or a network reset). | Run `whoisthat doctor` and read **Split-tunnel routing**; re-apply with `whoisthat settings set split-tunnel=exclude` (or restart TUN). Otherwise set **Split tunnel** to `exclude` (or `include`) in Settings, terminate all active instances (`pkill -9 spotify` or `pkill -f chrome`), then launch via `whoisthat run -d`. |
| **TUN toggle says "Checking root…" then nothing** | Kernel or security module (e.g. AppArmor/SELinux) blocks ambient capability elevation. | Launch the daemon manually: `whoisthat-core` and inspect `~/.config/whoisthat/core.log`. `CanTun()` exercises the capability test device `wt-capcheck`. |
| **Logs pane in TUI is completely empty** | Minimum severity filter is hiding messages or core hasn't written events yet. | Press <kbd>f</kbd> in the Logs tab to cycle level filter to `ALL`, or raise **Log level** in Settings. |
| **`split-tunnel runner is not running`** | On non-systemd hosts the per-user runner service is stopped or was never installed. | Start it: `sudo rc-service whoisthat-runner start` (or `dinitctl`/`sv`). Install it with `sudo /usr/lib/whoisthat/install-init runner enable $USER $HOME`. |
| **`split cgroup not available` / `cgroup not found`** | `/sys/fs/cgroup/whoisthat-<uid>` is missing (cgroup v2 not mounted, hook did not run, or reboot). | Run `sudo /usr/lib/whoisthat/cgroup-setup $USER`; on OpenRC make sure the `cgroups` service runs before `whoisthat-cgroup`. |
| **Autostart via `.desktop` starts the core but the TUI cannot connect** | The desktop entry must use the canonical runtime directory. | Re-enable it with `whoisthat autostart enable --backend desktop`; check `Exec` contains `XDG_RUNTIME_DIR=/run/whoisthat/<uid>`. |
| **`core stop` does not survive a reboot / core starts again** | `core-stop` is session-scoped; the boot hook clears it. | Expected. Use `whoisthat autostart disable` to stop autostart, or run `core stop` again after boot. |
| **`whoisthat run` refuses to start under sudo** | Running as root would break the cgroup uid and produce root-owned state. | Run it as your normal user; the runner needs no sudo. |
| **`nft socket cgroupv2 unsupported`** | The kernel or nftables is too old for cgroup-based socket matching. | Upgrade to kernel >= 5.13 and a matching nftables; split tunnelling cannot work without it. |
| **App closes when closing terminal** | Application was started in foreground interactive mode without the `-d` flag. | Use `whoisthat run -d <app>` (or `-b`, `--detach`) to spawn it independently of the terminal. |

:: tip First Step When Debugging Any Issue
Tail both logs in a separate terminal (or use the Logs tab, <kbd>s</kbd> switches core/tui):
```bash
tail -f ~/.config/whoisthat/core.log ~/.local/share/whoisthat/tui.log
whoisthat logs --level error
whoisthat logs --source tui --follow
whoisthat logs --since 1h --grep error
```
The core forwards exact dial errors, DNS lookup failures, and firewall command exit codes directly to `core.log`. Xray's own stderr is inlined as a single `[ERRO]` line when it exits unexpectedly. A daemon panic lands in `~/.local/share/whoisthat/core-crash.log`; journald only receives stdout/stderr when the core runs as a user service. If the logs grow too large, `whoisthat logs clear --yes` truncates them and removes rotated backups.
:::

---

## Frequently Asked Questions (FAQ)

### Can I run WhoisThat on a headless server without a GUI?
**Yes.** You can install `whoisthat-core`, manage it via `systemctl --user`, and control connections entirely through non-interactive CLI commands (`whoisthat connect`, `whoisthat toggle`, `whoisthat status --short`, `whoisthat profile list`, `whoisthat settings set ...`). The legacy `-c`/`-t` aliases still work.

### Does WhoisThat prevent DNS leaks?
It intercepts outbound DNS on destination port 53. In TUN mode, kernel-level
DNAT rules capture port-53 traffic; with a Happ DNS profile, queries go to the
reserved sink (`198.18.0.53` / `fd00::53`) and then through Xray's tagged
domestic/remote resolvers. Split-excluded apps deliberately keep resolving
directly. DoH/DoT to another port, or an application with its own resolver, is
not intercepted.

### Why does `ping <fake-ip>` sometimes get a reply?
FakeDNS replaces the address with a virtual one from `198.19.0.0/16` or `fc00::/18`. ICMP has no domain name to sniff, so it is never forwarded to the real host. The TUN stack may answer the echo locally instead (typically `ttl 64` and sub-millisecond latency); this synthetic reply proves nothing about remote reachability. Use TCP-based tools (`curl`, browser) to verify fake-IP routing.

### Why is FakeDNS not active even though the profile enables it?
FakeDNS only runs in TUN mode with `GlobalProxy=true` and split mode `off` or `exclude`. It is intentionally disabled in proxy mode, with `GlobalProxy=false`, and in split `include` mode.

### Why does HTTP/3/QUIC sometimes bypass domain routing?
`quic` is deliberately not in the TUN sniffing list to avoid the screen-share latency regression. HTTP/3 traffic may therefore not be mapped back from a fake IP to its domain; force HTTP/2 or use a TCP-based test.

### How do I completely uninstall WhoisThat?

Follow the uninstall section of [Maintenance, Upgrading &
Recovery](/guide/maintenance#uninstall). It covers script installs, packages,
the systemd unit and the data directory, including the warning about deleting
`db.key`.

## See also

- [Doctor Diagnostics](/reference/doctor)
- [Maintenance, Upgrading & Recovery](/guide/maintenance)
- [CLI Reference](/reference/cli)
