package main

import "testing"

func TestAcquireInstanceLock(t *testing.T) {
	t.Setenv("XDG_RUNTIME_DIR", t.TempDir())

	ok, err := acquireInstanceLock()
	if err != nil || !ok {
		t.Fatalf("first lock: ok=%v err=%v", ok, err)
	}

	ok, err = acquireInstanceLock()
	if err != nil {
		t.Fatalf("second lock returned an I/O error: %v", err)
	}
	if ok {
		t.Fatal("second lock acquired; expected (false, nil)")
	}

	instanceLock.Close()
	instanceLock = nil

	ok, err = acquireInstanceLock()
	if err != nil || !ok {
		t.Fatalf("lock after release: ok=%v err=%v", ok, err)
	}
	instanceLock.Close()
	instanceLock = nil
}
