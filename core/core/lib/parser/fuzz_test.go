package parser

import "testing"

func FuzzParseUri(f *testing.F) {
	for _, seed := range []string{
		"vless://uuid@example.com:443?test=1#MyName",
		"vless://uuid@host:443?security=reality&sni=google.com&pbk=pubkey&sid=sid#r",
		"vmess://uuid@host:443",
		"trojan://pw@host:443?sni=x#n",
		"ss://YWVzLTI1Ni1nY206cGFzcw==@host:8388#n",
		"hysteria2://pass@host:443#n",
		"socks5://user:pass@host:1080",
		"", "not-a-uri", "://", "vless://",
	} {
		f.Add(seed)
	}
	f.Fuzz(func(t *testing.T, s string) {
		_, _ = ParseUri(s, 1080, 8080)
		_, _ = GetMetadata(s)
	})
}
