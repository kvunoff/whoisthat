# Go Core Daemon (`whoisthat-core`)

The **WhoisThat Core** is an autonomous, event-driven daemon written in Go. It operates independently in the background, serving as the central coordinator for proxy processes, TUN interfaces, database transactions, and client communication.

---

## Daemon Startup Lifecycle

When `whoisthat-core` starts, it executes a rigorous preparation sequence:

```
[Start]
   │
   ├─► 1. Raise Ambient Capabilities (capset / prctl)
   │
   ├─► 2. Load Configuration (~/.config/whoisthat/config.json)
   │
   ├─► 3. Self-Heal: Reconcile Orphaned Firewall & Routing Rules
   │
   ├─► 4. Verify & Ensure Geo Assets (geoip.dat & geosite.dat)
   │
   ├─► 5. Initialize Encrypted Database & Load Master Key
   │
   ├─► 6. Initialize Proxy Manager & Dynamic PortPool
   │
   ├─► 7. Seed Latency Test Parameters
   │
   ├─► 8. Initialize Xray Runtime Manager (Verify Pinned v26.9.9)
   │
   ├─► 9. Open IPC Listeners (Unix Domain Socket & Optional TCP)
   │
   └─► 10. Enter Main Event Loop & Wait for Client Connections
```

### Key Initialization Steps

### 1. Ambient Capabilities Elevation
Before any file or network operations, the daemon elevates permitted capabilities (`CAP_NET_ADMIN`, `CAP_NET_RAW`, `CAP_SETPCAP`) into the inheritable and ambient sets via `prctl(PR_CAP_AMBIENT_RAISE)`. This ensures child processes (`ip`, `nft`, `iptables`, and `xray`) automatically inherit network management permissions without requiring `root` or `sudo`.

### 2. Startup Orphan Reconciliation
If a previous daemon session crashed, experienced an ungraceful shutdown, or lost power while TUN or kill-switch was active, dangling rules in `iptables` or `nftables` could leave network routes broken. 

On startup, before binding sockets, the daemon runs `tunmode.ReconcileOrphanedRules()`. If no other live core instance is detected, it tears down all leftover `whoisthat_*` tables and restores default gateway routing.

### 3. Isolated Geo Assets Management
The daemon verifies the existence and integrity of `geoip.dat` and `geosite.dat` under `~/.config/whoisthat/geo/`. If missing or corrupted (< 10 MB), it automatically initiates an exponential backoff download from official v2fly release repositories.

---

## Proxy Subprocess Orchestration

Instead of relying on fragile static configuration files on disk, `whoisthat-core` compiles full Xray JSON configs **entirely in memory**:

1. **Protocol Parsing**: Decodes proxy URIs (`vless://`, `vmess://`, `trojan://`, `ss://`, `socks://`, `hysteria2://`) into strongly typed Go configuration structures.
2. **Stream Settings Injection**: Configures TLS, Reality, xHTTP, gRPC, or Vision flow parameters based on server specifications.
3. **Routing Table Injection**: Merges default private-range bypasses, user-defined rules, and active Happ routing profiles into Xray's routing block.
4. **Execution**: Launches the pinned Xray-core binary with `-config stdin:`, piping the generated JSON directly to standard input.

---

## Dynamic PortPool Allocator

For latency testing, WhoisThat spins up lightweight temporary Xray instances running in parallel. To prevent port collisions:

* The daemon maintains a thread-safe `PortPool` within the configured range (default: `3095` to `30120`).
* When a batch test begins, ephemeral ports are leased from the pool.
* Once the test sample completes or times out, the port is returned immediately to the pool.

---

## Graceful Signal Handling & Teardown

The daemon listens for OS termination signals (`SIGINT`, `SIGTERM`, `SIGHUP`):

1. Closes client IPC connections.
2. Stops active proxy processes.
3. Tears down virtual TUN interfaces (`whoisthattun`).
4. Flushes nftables tables (`inet whoisthat_split`, `inet whoisthat_ks`).
5. Removes Table 100 / Table 200 ip rules and exits cleanly.
