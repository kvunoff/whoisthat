# Real-Time Traffic Monitoring

The Traffic Monitoring dashboard (<kbd>3</kbd>, or cycle with <kbd>[</kbd> / <kbd>]</kbd>) provides real-time visibility into your network throughput, data transfer totals, and interface health.

---

## High-Resolution Braille Charts

WhoisThat utilizes Unicode Braille dot patterns (`⠋`, `⠙`, `⠸`, etc.) to plot high-density bandwidth curves inside your terminal:

```
Bandwidth Monitor (RX: Green, TX: Red)
 12.4 MB/s ┼                     ⢠⣾⣿⣷⣄
           │                   ⢀⣼⣿⣿⣿⣿⣿⣦⡀
  8.2 MB/s ┼                 ⢠⣾⣿⣿⣿⣿⣿⣿⣿⣿⣷⡄
           │      ⢀⣀⡀   ⢠⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣷⡄
  4.1 MB/s ┼    ⢀⣴⣿⣿⣷⡄⢀⣼⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣦⡀
           │  ⢀⣴⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣦⡀
    0 B/s  ┴─────────────────────────────────────────────────────────────
```

### Key Visualization Features:
* **Color Differentiation**: Download traffic (**RX**) is rendered in vibrant green, while Upload traffic (**TX**) is rendered in red.
* **Dynamic Y-Axis Scaling**: The vertical scale dynamically recalculates based on recent peak transfer rates, shifting cleanly between `B/s`, `KB/s`, `MB/s`, and `GB/s`.
* **Rolling Time Window**: Maintains a 60-sample rolling ring buffer, updating synchronously every second.

---

## Metrics & Diagnostics Cards

The traffic screen displays structured cards detailing active transfer metrics:

### 1. Live Throughput
* **Current Download Rate**: Real-time incoming bandwidth (e.g. `4.8 MB/s`).
* **Current Upload Rate**: Real-time outgoing bandwidth (e.g. `620.0 KB/s`).
* **Peak Speeds**: Maximum recorded RX and TX speeds observed during the current connection session.

### 2. Session Data Totals
* **Session Download**: Total bytes downloaded through the proxy since establishing connection.
* **Session Upload**: Total bytes uploaded through the proxy.
* **Direct vs Proxy Split**: Separates bytes forwarded over the encrypted VPN tunnel from traffic matching `direct` routing rules routed over the physical gateway.

### 3. Interface & TUN Status
* **TUN State**: Indicates whether the virtual network interface (`whoisthattun`) is currently UP, DOWN, or UNCONFIGURED.
* **Firewall Engine**: Active backend (`nftables` or `iptables`).
* **MTU & Packet Counters**: Operational parameters of the active network interface.
