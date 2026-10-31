package logger

import (
	"bytes"
	"log"
	"strings"
	"sync"
	"testing"
)

func capture(t *testing.T) *bytes.Buffer {
	t.Helper()
	oldWriter := log.Writer()
	oldFlags := log.Flags()
	buf := &bytes.Buffer{}
	log.SetOutput(buf)
	log.SetFlags(0)
	t.Cleanup(func() {
		log.SetOutput(oldWriter)
		log.SetFlags(oldFlags)
		logLevel.Store(int32(INFO))
	})
	return buf
}

func TestEmptyLevelMeansInfoWithoutWarning(t *testing.T) {
	buf := capture(t)
	SetLevel("")
	Info("hello")
	if !strings.Contains(buf.String(), "[INFO] hello") {
		t.Fatalf("expected info output, got %q", buf.String())
	}
	if strings.Contains(buf.String(), "unknown log level") {
		t.Fatalf("empty env must not warn: %q", buf.String())
	}
}

func TestUnknownLevelWarnsAndFallsBackToInfo(t *testing.T) {
	buf := capture(t)
	SetLevel("verbose")
	if !strings.Contains(buf.String(), "unknown log level") {
		t.Fatalf("expected warning, got %q", buf.String())
	}
	buf.Reset()
	Info("visible")
	if !strings.Contains(buf.String(), "[INFO] visible") {
		t.Fatalf("info should be enabled after fallback, got %q", buf.String())
	}
}

func TestDebugOnlyAtDebugAndTrace(t *testing.T) {
	buf := capture(t)

	SetLevel("info")
	Debug("hidden")
	if strings.Contains(buf.String(), "[DEBU] hidden") {
		t.Fatalf("debug must be filtered at info: %q", buf.String())
	}

	SetLevel("trace")
	Debug("visible")
	if !strings.Contains(buf.String(), "[DEBU] visible") {
		t.Fatalf("trace must enable debug: %q", buf.String())
	}
}

func TestSetLevelIsRaceFree(t *testing.T) {
	capture(t)
	var wg sync.WaitGroup
	for i := 0; i < 8; i++ {
		wg.Add(1)
		go func(n int) {
			defer wg.Done()
			for j := 0; j < 100; j++ {
				switch n % 4 {
				case 0:
					SetLevel("debug")
				case 1:
					SetLevel("warn")
				case 2:
					Info("x")
					Debug("y")
				default:
					Warn("z")
				}
			}
		}(i)
	}
	wg.Wait()
}
