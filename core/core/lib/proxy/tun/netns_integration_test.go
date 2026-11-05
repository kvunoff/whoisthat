//go:build integration

package tunmode

import (
	"net"
	"os"
	"os/exec"
	"runtime"
	"strings"
	"testing"
	"time"

	"golang.org/x/sys/unix"

	"whoisthat-core/lib/netmark"
)

// These tests exercise the real ip/nftables/sysctl commands in a throwaway
// network namespace. They require root (or CAP_NET_ADMIN) and the nft binary,
// and are excluded from the normal `go test ./...` run via the integration
// build tag. Run with:
//
//	sudo -E go test -tags=integration ./lib/proxy/tun/
func enterTestNetns(t *testing.T) {
	t.Helper()
	if _, err := exec.LookPath("nft"); err != nil {
		t.Skip("nft not installed")
	}
	if _, err := exec.LookPath("ip"); err != nil {
		t.Skip("ip not installed")
	}

	// Lock the goroutine to this OS thread: network namespaces are per-thread
	// and we do not want to move the rest of the test process into the sandbox.
	runtime.LockOSThread()
	t.Cleanup(runtime.UnlockOSThread)

	if err := unix.Unshare(unix.CLONE_NEWNET); err != nil {
		t.Skipf("cannot create network namespace (need root/CAP_NET_ADMIN): %v", err)
	}
	if _, err := runScriptWithSh("ip link set lo up"); err != nil {
		t.Skipf("cannot bring up loopback in test namespace: %v", err)
	}
}

func TestNetnsTunLifecycleAndReconcile(t *testing.T) {
	enterTestNetns(t)
	t.Setenv("XDG_RUNTIME_DIR", t.TempDir())

	const tun = "wittest0"
	if err := createTun(tun, "198.18.0.1", "fd00::1"); err != nil {
		t.Skipf("cannot create TUN device in test namespace: %v", err)
	}
	if _, err := runScriptWithSh("ip link show " + tun); err != nil {
		t.Fatalf("TUN device not visible after createTun: %v", err)
	}

	// Simulate a crashed session: leftover nft table, policy rule and route.
	if _, err := runScriptWithSh(`
nft add table inet whoisthat_ks 2>/dev/null || true
nft add table inet whoisthat_split 2>/dev/null || true
ip rule add fwmark 1 table 100 2>/dev/null || true
ip rule add fwmark 2 table 200 2>/dev/null || true
ip route replace default dev ` + tun + ` table 100 2>/dev/null || true
ip route replace default dev ` + tun + ` table 200 2>/dev/null || true
`); err != nil {
		t.Fatalf("failed to seed orphan state: %v", err)
	}

	if err := ReconcileOrphanedRules(tun); err != nil {
		t.Fatalf("ReconcileOrphanedRules: %v", err)
	}

	if out, err := runScriptWithSh("ip link show " + tun); err == nil {
		t.Fatalf("TUN device still present after reconcile: %s", out)
	}
	if out, err := runScriptWithSh("ip rule show"); err == nil && strings.Contains(out, "lookup 100") {
		t.Errorf("table 100 rule still present after reconcile: %s", out)
	}
	if out, err := runScriptWithSh("ip rule show"); err == nil && strings.Contains(out, "lookup 200") {
		t.Errorf("table 200 rule still present after reconcile: %s", out)
	}
	if _, err := runScriptWithSh("nft list table inet whoisthat_ks"); err == nil {
		t.Error("kill-switch table still present after reconcile")
	}
	if _, err := runScriptWithSh("nft list table inet whoisthat_split"); err == nil {
		t.Error("split table still present after reconcile")
	}
}

func TestNetnsKillSwitchInstallAndRemove(t *testing.T) {
	enterTestNetns(t)

	if err := SetupKillSwitchBlock([]string{"192.0.2.10"}, "wittest0", []string{"1.1.1.1"}); err != nil {
		t.Fatalf("SetupKillSwitchBlock: %v", err)
	}
	if out, err := runScriptWithSh("nft list table inet whoisthat_ks"); err != nil {
		t.Fatalf("kill-switch table missing: %s", out)
	}
	if err := RemoveKillSwitchBlock(); err != nil {
		t.Fatalf("RemoveKillSwitchBlock: %v", err)
	}
	if _, err := runScriptWithSh("nft list table inet whoisthat_ks"); err == nil {
		t.Error("kill-switch table still present after remove")
	}
}

func TestNetnsRpFilterRestored(t *testing.T) {
	enterTestNetns(t)
	t.Setenv("XDG_RUNTIME_DIR", t.TempDir())

	const tun = "wittest0"
	if err := createTun(tun, "198.18.0.1", ""); err != nil {
		t.Skipf("cannot create TUN device: %v", err)
	}
	defer func() { _, _ = runScriptWithSh("ip link del dev " + tun + " 2>/dev/null || true") }()

	before, err := os.ReadFile("/proc/sys/net/ipv4/conf/" + tun + "/rp_filter")
	if err != nil {
		t.Skipf("rp_filter sysctl unavailable: %v", err)
	}
	original := strings.TrimSpace(string(before))

	if err := loosenRpFilter(tun, "lo"); err != nil {
		t.Fatalf("loosenRpFilter: %v", err)
	}
	after, _ := os.ReadFile("/proc/sys/net/ipv4/conf/" + tun + "/rp_filter")
	if strings.TrimSpace(string(after)) != "2" {
		t.Fatalf("rp_filter = %q, want 2", strings.TrimSpace(string(after)))
	}

	if err := restoreRpFilter(); err != nil {
		t.Fatalf("restoreRpFilter: %v", err)
	}
	restored, _ := os.ReadFile("/proc/sys/net/ipv4/conf/" + tun + "/rp_filter")
	if strings.TrimSpace(string(restored)) != original {
		t.Errorf("rp_filter restored to %q, want %q", strings.TrimSpace(string(restored)), original)
	}
}

func countDropRules(t *testing.T, rules string) int {
	t.Helper()
	count := 0
	for _, line := range strings.Split(rules, "\n") {
		if strings.TrimSpace(line) == "drop" {
			count++
		}
	}
	return count
}

func TestNetnsKillSwitchArmTwiceIsIdempotent(t *testing.T) {
	enterTestNetns(t)

	for i := 0; i < 2; i++ {
		if err := SetupKillSwitchBlock([]string{"192.0.2.10"}, "wittest0", []string{"1.1.1.1"}); err != nil {
			t.Fatalf("SetupKillSwitchBlock (attempt %d): %v", i+1, err)
		}
	}

	out, err := runScriptWithSh("nft list chain inet whoisthat_ks output")
	if err != nil {
		t.Fatalf("listing the chain: %v", err)
	}
	if got := countDropRules(t, out); got != 1 {
		t.Errorf("drop rules = %d, want 1:\n%s", got, out)
	}
	if got := strings.Count(out, "192.0.2.10"); got != 1 {
		t.Errorf("proxy IP appears %d times, want 1:\n%s", got, out)
	}
}

// An interrupted install could leave a table without its chain; the bootstrap
// must repair it instead of skipping and failing to flush a missing chain.
func TestNetnsKillSwitchRepairsTableWithoutChain(t *testing.T) {
	enterTestNetns(t)

	if _, err := runScriptWithSh("nft add table inet whoisthat_ks"); err != nil {
		t.Fatalf("seed empty table: %v", err)
	}
	if err := SetupKillSwitchBlock(nil, "wittest0", []string{"1.1.1.1"}); err != nil {
		t.Fatalf("SetupKillSwitchBlock over a chain-less table: %v", err)
	}

	out, err := runScriptWithSh("nft list chain inet whoisthat_ks output")
	if err != nil {
		t.Fatalf("listing the chain: %v", err)
	}
	if got := countDropRules(t, out); got != 1 {
		t.Errorf("drop rules = %d, want 1:\n%s", got, out)
	}
}

func TestNetnsKillSwitchArmedProbeAndRemove(t *testing.T) {
	enterTestNetns(t)

	if armed, err := KillSwitchArmed(); armed || err != nil {
		t.Fatalf("KillSwitchArmed() before arming = %v, %v; want false, nil", armed, err)
	}
	if err := SetupKillSwitchBlock([]string{"192.0.2.10"}, "wittest0", []string{"1.1.1.1"}); err != nil {
		t.Fatalf("SetupKillSwitchBlock: %v", err)
	}
	if armed, err := KillSwitchArmed(); !armed || err != nil {
		t.Fatalf("KillSwitchArmed() after arming = %v, %v; want true, nil", armed, err)
	}
	if err := RemoveKillSwitchBlock(); err != nil {
		t.Fatalf("RemoveKillSwitchBlock: %v", err)
	}
	if armed, err := KillSwitchArmed(); armed || err != nil {
		t.Fatalf("KillSwitchArmed() after removal = %v, %v; want false, nil", armed, err)
	}
}

// The daemon's own downloads (geo, runtime, subscriptions) must keep working
// while the block is armed; they rely on SO_MARK=DirectMark.
func TestNetnsNetmarkSetsDirectMark(t *testing.T) {
	enterTestNetns(t)

	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("listen: %v", err)
	}
	defer listener.Close()

	conn, err := netmark.Dialer(time.Second).Dial("tcp", listener.Addr().String())
	if err != nil {
		t.Fatalf("dial: %v", err)
	}
	defer conn.Close()

	tcp, ok := conn.(*net.TCPConn)
	if !ok {
		t.Fatalf("connection is %T, want *net.TCPConn", conn)
	}
	raw, err := tcp.SyscallConn()
	if err != nil {
		t.Fatalf("SyscallConn: %v", err)
	}
	var mark int
	if err := raw.Control(func(fd uintptr) {
		mark, _ = unix.GetsockoptInt(int(fd), unix.SOL_SOCKET, unix.SO_MARK)
	}); err != nil {
		t.Fatalf("control: %v", err)
	}
	if mark != netmark.DirectMark {
		t.Fatalf("SO_MARK = %d, want %d", mark, netmark.DirectMark)
	}
}
