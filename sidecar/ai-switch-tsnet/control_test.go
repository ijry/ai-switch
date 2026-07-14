package main

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"testing"
	"time"
)

type fakeNode struct {
	started bool
	online  bool
	authURL string
	host    string
	ip      string
	dns     string
	ln      net.Listener
}

func (f *fakeNode) Start(ctx context.Context, req StartRequest) (NodeInfo, error) {
	_ = ctx
	f.started = true
	f.host = req.Hostname
	if req.AuthKey != nil && strings.TrimSpace(*req.AuthKey) != "" {
		f.online = true
		f.ip = "100.64.0.12"
		f.dns = req.Hostname + ".tailnet.ts.net"
		return NodeInfo{DeviceName: req.Hostname, TailnetIP: f.ip, MagicDNSName: f.dns, Online: true}, nil
	}
	f.online = false
	return NodeInfo{DeviceName: req.Hostname, Online: false}, nil
}

func (f *fakeNode) LoginOAuth(ctx context.Context) (string, error) {
	_ = ctx
	f.authURL = "https://login.tailscale.com/a/example"
	return f.authURL, nil
}

func (f *fakeNode) Stop(ctx context.Context) error {
	_ = ctx
	f.online = false
	if f.ln != nil {
		_ = f.ln.Close()
		f.ln = nil
	}
	return nil
}

func (f *fakeNode) Logout(ctx context.Context) error {
	return f.Stop(ctx)
}

func (f *fakeNode) Status(ctx context.Context) (NodeInfo, error) {
	_ = ctx
	return NodeInfo{
		DeviceName:   f.host,
		TailnetIP:    f.ip,
		MagicDNSName: f.dns,
		LoginURL:     f.authURL,
		Online:       f.online,
	}, nil
}

func (f *fakeNode) Listen(ctx context.Context, network, addr string) (net.Listener, error) {
	_ = ctx
	ln, err := net.Listen(network, "127.0.0.1:0")
	if err != nil {
		return nil, err
	}
	f.ln = ln
	return ln, nil
}

func startTestBackend(t *testing.T) (addr string, cleanup func()) {
	t.Helper()
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("listen backend: %v", err)
	}
	go func() {
		for {
			conn, err := ln.Accept()
			if err != nil {
				return
			}
			_ = conn.Close()
		}
	}()
	return ln.Addr().String(), func() { _ = ln.Close() }
}

func TestStatusDefaultsToNeedsLogin(t *testing.T) {
	rt := NewRuntime(&fakeNode{})
	srv := httptest.NewServer(rt.Handler())
	defer srv.Close()

	res, err := http.Get(srv.URL + "/control/status")
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	if res.StatusCode != http.StatusOK {
		t.Fatalf("status code = %d", res.StatusCode)
	}
	var status Status
	if err := json.NewDecoder(res.Body).Decode(&status); err != nil {
		t.Fatal(err)
	}
	if status.State != "needsLogin" {
		t.Fatalf("state = %q", status.State)
	}
}

func TestStartWithAuthKeyMarksConnected(t *testing.T) {
	backend, cleanup := startTestBackend(t)
	defer cleanup()
	host, portStr, err := net.SplitHostPort(backend)
	if err != nil {
		t.Fatal(err)
	}
	port, err := strconv.Atoi(portStr)
	if err != nil {
		t.Fatal(err)
	}
	_ = host
	node := &fakeNode{}
	rt := NewRuntime(node)
	srv := httptest.NewServer(rt.Handler())
	defer srv.Close()

	body := fmt.Sprintf(`{"stateDir":"C:/tmp/ts","hostname":"ai-switch","authKey":"tskey-auth-test","backendAddr":"%s","servePort":%d}`, backend, port)
	res, err := http.Post(srv.URL+"/control/start", "application/json", strings.NewReader(body))
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	raw, _ := io.ReadAll(res.Body)
	if res.StatusCode != http.StatusOK {
		t.Fatalf("status=%d body=%s", res.StatusCode, string(raw))
	}
	var status Status
	if err := json.Unmarshal(raw, &status); err != nil {
		t.Fatal(err)
	}
	if status.State != "connected" {
		t.Fatalf("state=%q body=%s", status.State, string(raw))
	}
	if !status.Serving {
		t.Fatalf("expected serving true")
	}
	if status.TailnetIP == nil || *status.TailnetIP != "100.64.0.12" {
		t.Fatalf("tailnet ip missing")
	}
}

func TestLoginOAuthReturnsURL(t *testing.T) {
	rt := NewRuntime(&fakeNode{})
	srv := httptest.NewServer(rt.Handler())
	defer srv.Close()

	res, err := http.Post(srv.URL+"/control/login-oauth", "application/json", strings.NewReader("{}"))
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	var login LoginResponse
	if err := json.NewDecoder(res.Body).Decode(&login); err != nil {
		t.Fatal(err)
	}
	if login.LoginURL == nil || !strings.HasPrefix(*login.LoginURL, "https://") {
		t.Fatalf("login url = %#v", login.LoginURL)
	}
}

func TestRejectsNonLocalBackend(t *testing.T) {
	rt := NewRuntime(&fakeNode{})
	body := StartRequest{
		StateDir:    "C:/tmp/ts",
		Hostname:    "ai-switch",
		BackendAddr: "8.8.8.8:3090",
		ServePort:   3090,
	}
	status, err := rt.Start(body)
	if err == nil {
		t.Fatal("expected error")
	}
	if status.State != "error" {
		t.Fatalf("state=%q", status.State)
	}
}

func TestStopReturnsStopped(t *testing.T) {
	node := &fakeNode{online: true, host: "ai-switch"}
	rt := NewRuntime(node)
	status := rt.Stop()
	if status.State != "stopped" {
		t.Fatalf("state=%q", status.State)
	}
	time.Sleep(10 * time.Millisecond)
}

func TestRebindWithoutAuthKeyKeepsConnected(t *testing.T) {
	backendA, cleanupA := startTestBackend(t)
	defer cleanupA()
	backendB, cleanupB := startTestBackend(t)
	defer cleanupB()
	_, portAStr, err := net.SplitHostPort(backendA)
	if err != nil {
		t.Fatal(err)
	}
	portA, err := strconv.Atoi(portAStr)
	if err != nil {
		t.Fatal(err)
	}
	_, portBStr, err := net.SplitHostPort(backendB)
	if err != nil {
		t.Fatal(err)
	}
	portB, err := strconv.Atoi(portBStr)
	if err != nil {
		t.Fatal(err)
	}

	node := &fakeNode{}
	rt := NewRuntime(node)

	auth := "tskey-auth-test"
	first, err := rt.Start(StartRequest{
		StateDir:    "C:/tmp/ts",
		Hostname:    "ai-switch",
		AuthKey:     &auth,
		BackendAddr: backendA,
		ServePort:   uint16(portA),
	})
	if err != nil {
		t.Fatal(err)
	}
	if first.State != "connected" || !first.Serving {
		t.Fatalf("first start state=%q serving=%v", first.State, first.Serving)
	}

	rebound, err := rt.Start(StartRequest{
		StateDir:    "C:/tmp/ts",
		Hostname:    "ai-switch",
		BackendAddr: backendB,
		ServePort:   uint16(portB),
	})
	if err != nil {
		t.Fatal(err)
	}
	if rebound.State != "connected" {
		t.Fatalf("rebind demoted state=%q", rebound.State)
	}
	if !rebound.Serving {
		t.Fatalf("expected serving after rebind")
	}
	if rt.backendAddr != backendB || rt.servePort != uint16(portB) {
		t.Fatalf("backend not updated: %s %d", rt.backendAddr, rt.servePort)
	}
	if rt.activeBackend != backendB || rt.activeServePort != uint16(portB) {
		t.Fatalf("active backend not updated: %s %d", rt.activeBackend, rt.activeServePort)
	}
}
