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
	return CreateJsonNotificationWithID(msg, obj, "")
}

// CreateJsonNotificationWithID is CreateJsonNotification plus the optional
// request-correlation id. Clients that send an `id` receive it echoed on the
// notifications produced while handling that request.
func CreateJsonNotificationWithID(msg string, obj any, id string) []byte {
	data := structs.Message[any]{
		Msg:  msg,
		Data: obj,
		ID:   id,
	}
	json_data, err := json.Marshal(data)
	if err != nil {
		logger.Errorf("failed to marshal notification %q: %v", msg, err)
		return []byte(`{"msg":"error","data":{"key":"internal","content":"notification marshal failed"}}`)
	}
	return json_data
}
