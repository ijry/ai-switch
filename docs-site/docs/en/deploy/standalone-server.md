---
title: Standalone Server
description: ai-switch-server is AI Switch's headless server binary, for teams or machines without a desktop environment. Covers build commands, the complete environment variable table, PowerShell and bash launch examples, and static asset resolution.
---

# Standalone Server

`ai-switch-server` is a headless server binary: the same Rust core (`ai_switch_lib`), the same React UI, with only the HTTP and WebSocket entry points. It fits two situations:

- **Machines with no graphical environment** — a NAS at home, a cloud VM — running in the background long-term.
- **A team sharing one setup**, where several people use a browser against one instance and share the account pool and usage stats.

Protocol-wise and capability-wise it matches the Web Service mode built into the desktop app. The differences are in the comparison table further down.

## Building

Build the frontend first, then the Rust binary.

```bash
pnpm install
pnpm build
pnpm server:build:release
```

```powershell
pnpm install
pnpm build
pnpm server:build:release
```

`pnpm build` runs `tsc && vite build`, producing `dist/` at the repo root. `pnpm server:build:release` runs `cargo build --release --bin ai-switch-server` inside `src-tauri`.

For a debug build use `pnpm server:build` (no `--release`: faster to compile, slower to run, and it uses the separate `ai-switch-dev.db` development database). To type-check and borrow-check without producing a binary, use `pnpm server:check`.

Output paths:

| Build command | Artifact |
| --- | --- |
| `pnpm server:build:release` | `src-tauri/target/release/ai-switch-server` (`ai-switch-server.exe` on Windows) |
| `pnpm server:build` | `src-tauri/target/debug/ai-switch-server` (`ai-switch-server.exe` on Windows) |

If you would rather not compile it yourself, every release attaches per-platform `ai-switch-server` archives to the GitHub Release. See [Release Process](/en/dev/release).

The archive unzips into exactly the layout recommended under "How the frontend is located" below — `ai-switch-server`, `ai-switch-tsnet` and `web/` already sit together, so you do not need `AI_SWITCH_STATIC_DIR` at all:

```text
ai-switch-server_v0.7.3_windows-x86_64/
├── ai-switch-server.exe
├── ai-switch-tsnet.exe
└── web/
    ├── index.html
    └── assets/...
```

## Environment variables

Every runtime parameter comes from an environment variable. There are no command-line flags and no config file:

| Variable | Default | Required | Notes |
| --- | --- | --- | --- |
| `AI_SWITCH_HOST` | `127.0.0.1` | No | Bind address. A non-loopback address **requires** TLS or startup fails |
| `AI_SWITCH_PORT` | `3090` | No | Listening port. A value that does not parse as a port silently falls back to `3090` |
| `AI_SWITCH_TOKEN` | none | **Yes** | Access token, at least 16 characters. The server refuses to start if it is missing or too short |
| `AI_SWITCH_STATIC_DIR` | none | No | Frontend `dist` directory. Only honoured if it contains `index.html`; otherwise the built-in candidates apply |
| `AI_SWITCH_TLS_CERT_PATH` | none | Paired with the next | Path to the certificate chain PEM |
| `AI_SWITCH_TLS_KEY_PATH` | none | Paired with the previous | Path to the private key PEM |
| `AI_SWITCH_TSNET_PATH` | none | No | Path to the Tailscale sidecar executable. Defaults to `ai-switch-tsnet` next to the current executable |

A few things this table needs spelled out:

- **`AI_SWITCH_TOKEN` is mandatory.** If it is unset, whitespace-only, or shorter than 16 characters, the server refuses to start and prints why. That is deliberate: ordinary commands include ones that return an account's plaintext API key (`list_route_credentials`), so running without a token exposes the credential store to anyone who can reach the port.
- **The two TLS paths must be provided together.** Supplying only one fails with `web.tls_paths_incomplete` and the server does not start.
- **The data directory cannot be set by environment variable.** The server always writes to `~/.ai-switch` under the running user's home directory. `AI_SWITCH_DATA_DIR`, which appears in the README, is **not implemented** in the current code — setting it has no effect. To relocate the data, control the service account's home directory or mount a container volume there.

## Running it

Minimal local-only startup:

```bash
export AI_SWITCH_TOKEN="$(openssl rand -hex 32)"
export AI_SWITCH_STATIC_DIR=/opt/ai-switch/dist
/opt/ai-switch/ai-switch-server
```

```powershell
$env:AI_SWITCH_TOKEN = [guid]::NewGuid().ToString()
$env:AI_SWITCH_STATIC_DIR = "C:\ai-switch\dist"
C:\ai-switch\ai-switch-server.exe
```

On success it prints the listening address:

```text
AI Switch server listening on http://127.0.0.1:3090
```

Serving other hosts (non-loopback, so TLS is required):

```bash
export AI_SWITCH_HOST=0.0.0.0
export AI_SWITCH_PORT=3090
export AI_SWITCH_TOKEN="$(openssl rand -hex 32)"
export AI_SWITCH_STATIC_DIR=/opt/ai-switch/dist
export AI_SWITCH_TLS_CERT_PATH=/etc/ai-switch/fullchain.pem
export AI_SWITCH_TLS_KEY_PATH=/etc/ai-switch/privkey.pem
/opt/ai-switch/ai-switch-server
```

```powershell
$env:AI_SWITCH_HOST = "0.0.0.0"
$env:AI_SWITCH_PORT = "3090"
$env:AI_SWITCH_TOKEN = "<your-random-token>"
$env:AI_SWITCH_STATIC_DIR = "C:\ai-switch\dist"
$env:AI_SWITCH_TLS_CERT_PATH = "C:\ai-switch\certs\fullchain.pem"
$env:AI_SWITCH_TLS_KEY_PATH  = "C:\ai-switch\certs\privkey.pem"
C:\ai-switch\ai-switch-server.exe
```

If you would rather not terminate TLS in the server, keep `AI_SWITCH_HOST=127.0.0.1` and put an HTTPS reverse proxy in front. The server then satisfies the loopback condition and needs no certificate paths.

Once running, the endpoints and browser behaviour are identical to the desktop web service: `POST /api/:command`, `GET /ws/events`, and the unauthenticated `GET /health`. See [Web Service Mode](/en/deploy/web-service).

## How the frontend is located

`AI_SWITCH_STATIC_DIR` is not the only route. The resolution order is below; the first candidate containing `index.html` wins:

1. the directory named by `AI_SWITCH_STATIC_DIR`;
2. next to the executable: `web/`, `dist/`, `resources/web/`;
3. one level up from the executable: `../web/`, `../dist/`;
4. relative to the working directory: `web/`, `dist/`.

So the least-effort layout is to keep the binary and the assets together:

```text
/opt/ai-switch/
├── ai-switch-server
└── web/
    ├── index.html
    └── assets/...
```

With that layout you do not need `AI_SWITCH_STATIC_DIR` at all. Paths that match no static file fall back to `index.html` so client-side routing works.

## Differences from the desktop web service

| | Desktop web service | Standalone server |
| --- | --- | --- |
| Configuration | `~/.ai-switch/web-service.json` plus the settings UI | Environment variables |
| Sensitive-command gate | Decided at runtime from transport safety (HTTPS / loopback / Tailscale state) | Always open, which makes the token that much more important |
| Desktop-only commands | Available inside the desktop window | Unavailable (no native desktop environment) |
| Tailscale | Toggled and signed in from the settings UI | You supply the sidecar binary (`AI_SWITCH_TSNET_PATH` or a sibling file) |
| Tray and auto-update | Yes | No — bring your own process supervisor and upgrade process |

Because the standalone server does not gate sensitive commands dynamically, credential export, proxy key reads, and MCP/skill installation are all callable once the token check passes.

## Security notes

::: warning Before you deploy
- **`AI_SWITCH_TOKEN` must be set (the server will not start without it).** The standalone server does not downgrade sensitive commands, so the token is the only access control there is.
- **The token is equivalent to shell access.** The web API includes terminal session commands, so whoever holds the token can run commands on that server.
- **Non-loopback binds require TLS** or the server refuses to start. That is deliberate — do not try to work around it.
- **The data directory follows the service account.** The server always uses `~/.ai-switch` under the running user's home. Its SQLite database holds API keys and account credentials, so treat it as a credential directory.
- **Sharing means sharing everything.** Everyone on one instance sees the same accounts, the same usage, and the same sessions. There is no per-user permission model.
:::

## Next steps

- To reach this server from outside your network, see [Remote Access and HTTPS](/en/deploy/remote-access).
- For browser-side UI and endpoint details, see [Web Service Mode](/en/deploy/web-service).
- To get it running on a dev machine, see [Local Setup](/en/dev/local-setup).
- To see how the server and desktop share one command layer, see [Architecture](/en/dev/architecture).
