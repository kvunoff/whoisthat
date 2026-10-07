package parser

import (
	"encoding/base64"
	"encoding/json"
	"strings"
	"testing"
)

func TestGetUriProtocol(t *testing.T) {
	tests := []struct {
		uri      string
		expected string
		isErr    bool
	}{
		{"vless://uuid@host:443", "vless", false},
		{"vmess://uuid@host:443", "vmess", false},
		{"ss://YWJj@host:443", "shadowsocks", false},
		{"socks5://host:1080", "socks", false},
		{"socks4://host:1080", "socks", false},
		{"socks://host:1080", "socks", false},
		{"trojan://pw@host:443", "trojan", false},
		{"hysteria2://pw@host:443", "hysteria2", false},
		{"hy2://pw@host:443", "hysteria2", false},
		{"http://host:80", "http", false},
		{"unknown://host:80", "", true},
	}

	for _, tt := range tests {
		proto, err := GetUriProtocol(tt.uri)
		if tt.isErr {
			if err == nil {
				t.Errorf("GetUriProtocol(%s) expected error, got nil", tt.uri)
			}
		} else {
			if err != nil {
				t.Errorf("GetUriProtocol(%s) unexpected error: %v", tt.uri, err)
			}
			if proto != tt.expected {
				t.Errorf("GetUriProtocol(%s) expected %s, got %s", tt.uri, tt.expected, proto)
			}
		}
	}
}

func TestGetMetadata(t *testing.T) {
	meta, err := GetMetadata("vless://uuid@example.com:443?test=1#MyName")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if meta.Name != "MyName" {
		t.Errorf("expected name MyName, got %s", meta.Name)
	}
	if meta.Protocol != "vless" {
		t.Errorf("expected protocol vless, got %s", meta.Protocol)
	}
	if meta.Address == nil || *meta.Address != "example.com" {
		t.Errorf("expected address example.com, got %v", meta.Address)
	}
	if meta.Port == nil || *meta.Port != 443 {
		t.Errorf("expected port 443, got %v", meta.Port)
	}

	trojanMeta, err := GetMetadata("trojan://pw@example.com:443?test=1#TrojanName")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if trojanMeta.Name != "TrojanName" || trojanMeta.Protocol != "trojan" {
		t.Errorf("unexpected trojan metadata: %+v", trojanMeta)
	}

	_, err = GetMetadata("not-a-uri")
	if err == nil {
		t.Errorf("expected error for invalid URI, got nil")
	}
}

func TestGetMetadataBatch(t *testing.T) {
	input := "vless://uuid1@example.com:443?test=1#Profile1\n\ninvalid_line\ntrojan://pw2@example.org:443?test=2#Profile2\n"
	results, err := GetMetadataBatch(strings.NewReader(input))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(results) != 2 {
		t.Fatalf("expected 2 batch items, got %d", len(results))
	}
	if results[0].Name != "Profile1" || results[0].Protocol != "vless" {
		t.Errorf("unexpected item 0: %+v", results[0])
	}
	if results[1].Name != "Profile2" || results[1].Protocol != "trojan" {
		t.Errorf("unexpected item 1: %+v", results[1])
	}
}

func TestParseUri_Inbounds(t *testing.T) {
	data, err := ParseUri("vless://uuid@example.com:443?test=1", 3090, 3091)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	var cfg Config
	if err := json.Unmarshal(data, &cfg); err != nil {
		t.Fatalf("failed to unmarshal JSON: %v", err)
	}
	if len(cfg.Outbounds) != 1 {
		t.Errorf("expected 1 outbound, got %d", len(cfg.Outbounds))
	}
	if len(cfg.Inbounds) != 2 {
		t.Errorf("expected 2 inbounds, got %d", len(cfg.Inbounds))
	}
	if cfg.Outbounds[0].Protocol != "vless" || cfg.Outbounds[0].Tag != "proxy" {
		t.Errorf("unexpected outbound: %+v", cfg.Outbounds[0])
	}

	// Without inbounds
	dataNoInbounds, err := ParseUri("vless://uuid@example.com:443?test=1", 0, 0)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	var cfgNoInbounds Config
	if err := json.Unmarshal(dataNoInbounds, &cfgNoInbounds); err != nil {
		t.Fatalf("failed to unmarshal JSON: %v", err)
	}
	if len(cfgNoInbounds.Inbounds) != 0 {
		t.Errorf("expected 0 inbounds, got %d", len(cfgNoInbounds.Inbounds))
	}
}

func TestVlessOutbounds(t *testing.T) {
	// TLS
	outbound, _, err := createOutboundObject("vless://uuid@example.com:443?security=tls&sni=sni.com&type=tcp&fp=chrome#test")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if outbound.Protocol != "vless" || outbound.Tag != "proxy" {
		t.Errorf("unexpected outbound protocol/tag: %s / %s", outbound.Protocol, outbound.Tag)
	}
	if outbound.StreamSettings.Security == nil || *outbound.StreamSettings.Security != "tls" {
		t.Errorf("expected security tls, got %v", outbound.StreamSettings.Security)
	}
	if outbound.StreamSettings.TLSSettings == nil {
		t.Fatalf("expected tlsSettings, got nil")
	}
	if outbound.StreamSettings.TLSSettings.ServerName == nil || *outbound.StreamSettings.TLSSettings.ServerName != "sni.com" {
		t.Errorf("expected sni.com, got %v", outbound.StreamSettings.TLSSettings.ServerName)
	}
	if outbound.StreamSettings.TLSSettings.Fingerprint == nil || *outbound.StreamSettings.TLSSettings.Fingerprint != "chrome" {
		t.Errorf("expected chrome, got %v", outbound.StreamSettings.TLSSettings.Fingerprint)
	}

	// Reality
	outboundReality, _, err := createOutboundObject("vless://uuid@example.com:443?security=reality&sni=google.com&pbk=pubkey&sid=sid&fp=firefox&flow=xtls-rprx-vision")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if outboundReality.StreamSettings.Security == nil || *outboundReality.StreamSettings.Security != "reality" {
		t.Errorf("expected security reality, got %v", outboundReality.StreamSettings.Security)
	}
	reality := outboundReality.StreamSettings.RealitySettings
	if reality == nil {
		t.Fatalf("expected realitySettings, got nil")
	}
	if reality.ServerName == nil || *reality.ServerName != "google.com" ||
		reality.PublicKey == nil || *reality.PublicKey != "pubkey" ||
		reality.ShortId == nil || *reality.ShortId != "sid" ||
		reality.Fingerprint == nil || *reality.Fingerprint != "firefox" {
		t.Errorf("unexpected realitySettings: %+v", reality)
	}
	if outboundReality.StreamSettings.Network == nil || *outboundReality.StreamSettings.Network != "tcp" {
		t.Errorf("expected network tcp, got %v", outboundReality.StreamSettings.Network)
	}

	// WS
	outboundWS, _, err := createOutboundObject("vless://uuid@example.com:443?type=ws&host=ws.example.com&path=/ws-path")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if outboundWS.StreamSettings.Network == nil || *outboundWS.StreamSettings.Network != "ws" {
		t.Errorf("expected network ws, got %v", outboundWS.StreamSettings.Network)
	}
	if outboundWS.StreamSettings.WsSettings == nil ||
		outboundWS.StreamSettings.WsSettings.Host == nil || *outboundWS.StreamSettings.WsSettings.Host != "ws.example.com" ||
		outboundWS.StreamSettings.WsSettings.Path == nil || *outboundWS.StreamSettings.WsSettings.Path != "/ws-path" {
		t.Errorf("unexpected wsSettings: %+v", outboundWS.StreamSettings.WsSettings)
	}

	// gRPC with mode=multi
	outboundGRPC, _, err := createOutboundObject("vless://uuid@example.com:443?type=grpc&serviceName=svc&mode=multi&authority=auth")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if outboundGRPC.StreamSettings.GRPCSettings == nil {
		t.Fatalf("expected grpcSettings, got nil")
	}
	if outboundGRPC.StreamSettings.GRPCSettings.MultiMode == nil || !*outboundGRPC.StreamSettings.GRPCSettings.MultiMode {
		t.Errorf("expected multiMode true, got %v", outboundGRPC.StreamSettings.GRPCSettings.MultiMode)
	}
	if outboundGRPC.StreamSettings.GRPCSettings.ServiceName == nil || *outboundGRPC.StreamSettings.GRPCSettings.ServiceName != "svc" {
		t.Errorf("expected serviceName svc, got %v", outboundGRPC.StreamSettings.GRPCSettings.ServiceName)
	}

	// ALPN split and filter
	outboundALPN, _, err := createOutboundObject("vless://uuid@example.com:443?security=tls&sni=sni.com&alpn=h2,,http/1.1,")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	tlsALPN := outboundALPN.StreamSettings.TLSSettings
	if len(tlsALPN.ALPN) != 2 || tlsALPN.ALPN[0] != "h2" || tlsALPN.ALPN[1] != "http/1.1" {
		t.Errorf("expected [h2, http/1.1], got %v", tlsALPN.ALPN)
	}

	// allowInsecure
	outboundInsecure, _, err := createOutboundObject("vless://uuid@example.com:443?security=tls&sni=sni.com&allowInsecure=1")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !outboundInsecure.StreamSettings.TLSSettings.AllowInsecure {
		t.Errorf("expected allowInsecure true")
	}
}

func TestVmessBase64(t *testing.T) {
	jsonObj := map[string]any{
		"add":  "example.com",
		"port": "443",
		"id":   "uuid-vmess",
		"ps":   "name",
		"net":  "tcp",
		"tls":  "tls",
		"type": "none",
	}
	b, _ := json.Marshal(jsonObj)
	encoded := base64.StdEncoding.EncodeToString(b)
	uri := "vmess://" + encoded

	outbound, rd, err := createOutboundObject(uri)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if outbound.Protocol != "vmess" {
		t.Errorf("expected protocol vmess, got %s", outbound.Protocol)
	}
	if rd.Remarks != "name" {
		t.Errorf("expected remarks name, got %s", rd.Remarks)
	}
	if rd.UUID == nil || *rd.UUID != "uuid-vmess" {
		t.Errorf("expected uuid-vmess, got %v", rd.UUID)
	}
	if rd.Port == nil || *rd.Port != 443 {
		t.Errorf("expected port 443, got %v", rd.Port)
	}
}

func TestTrojan(t *testing.T) {
	// Trojan defaults to tls
	outbound, _, err := createOutboundObject("trojan://pw@example.com:443?test=1")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if outbound.Protocol != "trojan" {
		t.Errorf("expected protocol trojan, got %s", outbound.Protocol)
	}
	if outbound.StreamSettings.Security == nil || *outbound.StreamSettings.Security != "tls" {
		t.Errorf("expected security tls, got %v", outbound.StreamSettings.Security)
	}
	if outbound.StreamSettings.Network == nil || *outbound.StreamSettings.Network != "tcp" {
		t.Errorf("expected network tcp, got %v", outbound.StreamSettings.Network)
	}

	// Trojan explicit security=none
	outboundNone, _, err := createOutboundObject("trojan://pw@example.com:443?security=none")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if outboundNone.StreamSettings.Security == nil || *outboundNone.StreamSettings.Security != "none" {
		t.Errorf("expected security none, got %v", outboundNone.StreamSettings.Security)
	}
	if outboundNone.StreamSettings.TLSSettings != nil {
		t.Errorf("expected nil tlsSettings for security=none")
	}

	// Trojan with Reality
	outboundReality, _, err := createOutboundObject("trojan://pw@example.com:443?security=reality&sni=google.com&pbk=pubkey&sid=sid&fp=firefox")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if outboundReality.StreamSettings.RealitySettings == nil {
		t.Fatalf("expected realitySettings, got nil")
	}
	if outboundReality.StreamSettings.RealitySettings.PublicKey == nil || *outboundReality.StreamSettings.RealitySettings.PublicKey != "pubkey" {
		t.Errorf("expected pubkey, got %v", outboundReality.StreamSettings.RealitySettings.PublicKey)
	}
}

func TestShadowsocks(t *testing.T) {
	encoded := base64.StdEncoding.EncodeToString([]byte("chacha20-ietf-poly1305:secretpw"))
	uri := "ss://" + encoded + "@example.com:8388#MySS"

	outbound, rd, err := createOutboundObject(uri)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if outbound.Protocol != "shadowsocks" {
		t.Errorf("expected protocol shadowsocks, got %s", outbound.Protocol)
	}
	if rd.Remarks != "MySS" {
		t.Errorf("expected remarks MySS, got %s", rd.Remarks)
	}
	if rd.ServerMethod == nil || *rd.ServerMethod != "chacha20-ietf-poly1305" {
		t.Errorf("expected method chacha20-ietf-poly1305, got %v", rd.ServerMethod)
	}
	if rd.UUID == nil || *rd.UUID != "secretpw" {
		t.Errorf("expected password secretpw, got %v", rd.UUID)
	}
}

func TestSocks(t *testing.T) {
	outbound, rd, err := createOutboundObject("socks5://user:pass@example.com:1080#MySocks")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if outbound.Protocol != "socks" {
		t.Errorf("expected protocol socks, got %s", outbound.Protocol)
	}
	if rd.Remarks != "MySocks" {
		t.Errorf("expected remarks MySocks, got %s", rd.Remarks)
	}
	if rd.Username == nil || *rd.Username != "user" {
		t.Errorf("expected user, got %v", rd.Username)
	}
	if rd.UUID == nil || *rd.UUID != "pass" {
		t.Errorf("expected pass, got %v", rd.UUID)
	}
}

func TestHysteria2(t *testing.T) {
	uri := "hysteria2://mypassword@example.com:443?sni=sni.example.com&obfs=salamander&obfs-password=secret&up=100mbps&down=200mbps&ports=20000-30000#MyNode"
	outbound, rd, err := createOutboundObject(uri)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if outbound.Protocol != "hysteria" {
		t.Errorf("expected protocol hysteria, got %s", outbound.Protocol)
	}
	if outbound.StreamSettings.Network == nil || *outbound.StreamSettings.Network != "hysteria" {
		t.Errorf("expected network hysteria, got %v", outbound.StreamSettings.Network)
	}
	if outbound.StreamSettings.Security == nil || *outbound.StreamSettings.Security != "tls" {
		t.Errorf("expected security tls, got %v", outbound.StreamSettings.Security)
	}
	if rd.Remarks != "MyNode" {
		t.Errorf("expected remarks MyNode, got %s", rd.Remarks)
	}

	hy := outbound.StreamSettings.HysteriaSettings
	if hy == nil || hy.Version != 2 || hy.Auth != "mypassword" {
		t.Errorf("unexpected hysteriaSettings: %+v", hy)
	}

	fm := outbound.StreamSettings.Finalmask
	if fm == nil {
		t.Fatalf("expected finalmask, got nil")
	}
	if fm.QuicParams == nil || fm.QuicParams.BrutalUp == nil || *fm.QuicParams.BrutalUp != "100mbps" {
		t.Errorf("expected brutalUp 100mbps, got %v", fm.QuicParams)
	}
	if len(fm.UDP) != 2 {
		t.Errorf("expected 2 udp items, got %d", len(fm.UDP))
	}

	// Test YAML generation
	yamlBytes, err := CreateHysteria2ClientYAML(uri, 3090, 3091)
	if err != nil {
		t.Fatalf("unexpected error creating YAML: %v", err)
	}
	yamlStr := string(yamlBytes)
	if !strings.Contains(yamlStr, "server: example.com:443") || !strings.Contains(yamlStr, "auth: mypassword") {
		t.Errorf("unexpected yaml content:\n%s", yamlStr)
	}
}

func TestURLDecode(t *testing.T) {
	if URLDecode("hello") != "hello" {
		t.Errorf("failed plain")
	}
	if URLDecode("hello%20world") != "hello world" {
		t.Errorf("failed spaces")
	}
	if URLDecode("%D0%BF%D1%80%D0%B8%D0%B2%D0%B5%D1%82") != "привет" {
		t.Errorf("failed cyrillic")
	}
	if URLDecode("hello%2") != "hello%2" {
		t.Errorf("failed incomplete percent")
	}
	if URLDecode("hello%ZZ") != "hello%ZZ" {
		t.Errorf("failed invalid hex")
	}
}

func TestParseUriWithTun(t *testing.T) {
	uri := "vless://uuid@example.com:443?type=tcp&security=tls#TestTun"
	configBytes, err := ParseUriWithTun(uri, 3090, 3091, "whoisthattun")
	if err != nil {
		t.Fatalf("ParseUriWithTun failed: %v", err)
	}

	var cfg Config
	if err := json.Unmarshal(configBytes, &cfg); err != nil {
		t.Fatalf("failed to unmarshal generated json: %v", err)
	}

	if len(cfg.Inbounds) != 3 {
		t.Fatalf("expected 3 inbounds (socks, http, tun), got %d", len(cfg.Inbounds))
	}

	tunFound := false
	for _, ib := range cfg.Inbounds {
		if ib.Protocol == "tun" {
			tunFound = true
			if ib.Tag != "tun-in" {
				t.Errorf("expected tag tun-in, got %s", ib.Tag)
			}
			if ib.Port != 0 {
				t.Errorf("expected port 0 for tun, got %d", ib.Port)
			}
		}
	}
	if !tunFound {
		t.Errorf("tun inbound not found in generated config")
	}

	// Verify JSON does not contain "port": 0 for tun
	jsonStr := string(configBytes)
	if !strings.Contains(jsonStr, `"protocol":"tun"`) {
		t.Errorf("json does not contain protocol tun: %s", jsonStr)
	}
	if !strings.Contains(jsonStr, `"name":"whoisthattun"`) {
		t.Errorf("json does not contain tun name: %s", jsonStr)
	}
}

