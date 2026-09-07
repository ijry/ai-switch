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
| `AI_SWITCH_PORT` | `19527` | No | Listening port. A value that does not parse as a port silently falls back to `19527` |
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
AI Switch server listening on http://127.0.0.1:19527
```

Serving other hosts (non-loopback plaintext is disabled by default; explicitly allow it only behind an HTTPS reverse proxy):

```bash
export AI_SWITCH_HOST=0.0.0.0
export AI_SWITCH_PORT=19527
export AI_SWITCH_TOKEN="$(openssl rand -hex 32)"
export AI_SWITCH_STATIC_DIR=/opt/ai-switch/dist
export AI_SWITCH_TLS_CERT_PATH=/etc/ai-switch/fullchain.pem
export AI_SWITCH_TLS_KEY_PATH=/etc/ai-switch/privkey.pem
/opt/ai-switch/ai-switch-server
```

```powershell
$env:AI_SWITCH_HOST = "0.0.0.0"
$env:AI_SWITCH_PORT = "19527"
$env:AI_SWITCH_TOKEN = "<your-random-token>"
$env:AI_SWITCH_STATIC_DIR = "C:\ai-switch\dist"
$env:AI_SWITCH_TLS_CERT_PATH = "C:\ai-switch\certs\fullchain.pem"
$env:AI_SWITCH_TLS_KEY_PATH  = "C:\ai-switch\certs\privkey.pem"
C:\ai-switch\ai-switch-server.exe
```

If you would rather not terminate TLS in the server, keep `AI_SWITCH_HOST=127.0.0.1` and put an HTTPS reverse proxy in front. The server then satisfies the loopback condition and needs no certificate paths.

Once running, the endpoints and browser behaviour are identical to the desktop web service: `POST /api/:command`, `GET /ws/events`, and the unauthenticated `GET /health`. See [Web Service Mode](/en/deploy/web-service).

## Shared panel and compute-pool port

The standalone server owns one listener, `19527` by default. The browser panel and compute-pool API share it:

- panel routes (`/api/*`, `/ws/*`, `/health`, and frontend pages) keep panel access-token authentication;
- model API routes (`/models`, `/v1/*`, `/v1beta/*`, `/messages`, and `/responses`) go to the compute-pool proxy and require its separate route-proxy API key;
- the panel token and compute-pool API key are different credentials and cannot substitute for each other.

For `0.0.0.0` or another non-loopback bind, plaintext HTTP is rejected by default. The recommended setup is to terminate HTTPS in Nginx or Caddy and reverse proxy to `127.0.0.1:19527`. If you explicitly accept a trusted-network plaintext boundary, set:

```bash
export AI_SWITCH_ALLOW_INSECURE_HTTP=1
```

This only permits startup; it does not disable panel-token or compute-pool API-key authentication. Plain HTTP can expose tokens and request contents, so do not publish it directly to the internet. Built-in HTTPS remains available with `AI_SWITCH_TLS_CERT_PATH` and `AI_SWITCH_TLS_KEY_PATH`, but is optional when a reverse proxy terminates TLS.

## One-click Linux installation

On an x86_64 Linux server, install the latest Release with:

```bash
AI_SWITCH_PORT=19527 /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/ijry/ai-switch/main/scripts/install-server.sh)"
```

Replace `AI_SWITCH_PORT=19527` with the port you want; the default bind is `127.0.0.1`, and external access requires setting `AI_SWITCH_HOST`. The installer creates the `ai-switch` system user, installs the program under `/opt/ai-switch`, persists the token and port in `/etc/ai-switch/server.env`, and installs, enables, and starts `ai-switch-server.service`. When it finishes, it prints the panel URL, service status, and the command for reading the access token. Before upgrading, it stops the old service so replacing the binary does not fail with `Text file busy`.

The current Linux server binary still depends on the WebKitGTK 4.1 runtime. The installer checks for missing libraries with `ldd`; on Debian/Ubuntu it installs `libwebkit2gtk-4.1-0` automatically, while other distributions require the equivalent runtime package first.

A fresh install writes `AI_SWITCH_ALLOW_INSECURE_HTTP=1`, allowing plaintext HTTP to start on a non-loopback bind. This applies only to the installer path; manually running the server still rejects it by default. Panel-token and compute-pool API-key authentication remain enabled. Re-running preserves the existing token, environment configuration, and `~/.ai-switch` data, so an existing configuration is not rewritten with this default. The installer does not modify Nginx, Certbot, UFW, or firewall rules; configure HTTPS reverse proxying separately.
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
- **Non-loopback plaintext HTTP is disabled by default.** Use Nginx/Caddy to terminate HTTPS, or explicitly set `AI_SWITCH_ALLOW_INSECURE_HTTP=1` only inside a trusted network. Authentication remains enabled, and direct public exposure is unsafe.
- **The data directory follows the service account.** The server always uses `~/.ai-switch` under the running user's home. Its SQLite database holds API keys and account credentials, so treat it as a credential directory.
- **Sharing means sharing everything.** Everyone on one instance sees the same accounts, the same usage, and the same sessions. There is no per-user permission model.
:::

## Next steps

- To reach this server from outside your network, see [Remote Access and HTTPS](/en/deploy/remote-access).
- For browser-side UI and endpoint details, see [Web Service Mode](/en/deploy/web-service).
- To get it running on a dev machine, see [Local Setup](/en/dev/local-setup).
- To see how the server and desktop share one command layer, see [Architecture](/en/dev/architecture).
