package traffic

import (
	"context"
	"net"
	"testing"
	"time"

	"google.golang.org/grpc"

	statscmd "github.com/xtls/xray-core/app/stats/command"
)

func TestClassifyOutbounds(t *testing.T) {
	config := []byte(`{"outbounds":[
		{"tag":"proxy","protocol":"vless"},
		{"tag":"proxy-b","protocol":"vmess"},
		{"tag":"direct","protocol":"freedom"},
		{"tag":"block","protocol":"blackhole"},
		{"tag":"dns-out","protocol":"dns"},
		{"tag":"api","protocol":"dokodemo-door"}
	]}`)
	got, err := ClassifyOutbounds(config)
	if err != nil {
		t.Fatalf("ClassifyOutbounds: %v", err)
	}
	want := map[string]Category{
		"proxy": CategoryProxy, "proxy-b": CategoryProxy,
		"direct": CategoryDirect, "block": CategoryOther,
		"dns-out": CategoryOther,
	}
	for tag, category := range want {
		if got[tag] != category {
			t.Errorf("%s = %q, want %q", tag, got[tag], category)
		}
	}
	if _, has := got["api"]; has {
		t.Error("api outbound must be excluded from traffic categories")
	}
	if len(got) != len(want) {
		t.Errorf("categories = %+v, want exactly %d entries", got, len(want))
	}
}

type fakeStatsServer struct {
	statscmd.UnimplementedStatsServiceServer
	stats []*statscmd.Stat
}

func (f *fakeStatsServer) QueryStats(context.Context, *statscmd.QueryStatsRequest) (*statscmd.QueryStatsResponse, error) {
	return &statscmd.QueryStatsResponse{Stat: f.stats}, nil
}

func TestXraySourceRead(t *testing.T) {
	lis, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("listen: %v", err)
	}
	srv := grpc.NewServer()
	statscmd.RegisterStatsServiceServer(srv, &fakeStatsServer{stats: []*statscmd.Stat{
		{Name: "outbound>>>proxy>>>traffic>>>uplink", Value: 1000},
		{Name: "outbound>>>proxy>>>traffic>>>downlink", Value: 3000},
		{Name: "outbound>>>proxy-b>>>traffic>>>uplink", Value: 500},
		{Name: "outbound>>>direct>>>traffic>>>uplink", Value: 200},
		{Name: "outbound>>>direct>>>traffic>>>downlink", Value: 400},
		{Name: "outbound>>>block>>>traffic>>>downlink", Value: 99999},
		{Name: "outbound>>>api>>>traffic>>>uplink", Value: 99999},
		{Name: "inbound>>>tun-in>>>traffic>>>downlink", Value: 99999},
	}})
	go func() { _ = srv.Serve(lis) }()
	defer srv.Stop()

	categories := map[string]Category{
		"proxy": CategoryProxy, "proxy-b": CategoryProxy,
		"direct": CategoryDirect, "block": CategoryOther,
	}
	src := NewXraySource(lis.Addr().(*net.TCPAddr).Port, categories)
	defer src.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	counters, err := src.Read(ctx)
	if err != nil {
		t.Fatalf("Read: %v", err)
	}
	if !counters.XrayOK || counters.FirewallOK {
		t.Fatalf("availability = %+v, want XrayOK=true FirewallOK=false", counters)
	}
	if counters.ProxyUp != 1500 || counters.ProxyDown != 3000 {
		t.Errorf("proxy = %d/%d, want 1500/3000", counters.ProxyUp, counters.ProxyDown)
	}
	if counters.DirectUp != 200 || counters.DirectDown != 400 {
		t.Errorf("direct = %d/%d, want 200/400", counters.DirectUp, counters.DirectDown)
	}
	if counters.BypassUp != 0 || counters.BypassDown != 0 {
		t.Errorf("bypass must stay zero until FirewallSource is wired: %+v", counters)
	}
}

func TestXraySourceWithoutAPIPort(t *testing.T) {
	src := NewXraySource(0, map[string]Category{"proxy": CategoryProxy})
	counters, err := src.Read(context.Background())
	if err != nil {
		t.Fatalf("Read: %v", err)
	}
	if counters.XrayOK {
		t.Fatal("apiPort=0 must report the Xray source as unavailable")
	}
}
