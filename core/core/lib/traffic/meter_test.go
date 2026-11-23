package traffic

import (
	"context"
	"errors"
	"testing"
	"time"

	"whoisthat-core/structs"
)

type fakeSource struct {
	c     Counters
	err   error
	calls int
}

func (f *fakeSource) Read(context.Context) (Counters, error) {
	f.calls++
	return f.c, f.err
}

func testMeter(src Source) (*Meter, chan structs.TrafficSample) {
	out := make(chan structs.TrafficSample, 8)
	return New(src, "1-1000", out, nil), out
}

func counters(up, down int64) Counters {
	return Counters{ProxyUp: up, ProxyDown: down, XrayOK: true, FirewallOK: true}
}

func TestStep(t *testing.T) {
	if d, rollback := step(150, 100); d != 50 || rollback {
		t.Fatalf("step(150,100) = (%d,%v), want (50,false)", d, rollback)
	}
	if d, rollback := step(50, 100); d != 0 || !rollback {
		t.Fatalf("step(50,100) = (%d,%v), want (0,true)", d, rollback)
	}
}

func TestMeterRatesAndTotals(t *testing.T) {
	src := &fakeSource{c: counters(0, 0)}
	m, out := testMeter(src)
	base := time.Unix(1000, 0)

	m.emitAt(base, true)
	<-out // first sample: baseline, all rates zero

	src.c = counters(1000, 2000)
	m.emitAt(base.Add(time.Second), true)
	s := <-out
	if s.ProxyUp != 1000 || s.ProxyDown != 2000 {
		t.Fatalf("cumulative = %+v, want 1000/2000", s)
	}
	if s.ProxyRateUp != 1000 || s.ProxyRateDown != 2000 || s.IntervalMs != 1000 {
		t.Fatalf("1s rates = %d/%d over %dms, want 1000/2000 over 1000ms",
			s.ProxyRateUp, s.ProxyRateDown, s.IntervalMs)
	}

	// 4000/6000 bytes over 2 seconds must be reported as 2000/3000 B/s.
	src.c = counters(5000, 8000)
	m.emitAt(base.Add(3*time.Second), true)
	s = <-out
	if s.ProxyUp != 5000 || s.ProxyDown != 8000 {
		t.Fatalf("cumulative = %+v, want 5000/8000", s)
	}
	if s.ProxyRateUp != 2000 || s.ProxyRateDown != 3000 || s.IntervalMs != 2000 {
		t.Fatalf("2s rates = %d/%d over %dms, want 2000/3000 over 2000ms",
			s.ProxyRateUp, s.ProxyRateDown, s.IntervalMs)
	}
}

func TestMeterRollbackResyncs(t *testing.T) {
	src := &fakeSource{c: counters(0, 0)}
	m, out := testMeter(src)
	base := time.Unix(1000, 0)
	m.emitAt(base, true)
	<-out
	src.c = counters(5000, 5000)
	m.emitAt(base.Add(time.Second), true)
	<-out

	// Xray restarted: cumulative counters reset to a small value.
	src.c = counters(100, 100)
	m.emitAt(base.Add(2*time.Second), true)
	s := <-out
	if s.Source != "resync" || s.ProxyRateUp != 0 || s.ProxyUp != 5000 {
		t.Fatalf("rollback sample = %+v, want resync with zero rate and preserved total", s)
	}

	// Next sample continues from the new baseline.
	src.c = counters(600, 600)
	m.emitAt(base.Add(3*time.Second), true)
	s = <-out
	if s.ProxyUp != 5500 || s.ProxyRateUp != 500 {
		t.Fatalf("post-resync = %+v, want cumulative 5500 and rate 500", s)
	}
}

func TestMeterErrorKeepsTotals(t *testing.T) {
	src := &fakeSource{c: counters(1000, 1000)}
	m, out := testMeter(src)
	base := time.Unix(1000, 0)
	m.emitAt(base, true)
	<-out
	m.emitAt(base.Add(time.Second), true)
	<-out

	src.err = errors.New("grpc down")
	m.emitAt(base.Add(2*time.Second), true)
	s := <-out
	if s.Source != "unavailable" || s.ProxyUp != 1000 || s.ProxyRateUp != 0 {
		t.Fatalf("error sample = %+v, want unavailable with preserved total", s)
	}
}

func TestMeterStopEmitsDisconnected(t *testing.T) {
	src := &fakeSource{c: counters(0, 0)}
	m, out := testMeter(src)
	m.Start()
	<-out // initial frame
	m.Stop()
	s := <-out
	if s.Connected {
		t.Fatalf("final sample connected=%v, want false", s.Connected)
	}
	if src.calls < 2 {
		t.Fatalf("source read %d times, want at least 2", src.calls)
	}
}
