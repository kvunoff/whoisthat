package TCPServer

import (
	"bufio"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io"

	"whoisthat-core/structs"
)

// MaxFrameSize is the maximum accepted IPC frame. Kept in sync with
// src/core_client/connection.rs:MAX_FRAME_SIZE (64 MiB).
const MaxFrameSize = 64 * 1024 * 1024

// MaxHelloSize caps the pre-auth TCP hello. The payload is a token and a
// protocol number; without a separate limit an unauthenticated client could
// make the core allocate a full 64 MiB frame.
const MaxHelloSize = 64 * 1024

func readTCPMessage(reader *bufio.Reader) (structs.TCPMessage, error) {
	return readTCPMessageLimit(reader, MaxFrameSize)
}

func readTCPMessageLimit(reader *bufio.Reader, max uint32) (structs.TCPMessage, error) {
	var msg structs.TCPMessage

	lengthBuf := make([]byte, 4)
	if _, err := io.ReadFull(reader, lengthBuf); err != nil {
		return msg, err
	}

	length := binary.BigEndian.Uint32(lengthBuf)
	if length == 0 || length > max {
		return msg, fmt.Errorf("invalid frame length %d (max %d)", length, max)
	}

	payload := make([]byte, length)
	if _, err := io.ReadFull(reader, payload); err != nil {
		return msg, err
	}
	if err := json.Unmarshal(payload, &msg); err != nil {
		return msg, fmt.Errorf("invalid JSON: %w", err)
	}
	return msg, nil
}
