// Package traffic owns connection traffic accounting. The core samples
// cumulative counters, computes rates and publishes snapshots; clients only
// render them.
package traffic

import "context"

// Counters are cumulative byte counters for one sample. The sources report
// wire bytes; the meter turns them into a session-wide TrafficSample.
type Counters struct {
	ProxyUp, ProxyDown   int64
	DirectUp, DirectDown int64
	BypassUp, BypassDown int64

	// XrayOK is false when the StatsService could not be read. FirewallOK is
	// false when the kernel bypass counters are unavailable or not applicable.
	XrayOK     bool
	FirewallOK bool
}

// Source reads the current cumulative counters.
type Source interface {
	Read(ctx context.Context) (Counters, error)
}

// NopSource yields no traffic. It keeps the contract alive until the real
// sources are wired, and is used while disconnected.
type NopSource struct{}

func (NopSource) Read(context.Context) (Counters, error) {
	return Counters{XrayOK: true, FirewallOK: true}, nil
}

// SyntheticSource produces deterministic, monotonically increasing counters.
// It is enabled only in WHOISTHAT_TEST_MODE so the IPC contract can be
// exercised without Xray or a live firewall.
type SyntheticSource struct{ n int64 }

func (s *SyntheticSource) Read(context.Context) (Counters, error) {
	s.n++
	step := s.n * 1024
	return Counters{
		ProxyUp: step, ProxyDown: step * 3,
		DirectUp: step / 2, DirectDown: step,
		BypassUp: step / 4, BypassDown: step / 2,
		XrayOK: true, FirewallOK: true,
	}, nil
}
