package cmd

import "testing"

func TestReloadRoutingNilManagerIsNoop(t *testing.T) {
	(&Cmd{}).reloadRouting(nil, "")
}
