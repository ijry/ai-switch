package main

import (
        "flag"
        "fmt"
        "net"
        "net/http"
        "os"
)

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

        fmt.Printf("CONTROL %s\n", ln.Addr().String())
        if err := server.Serve(ln); err != nil && err != http.ErrServerClosed {
                fmt.Fprintf(os.Stderr, "control server failed: %v\n", err)
                os.Exit(1)
        }
}
