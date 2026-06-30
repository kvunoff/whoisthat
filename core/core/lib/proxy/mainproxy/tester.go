package mainproxy

import (
	"fmt"
	"net"
	"net/http"
	"regexp"
	"strconv"
	"time"
	"whoisthat-core/lib"
	"whoisthat-core/lib/proxy/xray"
	"whoisthat-core/structs"

	goproxy "golang.org/x/net/proxy"
)

type TestResult struct {
	Success bool
	Profile structs.Profile
}

type TestRequest struct {
	Profile structs.Profile
	Method  string
}

var rePort = regexp.MustCompile(`@[\w.\-\[\]:]+:(\d+)`)

func (p *ProxyManager) TestProfile(profile structs.Profile, method string) {
	p.testChannel <- TestRequest{Profile: profile, Method: method}
}

func (p *ProxyManager) listenForTests(tests_chan chan TestRequest) {
	sem := make(chan struct{}, 5)

	for req := range tests_chan {
		sem <- struct{}{}
		go func(req TestRequest) {
			ping := p.test(req)
			p.sendTestResult(req.Profile, ping)
			<-sem
		}(req)
	}
}

func (p *ProxyManager) test(req TestRequest) int {
	switch req.Method {
	case "tcp":
		return p.testTcp(req.Profile)
	case "http-head":
		return p.testHttp(req.Profile, "HEAD")
	default:
		return p.testHttp(req.Profile, "GET")
	}
}

func (p *ProxyManager) testTcp(profile structs.Profile) int {
	port := extractPort(profile.Uri, profile.Protocol)
	addr := profile.Address
	if addr == "" {
		addr = profile.Host
	}
	if addr == "" {
		return -1
	}
	target := net.JoinHostPort(addr, port)
	start := time.Now()
	conn, err := net.DialTimeout("tcp", target, 5*time.Second)
	if err != nil {
		return -1
	}
	conn.Close()
	return int(time.Since(start).Milliseconds())
}

func (p *ProxyManager) testHttp(profile structs.Profile, method string) int {
	port, err := p.portPool.GetPort()
	if err != nil {
		return -1
	}
	parsed, err := lib.ParseUri(profile.Uri, port, -1)
	if err != nil {
		return -1
	}

	xray_core := xray.XrayCore{
		Exited: make(chan error),
	}

	xray_core.Start(parsed)
	defer xray_core.Stop()
	time.Sleep(1 * time.Second)

	dialer, err := goproxy.SOCKS5("tcp", fmt.Sprintf("127.0.0.1:%d", port), nil, goproxy.Direct)
	if err != nil {
		return -1
	}

	transport := &http.Transport{
		Dial: dialer.Dial,
	}

	client := &http.Client{
		Transport: transport,
		Timeout:   5 * time.Second,
	}
	start_time := time.Now()

	var req *http.Request
	if method == "HEAD" {
		req, err = http.NewRequest("HEAD", "https://cp.cloudflare.com", nil)
	} else {
		req, err = http.NewRequest("GET", "https://cp.cloudflare.com", nil)
	}
	if err != nil {
		return -1
	}

	resp, err := client.Do(req)
	ping := time.Since(start_time)

	if err != nil {
		return -1
	}
	resp.Body.Close()

	return int(ping.Milliseconds())
}

func (p *ProxyManager) sendTestResult(profile structs.Profile, ping int) {
	profile.TestResult = ping
	p.TestResultChannel <- TestResult{
		Profile: profile,
	}
}

func extractPort(uri string, protocol string) string {
	m := rePort.FindStringSubmatch(uri)
	if len(m) == 2 {
		if _, err := strconv.Atoi(m[1]); err == nil {
			return m[1]
		}
	}
	switch protocol {
	case "vless", "vmess", "trojan":
		return "443"
	case "shadowsocks":
		return "8388"
	case "socks":
		return "1080"
	}
	return "443"
}
