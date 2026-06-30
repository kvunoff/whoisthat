package lib

import (
	"encoding/json"
	"whoisthat-core/lib/logger"
	"whoisthat-core/structs"
)

// CreateJsonNotification marshals a (msg, data) pair into the wire format.
// On marshal error it returns a hardcoded error-envelope rather than crashing
// the daemon — a single unserializable field shouldn't take down the VPN.
func CreateJsonNotification(msg string, obj any) []byte {
	data := structs.Message[any]{
		Msg:  msg,
		Data: obj,
	}
	json_data, err := json.Marshal(data)
	if err != nil {
		logger.Errorf("failed to marshal notification %q: %v", msg, err)
		return []byte(`{"msg":"error","data":{"key":"internal","content":"notification marshal failed"}}`)
	}
	return json_data
}
