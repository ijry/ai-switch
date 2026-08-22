package main

import (
	"io"
	"os"
	"testing"
	"time"
)

func TestWatchParentStdinStopsWhenWriteEndCloses(t *testing.T) {
	reader, writer := io.Pipe()
	stopped := make(chan struct{})

	go watchParentStdin(reader, func() { close(stopped) })

	// Nothing should fire while the parent still holds the write end.
	select {
	case <-stopped:
		t.Fatal("stop ran while the pipe was still open")
	case <-time.After(50 * time.Millisecond):
	}

	// Closing the write end is what a dying parent does to our stdin.
	if err := writer.Close(); err != nil {
		t.Fatalf("close write end: %v", err)
	}

	select {
	case <-stopped:
	case <-time.After(2 * time.Second):
		t.Fatal("stop did not run after the write end closed")
	}
}

func TestWatchParentStdinIgnoresDataUntilEOF(t *testing.T) {
	reader, writer := io.Pipe()
	stopped := make(chan struct{})

	go watchParentStdin(reader, func() { close(stopped) })

	go func() {
		_, _ = writer.Write([]byte("noise the parent may send\n"))
	}()

	select {
	case <-stopped:
		t.Fatal("stop ran on written data instead of EOF")
	case <-time.After(50 * time.Millisecond):
	}

	_ = writer.Close()
	select {
	case <-stopped:
	case <-time.After(2 * time.Second):
		t.Fatal("stop did not run after EOF")
	}
}

func TestStdinIsPipeDistinguishesPipeFromCharDevice(t *testing.T) {
	reader, writer, err := os.Pipe()
	if err != nil {
		t.Fatalf("os.Pipe: %v", err)
	}
	defer reader.Close()
	defer writer.Close()

	if !stdinIsPipe(reader) {
		t.Error("a pipe must be watched: that is how the desktop app spawns us")
	}

	// A console or a NUL/devnull redirect is a character device. Watching one
	// would make a hand-run sidecar exit immediately, so it must be skipped.
	devNull, err := os.Open(os.DevNull)
	if err != nil {
		t.Fatalf("open %s: %v", os.DevNull, err)
	}
	defer devNull.Close()

	if stdinIsPipe(devNull) {
		t.Errorf("%s must not be watched for EOF", os.DevNull)
	}
}
