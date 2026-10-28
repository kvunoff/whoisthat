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
* **Peak Speeds**: Maximum RX and TX rates recorded since the TUI started (not reset on reconnect).

### 2. Session Data Totals
* **Proxy Download / Upload**: Cumulative bytes transferred through the proxy since the TUI started.
* **Direct Bypass / Total Direct**: Bytes matching `direct` routing rules, shown live and cumulatively in the footer.
* **Profile / Status**: Current connection state and profile name.

### 3. Interface & TUN Footer
* **TUN State**: `TUN: active (<name>)` or `TUN: disabled`.
* **Direct Bypass**: Live and cumulative bytes routed directly over the physical gateway.
* The firewall backend, MTU and packet counters are not part of the Traffic tab; use `whoisthat doctor` or `nft`/`ip` for those.
