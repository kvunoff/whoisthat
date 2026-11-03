package cmd

import (
	"encoding/json"
	"strings"
	"testing"
	"time"

	"whoisthat-core/structs"
)

func TestGetProfileDetailsReturnsSanitizedStreamMetadata(t *testing.T) {
	cmd, _, msgChan := newTestCmd(t)

	group, err := cmd.DB.AddGroup("Test", "")
	if err != nil {
		t.Fatalf("AddGroup: %v", err)
	}
	raw := json.RawMessage(`{"outbounds":[{"protocol":"vless","settings":{"vnext":[{"address":"x.example","port":443,"users":[{"id":"secret-uuid","flow":"xtls-rprx-vision"}]}]},"streamSettings":{"network":"ws","security":"tls","tlsSettings":{"serverName":"sni.example","alpn":["h2"],"fingerprint":"chrome"},"wsSettings":{"path":"/ws","host":"cdn.example"}}}]}`)
	profile, err := cmd.DB.AddProfile(structs.DBAddProfileData{
		GroupId:     group.Id,
		Name:        "Managed",
		Protocol:    "vless",
		Address:     "x.example",
		Port:        443,
		Managed:     true,
		ProviderKey: "managed",
		Config:      raw,
	})
	if err != nil {
		t.Fatalf("AddProfile: %v", err)
	}

	cmd.GetProfileDetails(structs.ProfileID{GroupId: group.Id, Id: profile.Id})
	data := waitForMessage(t, msgChan, "profile-details", time.Second)

	var envelope struct {
		Data structs.ProfileDetails `json:"data"`
	}
	if err := json.Unmarshal(data, &envelope); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	details := envelope.Data
	if details.Transport != "ws" || details.Security != "tls" {
		t.Fatalf("transport/security = %q/%q", details.Transport, details.Security)
	}
	if details.SNI != "sni.example" || details.Host != "cdn.example" || details.Path != "/ws" {
		t.Fatalf("stream fields = %+v", details)
	}
	if details.Flow != "xtls-rprx-vision" || details.ALPN != "h2" {
		t.Fatalf("settings = %+v", details)
	}
	if strings.Contains(string(data), "secret-uuid") {
		t.Fatal("credential leaked in profile-details")
	}
}
