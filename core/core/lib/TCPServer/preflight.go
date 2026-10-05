package TCPServer

import (
	"fmt"
	"whoisthat-core/lib"
	"whoisthat-core/lib/logger"
	"whoisthat-core/lib/tunmgr"
	"whoisthat-core/lib/xraymgr"
	"whoisthat-core/structs"
	"whoisthat-core/utils"
)

// MissingBinary pairs a binary name with the actionable install hint the user
// will see in the TUI when that binary is absent at startup.
type MissingBinary struct {
	Name string
	Hint string
}

// CheckMissingBinaries probes the external binaries whoisthat-core
// shells out to (xray, tun2socks, whoisthat-parser) and returns the subset
// that is not installed.
//
// Order: xray first, then tun2socks (TUN mode), then parser.
func CheckMissingBinaries() []MissingBinary {
	var missing []MissingBinary

	xmStatus := xraymgr.GetManager().GetStatus()
	if xmStatus.Status == "missing" || xmStatus.Status == "error" {
		hint := fmt.Sprintf("xray-core %s is %s — background download initiated", xraymgr.PinnedVersion, xmStatus.Status)
		if xmStatus.Error != "" {
			hint = fmt.Sprintf("xray-core %s download error: %s", xraymgr.PinnedVersion, xmStatus.Error)
		}
		missing = append(missing, MissingBinary{
			Name: "xray",
			Hint: hint,
		})
	}
	tmStatus := tunmgr.GetManager().GetStatus()
	if tmStatus.Status == "missing" || tmStatus.Status == "error" {
		hint := fmt.Sprintf("tun2socks %s is %s — background download initiated", tunmgr.PinnedVersion, tmStatus.Status)
		if tmStatus.Error != "" {
			hint = fmt.Sprintf("tun2socks %s download error: %s", tunmgr.PinnedVersion, tmStatus.Error)
		}
		missing = append(missing, MissingBinary{
			Name: "tun2socks",
			Hint: hint,
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
