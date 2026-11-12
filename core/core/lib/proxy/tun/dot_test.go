package tunmode

import (
	"fmt"
	"strings"
	"testing"
)

// fakeResolvectl emulates `resolvectl dnsovertls <link> [value]` with a
// per-link store. stuck=true models a link whose DoT setting cannot be
// overridden (readback keeps the old value).
type fakeResolvectl struct {
	present bool
	value   string
	stuck   bool
	calls   []string
}

func (f *fakeResolvectl) install(t *testing.T) {
	t.Helper()
	oldLook, oldRun := resolvectlLookPath, resolvectlRun
	resolvectlLookPath = func(string) (string, error) {
		if f.present {
			return "/usr/bin/resolvectl", nil
		}
		return "", fmt.Errorf("not found")
	}
	resolvectlRun = func(args ...string) (string, error) {
		f.calls = append(f.calls, strings.Join(args, " "))
		if len(args) == 2 && args[0] == "dnsovertls" {
			return fmt.Sprintf("Link 7 (%s): %s\n", args[1], f.value), nil
		}
		if len(args) == 3 && args[0] == "dnsovertls" {
			if !f.stuck {
				f.value = args[2]
			}
			return "", nil
		}
		return "", fmt.Errorf("unexpected resolvectl %v", args)
	}
	t.Cleanup(func() {
		resolvectlLookPath, resolvectlRun = oldLook, oldRun
		resetDoTState()
	})
	resetDoTState()
}

func TestDoTNeutralisedOnArmAndPriorValueRestored(t *testing.T) {
	for _, prior := range []string{"yes", "opportunistic", "no"} {
		t.Run(prior, func(t *testing.T) {
			f := &fakeResolvectl{present: true, value: prior}
			f.install(t)
			if err := neutraliseDoT("wittun"); err != nil {
				t.Fatalf("neutraliseDoT: %v", err)
			}
			if f.value != "no" {
				t.Fatalf("armed dnsovertls = %q, want no", f.value)
			}
			if err := restoreDoT(); err != nil {
				t.Fatalf("restoreDoT: %v", err)
			}
			if f.value != prior {
				t.Fatalf("after off dnsovertls = %q, want prior %q", f.value, prior)
			}
		})
	}
}

func TestDoTRefusesToArmWhenItCannotBeNeutralised(t *testing.T) {
	f := &fakeResolvectl{present: true, value: "yes", stuck: true}
	f.install(t)
	err := neutraliseDoT("wittun")
	if err == nil {
		t.Fatal("must refuse to arm: DoT-only resolver would be blackholed by the drop chain")
	}
	if !strings.Contains(err.Error(), "DNS-over-TLS") {
		t.Fatalf("error should explain the cause, got: %v", err)
	}
}

func TestDoTNoopWithoutResolvedOrTun(t *testing.T) {
	f := &fakeResolvectl{present: false, value: "yes"}
	f.install(t)
	if err := neutraliseDoT("wittun"); err != nil || len(f.calls) != 0 {
		t.Fatalf("no resolvectl: err=%v calls=%v", err, f.calls)
	}
	f.present = true
	if err := neutraliseDoT(""); err != nil || len(f.calls) != 0 {
		t.Fatalf("no tun: err=%v calls=%v", err, f.calls)
	}
	if err := restoreDoT(); err != nil {
		t.Fatalf("restore with nothing recorded: %v", err)
	}
}

func TestDoTDeferredUntilTunDeviceExists(t *testing.T) {
	f := &fakeResolvectl{present: true, value: "yes"}
	f.install(t)
	real := resolvectlRun
	up := false
	resolvectlRun = func(args ...string) (string, error) {
		if !up {
			return "Failed to resolve interface \"wittun\": No such device\n", fmt.Errorf("exit status 1")
		}
		return real(args...)
	}
	if err := neutraliseDoT("wittun"); err != nil {
		t.Fatalf("arming before the device exists must not fail: %v", err)
	}
	if err := ReconcileDoT(); err != nil { // still absent: stays pending, not an error
		t.Fatalf("reconcile while absent: %v", err)
	}
	up = true
	if err := ReconcileDoT(); err != nil {
		t.Fatalf("reconcile: %v", err)
	}
	if f.value != "no" {
		t.Fatalf("dnsovertls = %q after reconcile, want no", f.value)
	}
	if err := restoreDoT(); err != nil || f.value != "yes" {
		t.Fatalf("restore: err=%v value=%q", err, f.value)
	}
}
