package logger

import (
	"log"
	"strings"
	"sync/atomic"
)

type Level int32

const (
	ERROR Level = iota
	WARN
	INFO
	DEBUG
)

var logLevel atomic.Int32

func init() {
	logLevel.Store(int32(INFO))
}

// SetLevel is safe to call at runtime (the TUI changes it over IPC). An empty
// value (no WHOISTHAT_LOG_LEVEL) means the default info level.
func SetLevel(s string) {
	trimmed := strings.TrimSpace(strings.ToLower(s))

	var level Level
	unknown := false
	switch trimmed {
	case "", "info":
		level = INFO
	case "error":
		level = ERROR
	case "warn":
		level = WARN
	case "debug", "trace":
		// The daemon has no separate trace level; trace is the most verbose.
		level = DEBUG
	default:
		level = INFO
		unknown = true
	}

	logLevel.Store(int32(level))
	if unknown {
		Warnf("unknown log level %q, using info", s)
	}
}

func enabled(level Level) bool {
	return Level(logLevel.Load()) >= level
}

func Info(v ...interface{}) {
	if enabled(INFO) {
		log.Println(append([]interface{}{"[INFO]"}, v...)...)
	}
}

func Infof(format string, v ...interface{}) {
	if enabled(INFO) {
		log.Printf("[INFO] "+format, v...)
	}
}

func Debug(v ...interface{}) {
	if enabled(DEBUG) {
		log.Println(append([]interface{}{"[DEBU]"}, v...)...)
	}
}

func Debugf(format string, v ...interface{}) {
	if enabled(DEBUG) {
		log.Printf("[DEBU] "+format, v...)
	}
}

func Warn(v ...interface{}) {
	if enabled(WARN) {
		log.Println(append([]interface{}{"[WARN]"}, v...)...)
	}
}

func Warnf(format string, v ...interface{}) {
	if enabled(WARN) {
		log.Printf("[WARN] "+format, v...)
	}
}

func Error(v ...interface{}) {
	log.Println(append([]interface{}{"[ERRO]"}, v...)...)
}

func Errorf(format string, v ...interface{}) {
	log.Printf("[ERRO] "+format, v...)
}

func Fatal(v ...interface{}) {
	log.Fatal(append([]interface{}{"[FATA]"}, v...)...)
}

func Fatalf(format string, v ...interface{}) {
	log.Fatalf("[FATA] "+format, v...)
}
