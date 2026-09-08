package lib

import (
	"encoding/base64"
	"testing"
)

func TestGetDBAddProfileDatasFromStr(t *testing.T) {
	uris := "vless://uuid1@example.com:443?security=reality#Node1\ntrojan://pw2@example.org:443#Node2"
	b64 := base64.StdEncoding.EncodeToString([]byte(uris))

	profiles := GetDBAddProfileDatasFromStr(b64, 1)
	if len(profiles) != 2 {
		t.Fatalf("expected 2 profiles, got %d", len(profiles))
	}

	if profiles[0].Name != "Node1" || profiles[0].Protocol != "vless" {
		t.Errorf("profile 0 unexpected: %+v", profiles[0])
	}
	if profiles[1].Name != "Node2" || profiles[1].Protocol != "trojan" {
		t.Errorf("profile 1 unexpected: %+v", profiles[1])
	}
}
