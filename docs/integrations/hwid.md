---
title: "HWID & Device Fingerprinting"
description: "How the HWID device fingerprint is sent and how to control it."
audience: "user"
type: "explanation"
---

# HWID & Device Fingerprinting

WhoisThat implements the Hardware ID (HWID) convention used by many subscription
providers (including Remnawave and Happ panels) to associate a machine with a
plan's device limit.

## What is sent

When HWID is enabled, subscription requests include:

| Header | Example | Source |
| :--- | :--- | :--- |
| `x-hwid` | `1fb1e0141ab3e35a` | 16-character hexadecimal token generated on first run |
| `x-device-os` | `linux` | Go `runtime.GOOS` |
| `x-ver-os` | `6.12.0-arch1-1` | Active kernel release (`uname -r`) |
| `x-device-model` | `Arch Linux` | `PRETTY_NAME` from `/etc/os-release` |
| `user-agent` | `whoisthat/v0.5.4` | Configurable, always sent |

Disabling HWID omits all four `x-*` headers. The `user-agent` is still sent
because providers use it to select the subscription format.

## Response headers

WhoisThat reads `subscription-userinfo` (traffic, expiry, last update) and
`profile-update-interval` (refresh cadence) from the response, including the
body equivalents. Provider-specific `x-hwid-*` response headers
(`x-hwid-max-devices-reached`, `x-hwid-limit`, `x-hwid-not-supported`) are not
interpreted and do not produce an in-app warning; the fetch succeeds or fails
based on status and body.

## Control HWID

In Settings, the row is **HWID Enabled** (`on`/`off`); it is not a separate
Hardware page. **Reset HWID** generates a new token.

From the CLI:

```bash
whoisthat hwid status
whoisthat hwid disable
whoisthat hwid enable
whoisthat hwid reset
whoisthat hwid user-agent "whoisthat/v0.5.4"
```

`hwid status` prints the enabled flag, the token, the user-agent and the
platform/kernel/model values that would be sent.

## Reset and provider registration

Reset changes the local token immediately. If the provider already registered
the old token, clear the old device in the provider dashboard or wait for its
device list to expire; WhoisThat cannot remove a server-side registration. The
new token is used by the next subscription fetch.

## Privacy

HWID is an identifier, not a secret. It is sent only to the subscription URL,
not to the VPN server. If a provider does not require it, disable it. Keep
`user-agent` overrides minimal; a custom value is visible to the provider.

## See also

- [Profiles & Subscriptions](/usage/profiles)
- [Settings Reference](/usage/settings)
- [Configuration](/reference/configuration)
- [CLI Reference](/reference/cli)
