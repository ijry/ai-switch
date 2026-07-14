# ai-switch-tsnet

Built-in secure network helper for AI Switch.

## Role

- Authenticate with Tailscale using OAuth or auth key
- Serve on the private network
- Reverse-proxy remote requests to the local AI Switch web service on `127.0.0.1`

## Control API (localhost only)

Printed on startup:

```text
CONTROL 127.0.0.1:<port>
```

Endpoints:

- `POST /control/start`
- `POST /control/login-oauth`
- `POST /control/stop`
- `POST /control/logout`
- `GET /control/status`

## Build

```powershell
go test ./...
go build -o ai-switch-tsnet.exe .
```

## Runtime flags

- `--control-addr` default `127.0.0.1:0`
