package main

import (
	"flag"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"time"
)

// shutdownGrace bounds how long tsnet teardown may take before we exit anyway.
// The whole point of exiting is to release the lock Windows holds on this
// executable so the installer can overwrite it, so a hung teardown must never
// keep us alive.
const shutdownGrace = 5 * time.Second

// stdinIsPipe reports whether stdin is a pipe, which is how the desktop app
// spawns us. A console — and, on Windows, a redirect from NUL — is a character
// device instead; watching one of those for EOF would make a hand-run sidecar
// exit immediately.
func stdinIsPipe(file *os.File) bool {
	info, err := file.Stat()
	if err != nil {
		return false
	}
	return info.Mode()&os.ModeCharDevice == 0
}

// watchParentStdin blocks until the parent closes our stdin, then calls stop.
//
// The parent holds the write end of this pipe for its entire life, so the read
// ends on *any* parent death — clean exit, crash, or an external kill. That
// makes it the only shutdown signal that survives the paths where the parent
// never gets to tell us anything, notably the updater's relaunch, which
// deliberately skips Tauri's exit events.
func watchParentStdin(reader io.Reader, stop func()) {
	_, _ = io.Copy(io.Discard, reader)
	stop()
}

// shutdown tears tsnet down so the node deregisters, then exits regardless of
// whether that finished within shutdownGrace.
func shutdown(runtime *Runtime) {
	done := make(chan struct{})
	go func() {
		runtime.Stop()
		close(done)
	}()
	select {
	case <-done:
	case <-time.After(shutdownGrace):
	}
	os.Exit(0)
}

func main() {
	controlAddr := flag.String("control-addr", "127.0.0.1:0", "localhost control listen address")
	flag.Parse()

	ln, err := net.Listen("tcp", *controlAddr)
	if err != nil {
		fmt.Fprintf(os.Stderr, "control listen failed: %v\n", err)
		os.Exit(1)
	}

	runtime := NewRuntime(NewTsnetNode())
	server := &http.Server{Handler: runtime.Handler()}

	if stdinIsPipe(os.Stdin) {
		go watchParentStdin(os.Stdin, func() { shutdown(runtime) })
	}

	fmt.Printf("CONTROL %s\n", ln.Addr().String())
	if err := server.Serve(ln); err != nil && err != http.ErrServerClosed {
		fmt.Fprintf(os.Stderr, "control server failed: %v\n", err)
		os.Exit(1)
	}
}
