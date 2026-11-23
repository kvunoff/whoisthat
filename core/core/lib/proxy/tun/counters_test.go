package tunmode

import "testing"

const nftFixture = `{"nftables": [
  {"metainfo": {"version": "1.1.7"}},
  {"counter": {"family": "inet", "name": "bypass_down", "table": "wt", "packets": 3, "bytes": 252}},
  {"rule": {"family": "inet", "table": "wt", "chain": "output", "comment": "wt-bypass-up", "expr": [
    {"mangle": {"key": {"meta": {"key": "mark"}}, "value": 1}},
    {"counter": {"packets": 5, "bytes": 420}}
  ]}},
  {"rule": {"family": "inet", "table": "wt", "chain": "input", "comment": "wt-bypass-down", "expr": [
    {"counter": "bypass_down"}
  ]}},
  {"rule": {"family": "inet", "table": "wt", "chain": "prerouting", "comment": "wt-bypass-down", "expr": [
    {"counter": {"packets": 2, "bytes": 168}}
  ]}},
  {"rule": {"family": "inet", "table": "wt", "chain": "prerouting", "expr": [
    {"mangle": {"key": {"ct": {"key": "mark"}}, "value": 1}},
    {"counter": {"packets": 99, "bytes": 99999}}
  ]}}
]}`

func TestParseNftCounters(t *testing.T) {
	up, down, found, err := parseNftCounters(nftFixture)
	if err != nil {
		t.Fatalf("parseNftCounters: %v", err)
	}
	if !found {
		t.Fatal("expected accounting rules to be found")
	}
	if up != 420 {
		t.Errorf("up = %d, want 420", up)
	}
	// Named counter (252) + anonymous prerouting counter (168).
	if down != 420 {
		t.Errorf("down = %d, want 420", down)
	}
}

func TestParseNftCountersIgnoresOtherRules(t *testing.T) {
	up, down, found, err := parseNftCounters(`{"nftables": [
	  {"rule": {"comment": "other", "expr": [{"counter": {"packets": 1, "bytes": 1000}}]}}
	]}`)
	if err != nil {
		t.Fatalf("parseNftCounters: %v", err)
	}
	if found || up != 0 || down != 0 {
		t.Fatalf("non-accounting rules counted: (%d,%d,%v)", up, down, found)
	}
}

func TestParseIptablesCounters(t *testing.T) {
	raw := `Chain PREROUTING (policy ACCEPT 0 packets, 0 bytes)
    pkts bytes target     prot opt in     out     source               destination
       3   252 WT_BYPASS_DOWN  all  --  *      *       0.0.0.0/0            0.0.0.0/0            /* wt-bypass-down */
       2   168 MARK       all  --  *      *       0.0.0.0/0            0.0.0.0/0            /* wt-bypass-up */ MARK set 0x1
       1    84            all  --  *      *       0.0.0.0/0            0.0.0.0/0
`
	up, down, found := parseIptablesCounters(raw)
	if !found {
		t.Fatal("expected commented rules to be found")
	}
	if down != 252 || up != 168 {
		t.Errorf("up/down = %d/%d, want 168/252", up, down)
	}
}
