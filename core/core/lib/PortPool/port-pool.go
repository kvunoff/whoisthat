package portpool

import (
	"errors"
	"sync"
	"whoisthat-core/lib/logger"
)

type PortPool struct {
	start_port int
	end_port   int
	in_use     map[int]struct{}
	mu         sync.Mutex
	next_hint  int
}

func CreatePortPool(start_port int, end_port int) *PortPool {
	if start_port < 0 || end_port < 0 || end_port <= start_port {
		logger.Fatal("invalid testing port range")
	}
	return &PortPool{
		start_port: start_port,
		end_port:   end_port,
		in_use:     make(map[int]struct{}),
		next_hint:  start_port,
	}
}

// ReleasePort marks port as available. Safe to call with a port that was
// never acquired (no-op).
func (p *PortPool) ReleasePort(port int) {
	if port <= 0 {
		return
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	delete(p.in_use, port)
}

// GetPort returns the next available port. Hot path is O(1) when the
// range is not exhausted: we resume scanning from the cursor advanced by
// the previous allocation, so a tight alloc/release cycle never rescans
// the already-freed prefix. The in-memory in_use map is the source of
// truth — we do NOT do a kernel listen probe per allocation. The caller
// is about to bind a SOCKS5 listener on this port, which will surface a
// conflict naturally if the kernel happens to disagree.
func (p *PortPool) GetPort() (int, error) {
	p.mu.Lock()
	defer p.mu.Unlock()
	range_size := p.end_port - p.start_port
	for i := 0; i < range_size; i++ {
		port := p.start_port + (p.next_hint+i-p.start_port)%range_size
		if _, used := p.in_use[port]; used {
			continue
		}
		p.in_use[port] = struct{}{}
		p.next_hint = port + 1
		if p.next_hint >= p.end_port {
			p.next_hint = p.start_port
		}
		return port, nil
	}
	return -1, errors.New("no port available")
}
