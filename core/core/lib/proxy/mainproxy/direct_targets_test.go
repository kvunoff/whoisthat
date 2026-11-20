package mainproxy

import (
	"reflect"
	"testing"

	"whoisthat-core/structs"
)

// Every writer (CLI route add, TUI form, default config, Happ import) stores
// the match in Domain/IP and leaves Type as "field". Keying the extractor off
// the type string produced an empty exception set for every real rule.
func TestDirectTargetsFromStoredFieldRules(t *testing.T) {
	got := directTargetsFromRules([]structs.RoutingRule{
		{Type: "field", Domain: "full:api.example.test, geosite:cn, *.wild.test", OutboundTag: "direct", Enabled: true},
		{Type: "field", IP: "203.0.113.0/24,geoip:private,2001:db8::1", OutboundTag: "direct", Enabled: true},
		{Type: "field", Domain: "proxied.test", OutboundTag: "proxy", Enabled: true},
		{Type: "field", Domain: "off.test", OutboundTag: "direct", Enabled: false},
		{Type: "field", Domain: "API.example.test", OutboundTag: "direct", Enabled: true},
	})
	want := []string{"api.example.test", "203.0.113.0/24", "2001:db8::1"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %v want %v", got, want)
	}
}

// Legacy rows may still carry the old type names; the populated field is what
// matters, so they must keep working too.
func TestDirectTargetsFromLegacyTypeNames(t *testing.T) {
	if got := directTargetsFromRules([]structs.RoutingRule{
		{Type: "domain", Domain: "legacy.example", OutboundTag: "direct", Enabled: true},
		{Type: "ip", IP: "198.51.100.7", OutboundTag: "direct", Enabled: true},
	}); !reflect.DeepEqual(got, []string{"legacy.example", "198.51.100.7"}) {
		t.Fatalf("got %v", got)
	}
}

// A provider-supplied whole-family rule would accept everything and neutralise
// the switch while it still reports armed.
func TestDirectTargetsSkipWholeFamilyPrefixes(t *testing.T) {
	got := directTargetsFromRules([]structs.RoutingRule{
		{Type: "field", IP: "0.0.0.0/0,::/0,::ffff:0:0/96,::ffff:0:0/95", OutboundTag: "direct", Enabled: true},
		{Type: "field", IP: "10.0.0.0/8,2001:db8::/32,198.51.100.7", OutboundTag: "direct", Enabled: true},
	})
	want := []string{"10.0.0.0/8", "2001:db8::/32", "198.51.100.7"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %v want %v", got, want)
	}
}

func TestTooWide(t *testing.T) {
	wide := []string{
		"0.0.0.0/0", "::/0", "::ffff:0:0/96", "::ffff:0.0.0.0/96",
		"0:0:0:0:0:ffff:0:0/96", "::ffff:0:0/95", "::ffff:0:0/8",
		"::fffe:0:0/95", "10.0.0.0/7", "2001:db8::/15", "not-a-cidr/33",
	}
	for _, v := range wide {
		if !tooWide(v) {
			t.Errorf("tooWide(%q) = false, want true", v)
		}
	}
	narrow := []string{
		"10.0.0.0/8", "192.168.0.0/16", "2001:db8::/16", "198.51.100.20",
		"::ffff:10.0.0.0/104", "::ffff:192.0.2.0/120",
	}
	for _, v := range narrow {
		if tooWide(v) {
			t.Errorf("tooWide(%q) = true, want false", v)
		}
	}
}

func TestHasMatcherPrefix(t *testing.T) {
	for _, v := range []string{"geosite:cn", "geoip:ru", "regexp:.*\\.example", "keyword:ads", "ext:ads.txt:example.org", "ext-ip:1.2.3.4:443"} {
		if !hasMatcherPrefix(v) {
			t.Errorf("hasMatcherPrefix(%q) = false", v)
		}
	}
	for _, v := range []string{"example.org", "198.51.100.20", "domain:example.org", "geosite_like"} {
		if hasMatcherPrefix(v) {
			t.Errorf("hasMatcherPrefix(%q) = true", v)
		}
	}
}
