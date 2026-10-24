# Systemd User Service

WhoisThat Core can run as a persistent **systemd user service**, allowing the VPN daemon to launch automatically upon system boot—even before you log in graphically—via systemd user lingering.

---

## Unit File Configuration

When you toggle **Systemd autostart** in Settings (or run `whoisthat --systemd-on`), WhoisThat generates and enables the following unit file:

**Location**: `~/.config/systemd/user/whoisthat-core.service`

```ini
[Unit]
Description=WhoisThat VPN Core
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/whoisthat-core
Environment=WHOISTHAT_LOG_LEVEL=warn
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

---

## Boot Autostart via User Lingering

By default, Linux systemd user services only start when you physically log into a graphical or TTY session, and stop when you log out. 

To enable the VPN engine to start at boot before interactive login:

```bash
# Verify current lingering status
loginctl show-user $USER --property=Linger
# Output: Linger=yes or Linger=no

# Enable user lingering
sudo loginctl enable-linger $USER
```

::: tip Automated by the TUI
When toggling **Systemd autostart** inside the TUI Settings tab, WhoisThat checks your linger status and automatically prompts for polkit authentication to run `loginctl enable-linger` if required.
:::

---

## Service Management Commands

Manage the daemon directly through standard `systemctl` user invocations:

```bash
# Inspect current service status
systemctl --user status whoisthat-core

# Start the daemon
systemctl --user start whoisthat-core

# Stop the daemon
systemctl --user stop whoisthat-core

# Restart the daemon (e.g. after recompiling whoisthat-core)
systemctl --user restart whoisthat-core

# View journal logs
journalctl --user -u whoisthat-core -f
```

---

## Detaching (<kbd>q</kbd>) vs Systemd

* **Detaching with <kbd>q</kbd>**: Exits the terminal TUI while leaving the daemon running *for the current user login session*. If you reboot, the daemon will not start automatically unless the systemd service is active.
* **Systemd User Service**: Guarantees the core daemon launches during boot, reconnects automatically if **Autoconnect** is enabled, and restarts automatically in case of unexpected crashes.
