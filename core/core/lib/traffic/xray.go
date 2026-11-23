package traffic

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"sync"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"

	statscmd "github.com/xtls/xray-core/app/stats/command"
)

// Category classifies an outbound by what its bytes mean to the user.
type Category string

const (
	CategoryProxy  Category = "proxy"
	CategoryDirect Category = "direct"
	CategoryOther  Category = "other"
)

// ClassifyOutbounds derives categories from the config that is actually handed
// to Xray, so provider JSON, balancers and chained outbounds are all covered.
// Service outbounds (dns, blackhole, api) never carry user traffic.
func ClassifyOutbounds(configJSON []byte) (map[string]Category, error) {
	var cfg struct {
		Outbounds []struct {
			Tag      string `json:"tag"`
			Protocol string `json:"protocol"`
		} `json:"outbounds"`
	}
	if err := json.Unmarshal(configJSON, &cfg); err != nil {
		return nil, err
	}
	categories := make(map[string]Category, len(cfg.Outbounds))
	for _, ob := range cfg.Outbounds {
		tag := strings.TrimSpace(ob.Tag)
		if tag == "" || tag == "api" {
			continue
		}
		switch strings.ToLower(strings.TrimSpace(ob.Protocol)) {
		case "freedom":
			categories[tag] = CategoryDirect
		case "blackhole", "dns", "loopback":
			categories[tag] = CategoryOther
		default:
			categories[tag] = CategoryProxy
		}
	}
	return categories, nil
}

// XraySource reads the per-outbound cumulative counters from Xray's
// StatsService. The gRPC connection is reused across samples.
type XraySource struct {
	apiPort    int
	categories map[string]Category

	mu     sync.Mutex
	conn   *grpc.ClientConn
	client statscmd.StatsServiceClient
}

func NewXraySource(apiPort int, categories map[string]Category) *XraySource {
	return &XraySource{apiPort: apiPort, categories: categories}
}

// Close releases the gRPC connection. Called when a meter is retired.
func (s *XraySource) Close() {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.conn != nil {
		_ = s.conn.Close()
		s.conn = nil
		s.client = nil
	}
}

func (s *XraySource) Read(ctx context.Context) (Counters, error) {
	if s.apiPort <= 0 {
		return Counters{XrayOK: false, FirewallOK: false}, nil
	}

	s.mu.Lock()
	if s.client == nil {
		conn, err := grpc.NewClient(
			fmt.Sprintf("127.0.0.1:%d", s.apiPort),
			grpc.WithTransportCredentials(insecure.NewCredentials()),
		)
		if err != nil {
			s.mu.Unlock()
			return Counters{XrayOK: false, FirewallOK: false}, err
		}
		s.conn = conn
		s.client = statscmd.NewStatsServiceClient(conn)
	}
	client := s.client
	s.mu.Unlock()

	// An empty pattern matches every counter (Xray uses strings.Contains).
	// Inbound counters are intentionally ignored: under R3 totals are wire
	// bytes measured at the outbound dispatcher.
	resp, err := client.QueryStats(ctx, &statscmd.QueryStatsRequest{Pattern: "", Reset_: false})
	if err != nil {
		s.Close()
		return Counters{XrayOK: false, FirewallOK: false}, err
	}

	var counters Counters
	for _, stat := range resp.Stat {
		parts := strings.Split(stat.Name, ">>>")
		if len(parts) < 2 || parts[0] != "outbound" {
			continue
		}
		var up, down *int64
		switch s.categories[parts[1]] {
		case CategoryProxy:
			up, down = &counters.ProxyUp, &counters.ProxyDown
		case CategoryDirect:
			up, down = &counters.DirectUp, &counters.DirectDown
		default:
			continue
		}
		switch {
		case strings.HasSuffix(stat.Name, "uplink"):
			*up += stat.Value
		case strings.HasSuffix(stat.Name, "downlink"):
			*down += stat.Value
		}
	}
	counters.XrayOK = true
	// Firewall counters are wired by FirewallSource; until then the sample is
	// explicitly partial rather than claiming complete coverage.
	counters.FirewallOK = false
	return counters, nil
}
