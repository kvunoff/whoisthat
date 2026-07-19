package portpool

import (
	"sync"
	"testing"
	"time"
)

func TestPortPoolAllocRelease(t *testing.T) {
	p := CreatePortPool(40000, 40100)
	port, err := p.GetPort()
	if err != nil {
		t.Fatalf("GetPort: %v", err)
	}
	if port < 40000 || port >= 40100 {
		t.Errorf("port = %d, want in [40000,40100)", port)
	}
	// The cursor advances after each allocation, so after release we
	// can't predict exactly which port comes next — but it must be a
	// valid in-range port that is currently free.
	p.ReleasePort(port)
	port2, err := p.GetPort()
	if err != nil {
		t.Fatalf("GetPort after release: %v", err)
	}
	if port2 < 40000 || port2 >= 40100 {
		t.Errorf("second port = %d, want in [40000,40100)", port2)
	}
	p.ReleasePort(port2)
}

func TestPortPoolExhaustion(t *testing.T) {
	p := CreatePortPool(40200, 40204) // 4 ports
	allocated := map[int]bool{}
	for i := 0; i < 4; i++ {
		port, err := p.GetPort()
		if err != nil {
			t.Fatalf("GetPort %d: %v", i, err)
		}
		if allocated[port] {
			t.Errorf("GetPort returned duplicate %d", port)
		}
		allocated[port] = true
	}
	if _, err := p.GetPort(); err == nil {
		t.Error("expected exhaustion error, got nil")
	}
	for port := range allocated {
		p.ReleasePort(port)
	}
}

func TestPortPoolConcurrentAlloc(t *testing.T) {
	p := CreatePortPool(40300, 40400) // 100 ports
	const workers = 20
	const iters = 5 // each worker holds 5 ports at once → up to 100 held concurrently
	var wg sync.WaitGroup
	var mu sync.Mutex
	held := map[int]bool{}
	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			local := []int{}
			for i := 0; i < iters; i++ {
				port, err := p.GetPort()
				if err != nil {
					t.Errorf("GetPort failed: %v", err)
					return
				}
				mu.Lock()
				if held[port] {
					t.Errorf("concurrent GetPort returned duplicate %d", port)
				}
				held[port] = true
				mu.Unlock()
				local = append(local, port)
			}
			// Release after collecting all so they were held simultaneously.
			for _, port := range local {
				p.ReleasePort(port)
			}
		}()
	}
	wg.Wait()
}

// TestPortPoolAllocReleaseCyclePerf guards against the O(n) kernel-listen
// probe that used to live inside GetPort. 200 alloc/release cycles should
// complete in well under 50ms on any reasonable machine. If this regresses
// back to a per-call net.Listen probe, it'll blow past the budget.
func TestPortPoolAllocReleaseCyclePerf(t *testing.T) {
	p := CreatePortPool(40500, 40700)
	start := time.Now()
	for i := 0; i < 200; i++ {
		port, err := p.GetPort()
		if err != nil {
			t.Fatalf("GetPort %d: %v", i, err)
		}
		p.ReleasePort(port)
	}
	elapsed := time.Since(start)
	if elapsed > 50*time.Millisecond {
		t.Errorf("200 alloc/release cycles took %v, want < 50ms (cursor regression?)", elapsed)
	}
}

func TestPortPoolReleaseUnacquiredIsNoop(t *testing.T) {
	p := CreatePortPool(40800, 40900)
	p.ReleasePort(0)     // zero is special
	p.ReleasePort(99999) // out of range, never acquired — should not panic
	port, err := p.GetPort()
	if err != nil {
		t.Fatalf("GetPort: %v", err)
	}
	if port != 40800 {
		t.Errorf("first allocation = %d, want 40800 (cursor should start at start_port)", port)
	}
}
