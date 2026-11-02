# Reporting a Bug

A good report lets a maintainer reproduce the problem without guessing. WhoisThat has a built-in diagnostic report; collect it before opening an issue.

---

## 1. One-Command Report

```bash
whoisthat doctor --json > whoisthat-doctor.json
whoisthat version --json
whoisthat core status --json
```

`doctor --json` includes binaries, capabilities, polkit, IPC/daemon state, DNS/routing checks, storage paths and a summary. Add log diagnostics with:

```bash
whoisthat doctor --logs --json > whoisthat-logs.json
```

---

## 2. Logs

```bash
whoisthat logs --level error --lines 200
whoisthat logs --source tui --level warn --lines 200
whoisthat logs --since 1h --grep error
```

On a systemd user service, also check:

```bash
journalctl --user -u whoisthat-core -n 100 --no-pager
```

`core-crash.log` contains Go panic output and is summarised by `doctor --logs`.

---

## 3. Environment

Include the parts relevant to the problem:

```bash
uname -a
cat /etc/os-release
ip -V
nft --version
systemctl --version | head -1
```

Also state:

- how WhoisThat was installed (installer, `.deb`/`.rpm`, AUR `whoisthat-bin` or source `whoisthat`, manual build) and the exact release/tag;
- connection mode (proxy or TUN), split-tunnel mode and whether the kill-switch was armed;
- firewall backend (nftables/iptables) and whether Docker/libvirt/other network managers are running;
- steps to reproduce, expected result and actual result;
- whether the problem survives `whoisthat core restart`.

---

## 4. Before You Post

::: danger Redact secrets
Logs, `doctor` output and profile listings can contain subscription URLs, server addresses, HWIDs and tokens.

- Never attach `~/.config/whoisthat/db.key` or the encrypted database.
- Do not paste raw `vless://`, `vmess://`, `trojan://`, `ss://` or `hysteria2://` URIs.
- Mask subscription tokens: `whoisthat profile list` and `sub list` already hide them unless `--reveal` is used.
- Review `whoisthat logs` output before posting; credentials in a URL **path** are not masked automatically.
:::

A minimal, redacted copy is more useful than a complete dump with live credentials.

---

## 5. Where to File

- Bugs and feature requests: [github.com/kvunoff/whoisthat/issues](https://github.com/kvunoff/whoisthat/issues) (issue forms ask for the doctor output).
- Documentation problems: same tracker, with the page URL and what is wrong.
- Sensitive reports (credentials, privilege escalation, firewall bypass): follow [SECURITY.md](https://github.com/kvunoff/whoisthat/security/policy) and use GitHub's private vulnerability reporting. Do not open a public issue with the details.

Attach the redacted `whoisthat-doctor.json`, the relevant log excerpt, and the environment block above.
