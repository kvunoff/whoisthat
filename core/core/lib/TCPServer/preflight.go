package TCPServer

import (
	"fmt"
	"whoisthat-core/lib"
	"whoisthat-core/lib/logger"
	"whoisthat-core/structs"
	"whoisthat-core/utils"
)

// MissingBinary pairs a binary name with the actionable install hint the user
// will see in the TUI when that binary is absent at startup.
type MissingBinary struct {
	Name string
	Hint string
}

// CheckMissingBinaries probes the four external binaries whoisthat-core
// shells out to (parser, xray, hysteria, tun2socks) and returns the subset
// that is not installed. Inputs are nil-safe: each probe is independent so a
// missing hysteria binary still lets the user run vless/vmess profiles.
//
// Order: xray first (hardest dep — every non-hysteria protocol), then
// hysteria (hy2), then tun2socks (TUN mode), then parser (add-profile and
// update-subscription). The TUI receives one warn per missing binary in
// this order.
func CheckMissingBinaries() []MissingBinary {
	var missing []MissingBinary

	if _, err := utils.GetXrayBin(); err != nil {
		missing = append(missing, MissingBinary{
			Name: "xray",
			Hint: "xray binary not installed — no non-hysteria2 profile will work. Install via your distro's package manager or https://github.com/XTLS/Xray-core",
		})
	}
	if _, err := utils.GetHysteriaBin(); err != nil {
		missing = append(missing, MissingBinary{
			Name: "hysteria",
			Hint: "hysteria binary not installed — hysteria2:// / hy2:// profiles will not work. Install: go install github.com/apernet/hysteria2/v2@latest (or use install.sh)",
		})
	}
	if _, err := utils.GetTun2socksBin(); err != nil {
		missing = append(missing, MissingBinary{
			Name: "tun2socks",
			Hint: "tun2socks binary not installed — TUN mode will not be available. Install 'tun2socks' or 'tun2socks-bin' from your distro's package manager",
		})
	}
	if _, err := utils.GetParserBin(); err != nil {
		missing = append(missing, MissingBinary{
			Name: "parser",
			Hint: "whoisthat-parser binary not found — add-profiles and update-subscription will fail. Reinstall whoisthat or place whoisthat-parser in /usr/bin or /usr/local/bin",
		})
	}
	return missing
}

// sendMissingBinaryWarnings delivers one warn notification per missing binary
// to a SINGLE newly-connected client. This is a unicast path — it writes only
// to the passed clientConn's outbound channel, never to s.clients (which
// would broadcast to all connected TUIs).
//
// We send to cc.out directly because Broadcast reaches all clients; we only
// want this specific client to receive the warnings once on connect. If the
// client's outbound queue fills up we drop the warning rather than blocking
// accept — the user will see it as soon as the queue drains or on the next
// reconnect.
func sendMissingBinaryWarnings(cc *clientConn, missing []MissingBinary) {
	for _, mb := range missing {
		key := fmt.Sprintf("missing-binary-%s", mb.Name)
		msg := lib.CreateJsonNotification("warn", structs.Warning{Key: key, Content: mb.Hint})
		select {
		case cc.out <- msg:
		default:
			logger.Warnf("missing-binary warn: outbound queue full for %s, skipping %s warn", cc.conn.RemoteAddr(), mb.Name)
			return
		}
	}
}