package traffic

import "context"

// FirewallReader reports cumulative bypass bytes. ok=false means the counters
// are unavailable or not applicable (proxy mode, include mode, backend error).
type FirewallReader func(ctx context.Context) (up, down int64, ok bool, err error)

// CompositeSource merges the Xray StatsService counters with kernel bypass
// counters. A nil reader means bypass traffic cannot exist (proxy mode), which
// is complete coverage rather than a degraded sample.
type CompositeSource struct {
	xray     Source
	firewall FirewallReader
}

func NewCompositeSource(xray Source, firewall FirewallReader) *CompositeSource {
	return &CompositeSource{xray: xray, firewall: firewall}
}

func (c *CompositeSource) Read(ctx context.Context) (Counters, error) {
	counters, err := c.xray.Read(ctx)
	if c.firewall == nil {
		counters.FirewallOK = true
		return counters, err
	}
	up, down, ok, firewallErr := c.firewall(ctx)
	if firewallErr == nil && ok {
		counters.BypassUp = up
		counters.BypassDown = down
		counters.FirewallOK = true
	} else {
		counters.FirewallOK = false
	}
	return counters, err
}

func (c *CompositeSource) Close() {
	if closer, ok := c.xray.(interface{ Close() }); ok {
		closer.Close()
	}
}
