package utils

import "os"

// TestMode reports WHOISTHAT_TEST_MODE=1. The sandboxed IPC tests run a real
// daemon against the user's machine, so nothing may touch the global firewall,
// TUN devices or routing tables.
func TestMode() bool { return os.Getenv("WHOISTHAT_TEST_MODE") == "1" }
