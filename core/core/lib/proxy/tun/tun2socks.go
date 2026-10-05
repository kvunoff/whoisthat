package tunmode

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"sync"
	"syscall"
	"whoisthat-core/lib/logger"
	"whoisthat-core/lib/tunmgr"

	"golang.org/x/sys/unix"
)

type Tun2Socks struct {
	mu             sync.Mutex
	cmd            *exec.Cmd
	cancel         context.CancelFunc
	running        bool
	channel_closed bool
	Exited         chan error
}

func (n *Tun2Socks) Start(tun_name string, port int) error {
	n.mu.Lock()
	defer n.mu.Unlock()

	if n.running {
		return fmt.Errorf("command is already running")
	}

	if n.Exited == nil || n.channel_closed {
		n.Exited = make(chan error, 1)
		n.channel_closed = false
	}

	tun2socksbin, err := tunmgr.GetManager().GetTun2socksBin()
	if err != nil {
		return fmt.Errorf("failed to start tun: %w", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	socks_proxy := fmt.Sprintf("socks5://127.0.0.1:%d", port)
	cmd := exec.CommandContext(ctx, tun2socksbin, "--device", tun_name, "--proxy", socks_proxy)
	cmd.SysProcAttr = &syscall.SysProcAttr{
		AmbientCaps: []uintptr{unix.CAP_NET_ADMIN},
	}

	cmd.Stdout = nil
	t2sLog, err := os.CreateTemp("", "whoisthat-tun2socks-*.log")
	if err == nil {
		cmd.Stderr = t2sLog
		logger.Infof("tun2socks stderr -> %s", t2sLog.Name())
	} else {
		t2sLog = nil
		cmd.Stderr = nil
	}

	if err := cmd.Start(); err != nil {
		cancel()
		if t2sLog != nil {
			_ = t2sLog.Close()
			_ = os.Remove(t2sLog.Name())
		}
		return err
	}

	n.cmd = cmd
	n.cancel = cancel
	n.running = true

	go func() {
		err := cmd.Wait()
		if t2sLog != nil {
			_ = t2sLog.Close()
			if err == nil && ctx.Err() == nil {
				_ = os.Remove(t2sLog.Name())
			} else {
				logger.Warnf("tun2socks exited; stderr retained at %s (err=%v)", t2sLog.Name(), err)
			}
		}
		n.mu.Lock()
		defer n.mu.Unlock()
		if ctx.Err() == nil {
			n.running = false
			if !n.channel_closed {
				select {
				case n.Exited <- err:
				default:
				}
				close(n.Exited)
				n.channel_closed = true
			}
		}

	}()

	return nil
}

func (n *Tun2Socks) Stop() {
	n.mu.Lock()
	defer n.mu.Unlock()
	if !n.channel_closed {
		close(n.Exited)
		n.channel_closed = true
	}
	if n.cmd != nil && n.cmd.Process != nil {
		err := n.cmd.Process.Kill()
		if err != nil {
			logger.Warn("error killing tun2socks process:", err)
		}
	}
	if n.cancel != nil {
		n.cancel()
	}
	n.running = false
}

func (n *Tun2Socks) IsRunning() bool {
	n.mu.Lock()
	defer n.mu.Unlock()
	return n.running
}
