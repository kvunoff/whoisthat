package utils

import "net/url"

// SubscriptionHost returns the display fallback for a subscription: its host
// without scheme, path or credentials. Empty when the URL has no host.
func SubscriptionHost(subscriptionURL string) string {
	u, err := url.Parse(subscriptionURL)
	if err != nil {
		return ""
	}
	return u.Hostname()
}
