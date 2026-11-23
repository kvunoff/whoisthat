package traffic

import (
	"context"
	"sync"
	"time"

	"whoisthat-core/structs"
)

const sampleInterval = time.Second
const readTimeout = 500 * time.Millisecond

// Meter samples one Source, keeps monotone session totals and publishes a
// TrafficSample every tick (including idle and degraded ticks).
type Meter struct {
	source  Source
	session string
	out     chan structs.TrafficSample
	store   func(structs.TrafficSample)

	mu      sync.Mutex
	cancel  chan struct{}
	done    chan struct{}
	running bool

	// Owned by the loop goroutine (and by Stop after done is closed).
	prev    Counters
	prevAt  time.Time
	cum     structs.TrafficSample
	started bool
}

// New builds a meter. store receives every snapshot for synchronous reads
// (application-state); out carries the broadcast frames.
func New(source Source, session string, out chan structs.TrafficSample, store func(structs.TrafficSample)) *Meter {
	return &Meter{source: source, session: session, out: out, store: store}
}

func (m *Meter) Start() {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.running {
		return
	}
	m.cancel = make(chan struct{})
	m.done = make(chan struct{})
	m.running = true
	go m.loop(m.cancel, m.done)
}

// Stop ends the loop and publishes one final frame with connected=false.
func (m *Meter) Stop() {
	m.mu.Lock()
	if !m.running {
		m.mu.Unlock()
		return
	}
	cancel, done := m.cancel, m.done
	m.running = false
	m.mu.Unlock()
	close(cancel)
	<-done
	m.emit(false)
	if closer, ok := m.source.(interface{ Close() }); ok {
		closer.Close()
	}
}

func (m *Meter) loop(cancel <-chan struct{}, done chan<- struct{}) {
	defer close(done)
	ticker := time.NewTicker(sampleInterval)
	defer ticker.Stop()
	m.emit(true)
	for {
		select {
		case <-cancel:
			return
		case <-ticker.C:
			m.emit(true)
		}
	}
}

// step returns the delta since the previous counter and reports a rollback.
// A rollback resyncs the baseline instead of producing a negative delta.
func step(now, prev int64) (int64, bool) {
	if now < prev {
		return 0, true
	}
	return now - prev, false
}

func (m *Meter) emit(connected bool) {
	m.emitAt(time.Now(), connected)
}

// emitAt is the testable core of emit: now is injected so rates and intervals
// can be asserted without sleeping.
func (m *Meter) emitAt(now time.Time, connected bool) {
	ctx, cancel := context.WithTimeout(context.Background(), readTimeout)
	counters, err := m.source.Read(ctx)
	cancel()

	elapsedMs := int64(0)
	if m.started {
		elapsedMs = now.Sub(m.prevAt).Milliseconds()
	}
	if elapsedMs < 0 {
		elapsedMs = 0
	}

	source := "xray"
	if err != nil {
		// Keep the previous values: no movement, no negative deltas.
		counters = m.prev
		source = "unavailable"
	} else if !counters.XrayOK {
		source = "unavailable"
	} else if !counters.FirewallOK {
		source = "partial"
	}

	dProxyUp, r1 := step(counters.ProxyUp, m.prev.ProxyUp)
	dProxyDown, r2 := step(counters.ProxyDown, m.prev.ProxyDown)
	dDirectUp, r3 := step(counters.DirectUp, m.prev.DirectUp)
	dDirectDown, r4 := step(counters.DirectDown, m.prev.DirectDown)
	dBypassUp, r5 := step(counters.BypassUp, m.prev.BypassUp)
	dBypassDown, r6 := step(counters.BypassDown, m.prev.BypassDown)
	if r1 || r2 || r3 || r4 || r5 || r6 {
		source = "resync"
	}

	m.cum.ProxyUp += dProxyUp
	m.cum.ProxyDown += dProxyDown
	m.cum.DirectUp += dDirectUp
	m.cum.DirectDown += dDirectDown
	m.cum.BypassUp += dBypassUp
	m.cum.BypassDown += dBypassDown

	rate := func(delta int64) int64 { return 0 }
	if elapsedMs > 0 {
		rate = func(delta int64) int64 { return delta * 1000 / elapsedMs }
	}

	sample := structs.TrafficSample{
		SessionID:      m.session,
		TimestampMs:    now.UnixMilli(),
		IntervalMs:     elapsedMs,
		Connected:      connected,
		Source:         source,
		ProxyUp:        m.cum.ProxyUp,
		ProxyDown:      m.cum.ProxyDown,
		DirectUp:       m.cum.DirectUp,
		DirectDown:     m.cum.DirectDown,
		BypassUp:       m.cum.BypassUp,
		BypassDown:     m.cum.BypassDown,
		ProxyRateUp:    rate(dProxyUp),
		ProxyRateDown:  rate(dProxyDown),
		DirectRateUp:   rate(dDirectUp),
		DirectRateDown: rate(dDirectDown),
		BypassRateUp:   rate(dBypassUp),
		BypassRateDown: rate(dBypassDown),
	}

	m.prev = counters
	m.prevAt = now
	m.started = true
	m.publish(sample)
}

// publish keeps the last snapshot and replaces a stale queued frame rather
// than blocking the sampler or accumulating outdated samples.
func (m *Meter) publish(sample structs.TrafficSample) {
	if m.store != nil {
		m.store(sample)
	}
	select {
	case m.out <- sample:
		return
	default:
	}
	select {
	case <-m.out:
	default:
	}
	select {
	case m.out <- sample:
	default:
	}
}
