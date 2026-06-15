package xray

import (
	"whoisthat-core/lib/geo"
	"whoisthat-core/lib/logger"
	"whoisthat-core/utils"
	"context"
	"fmt"
	"os"
	"os/exec"
	"sync"
	"syscall"
	"time"
)

type XrayCore struct {
	mu             sync.Mutex
	cmd            *exec.Cmd
	cancel         context.CancelFunc
	running        bool
	channel_closed bool
	Exited         chan error
}

func (x *XrayCore) Start(stdinPipe []byte) error {
	x.mu.Lock()
	defer x.mu.Unlock()

	if x.running {
		return fmt.Errorf("command is already running")
	}

	xraybin, err := utils.GetXrayBin()
	if err != nil {
		return fmt.Errorf("failed to start xray %w", err)
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
		logger.Infof("xray stderr -> %s", xrayLog.Name())
	} else {
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
		logger.Infof("xray will run as uid=%d gid=%d", uid, gid)
	}

	if ad := geo.AssetDir(); ad != "" {
		geo.WaitReady(3 * time.Second)
		if geo.IsReady() {
			cmd.Env = append(os.Environ(), "XRAY_LOCATION_ASSET="+ad)
			logger.Infof("xray XRAY_LOCATION_ASSET=%s", ad)
		} else {
			logger.Warn("xray: geo assets not ready, skipping XRAY_LOCATION_ASSET")
		}
	}

	if err := cmd.Start(); err != nil {
		cancel()
		return err
	}

	logger.Infof("xray started (pid=%d)", cmd.Process.Pid)

	go func() {
		defer stdin.Close()
		_, _ = stdin.Write(stdinPipe)
	}()

	x.cmd = cmd
	x.cancel = cancel
	x.running = true

	go func() {
		err := cmd.Wait()
		logger.Infof("xray stopped (pid=%d)", cmd.Process.Pid)
		x.mu.Lock()
		defer x.mu.Unlock()
		if ctx.Err() == nil {
			select {
			case x.Exited <- err:
				if x.running {
					close(x.Exited)
					x.channel_closed = true
				}
				x.running = false
			default:
				// Channel is full or no reader, don't block
			}
		}

	}()

	return nil
}

func (x *XrayCore) Stop() {
	x.mu.Lock()
	defer x.mu.Unlock()
	if !x.channel_closed {
		close(x.Exited)
		x.channel_closed = true
	}
	if x.cmd != nil && x.cmd.Process != nil {
		err := x.cmd.Process.Kill()
		if err != nil {
			logger.Warn("error killing xray process:", err)
		}
	}
	if x.cancel != nil {
		x.cancel()
	}
	x.running = false
}

func (x *XrayCore) IsRunning() bool {
	x.mu.Lock()
	defer x.mu.Unlock()
	return x.running
}
