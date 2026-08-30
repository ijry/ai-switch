package main

import (
	"net"
	"testing"

	"tailscale.com/tsnet"
)

type listenerModeServer struct {
	mode string
}

func (s *listenerModeServer) Listen(network, addr string) (net.Listener, error) {
	s.mode = "http"
	return net.Listen("tcp", "127.0.0.1:0")
}

func (s *listenerModeServer) ListenTLS(network, addr string) (net.Listener, error) {
	s.mode = "tls"
	return net.Listen("tcp", "127.0.0.1:0")
}

func (s *listenerModeServer) ListenFunnel(network, addr string, _ ...tsnet.FunnelOption) (net.Listener, error) {
	s.mode = "funnel"
	return net.Listen("tcp", "127.0.0.1:0")
}

func TestListenOnTsnetUsesTLSForPrivateAndFunnelForPublic(t *testing.T) {
	for _, tc := range []struct {
		name   string
		public bool
		want   string
	}{
		{name: "private", public: false, want: "tls"},
		{name: "public", public: true, want: "funnel"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			server := &listenerModeServer{}
			listener, err := listenOnTsnet(server, "tcp", ":443", tc.public)
			if err != nil {
				t.Fatal(err)
			}
			_ = listener.Close()
			if server.mode != tc.want {
				t.Fatalf("listener mode=%q, want %q", server.mode, tc.want)
			}
		})
	}
}
