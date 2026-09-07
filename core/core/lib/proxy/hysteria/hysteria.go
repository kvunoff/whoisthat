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
// Start feeds a YAML config via a temporary file, Stop kills the process,
// Exited reports the wait error to exactly one reader.
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
	cfgPath        string
}

// ExitedCh returns the channel closed/ fed when the subprocess terminates.
// Satisfies mainproxy.coreProc.
func (h *HysteriaCore) ExitedCh() chan error {
	return h.Exited
}

// Start launches `hysteria client -c <config.yaml>` with the given YAML bytes.
// The config is written to a temporary file with 0600 permissions. The subprocess
// drops privileges to the dedicated uid/gid if configured (same as xray).
func (h *HysteriaCore) Start(stdinPipe []byte) error {
	h.mu.Lock()
	defer h.mu.Unlock()

	if h.running {
		return fmt.Errorf("hysteria is already running")
	}

	if h.Exited == nil || h.channel_closed {
		h.Exited = make(chan error, 1)
		h.channel_closed = false
	}

	hybin, err := utils.GetHysteriaBin()
	if err != nil {
		return fmt.Errorf("failed to start hysteria: %w", err)
	}

	// Write YAML config to a temporary file. Hysteria v2 uses Viper which determines
	// the unmarshaler from the file extension; reading from stdin ("-") fails with
	// "Unsupported Config Type \"\"". Mode 0600 ensures only the owner can read it.
	tmpCfg, err := os.CreateTemp("", "whoisthat-hy2-*.yaml")
	if err != nil {
		return fmt.Errorf("failed to create temp config: %w", err)
	}
	cfgPath := tmpCfg.Name()
	if _, err := tmpCfg.Write(stdinPipe); err != nil {
		tmpCfg.Close()
		_ = os.Remove(cfgPath)
		return fmt.Errorf("failed to write temp config: %w", err)
	}
	tmpCfg.Close()
	_ = os.Chmod(cfgPath, 0600)

	ctx, cancel := context.WithCancel(context.Background())
	cmd := exec.CommandContext(ctx, hybin, "client", "-c", cfgPath)
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
		_ = os.Chown(cfgPath, uid, gid)
		cmd.SysProcAttr = &syscall.SysProcAttr{
			Credential: &syscall.Credential{
				Uid: uint32(uid),
				Gid: uint32(gid),
			},
		}
		logger.Infof("hysteria will run as uid=%d gid=%d", uid, gid)
	}

	if err := cmd.Start(); err != nil {
		if hyLog != nil {
			hyLog.Close()
			_ = os.Remove(hyLog.Name())
		}
		_ = os.Remove(cfgPath)
		cancel()
		return err
	}

	// Close log fd in parent process once child has inherited it to prevent fd leak
	if hyLog != nil {
		hyLog.Close()
	}

	logger.Infof("hysteria started (pid=%d)", cmd.Process.Pid)

	h.cmd = cmd
	h.cancel = cancel
	h.cfgPath = cfgPath
	h.running = true

	go func() {
		err := cmd.Wait()
		logger.Infof("hysteria stopped (pid=%d)", cmd.Process.Pid)

		// Always clean up the temporary config file
		_ = os.Remove(cfgPath)

		// Keep stderr log only on unexpected failure. Clean exit (err == nil)
		// or expected stop (ctx cancelled) frees the temp log.
		if hyLog != nil && (err == nil || ctx.Err() != nil) {
			_ = os.Remove(hyLog.Name())
		} else if hyLog != nil {
			logger.Warnf("hysteria exited with error; stderr retained at %s (err=%v)", hyLog.Name(), err)
		}

		h.mu.Lock()
		defer h.mu.Unlock()
		if ctx.Err() == nil {
			h.running = false
			if !h.channel_closed {
				select {
				case h.Exited <- err:
				default:
				}
				close(h.Exited)
				h.channel_closed = true
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
	if h.cancel != nil {
		h.cancel()
	}
	if h.cmd != nil && h.cmd.Process != nil {
		if err := h.cmd.Process.Kill(); err != nil {
			logger.Warn("error killing hysteria process:", err)
		}
	}
	if h.cfgPath != "" {
		_ = os.Remove(h.cfgPath)
		h.cfgPath = ""
	}
	h.running = false
}

func (h *HysteriaCore) IsRunning() bool {
	h.mu.Lock()
	defer h.mu.Unlock()
	return h.running
}
