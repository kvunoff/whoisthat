package xray

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"syscall"
	"time"
	"whoisthat-core/lib/geo"
	"whoisthat-core/lib/logger"
	"whoisthat-core/lib/xraymgr"
	"whoisthat-core/utils"

	"golang.org/x/sys/unix"
)

type XrayCore struct {
	mu             sync.Mutex
	cmd            *exec.Cmd
	cancel         context.CancelFunc
	running        bool
	channel_closed bool
	Exited         chan error
	waitDone       chan struct{}
}

// ExitedCh returns the channel used to report subprocess termination. It
// satisfies mainproxy's coreProc interface.
func (x *XrayCore) ExitedCh() chan error {
	return x.Exited
}

// failedUnexpectedly distinguishes a real xray crash from an intentional
// stop: Stop() cancels the context before killing the process, so ctxErr is
// non-nil for normal stops.
func failedUnexpectedly(ctxErr error, waitErr error) bool {
	return ctxErr == nil && waitErr != nil
}

// crashTail returns at most limit bytes from the end of name. xray config
// errors are short; reading the whole file would let a chatty runtime fill
// core.log.
func crashTail(name string, limit int64) string {
	file, err := os.Open(name)
	if err != nil {
		return ""
	}
	defer file.Close()

	info, err := file.Stat()
	if err != nil {
		return ""
	}
	offset := info.Size() - limit
	if offset < 0 {
		offset = 0
	}
	if _, err := file.Seek(offset, io.SeekStart); err != nil {
		return ""
	}
	tail, _ := io.ReadAll(io.LimitReader(file, limit))
	return strings.TrimSpace(string(tail))
}

func setOrReplaceEnv(env []string, key, val string) []string {
	prefix := key + "="
	var res []string
	for _, e := range env {
		if !strings.HasPrefix(e, prefix) {
			res = append(res, e)
		}
	}
	return append(res, prefix+val)
}

func (x *XrayCore) Start(stdinPipe []byte, assetDir string) error {
	x.mu.Lock()
	defer x.mu.Unlock()

	if x.running {
		return fmt.Errorf("command is already running")
	}

	if x.Exited == nil || x.channel_closed {
		x.Exited = make(chan error, 1)
		x.channel_closed = false
	}

	xraybin, err := xraymgr.GetManager().GetXrayBin()
	if err != nil {
		return fmt.Errorf("failed to start xray: %w", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	cmd := exec.CommandContext(ctx, xraybin, "run")
	stdin, err := cmd.StdinPipe()

	if err != nil {
		cancel()
		return fmt.Errorf("failed to get stdin %w", err)
	}

	cmd.Stdout = nil
	xrayLog, err := os.CreateTemp("", "whoisthat-xray-*.log")
	if err == nil {
		cmd.Stderr = xrayLog
	} else {
		xrayLog = nil
		cmd.Stderr = nil
		logger.Warnf("xray: cannot capture stderr: %v", err)
	}

	cmd.SysProcAttr = &syscall.SysProcAttr{
		AmbientCaps: []uintptr{unix.CAP_NET_ADMIN, unix.CAP_NET_RAW},
	}

	if uid := utils.DedicatedUid(); uid > 0 {
		gid := utils.DedicatedGid()
		if gid == 0 {
			gid = uid
		}
		cmd.SysProcAttr.Credential = &syscall.Credential{
			Uid: uint32(uid),
			Gid: uint32(gid),
		}
		logger.Infof("xray will run as uid=%d gid=%d", uid, gid)
	}

	targetAssetDir := assetDir
	if targetAssetDir == "" {
		targetAssetDir = geo.AssetDir()
	}

	if targetAssetDir != "" {
		if assetDir == "" {
			geo.WaitReady(3 * time.Second)
		}
		if assetDir != "" || geo.IsReady() {
			cmd.Env = setOrReplaceEnv(os.Environ(), "XRAY_LOCATION_ASSET", targetAssetDir)
			logger.Infof("xray XRAY_LOCATION_ASSET=%s", targetAssetDir)
		} else if _, err := os.Stat(filepath.Join(targetAssetDir, "geoip.dat")); err == nil {
			cmd.Env = setOrReplaceEnv(os.Environ(), "XRAY_LOCATION_ASSET", targetAssetDir)
			logger.Infof("xray XRAY_LOCATION_ASSET=%s (fallback: files exist)", targetAssetDir)
		} else {
			logger.Warn("xray: geo assets not ready, skipping XRAY_LOCATION_ASSET")
		}
	}

	if err := cmd.Start(); err != nil {
		cancel()
		if xrayLog != nil {
			_ = xrayLog.Close()
			_ = os.Remove(xrayLog.Name())
		}
		return err
	}

	// The child inherited its own fd; the parent copy is no longer needed.
	if xrayLog != nil {
		_ = xrayLog.Close()
	}

	logger.Infof("xray started (pid=%d)", cmd.Process.Pid)

	waitDone := make(chan struct{})
	x.waitDone = waitDone

	go func() {
		defer stdin.Close()
		_, _ = stdin.Write(stdinPipe)
	}()

	x.cmd = cmd
	x.cancel = cancel
	x.running = true

	go func() {
		defer close(waitDone)
		err := cmd.Wait()
		logger.Infof("xray stopped (pid=%d)", cmd.Process.Pid)
		// An intentional stop cancels ctx before killing the process; both
		// "signal: killed" and "context canceled" land here and are not
		// failures. A real crash (including xray rejecting the config, e.g.
		// hysteria2 fed to it by mistake) has ctx.Err() == nil.
		if xrayLog != nil {
			name := xrayLog.Name()
			if failedUnexpectedly(ctx.Err(), err) {
				logger.Errorf("xray exited with error: %v", err)
				if tail := crashTail(name, 4096); tail != "" {
					// One line keeps the level tag/parse target intact.
					logger.Errorf("xray stderr: %s", strings.ReplaceAll(tail, "\n", " | "))
				}
			}
			_ = os.Remove(name)
		}
		x.mu.Lock()
		defer x.mu.Unlock()
		if ctx.Err() == nil {
			x.running = false
			if !x.channel_closed {
				select {
				case x.Exited <- err:
				default:
				}
				close(x.Exited)
				x.channel_closed = true
			}
		}

	}()

	return nil
}

func (x *XrayCore) Stop() {
	x.mu.Lock()
	if !x.channel_closed {
		close(x.Exited)
		x.channel_closed = true
	}
	cmd := x.cmd
	cancel := x.cancel
	waitDone := x.waitDone
	x.running = false
	x.mu.Unlock()

	if cancel != nil {
		cancel()
	}
	if cmd != nil && cmd.Process != nil {
		err := cmd.Process.Kill()
		if err != nil && !errors.Is(err, os.ErrProcessDone) {
			logger.Warn("error killing xray process:", err)
		}
	}
	if waitDone != nil {
		select {
		case <-waitDone:
		case <-time.After(500 * time.Millisecond):
			logger.Warn("xray stop: timed out waiting for process to exit")
		}
	}
}

func (x *XrayCore) IsRunning() bool {
	x.mu.Lock()
	defer x.mu.Unlock()
	return x.running
}
