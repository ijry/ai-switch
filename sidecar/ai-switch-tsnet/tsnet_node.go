package main

import (
	"context"
	"errors"
	"net"
	"strings"
	"sync"
	"time"

	"tailscale.com/ipn/ipnstate"
	"tailscale.com/tsnet"
)

type TsnetNode struct {
	mu      sync.Mutex
	server  *tsnet.Server
	request StartRequest
}

func NewTsnetNode() *TsnetNode {
	return &TsnetNode{}
}

func (n *TsnetNode) ensureServer(req StartRequest) *tsnet.Server {
	n.mu.Lock()
	defer n.mu.Unlock()

	n.request = req
	if n.server != nil {
		if req.AuthKey != nil {
			n.server.AuthKey = strings.TrimSpace(*req.AuthKey)
		}
		if req.Hostname != "" {
			n.server.Hostname = req.Hostname
		}
		if req.StateDir != "" {
			n.server.Dir = req.StateDir
		}
		return n.server
	}

	auth := ""
	if req.AuthKey != nil {
		auth = strings.TrimSpace(*req.AuthKey)
	}
	n.server = &tsnet.Server{
		Dir:      req.StateDir,
		Hostname: req.Hostname,
		AuthKey:  auth,
	}
	return n.server
}

func (n *TsnetNode) Start(ctx context.Context, req StartRequest) (NodeInfo, error) {
	srv := n.ensureServer(req)
	authKey := ""
	if req.AuthKey != nil {
		authKey = strings.TrimSpace(*req.AuthKey)
	}

	// Auth-key path can wait until the node is running.
	if authKey != "" {
		status, err := srv.Up(ctx)
		if err != nil {
			return NodeInfo{}, err
		}
		return nodeInfoFromIPN(req.Hostname, status), nil
	}

	// Non-auth-key start must not force interactive login. Reuse stored credentials when present,
	// otherwise return needsLogin so the UI can call LoginOAuth explicitly.
	if err := srv.Start(); err != nil {
		return NodeInfo{}, err
	}
	if info, ok := n.waitForAuthOrOnline(ctx, req.Hostname, 8*time.Second); ok {
		return info, nil
	}
	return NodeInfo{DeviceName: req.Hostname, Online: false}, nil
}

func (n *TsnetNode) LoginOAuth(ctx context.Context) (string, error) {
	n.mu.Lock()
	req := n.request
	srv := n.server
	n.mu.Unlock()

	if strings.TrimSpace(req.StateDir) == "" {
		return "", errors.New("secure network is not started")
	}
	if srv == nil {
		emptyKey := ""
		req.AuthKey = &emptyKey
		srv = n.ensureServer(req)
	}
	srv.AuthKey = ""

	if err := srv.Start(); err != nil {
		return "", err
	}
	lc, err := srv.LocalClient()
	if err != nil {
		return "", err
	}
	if err := lc.StartLoginInteractive(ctx); err != nil {
		// Continue polling; some states already have a pending AuthURL.
		_ = err
	}
	if info, ok := n.waitForAuthOrOnline(ctx, req.Hostname, 60*time.Second); ok {
		if info.Online {
			return "", errors.New("secure network is already connected")
		}
		if strings.TrimSpace(info.LoginURL) != "" {
			return info.LoginURL, nil
		}
	}
	return "", errors.New("login URL is not ready yet; try again")
}

func (n *TsnetNode) Stop(ctx context.Context) error {
	_ = ctx
	n.mu.Lock()
	defer n.mu.Unlock()
	if n.server == nil {
		return nil
	}
	err := n.server.Close()
	n.server = nil
	return err
}

func (n *TsnetNode) Logout(ctx context.Context) error {
	n.mu.Lock()
	srv := n.server
	n.mu.Unlock()
	if srv != nil {
		if lc, err := srv.LocalClient(); err == nil {
			_ = lc.Logout(ctx)
		}
	}
	return n.Stop(ctx)
}

func (n *TsnetNode) Status(ctx context.Context) (NodeInfo, error) {
	n.mu.Lock()
	srv := n.server
	hostname := n.request.Hostname
	n.mu.Unlock()
	if srv == nil {
		return NodeInfo{DeviceName: hostname, Online: false}, nil
	}
	lc, err := srv.LocalClient()
	if err != nil {
		return NodeInfo{DeviceName: hostname, Online: false}, nil
	}
	st, err := lc.StatusWithoutPeers(ctx)
	if err != nil {
		return NodeInfo{DeviceName: hostname, Online: false}, nil
	}
	return nodeInfoFromIPN(hostname, st), nil
}

func (n *TsnetNode) Listen(ctx context.Context, network, addr string) (net.Listener, error) {
	n.mu.Lock()
	srv := n.server
	n.mu.Unlock()
	if srv == nil {
		return nil, errors.New("secure network is not started")
	}
	ln, err := srv.Listen(network, addr)
	if err != nil {
		return nil, err
	}
	go func() {
		<-ctx.Done()
		_ = ln.Close()
	}()
	return ln, nil
}

func (n *TsnetNode) waitForAuthOrOnline(ctx context.Context, hostname string, timeout time.Duration) (NodeInfo, bool) {
	deadline := time.Now().Add(timeout)
	for {
		info, err := n.Status(ctx)
		if err == nil {
			if info.DeviceName == "" {
				info.DeviceName = hostname
			}
			if info.Online || strings.TrimSpace(info.LoginURL) != "" {
				return info, true
			}
		}
		if !time.Now().Before(deadline) {
			return NodeInfo{DeviceName: hostname, Online: false}, false
		}
		select {
		case <-ctx.Done():
			return NodeInfo{DeviceName: hostname, Online: false}, false
		case <-time.After(250 * time.Millisecond):
		}
	}
}

func nodeInfoFromIPN(hostname string, st *ipnstate.Status) NodeInfo {
	info := NodeInfo{
		DeviceName: hostname,
		Online:     false,
	}
	if st == nil {
		return info
	}
	info.Online = st.BackendState == "Running"
	info.LoginURL = st.AuthURL
	if st.Self != nil {
		if st.Self.HostName != "" {
			info.DeviceName = st.Self.HostName
		}
		if st.Self.DNSName != "" {
			info.MagicDNSName = strings.TrimSuffix(st.Self.DNSName, ".")
		}
		for _, ip := range st.Self.TailscaleIPs {
			if ip.Is4() {
				info.TailnetIP = ip.String()
				break
			}
		}
		if info.TailnetIP == "" && len(st.Self.TailscaleIPs) > 0 {
			info.TailnetIP = st.Self.TailscaleIPs[0].String()
		}
	}
	return info
}
