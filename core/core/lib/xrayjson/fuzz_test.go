package xrayjson

import (
	"encoding/json"
	"testing"
)

func FuzzParse(f *testing.F) {
	for _, seed := range []string{
		`{"remarks":"Only","outbounds":[{"tag":"proxy","protocol":"vless","settings":{"vnext":[{"address":"x.example","port":1}]}}]}`,
		`{"outbounds":[{"protocol":"trojan","settings":{"servers":[{"address":"t.example","port":443}]}}]}`,
		`{"outbounds":[]}`,
		`{}`, `[]`, `null`, `{"error":"nope"}`,
	} {
		f.Add([]byte(seed))
	}
	f.Fuzz(func(t *testing.T, b []byte) {
		_ = IsConfigBody(b)
		_, _ = Parse(b)
		_ = ServerAddresses(json.RawMessage(b))
	})
}
