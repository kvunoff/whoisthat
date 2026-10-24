# IPC Transport & Framing

Communication between the user-facing interfaces (Rust TUI, CLI, status scripts) and the core daemon (`whoisthat-core`) takes place over a dedicated, high-speed Inter-Process Communication (IPC) transport.

---

## Wire Format Specification

All communication—both client-to-core requests and core-to-client notifications—adheres to a strict length-prefixed framing specification:

```
0                   1                   2                   3
0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                 Length (32-bit Big-Endian)                    |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+                     JSON Payload (UTF-8)                      +
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

### Protocol Properties:
1. **Header**: Exactly 4 bytes representing an unsigned 32-bit integer (`uint32`) in big-endian (network) byte order. This specifies the exact byte length of the trailing JSON payload.
2. **Payload**: A valid UTF-8 encoded JSON object formatted as `{"msg": "<command>", "data": { ... }}`.
3. **No Delimiters**: There are no trailing newlines (`\n`) or null terminators (`\0`). Message boundaries are determined strictly by the 4-byte length prefix.

---

## Supported Transports

### 1. Unix Domain Socket (Default & Recommended)

By default, the daemon binds to a local Unix domain stream socket:

```
$XDG_RUNTIME_DIR/whoisthat/core.sock
```
*(Typically `/run/user/<UID>/whoisthat/core.sock`)*

#### Security Advantages:
* **Filesystem Permissions**: The socket file is created with file mode `0600` (readable and writable only by the owner).
* **Local User Isolation**: Other users on a shared multi-tenant Linux workstation cannot connect to the socket or manipulate the VPN daemon.
* **Kernel Overhead**: Avoids TCP/IP stack overhead and loopback latency.

### 2. TCP Socket (Opt-In / Legacy)

For specialized environments (e.g. headless containers, network namespaces, or development inspection), WhoisThat supports a standard loopback TCP listener on `127.0.0.1:4897`.

To enable TCP transport:

**Core Configuration (`config.json`):**
```json
{
  "tcp-enabled": true,
  "core-tcp-port": 4897
}
```

**TUI Configuration (`config.toml`):**
```toml
use_tcp = true
core_tcp_port = 4897
core_host = "127.0.0.1"
```

---

## Message Exchange Patterns

The protocol supports two concurrent exchange patterns over the same stream:

### 1. Request / Response
The client sends an action command, and the core daemon responds with the result:

```
Client                                     Core Daemon
  │                                             │
  ├────── connect {"profile": {"id": 1}} ──────►│
  │                                             │ (Activates Xray)
  │◄───── status-changed {"connection": "..."} ─┤
  │                                             │
```

### 2. Reactive Broadcast Notifications
The core continuously emits state updates to **all connected clients** without requiring polling:

```
Core Daemon                                All Clients
  │                                             │
  ├────── traffic-stats {"proxy_down": ...} ───►│ (Periodic 1s ticks)
  ├────── profile-updated {"test-result": 45} ─►│ (As latency tests finish)
  ├────── tun-status-changed {"enabled": true} ─►│ (On mode toggle)
  │                                             │
```

For the complete schema of all commands and notifications, see the [TCP & IPC Wire Protocol Reference](/reference/protocol).
