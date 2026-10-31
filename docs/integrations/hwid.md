# HWID & Device Fingerprinting

WhoisThat implements the **Hardware ID (HWID)** device identification specification widely adopted by modern subscription providers (including Remnawave and Happ panels).

---

## How HWID Works

When updating subscription groups, providers often enforce multi-device limits (e.g. a maximum of 3 concurrent active devices per subscription plan). To associate your Linux machine with your account, `whoisthat-core` sends identification headers with subscription HTTP requests:

| HTTP Header | Example Value | Source |
| :--- | :--- | :--- |
| `x-hwid` | `1fb1e0141ab3e35a` | Unique 16-character hexadecimal token auto-generated on first run. |
| `x-device-os` | `linux` | Go `runtime.GOOS` identifier (lowercase OS name, not architecture). |
| `x-ver-os` | `6.12.0-arch1-1` | Active Linux kernel release (`uname -r`). |
| `x-device-model` | `Arch Linux` | Distribution name extracted from `/etc/os-release` (`PRETTY_NAME`). |
| `user-agent` | `whoisthat/v0.3.11` | Configurable HTTP User-Agent header. |

---

## Response Header Handling

WhoisThat parses only the standard `subscription-userinfo` response header (traffic quota, expiry, last update). Provider-specific `x-hwid-*` response headers (`x-hwid-max-devices-reached`, `x-hwid-limit`, `x-hwid-not-supported`) are currently **not** interpreted and do not produce an in-app warning; the subscription fetch succeeds or fails based on the HTTP status and body.

---

## Privacy & Customization Controls

You maintain full control over device tracking in **Settings → Hardware**:

### 1. Toggle HWID Off
If your subscription provider does not require HWID, or if you prefer not to transmit device headers, toggle **HWID: Enabled** to `off`. Subsequent subscription requests will omit all `x-hwid-*` headers.

### 2. Reset HWID
If you migrate accounts or need to clear an old device registration from your provider's dashboard, navigate to **Reset HWID** and press <kbd>Enter</kbd>. A new 16-character cryptographically random token is generated immediately.

### 3. Customize User-Agent
Some subscription servers restrict downloads to specific clients (e.g., Clash, v2rayN, Sing-Box). You can override the default User-Agent string to match any custom client signature.
