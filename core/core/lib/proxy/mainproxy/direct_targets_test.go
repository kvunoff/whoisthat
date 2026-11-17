package mainproxy

import (
	"reflect"
	"testing"

	"whoisthat-core/structs"
)

func TestDirectTargetsFromRules(t *testing.T) {
	got := directTargetsFromRules([]structs.RoutingRule{
		{Type: "domain", Domain: "full:api.example.test, geosite:cn, *.wild.test", OutboundTag: "direct", Enabled: true},
		{Type: "ip", IP: "203.0.113.0/24,geoip:private,2001:db8::1", OutboundTag: "direct", Enabled: true},
		{Type: "domain", Domain: "proxied.test", OutboundTag: "proxy", Enabled: true},
		{Type: "domain", Domain: "off.test", OutboundTag: "direct", Enabled: false},
		{Type: "domain", Domain: "API.example.test", OutboundTag: "direct", Enabled: true},
		{Type: "field", Domain: "field.example.test", OutboundTag: "direct", Enabled: true},
		{Type: "field", IP: "198.51.100.7", OutboundTag: "direct", Enabled: true},
	})
	want := []string{"api.example.test", "203.0.113.0/24", "2001:db8::1", "field.example.test", "198.51.100.7"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %v want %v", got, want)
	}
}
