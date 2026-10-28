package cmd

import "testing"

func TestRedactSecret(t *testing.T) {
	cases := map[string]string{
		"https://example.com/sub?token=abc&id=1":     "https://example.com/sub?…",
		"https://user:pass@example.com/sub":          "https://…@example.com/sub",
		"https://user:pass@example.com/sub?t=secret": "https://…@example.com/sub?…",
		"https://example.com/sub":                    "https://example.com/sub",
		"vless://uuid@host:443?type=ws":              "vless://…@host:443?…",
	}
	for in, want := range cases {
		if got := redactSecret(in); got != want {
			t.Errorf("redactSecret(%q) = %q, want %q", in, got, want)
		}
	}
}
