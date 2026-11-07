package happ

import "testing"

func FuzzParseLink(f *testing.F) {
	for _, seed := range []string{
		"happ://routing/add/eyJOYW1lIjoiUDEifQ==",
		"happ://routing/onadd/eyJOYW1lIjoiUDEifQ==",
		"happ://routing/off",
		"happ://routing/",
		"", "not-a-link", "happ://", "happ://routing/add/",
	} {
		f.Add(seed)
	}
	f.Fuzz(func(t *testing.T, s string) {
		parsed, err := ParseLink(s)
		if err != nil || parsed.Profile == nil {
			return
		}
		_, _, _ = ToRoutingConfig(*parsed.Profile)
	})
}
