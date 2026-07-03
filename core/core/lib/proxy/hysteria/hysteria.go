package hysteria

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"sync"
	"syscall"
	"whoisthat-core/lib/logger"
	"whoisthat-core/utils"
)

// HysteriaCore manages the official hysteria2 client subprocess
// (github.com/apernet/hysteria2). xray-core does NOT implement hysteria2,
// so a separate binary is required. The lifecycle mirrors xray.XrayCore:
// Start feeds a YAML config via stdin, Stop kills the process, Exited
// reports the wait error to exactly one reader.
//
// The API intentionally matches xray.XrayCore so mainproxy can treat both
// interchangeably via a small interface (see mainproxy.coreProc).
type HysteriaCore struct {
	mu             sync.Mutex
	cmd            *exec.Cmd
	cancel         context.CancelFunc
	running        bool
	channel_closed bool
	Exited         chan error
}

// ExitedCh returns the channel closed/ fed when the subprocess terminates.
// Satisfies mainproxy.coreProc.
func (h *HysteriaCore) ExitedCh() chan error {
	return h.Exited
}

// Start launches `hysteria run -c -` (config read from stdin) with the given
// YAML bytes. The subprocess drops privileges to the dedicated uid/gid if
// configured (same as xray).
func (h *HysteriaCore) Start(stdinPipe []byte) error {
	h.mu.Lock()
	defer h.mu.Unlock()

	if h.running {
		return fmt.Errorf("hysteria is already running")
	}

	hybin, err := utils.GetHysteriaBin()
	if err != nil {
		return fmt.Errorf("failed to start hysteria: %w", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	// `hysteria run -c -` reads the client config from stdin. Avoids leaving
	// the (password-bearing) YAML on disk for the lifetime of the session.
	cmd := exec.CommandContext(ctx, hybin, "run", "-c", "-")
	stdin, err := cmd.StdinPipe()
	if err != nil {
		cancel()
		return fmt.Errorf("failed to get stdin: %w", err)
	}

	cmd.Stdout = nil
	hyLog, err := os.CreateTemp("", "whoisthat-hysteria-*.log")
	if err == nil {
		cmd.Stderr = hyLog
		logger.Infof("hysteria stderr -> %s", hyLog.Name())
	} else {
		hyLog = nil
		cmd.Stderr = nil
	}

	if uid := utils.DedicatedUid(); uid > 0 {
		gid := utils.DedicatedGid()
		if gid == 0 {
			gid = uid
		}
		cmd.SysProcAttr = &syscall.SysProcAttr{
			Credential: &syscall.Credential{
				Uid: uint32(uid),
				Gid: uint32(gid),
			},
		}
		logger.Infof("hysteria will run as uid=%d gid=%d", uid, gid)
	}

	if err := cmd.Start(); err != nil {
		cancel()
		return err
	}

	logger.Infof("hysteria started (pid=%d)", cmd.Process.Pid)

	go func() {
		defer stdin.Close()
		_, _ = stdin.Write(stdinPipe)
	}()

	h.cmd = cmd
	h.cancel = cancel
	h.running = true

	go func() {
		err := cmd.Wait()
		logger.Infof("hysteria stopped (pid=%d)", cmd.Process.Pid)
		// Keep the stderr log when the process exited with an error so
		// failures are diagnosable. Clean exit (err == nil and ctx not
		// cancelled) frees the temp log.
		if hyLog != nil && err == nil && ctx.Err() == nil {
			_ = os.Remove(hyLog.Name())
		} else if hyLog != nil {
			logger.Warnf("hysteria exited with error; stderr retained at %s (err=%v)", hyLog.Name(), err)
		}
		h.mu.Lock()
		defer h.mu.Unlock()
		if ctx.Err() == nil {
			select {
			case h.Exited <- err:
				if h.running {
					close(h.Exited)
					h.channel_closed = true
				}
				h.running = false
			default:
				// Channel full or no reader, don't block.
			}
		}
	}()

	return nil
}

func (h *HysteriaCore) Stop() {
	h.mu.Lock()
	defer h.mu.Unlock()
	if !h.channel_closed {
		close(h.Exited)
		h.channel_closed = true
	}
	if h.cmd != nil && h.cmd.Process != nil {
		if err := h.cmd.Process.Kill(); err != nil {
			logger.Warn("error killing hysteria process:", err)
		}
	}
	if h.cancel != nil {
		h.cancel()
	}
	h.running = false
}

func (h *HysteriaCore) IsRunning() bool {
	h.mu.Lock()
	defer h.mu.Unlock()
	return h.running
}