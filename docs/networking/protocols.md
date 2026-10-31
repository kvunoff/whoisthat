# Supported Protocols & Transports

WhoisThat uses a unified proxy architecture powered by **Xray-core v26.10.29**. All protocols—including advanced QUIC and Reality handshakes—are parsed directly into Xray configurations, benefiting equally from native TUN routing, live gRPC traffic metrics, and DNS protection.

---

## Protocol Matrix

| Protocol | URI Scheme | Transports & Features |
| :--- | :--- | :--- |
| **VLESS** | `vless://` | Reality, Vision flow, xHTTP (SplitHTTP), gRPC, WebSocket, HTTPUpgrade, QUIC, mKCP, TCP/TLS |
| **VMess** | `vmess://` | TCP, WebSocket, gRPC, HTTPUpgrade, QUIC, mKCP, TLS, AEAD |
| **Trojan** | `trojan://` | Native TLS, Reality, WebSocket, gRPC, HTTPUpgrade, QUIC, mKCP |
| **Shadowsocks**| `ss://` | AEAD (AES-256-GCM, ChaCha20-Poly1305), Shadowsocks 2022 |
| **SOCKS** | `socks://`, `socks5://`, `socks4://` | SOCKS5 with optional username/password authentication; SOCKS4 accepted as a legacy alias |
| **Hysteria2** | `hysteria2://`, `hy2://` | Native QUIC, TLS, Salamander obfuscation, UDP Port Hopping |

---

## Detailed Protocol Specifications

### 1. VLESS (XTLS / Reality / Vision)

VLESS is the modern standard for lightweight, unencrypted-payload proxying wrapped inside legitimate TLS handshakes:

* **Reality**: Emulates real web server TLS certificates (e.g. `gateway.icloud.com` or `microsoft.com`) using public-key authentication (`pbk=...`) and Short ID (`sid=...`), preventing active DPI probing.
* **xHTTP (SplitHTTP)**: Chunked HTTP upload and download streams that mimic standard web traffic through strict CDN reverse proxies.
* **Vision Flow (`xtls-rprx-vision`)**: Pattern-masks packet lengths and timing characteristics to resist machine-learning traffic analysis.

**Example URI:**
```
vless://uuid-1234@server.com:443?encryption=none&security=reality&sni=example.com&fp=chrome&pbk=AbCdEf1234&sid=123456&type=tcp&flow=xtls-rprx-vision#MyVLESS
```

---

### 2. Hysteria2 (`hysteria2://` / `hy2://`)

Hysteria2 is a customized UDP protocol based on QUIC, designed specifically for lossy, high-jitter, or throttled network connections.

* **Salamander Obfuscation**: Scrambles QUIC packets with a shared password (`obfs=salamander&obfs-password=...`), rendering UDP traffic indistinguishable from random noise.
* **UDP Port Hopping**: Rotates client UDP destination ports across a configured port range (`mport=20000-40000`) to defeat ISP UDP rate-limiting.
* **Native Xray Engine**: Fully compiled into Xray-core; **no standalone `hysteria` binary is needed**.

**Example URI:**
```
hysteria2://password@server.com:443?sni=example.com&obfs=salamander&obfs-password=secret&mport=20000-40000#MyHysteria2
```

---

### 3. Trojan

Emulates legitimate HTTPS web traffic by authenticating via a password hash sent directly inside the TLS handshake:

* Supports modern TLS 1.3, ALPN negotiation (`h2`, `http/1.1`), and WebSocket/gRPC multiplexing.

**Example URI:**
```
trojan://password123@server.com:443?security=tls&sni=mycdn.com&type=ws&path=%2Ftrojan#MyTrojan
```

---

### 4. Shadowsocks & Shadowsocks 2022

* Supports standard AEAD ciphers (`aes-256-gcm`, `chacha20-ietf-poly1305`).
* Full support for Shadowsocks 2022 specifications (`2022-blake3-aes-128-gcm`, `2022-blake3-aes-256-gcm`, `2022-blake3-chacha20-poly1305`).
