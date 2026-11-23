package traffic

import (
	"context"
	"errors"
	"testing"
)

func TestCompositeSourceNilReaderIsComplete(t *testing.T) {
	src := NewCompositeSource(&fakeSource{c: counters(100, 200)}, nil)
	got, err := src.Read(context.Background())
	if err != nil {
		t.Fatalf("Read: %v", err)
	}
	if !got.XrayOK || !got.FirewallOK {
		t.Fatalf("availability = %+v, want both OK in proxy mode", got)
	}
	if got.BypassUp != 0 || got.BypassDown != 0 {
		t.Errorf("bypass = %+v, want zero", got)
	}
}

func TestCompositeSourceAddsBypass(t *testing.T) {
	reader := func(context.Context) (int64, int64, bool, error) {
		return 10, 20, true, nil
	}
	src := NewCompositeSource(&fakeSource{c: counters(100, 200)}, reader)
	got, err := src.Read(context.Background())
	if err != nil {
		t.Fatalf("Read: %v", err)
	}
	if !got.FirewallOK || got.BypassUp != 10 || got.BypassDown != 20 {
		t.Fatalf("bypass = %+v, want 10/20 with FirewallOK", got)
	}
}

func TestCompositeSourceDegradesOnFirewallError(t *testing.T) {
	reader := func(context.Context) (int64, int64, bool, error) {
		return 0, 0, false, errors.New("nft failed")
	}
	src := NewCompositeSource(&fakeSource{c: counters(100, 200)}, reader)
	got, err := src.Read(context.Background())
	if err != nil {
		t.Fatalf("Read: %v", err)
	}
	if got.FirewallOK {
		t.Fatal("firewall failure must mark the sample partial")
	}
	if !got.XrayOK || got.ProxyUp != 100 {
		t.Fatalf("Xray data lost on firewall error: %+v", got)
	}
}
