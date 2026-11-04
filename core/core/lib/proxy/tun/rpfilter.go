package tunmode

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"whoisthat-core/utils"
)

func rpFilterStatePath() string {
	return filepath.Join(utils.RuntimeDir(), "rp_filter.state")
}

func readRpFilterValue(iface string) (string, bool) {
	if !validInterfaceName(iface) {
		return "", false
	}
	data, err := os.ReadFile(filepath.Join("/proc/sys/net/ipv4/conf", iface, "rp_filter"))
	if err != nil {
		return "", false
	}
	value := strings.TrimSpace(string(data))
	switch value {
	case "0", "1", "2":
		return value, true
	default:
		return "", false
	}
}

func validInterfaceName(iface string) bool {
	if iface == "" || len(iface) > 15 {
		return false
	}
	for _, r := range iface {
		if (r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z') ||
			(r >= '0' && r <= '9') || r == '_' || r == '-' || r == '.' {
			continue
		}
		return false
	}
	return true
}

// loosenRpFilter sets rp_filter=2 for the physical and TUN interfaces so
// policy-routed packets are not dropped. The previous values are saved to a
// runtime state file so a clean stop (or the next startup after a crash) can
// restore them instead of leaving the system loosened forever.
func loosenRpFilter(tun_name string, default_interface_name string) error {
	statePath := rpFilterStatePath()
	if _, err := os.Stat(statePath); os.IsNotExist(err) {
		var state strings.Builder
		for _, iface := range []string{default_interface_name, tun_name} {
			if value, ok := readRpFilterValue(iface); ok {
				fmt.Fprintf(&state, "%s=%s\n", iface, value)
			}
		}
		if state.Len() > 0 {
			_ = utils.AtomicWriteFile(statePath, []byte(state.String()), 0600)
		}
	}

	script := fmt.Sprintf(`
TUN_NAME="%s"
DEF_IFACE="%s"

for IFACE in "$DEF_IFACE" "$TUN_NAME"; do
    if ip link show "$IFACE" &>/dev/null; then
        echo "Setting rp_filter=2 for $IFACE (temporary)"
        sysctl -w net.ipv4.conf."$IFACE".rp_filter=2
    else
        echo "Warning: Interface '$IFACE' not found, skipping."
    fi
done
	`, tun_name, default_interface_name)
	_, err := runScriptWithSh(script)
	return err
}

// restoreRpFilter re-applies the values saved by loosenRpFilter and removes the
// state file. Missing state is a no-op, so this is safe on a clean boot.
func restoreRpFilter() error {
	statePath := rpFilterStatePath()
	data, err := os.ReadFile(statePath)
	if os.IsNotExist(err) {
		return nil
	}
	if err != nil {
		return err
	}

	var script strings.Builder
	for _, line := range strings.Split(string(data), "\n") {
		iface, value, ok := strings.Cut(strings.TrimSpace(line), "=")
		if !ok || !validInterfaceName(iface) {
			continue
		}
		if value != "0" && value != "1" && value != "2" {
			continue
		}
		fmt.Fprintf(&script, "if [ -e /proc/sys/net/ipv4/conf/%s/rp_filter ]; then sysctl -w net.ipv4.conf.%s.rp_filter=%s; fi\n", iface, iface, value)
	}
	if script.Len() == 0 {
		_ = os.Remove(statePath)
		return nil
	}
	if _, err := runScriptWithSh(script.String()); err != nil {
		return err
	}
	return os.Remove(statePath)
}
