package logger

import (
	"log"
	"strings"
)

type Level int

const (
	ERROR Level = iota
	WARN
	INFO
)

var logLevel = INFO

func SetLevel(s string) {
	switch strings.ToLower(s) {
	case "error":
		logLevel = ERROR
	case "warn":
		logLevel = WARN
	case "debug", "trace":
		logLevel = INFO
	default:
		logLevel = INFO
	}
}

func Info(v ...interface{}) {
	if logLevel >= INFO {
		log.Println(append([]interface{}{"[INFO]"}, v...)...)
	}
}

func Infof(format string, v ...interface{}) {
	if logLevel >= INFO {
		log.Printf("[INFO] "+format, v...)
	}
}

func Warn(v ...interface{}) {
	if logLevel >= WARN {
		log.Println(append([]interface{}{"[WARN]"}, v...)...)
	}
}

func Warnf(format string, v ...interface{}) {
	if logLevel >= WARN {
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
