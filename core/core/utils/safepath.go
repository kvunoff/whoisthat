package utils

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
)

// SafePath is the only PATH the core and everything it spawns may use: root-
// owned system directories. The caller's PATH (user bin dirs, a planted
// directory, the cwd) never decides which nft/ip/xray runs with net_admin.
const SafePath = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"

var safeBinCache sync.Map // name -> absolute path, resolved once

// SafeLookPath resolves name against SafePath only and caches the result, so
// the binary is fixed for the life of the process.
func SafeLookPath(name string) (string, error) {
	if v, ok := safeBinCache.Load(name); ok {
		return v.(string), nil
	}
	if strings.ContainsRune(name, '/') {
		return "", fmt.Errorf("SafeLookPath: %q must be a bare command name", name)
	}
	for _, dir := range strings.Split(SafePath, ":") {
		p := filepath.Join(dir, name)
		if info, err := os.Stat(p); err == nil && !info.IsDir() && info.Mode()&0o111 != 0 {
			safeBinCache.Store(name, p)
			return p, nil
		}
	}
	return "", &exec.Error{Name: name, Err: exec.ErrNotFound}
}

// SafeEnv returns env with PATH replaced by SafePath.
func SafeEnv(env []string) []string {
	out := make([]string, 0, len(env)+1)
	for _, e := range env {
		if !strings.HasPrefix(e, "PATH=") {
			out = append(out, e)
		}
	}
	return append(out, "PATH="+SafePath)
}

// SafeCommand is exec.Command with an absolute, pinned binary and PATH. When
// the binary is not found the returned Cmd fails on Run/Output like any
// missing command would.
func SafeCommand(name string, args ...string) *exec.Cmd {
	bin, err := SafeLookPath(name)
	if err != nil {
		cmd := exec.Command("/nonexistent/" + name)
		cmd.Args = append([]string{name}, args...)
		return cmd
	}
	cmd := exec.Command(bin, args...)
	cmd.Env = SafeEnv(os.Environ())
	return cmd
}
