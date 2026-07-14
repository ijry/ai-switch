package main

import (
        "context"
        "encoding/json"
        "errors"
        "fmt"
        "io"
        "net"
        "net/http"
        "net/http/httputil"
        "net/url"
        "strings"
        "sync"
        "time"
)

type Status struct {
        State        string   `json:"state"`
        DeviceName   *string  `json:"deviceName"`
        TailnetIP    *string  `json:"tailnetIp"`
        MagicDNSName *string  `json:"magicDnsName"`
        LoginURL     *string  `json:"loginUrl"`
        AccessURLs   []string `json:"accessUrls"`
        Serving      bool     `json:"serving"`
        Message      *string  `json:"message"`
}

type LoginResponse struct {
        LoginURL *string `json:"loginUrl"`
        Message  string  `json:"message"`
}

type StartRequest struct {
        StateDir    string  `json:"stateDir"`
        Hostname    string  `json:"hostname"`
        AuthKey     *string `json:"authKey"`
        BackendAddr string  `json:"backendAddr"`
        ServePort   uint16  `json:"servePort"`
}

type NodeInfo struct {
        DeviceName   string
        TailnetIP    string
        MagicDNSName string
        LoginURL     string
        Online       bool
}

type Node interface {
        Start(ctx context.Context, req StartRequest) (NodeInfo, error)
        LoginOAuth(ctx context.Context) (string, error)
        Stop(ctx context.Context) error
        Logout(ctx context.Context) error
        Status(ctx context.Context) (NodeInfo, error)
        Listen(ctx context.Context, network, addr string) (net.Listener, error)
}

type Runtime struct {
        mu          sync.Mutex
        node        Node
        status      Status
        backendAddr string
        servePort   uint16
        serveCancel context.CancelFunc
        serveLn     net.Listener
}

func NewRuntime(node Node) *Runtime {
        return &Runtime{
                node:   node,
                status: needsLoginStatus("Sign in to connect secure network"),
        }
}

func needsLoginStatus(message string) Status {
        msg := message
        return Status{
                State:      "needsLogin",
                AccessURLs: []string{},
                Serving:    false,
                Message:    &msg,
        }
}

func stoppedStatus(message string) Status {
        msg := message
        return Status{
                State:      "stopped",
                AccessURLs: []string{},
                Serving:    false,
                Message:    &msg,
        }
}

func errorStatus(message string) Status {
        msg := message
        return Status{
                State:      "error",
                AccessURLs: []string{},
                Serving:    false,
                Message:    &msg,
        }
}

func strPtr(v string) *string {
        if strings.TrimSpace(v) == "" {
                return nil
        }
        s := v
        return &s
}

func (r *Runtime) snapshot() Status {
        r.mu.Lock()
        defer r.mu.Unlock()
        out := r.status
        if out.AccessURLs == nil {
                out.AccessURLs = []string{}
        }
        return out
}

func (r *Runtime) setStatus(status Status) {
        if status.AccessURLs == nil {
                status.AccessURLs = []string{}
        }
        r.status = status
}

func (r *Runtime) applyNodeInfo(info NodeInfo, serving bool, message string) Status {
        status := Status{
                State:        "connected",
                DeviceName:   strPtr(info.DeviceName),
                TailnetIP:    strPtr(info.TailnetIP),
                MagicDNSName: strPtr(info.MagicDNSName),
                LoginURL:     strPtr(info.LoginURL),
                AccessURLs:   []string{},
                Serving:      serving,
                Message:      strPtr(message),
        }
        if !info.Online && info.LoginURL != "" {
                status.State = "needsLogin"
                status.Serving = false
                if message == "" {
                        status.Message = strPtr("Complete browser sign-in")
                }
        } else if !info.Online {
                status.State = "needsLogin"
                status.Serving = false
        }
        return status
}

func (r *Runtime) Start(req StartRequest) (Status, error) {
        r.mu.Lock()
        defer r.mu.Unlock()

        if strings.TrimSpace(req.StateDir) == "" {
                return errorStatus("stateDir is required"), errors.New("stateDir is required")
        }
        if strings.TrimSpace(req.Hostname) == "" {
                return errorStatus("hostname is required"), errors.New("hostname is required")
        }
        if strings.TrimSpace(req.BackendAddr) == "" {
                return errorStatus("backendAddr is required"), errors.New("backendAddr is required")
        }
        if req.ServePort == 0 {
                return errorStatus("servePort is required"), errors.New("servePort is required")
        }
        if !isLocalBackend(req.BackendAddr) {
                return errorStatus("backend must be localhost"), errors.New("backend must be localhost")
        }

        r.stopServingLocked()
        r.backendAddr = req.BackendAddr
        r.servePort = req.ServePort

        ctx, cancel := context.WithTimeout(context.Background(), 75*time.Second)
        defer cancel()

        info, err := r.node.Start(ctx, req)
        if err != nil {
                status := errorStatus(err.Error())
                r.setStatus(status)
                return status, err
        }

        status := r.applyNodeInfo(info, false, "")
        if status.State == "connected" {
                if err := r.startServingLocked(req.BackendAddr, req.ServePort); err != nil {
                        status = errorStatus(err.Error())
                        r.setStatus(status)
                        return status, err
                }
                status.Serving = true
        }
        r.setStatus(status)
        return status, nil
}

func (r *Runtime) LoginOAuth() (LoginResponse, error) {
        r.mu.Lock()
        defer r.mu.Unlock()

        ctx, cancel := context.WithTimeout(context.Background(), 75*time.Second)
        defer cancel()

        loginURL, err := r.node.LoginOAuth(ctx)
        if err != nil {
                status := errorStatus(err.Error())
                r.setStatus(status)
                return LoginResponse{Message: err.Error()}, err
        }
        if strings.TrimSpace(loginURL) == "" {
                err := errors.New("login URL is not ready yet; try again")
                status := errorStatus(err.Error())
                r.setStatus(status)
                return LoginResponse{Message: err.Error()}, err
        }

        status := needsLoginStatus("Complete browser sign-in")
        status.LoginURL = strPtr(loginURL)
        r.setStatus(status)
        return LoginResponse{
                LoginURL: strPtr(loginURL),
                Message:  "Open the secure network sign-in page",
        }, nil
}

func (r *Runtime) Stop() Status {
        r.mu.Lock()
        defer r.mu.Unlock()

        r.stopServingLocked()
        ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
        defer cancel()
        _ = r.node.Stop(ctx)
        status := stoppedStatus("Secure network stopped")
        r.setStatus(status)
        return status
}

func (r *Runtime) Logout() Status {
        r.mu.Lock()
        defer r.mu.Unlock()

        r.stopServingLocked()
        ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
        defer cancel()
        _ = r.node.Logout(ctx)
        status := needsLoginStatus("Signed out of secure network")
        r.setStatus(status)
        return status
}

func (r *Runtime) Status() Status {
        r.mu.Lock()
        defer r.mu.Unlock()

        ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
        defer cancel()
        info, err := r.node.Status(ctx)
        if err != nil {
                // Keep last known status when probe fails after start.
                out := r.status
                if out.AccessURLs == nil {
                        out.AccessURLs = []string{}
                }
                return out
        }
        if info.Online {
                serving := r.serveLn != nil
                if !serving && r.backendAddr != "" && r.servePort != 0 {
                        if err := r.startServingLocked(r.backendAddr, r.servePort); err == nil {
                                serving = true
                        }
                }
                status := r.applyNodeInfo(info, serving, "")
                r.setStatus(status)
                return status
        }
        if info.LoginURL != "" {
                status := needsLoginStatus("Complete browser sign-in")
                status.LoginURL = strPtr(info.LoginURL)
                r.setStatus(status)
                return status
        }
        out := r.status
        if out.AccessURLs == nil {
                out.AccessURLs = []string{}
        }
        return out
}

func (r *Runtime) startServingLocked(backendAddr string, servePort uint16) error {
        if r.serveLn != nil {
                return nil
        }
        ctx, cancel := context.WithCancel(context.Background())
        ln, err := r.node.Listen(ctx, "tcp", fmt.Sprintf(":%d", servePort))
        if err != nil {
                cancel()
                return err
        }
        handler, err := newReverseProxy(backendAddr)
        if err != nil {
                cancel()
                _ = ln.Close()
                return err
        }
        server := &http.Server{Handler: handler}
        go func() {
                _ = server.Serve(ln)
        }()
        go func() {
                <-ctx.Done()
                shutdownCtx, shutdownCancel := context.WithTimeout(context.Background(), 5*time.Second)
                defer shutdownCancel()
                _ = server.Shutdown(shutdownCtx)
        }()
        r.serveCancel = cancel
        r.serveLn = ln
        return nil
}

func (r *Runtime) stopServingLocked() {
        if r.serveCancel != nil {
                r.serveCancel()
                r.serveCancel = nil
        }
        if r.serveLn != nil {
                _ = r.serveLn.Close()
                r.serveLn = nil
        }
}

func isLocalBackend(addr string) bool {
        host := addr
        if strings.Contains(addr, "://") {
                u, err := url.Parse(addr)
                if err != nil {
                        return false
                }
                host = u.Host
        }
        if h, _, err := net.SplitHostPort(host); err == nil {
                host = h
        }
        host = strings.Trim(host, "[]")
        return host == "127.0.0.1" || host == "localhost" || host == "::1"
}

func newReverseProxy(backendAddr string) (http.Handler, error) {
        target := backendAddr
        if !strings.Contains(target, "://") {
                target = "http://" + target
        }
        u, err := url.Parse(target)
        if err != nil {
                return nil, err
        }
        if !isLocalBackend(u.Host) {
                return nil, errors.New("backend must be localhost")
        }
        proxy := httputil.NewSingleHostReverseProxy(u)
        originalDirector := proxy.Director
        proxy.Director = func(req *http.Request) {
                originalDirector(req)
                req.Host = u.Host
        }
        proxy.ErrorHandler = func(w http.ResponseWriter, _ *http.Request, err error) {
                http.Error(w, "upstream unavailable", http.StatusBadGateway)
                _ = err
        }
        return proxy, nil
}

func writeJSON(w http.ResponseWriter, statusCode int, payload any) {
        w.Header().Set("Content-Type", "application/json")
        w.WriteHeader(statusCode)
        _ = json.NewEncoder(w).Encode(payload)
}

func readJSON(r *http.Request, dst any) error {
        defer r.Body.Close()
        decoder := json.NewDecoder(io.LimitReader(r.Body, 1<<20))
        decoder.DisallowUnknownFields()
        return decoder.Decode(dst)
}

func (r *Runtime) Handler() http.Handler {
        mux := http.NewServeMux()
        mux.HandleFunc("/control/status", func(w http.ResponseWriter, req *http.Request) {
                if req.Method != http.MethodGet {
                        http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
                        return
                }
                writeJSON(w, http.StatusOK, r.Status())
        })
        mux.HandleFunc("/control/start", func(w http.ResponseWriter, req *http.Request) {
                if req.Method != http.MethodPost {
                        http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
                        return
                }
                var body StartRequest
                if err := readJSON(req, &body); err != nil {
                        writeJSON(w, http.StatusBadRequest, errorStatus("invalid start payload"))
                        return
                }
                status, err := r.Start(body)
                if err != nil {
                        writeJSON(w, http.StatusBadRequest, status)
                        return
                }
                writeJSON(w, http.StatusOK, status)
        })
        mux.HandleFunc("/control/login-oauth", func(w http.ResponseWriter, req *http.Request) {
                if req.Method != http.MethodPost {
                        http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
                        return
                }
                resp, err := r.LoginOAuth()
                if err != nil {
                        writeJSON(w, http.StatusBadRequest, resp)
                        return
                }
                writeJSON(w, http.StatusOK, resp)
        })
        mux.HandleFunc("/control/stop", func(w http.ResponseWriter, req *http.Request) {
                if req.Method != http.MethodPost {
                        http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
                        return
                }
                writeJSON(w, http.StatusOK, r.Stop())
        })
        mux.HandleFunc("/control/logout", func(w http.ResponseWriter, req *http.Request) {
                if req.Method != http.MethodPost {
                        http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
                        return
                }
                writeJSON(w, http.StatusOK, r.Logout())
        })
        return mux
}
