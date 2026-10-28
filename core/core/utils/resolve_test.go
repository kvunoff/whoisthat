package utils

import "testing"

func TestDNSServerAddr(t *testing.T) {
	cases := map[string]string{
		"1.1.1.1":              "1.1.1.1:53",
		"2606:4700:4700::1111": "[2606:4700:4700::1111]:53",
		"2001:db8::1":          "[2001:db8::1]:53",
	}
	for in, want := range cases {
		if got := dnsServerAddr(in); got != want {
			t.Errorf("%q -> %q, want %q", in, got, want)
		}
	}
}
