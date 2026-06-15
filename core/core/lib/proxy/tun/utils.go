package tunmode

import (
	"fmt"
	"net"
	"os/exec"
	"strings"
)

func GetDefaultInterfaceAndIP() (name string, ip4 string, ip6 string, err error) {
	out, err := exec.Command("ip", "route", "show", "default").Output()
	if err != nil {
		return "", "", "", fmt.Errorf("failed to get default route: %w", err)
	}
	output := string(out)
	fields := strings.Fields(output)
	var iface, gatewayIP string
	for i := range fields {
		if fields[i] == "dev" && i+1 < len(fields) {
			iface = fields[i+1]
		}
		if fields[i] == "via" && i+1 < len(fields) {
			gatewayIP = fields[i+1]
		}
	}
	if iface == "" || gatewayIP == "" {
		return "", "", "", fmt.Errorf("could not parse interface or gateway IP from output: %s", output)
	}

	ip6gw := ""
	out6, err := exec.Command("ip", "-6", "route", "show", "default").Output()
	if err == nil {
		output6 := string(out6)
		fields6 := strings.Fields(output6)
		for i := range fields6 {
			if fields6[i] == "via" && i+1 < len(fields6) {
				ip := net.ParseIP(fields6[i+1])
				if ip != nil && ip.To16() != nil && ip.To4() == nil {
					ip6gw = fields6[i+1]
				}
				break
			}
		}
	}

	return iface, gatewayIP, ip6gw, nil
}
