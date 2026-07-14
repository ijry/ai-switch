# AI Switch Built-in Tailscale (libtailscale/tsnet Sidecar) Design

## Context

AI Switch already has:

- a shared Rust core
- desktop Tauri runtime
- local Axum web service + web transport
- settings UI for web service host/port/token and a Tailscale section

The current Tailscale integration shells out to the system `tailscale` CLI. That is not a built-in node, depends on a host install, and does not match the product goal of private remote access from phone/other devices through an app-managed secure network.

This design upgrades Tailscale to an app-owned embedded node using official Tailscale building blocks (`tsnet` / `libtailscale`) behind a Go sidecar, while keeping the existing local Axum web service as the business backend.

## Goals

- Provide a true built-in Tailscale node for AI Switch desktop.
- Expose the existing web service over the tailnet through the embedded node.
- Keep localhost access working at `http://127.0.0.1:<port>`.
- Support both browser OAuth login and auth key login.
- Keep system Tailscale fully isolated; do not reuse or control it.
- Preserve existing web token authentication on every remote request.
- Leave a clean path to later in-process `libtailscale` FFI without changing UI APIs.

## Non-Goals

- No pure-Rust reimplementation of Tailscale.
- No first-version in-process FFI hard-link of `libtailscale` into the Tauri binary.
- No reuse of the system Tailscale client/state.
- No default public bind to `0.0.0.0`.
- No multi-user RBAC or shared multi-tenant auth.
- No automatic silent login on app startup.
- No replacement of the current Axum web service.

## Decisions

| Topic | Decision |
|---|---|
| Exposure model | Embedded node directly serves remote access |
| Auth | OAuth + Auth Key |
| Coexistence | Built-in node only, independent state dir |
| Integration style | Go sidecar managed by Rust (Approach B) |
| Local web bind | Keep `127.0.0.1` |
| System Tailscale fallback | None |

## Architecture

```text
React settings UI
  -> Tauri / Web command
  -> TailscaleService (Rust facade)
  -> ai-switch-tsnet sidecar (Go/tsnet)
       |- OAuth / Auth Key login
       |- independent state dir
       |- tailnet listener
       `- reverse proxy to local web

Local web service (existing Axum)
  bind: 127.0.0.1:port
  auth: Bearer token unchanged
```

### Component responsibilities

1. **Rust `TailscaleService`**
   - Single facade used by desktop commands and web handlers.
   - Owns sidecar process lifecycle.
   - Aggregates status for the UI.
   - Never shells out to system `tailscale`.

2. **Sidecar `ai-switch-tsnet`**
   - Built from official Tailscale `tsnet` / `libtailscale` capability.
   - Runs as a sibling process next to the desktop app.
   - Handles node identity, auth, tailnet serve, and reverse proxy.
   - Crash-isolated from the main desktop process.

3. **Local Axum web service**
   - Continues to own business APIs, static assets, websocket events, and token checks.
   - Remains bound to localhost.
   - Is the only backend the sidecar may proxy to.

4. **State directory**
   - `~/.ai-switch/tailscale/`
   - Completely separate from system Tailscale state.
   - Stores node state; auth key material is not written into ordinary JSON config or logs.

### Access paths

- Local: `http://127.0.0.1:<port>`
- Remote tailnet: `http://100.x.x.x:<port>` and/or MagicDNS name when available
- Both paths require the AI Switch access token

## Login Flow And State Machine

### States

- `disabled`: feature switch off
- `stopped`: enabled, sidecar not running
- `starting`: sidecar launching
- `needsLogin`: node present but unauthorized
- `connecting`: authorized and coming online
- `connected`: on tailnet; remote URLs available when web is running
- `error`: recoverable failure with user-facing message

### OAuth flow

1. User clicks "Login with Tailscale".
2. Rust starts sidecar if needed and requests OAuth login.
3. Sidecar returns `loginUrl`.
4. Desktop opens the browser.
5. Frontend polls `get_tailscale_status` until `connected`, `needsLogin`, or `error`.

### Auth key flow

1. User pastes auth key in settings.
2. Frontend calls `start_tailscale_with_auth_key`.
3. Rust passes the key to sidecar over local control channel only.
4. UI clears the visible key and only shows saved/unsaved state.
5. On success: `connecting -> connected`.
6. On invalid key: remain unconnected with an error message.

### Disconnect

- `disconnect_tailscale`:
  - stop serve/proxy
  - stop sidecar
  - keep state dir for faster reconnect by default
- Optional stronger "sign out" can clear node auth later; not required for P0 if disconnect is explicit and reliable.

### Auto behavior

- No silent login at app start.
- Start sidecar only when:
  - `tailscaleEnabled = true`
  - web service is running
  - and either saved auth exists or the user explicitly logs in / submits an auth key
- When web service stops, stop sidecar as well so the node does not linger unused.

### Status payload

```ts
{
  state: "connected",
  deviceName?: string,
  tailnetIp?: string,
  magicDnsName?: string,
  loginUrl?: string,
  accessUrls?: string[],
  message?: string
}
```

## Sidecar Protocol And Exposure

### Binary and discovery

- Binary name: `ai-switch-tsnet` / `ai-switch-tsnet.exe`
- Packaged next to the desktop executable
- Lookup order:
  1. same directory as `ai-switch.exe`
  2. dev path or `AI_SWITCH_TSNET_PATH`
  3. otherwise `error` with "built-in network component missing"
- Do not fall back to system CLI

### Control channel

Use a localhost-only control server owned by the sidecar.

Suggested endpoints:

```text
POST /control/start
{
  "stateDir": ".../.ai-switch/tailscale",
  "hostname": "ai-switch-<device>",
  "authKey": "tskey-...",
  "backendAddr": "127.0.0.1:10086",
  "servePort": 10086
}

POST /control/login-oauth
-> { "loginUrl": "https://login.tailscale.com/..." }

POST /control/stop
POST /control/logout

GET /control/status
-> {
  "state": "connected",
  "deviceName": "...",
  "tailnetIp": "100.x.x.x",
  "magicDnsName": "...",
  "serving": true,
  "message": null
}
```

### Data path

```text
Phone / remote machine
  --tailnet--> sidecar servePort
                 --localhost--> axum 127.0.0.1:port
```

Rules:

- Web config `port` is used for both local Axum bind and sidecar serve port.
- Sidecar does not implement business auth.
- Sidecar may only reverse-proxy to the configured localhost backend.
- Sidecar control port is never exposed on the tailnet.

### Lifecycle

1. User enables built-in secure network and starts web service.
2. Rust verifies local web is running.
3. Rust starts sidecar with `backendAddr=127.0.0.1:port`.
4. If node already authorized: go to `connected` and serve.
5. Else enter `needsLogin` until OAuth or auth key succeeds.
6. On web stop / disconnect / app exit: Rust stops serve and reaps sidecar.

## Configuration And Persistence

Extend `web-service.json` without breaking old configs:

```json
{
  "host": "127.0.0.1",
  "port": 10086,
  "token": "...",
  "autoStart": false,
  "tailscaleEnabled": true,
  "tailscaleHostname": null,
  "tailscaleAuthKeyPresent": false
}
```

Auth key storage:

- Do not store the raw auth key in `web-service.json`.
- Store it in a restricted local file under `~/.ai-switch/tailscale/` or OS keychain if low-cost.
- Frontend only knows whether a key is present.

`AppPaths` gains a `tailscale_dir`.

## Settings UI

Stay inside the existing web service settings page.

### Web service block

- host / port / token / auto start
- start / stop
- local URL display

### Secure network block

- enable switch
- status badge
- device name / tailnet IP / MagicDNS
- remote access URL list with copy action
- actions:
  - Login with Tailscale
  - Connect with auth key
  - Disconnect
  - Refresh
- auth key field is password-style and clears after submit

Copy rules:

- Prefer "Secure network", "Remote access", "Auth key"
- Do not show developer words such as sidecar, tsnet, FFI, daemon

## API Surface

Keep existing commands and extend:

- `get_tailscale_status`
- `start_tailscale_login` (OAuth)
- `start_tailscale_with_auth_key` (new)
- `disconnect_tailscale`
- optional later: `clear_tailscale_auth`

`get_web_server_status` may also include:

- `localUrl`
- `remoteUrls[]`

Desktop Tauri commands and web handlers must expose the same surface through the shared facade.

## Security

- Localhost default for Axum remains mandatory.
- Remote reachability is only through the embedded tailnet node.
- AI Switch bearer token is still required over Tailscale.
- Control channel is localhost-only.
- Auth keys never appear in ordinary logs or UI after submit.
- No silent network login on startup.
- System Tailscale is neither read nor modified.

## Testing

### Rust

- Fake sidecar client for protocol/state transitions
- Missing binary
- Start failure
- OAuth timeout
- Invalid auth key
- Stop/reap on web stop and disconnect

### Sidecar smoke

- Control API start/status/stop
- Reverse proxy to a mock localhost backend
- Serve only after authorized

### Frontend

- Enable switch
- OAuth button opens login URL
- Auth key submit path
- Connected remote URL rendering
- Prompt when secure network is connected but web service is stopped

### Manual acceptance

- Desktop app opens without depending on `127.0.0.1:1420`
- Start web service locally
- Login with OAuth or auth key
- Same-tailnet device can open remote URL
- Request without token returns 401
- Disconnect makes remote URL unreachable
- Presence/absence of system Tailscale does not change built-in behavior

## Delivery Phases

### P0

- Replace CLI-based `TailscaleService` with sidecar facade
- Ship minimal `ai-switch-tsnet`
- OAuth + auth key
- reverse proxy exposure
- settings status + remote URLs
- package sidecar with desktop build

### P1

- MagicDNS-first URL display
- explicit sign-out/clear auth
- richer diagnostics for missing binary, port conflict, login timeout

### P2

- Evaluate collapsing sidecar into in-process `libtailscale` FFI behind the same facade
- Mobile pairing polish

## Risks

| Risk | Mitigation |
|---|---|
| Windows packaging/signing complexity | Keep sidecar as adjacent binary versioned with the app |
| Confusion with system Tailscale | Independent state dir + UI wording as app-owned secure network |
| Long OAuth wait | Polling + timeout back to `needsLogin` |
| Auth key leakage | No echo, no plain JSON, no logs |
| Node online while web stopped | Allow connected state but show actionable start web service guidance |
| Sidecar crash | Main app stays up; status becomes error/stopped; user can retry |

## Completion Criteria

- Desktop no longer depends on system `tailscale` CLI for this feature
- User can connect with OAuth or auth key from settings
- After connect, same-tailnet clients can reach the web service
- Localhost access still works
- Web token auth still enforced
- Design/API leaves room for later in-process libtailscale without UI rewrite
