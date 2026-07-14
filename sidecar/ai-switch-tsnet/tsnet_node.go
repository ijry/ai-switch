package main

import (
        "context"
        "errors"
        "net"
        "strings"
        "sync"

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
        status, err := srv.Up(ctx)
        if err != nil {
                // Interactive auth may still be pending; surface needsLogin instead of hard fail.
                lower := strings.ToLower(err.Error())
                if strings.Contains(lower, "login") || strings.Contains(lower, "auth") || strings.Contains(lower, "needs login") {
                        info := NodeInfo{
                                DeviceName: req.Hostname,
                                Online:     false,
                        }
                        if authURL := n.authURL(ctx); authURL != "" {
                                info.LoginURL = authURL
                        }
                        return info, nil
                }
                return NodeInfo{}, err
        }
        return nodeInfoFromIPN(req.Hostname, status), nil
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

        _, err := srv.Up(ctx)
        if authURL := n.authURL(ctx); authURL != "" {
                return authURL, nil
        }
        if err != nil {
                // Browser login page is still the product-facing outcome.
                return "https://login.tailscale.com", nil
        }
        return "https://login.tailscale.com", nil
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

func (n *TsnetNode) authURL(ctx context.Context) string {
        n.mu.Lock()
        srv := n.server
        n.mu.Unlock()
        if srv == nil {
                return ""
        }
        lc, err := srv.LocalClient()
        if err != nil {
                return ""
        }
        st, err := lc.StatusWithoutPeers(ctx)
        if err != nil || st == nil {
                return ""
        }
        return st.AuthURL
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
