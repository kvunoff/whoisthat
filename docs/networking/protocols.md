---
title: "Supported Protocols & Transports"
description: "Supported proxy protocols, transports, and how to choose between them."
audience: "user"
type: "explanation"
---

# Supported Protocols & Transports

WhoisThat parses share links into Xray configurations and runs them on the pinned
**Xray-core v26.10.31**. Every protocol benefits from the same TUN, routing, DNS
and traffic-monitoring path.

## Protocol matrix

| Protocol | URI scheme | Common transports and features |
| :--- | :--- | :--- |
| **VLESS** | `vless://` | Reality, Vision, xHTTP (SplitHTTP), gRPC, WebSocket, HTTPUpgrade, QUIC, mKCP, TCP/TLS |
| **VMess** | `vmess://` | TCP, WebSocket, gRPC, HTTPUpgrade, QUIC, mKCP, TLS, AEAD |
| **Trojan** | `trojan://` | TLS, Reality, WebSocket, gRPC, HTTPUpgrade, QUIC, mKCP |
| **Shadowsocks** | `ss://` | AEAD (AES-256-GCM, ChaCha20-Poly1305), Shadowsocks 2022 |
| **SOCKS** | `socks://`, `socks5://`, `socks4://` | SOCKS5 with optional username/password; SOCKS4 is a legacy alias |
| **Hysteria2** | `hysteria2://`, `hy2://` | Native QUIC, TLS, Salamander obfuscation, UDP port hopping |

## Choosing a protocol

- **VLESS + Reality** is the usual choice for a modern server: it needs no
  domain or certificate and resists active probing.
- **VLESS + WebSocket/gRPC + TLS** is for CDN fronting and restrictive networks.
- **Hysteria2** is for lossy, high-jitter, or UDP-throttled links. It needs UDP
  to the server; networks that block QUIC will not work.
- **Trojan** is a good fit when you already have a TLS reverse proxy.
- **Shadowsocks** is lightweight and widely supported; 2022 ciphers are
  preferred over legacy AEAD.
- **SOCKS5** is a direct proxy hop, not an encrypted tunnel. Use it only when
  the server and path are trusted.

## VLESS

VLESS carries an unencrypted payload inside a TLS-like handshake:

- **Reality** emulates a real TLS server with a public key (`pbk`) and short ID
  (`sid`), so active probes see a legitimate site.
- **Vision** (`xtls-rprx-vision`) masks packet lengths and timing.
- **xHTTP (SplitHTTP)** splits upload/download into HTTP streams for CDN
  reverse proxies.

```text
vless://uuid@server:443?encryption=none&security=reality&sni=example.com&fp=chrome&pbk=...&sid=...&type=tcp&flow=xtls-rprx-vision#Name
```

## VMess

VMess is an older encrypted protocol. It remains useful for compatibility with
existing providers. Prefer AEAD and TLS; plain VMess over TCP is detectable.

## Trojan

Trojan authenticates with a password inside a TLS handshake and looks like
normal HTTPS. It supports TLS 1.3, ALPN, WebSocket and gRPC.

```text
trojan://password@server:443?security=tls&sni=cdn.example.com&type=ws&path=%2Ftrojan#Name
```

## Shadowsocks

AEAD ciphers (`aes-256-gcm`, `chacha20-ietf-poly1305`) and the 2022 suite
(`2022-blake3-aes-128-gcm`, `2022-blake3-aes-256-gcm`,
`2022-blake3-chacha20-poly1305`) are supported. Use 2022 where the server
supports it.

## SOCKS

SOCKS5 supports optional username/password authentication. SOCKS4 is accepted
as a legacy alias. Traffic between the client and the SOCKS server is not
encrypted by the protocol itself.

## Hysteria2

Hysteria2 is QUIC-based and tolerates loss better than TCP. Salamander
obfuscation makes UDP look random, and port hopping rotates destination ports to
avoid ISP rate limits. It is compiled into Xray; no separate `hysteria` binary is
needed.

```text
hysteria2://password@server:443?sni=example.com&obfs=salamander&obfs-password=...&mport=20000-40000#Name
```

## Not supported

WireGuard and OpenVPN are not proxy protocols in this engine and are not
imported from share links. Use a dedicated client for them. Plain HTTP proxies
are also not a supported import format.

## Validation

Import failures are reported per URI. A provider may return a valid-looking link
whose transport or cipher is unsupported; the profile is skipped, and the
remaining links are still imported. Run `whoisthat profile list` to see what was
accepted.

## See also

- [Routing](/usage/routing)
- [Happ Routing Profiles](/networking/happ-routing)
- [JSON Subscriptions](/networking/json-subscriptions)
- [Troubleshooting & FAQ](/reference/troubleshooting)
