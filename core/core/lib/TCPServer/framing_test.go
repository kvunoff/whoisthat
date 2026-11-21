package TCPServer

import (
	"bufio"
	"bytes"
	"encoding/binary"
	"strings"
	"testing"
)

func framedReader(t *testing.T, payload []byte) *bufio.Reader {
	t.Helper()
	var buf bytes.Buffer
	if err := binary.Write(&buf, binary.BigEndian, uint32(len(payload))); err != nil {
		t.Fatal(err)
	}
	buf.Write(payload)
	return bufio.NewReader(&buf)
}

func TestReadTCPMessageLimitAcceptsAtLimit(t *testing.T) {
	payload := []byte(`{"msg":"hello","data":{}}`)
	msg, err := readTCPMessageLimit(framedReader(t, payload), uint32(len(payload)))
	if err != nil {
		t.Fatalf("readTCPMessageLimit: %v", err)
	}
	if msg.Msg != "hello" {
		t.Fatalf("msg = %q, want hello", msg.Msg)
	}
}

func TestReadTCPMessageLimitRejectsOversizedBeforeBody(t *testing.T) {
	var buf bytes.Buffer
	if err := binary.Write(&buf, binary.BigEndian, uint32(MaxHelloSize+1)); err != nil {
		t.Fatal(err)
	}
	// No body follows: the limit must be enforced from the length prefix.
	_, err := readTCPMessageLimit(bufio.NewReader(&buf), MaxHelloSize)
	if err == nil || !strings.Contains(err.Error(), "max") {
		t.Fatalf("oversized hello = %v, want a max-length error", err)
	}
}

func TestReadTCPMessageLimitRejectsZero(t *testing.T) {
	var buf bytes.Buffer
	if err := binary.Write(&buf, binary.BigEndian, uint32(0)); err != nil {
		t.Fatal(err)
	}
	if _, err := readTCPMessageLimit(bufio.NewReader(&buf), MaxHelloSize); err == nil {
		t.Fatal("zero-length frame accepted")
	}
}
