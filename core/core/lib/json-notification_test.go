package lib

import (
	"encoding/json"
	"testing"
)

func TestCreateJsonNotificationWithIDEchoesID(t *testing.T) {
	raw := CreateJsonNotificationWithID("warn", map[string]string{"content": "x"}, "req-42")
	var msg struct {
		Msg string `json:"msg"`
		ID  string `json:"id"`
	}
	if err := json.Unmarshal(raw, &msg); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if msg.ID != "req-42" {
		t.Errorf("id = %q, want req-42", msg.ID)
	}
}

func TestCreateJsonNotificationOmitsEmptyID(t *testing.T) {
	raw := CreateJsonNotification("warn", map[string]string{"content": "x"})
	var msg map[string]any
	if err := json.Unmarshal(raw, &msg); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if _, exists := msg["id"]; exists {
		t.Error("empty id must be omitted from the wire payload")
	}
}
