package lib

import (
	"whoisthat-core/lib/logger"
	"whoisthat-core/structs"
	"encoding/json"
)

func CreateJsonNotification(msg string, obj any) []byte {
	data := structs.Message[any]{
		Msg:  msg,
		Data: obj,
	}
	json_data, err := json.Marshal(data)
	if err != nil {
		logger.Fatalf("failed to parse json trying to send a message %v %s", data, err)
	}
	return json_data
}
