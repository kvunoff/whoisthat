package tunmode

import "fmt"

// ---------------------------------------------------------------------------
// systemd-resolved (resolvectl)
// ---------------------------------------------------------------------------

func setupResolvectlDns(tunName string, dnsIp string) error {
	if tunName == "" || dnsIp == "" {
		return nil
	}
	script := fmt.Sprintf(`
resolvectl dns %s %s 2>/dev/null || true
resolvectl domain %s "~." 2>/dev/null || true
`, tunName, dnsIp, tunName)
	_, err := runScriptWithSh(script)
	return err
}

func revertResolvectlDns(tunName string) error {
	if tunName == "" {
		return nil
	}
	script := fmt.Sprintf(`
resolvectl revert %s 2>/dev/null || true
`, tunName)
	_, err := runScriptWithSh(script)
	return err
}
