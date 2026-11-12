package cmd

import (
	"net/http"
	"net/url"
	"strings"
	"testing"
)

// F-17: the subscription URL carries the credential and the response carries
// the whole server list, so a cleartext fetch hands both to anyone on path.
func TestFetchSubscriptionRejectsPlainHTTP(t *testing.T) {
	// Never dials: the scheme check must fire first (.invalid cannot resolve).
	_, _, err := fetchSubscriptionURL("http://sub.example.invalid/api/sub/abc")
	if err == nil || !strings.Contains(err.Error(), "https") {
		t.Fatalf("plain http subscription must be rejected with an https error, got: %v", err)
	}
}

func TestSubscriptionRedirectHopMustBeHTTPS(t *testing.T) {
	mk := func(raw string) *http.Request {
		u, _ := url.Parse(raw)
		return &http.Request{URL: u}
	}
	via := []*http.Request{mk("https://panel.example/sub/abc")}
	if err := subscriptionCheckRedirect(mk("http://panel.example/sub/abc"), via); err == nil {
		t.Fatal("https -> http redirect must be refused (downgrade)")
	}
	if err := subscriptionCheckRedirect(mk("https://cdn.example/sub/abc"), via); err != nil {
		t.Fatalf("https -> https redirect must be allowed: %v", err)
	}
	many := make([]*http.Request, 10)
	for i := range many {
		many[i] = via[0]
	}
	if err := subscriptionCheckRedirect(mk("https://cdn.example/x"), many); err == nil {
		t.Fatal("redirect chains must be bounded")
	}
}
