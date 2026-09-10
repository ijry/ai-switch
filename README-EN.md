# ai-switch

English | [简体中文](README.md)

AI Switch is a desktop and self-hosted Web app for AI provider and official account switching.

<img width="2360" height="1520" alt="ai-switch" src="https://github.com/user-attachments/assets/fbd3932e-29a7-4e3f-a980-e93fb093b643" />

Current foundation includes:

- Tauri 2 + React + TypeScript desktop shell
- Shared Rust core with desktop and Web transports
- Standalone `ai-switch-server` binary for browser/mobile access
- SQLite foundation schema
- Account, session, terminal, and route-proxy workflows
- Settings stored in `~/.ai-switch/settings.json`
- Web Service settings with token-protected HTTP access
- Tailscale login entry for private remote access, with MagicDNS HTTPS and mobile pairing

## Platform Support

| Platform | Route credentials and API routing | Native config writing | Official import and quota |
| --- | --- | --- | --- |
| Codex | Supported | Supported | Supported |
| Claude Code | Supported | Supported | Supported where the upstream account flow allows it |
| Gemini CLI | Supported | Supported | Import supported; official quota is not claimed |
| Grok | Supported | Supported | Supported where the upstream account flow allows it |
| OpenCode | Partial: API credentials require an explicit base URL and API dialect | Supported | Not supported |
| OpenClaw | Partial: API credentials require an explicit base URL and API dialect | Supported | Not supported |
| Hermes | Partial: API credentials require an explicit base URL and API dialect | Supported | Not supported |

Those three are agent harnesses rather than model vendors, so they have no official sign-in of their own: official-account import, official-account routing, deeplink, and quota lookup do not exist for them. That is the whole of what "partial" means.

Native config writing uses safe direct writes: AI Switch prepares a snapshot before mutation, writes atomically, detects concurrent changes, and supports guarded rollback. All seven platforms write their own file — Codex's `~/.codex/config.toml`, the `settings.json` of Claude Code / Gemini CLI / Grok, and an `ai-switch` custom provider inside `~/.config/opencode/opencode.json`, `~/.openclaw/openclaw.json`, and `~/.hermes/config.yaml`.

### Protocol Routing

API route credentials for Codex and Claude can target four upstream dialects: `openai`, `openai-responses`, `anthropic`, and `gemini`. The Codex local entrypoint still speaks OpenAI Responses; the Claude local entrypoint still speaks Anthropic Messages. AI Switch bridges the two whenever the local entrypoint protocol and the upstream account protocol differ. The Gemini CLI local entrypoint stays Gemini native for now and only routes to `gemini` accounts.

## Development

Install dependencies:

```powershell
corepack enable
pnpm install
```

Run frontend checks:

```powershell
pnpm typecheck
pnpm test:run
```

Run Rust checks:

```powershell
pnpm rust:check
pnpm rust:test
pnpm server:check
```

Run the desktop app in development mode:

```powershell
pnpm tauri:dev
```

Build the desktop frontend and installer:

```powershell
pnpm build
pnpm tauri:build
```

## Release Automation

GitHub Actions automatically builds and publishes cross-platform release assets when a version tag is pushed.

Required repository secret:

- `TAURI_SIGNING_PRIVATE_KEY`

Optional repository secret:

- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

Create and push a version tag:

```bash
git tag v0.4.2
git push origin v0.4.2
```

Tags containing `-rc`, `-beta`, or `-alpha` are published as prereleases. For example:

```bash
git tag v0.4.2-rc.1
git push origin v0.4.2-rc.1
```

The tag version without the `v` prefix must exactly match both `package.json` and `src-tauri/tauri.conf.json`, including any prerelease suffix. The tagged commit must belong to the repository's default branch.

The workflow builds signed Tauri desktop bundles, `ai-switch-server`, `ai-switch-tsnet`, and `latest.json` updater metadata for GitHub Releases.

### Package Managers

A separate workflow, `.github/workflows/package-managers.yml`, publishes an **already published** release to Homebrew and WinGet. It runs on `release: published`, and `workflow_dispatch` accepts a `tag` so any past release can be re-submitted without rebuilding it. Drafts and prereleases are skipped.

Both paths write to another repository and need a secret. A missing secret logs a warning and skips that path instead of failing the run:

- `HOMEBREW_TAP_TOKEN` — PAT with `contents: write` on the tap repository (`HOMEBREW_TAP_REPO`, default `ai-switch/homebrew-ai-switch`)
- `WINGET_TOKEN` — classic PAT with the `public_repo` scope, plus a fork of `microsoft/winget-pkgs` under `WINGET_FORK_USER`

Two one-time steps are not automatable: create the public `homebrew-` tap repository, and submit the first `Lingyun.AISwitch` version to winget-pkgs by hand — the action only bumps a package that already exists there. See [Release Process](https://ijry.github.io/ai-switch/en/dev/release) for the full setup.

## Web Service And Server Mode

Desktop and browser share one React UI. Desktop uses Tauri IPC. Browser mode uses:

- `POST /api/:command`
- `GET /ws/events`
- token auth on both endpoints

### Configure from desktop

1. Open Settings
2. Choose **Web Service**
3. Set host, service port, and access token
4. Click **Save**, then start the **shared service port** when needed
5. Optionally enable Tailscale, choose private or public access, and click **Login with Tailscale**

Like the standalone server, the GUI hosts Web pages, SaaS (when enabled), and compute-pool model APIs on one listener. Settings split this into two blocks: **Shared service port** owns the bind address, port, TLS, and listener start/stop; **Compute-pool route access** decides whether model APIs accept pool routing. Enabling route access starts the shared port when needed, while disabling it leaves the port available for Web pages. Desktop dev mode defaults to port `10086` and uses a separate `web-service-dev.json`, so it never changes the installed release configuration; installed and standalone-server configurations default to service port `19527`. A successfully started shared listener takes over any legacy independent pool listeners instead of leaving a second set of ports running.

The desktop remembers the route-access switch. If it is on when the app starts, the shared port is restored automatically, so there is no separate Web-service auto-start option. Standalone server and Docker listeners are always managed by environment variables and the process supervisor, and the browser cannot change or restart them; the route-access switch remains available in the browser.

Configure HTTPS under **Web Service → TLS**, not on the legacy independent pool HTTPS port. Restart the shared service after changing its host, service port, or TLS settings, and rewrite client route configs if their endpoint changes. Sharing a port does not merge administrator, mobile, compute-pool-key, or SaaS-key permissions. When route access is disabled, model APIs return `route_proxy.access_disabled`; panel routes and the health check remain available.

Default bind is `127.0.0.1:19527`. Without TLS, non-loopback hosts such as `0.0.0.0` are rejected; enable Web service TLS before binding to all interfaces.

For private access, the desktop publishes `https://<magicdns-name>:<port>` through Tailscale `ListenTLS`. Enable MagicDNS and HTTPS certificates in the Tailscale admin console; do not use the `100.x.y.z` IP as the mobile URL because the certificate is issued for the MagicDNS name. The phone must have the official Tailscale App signed in to the same tailnet. The uni-app client does not embed a Tailscale SDK.

For H5 and mini-program clients, use the public HTTPS URL as the default cross-platform endpoint. H5 needs CORS and a mini-program needs the hostname on its allowed request-domain list. The secure-network panel can show a short-lived, single-use mobile pairing QR: it contains the URL and pairing code, never the long-lived Web Service token. Scanning fills the form only; mobile users can still enter or edit the URL and token manually.

### Standalone server

Build:

```powershell
pnpm build
pnpm server:build
```

Run:

```powershell
$env:AI_SWITCH_HOST = "127.0.0.1"
$env:AI_SWITCH_PORT = "19527"
$env:AI_SWITCH_TOKEN = [guid]::NewGuid().ToString()
$env:AI_SWITCH_STATIC_DIR = "$PWD\dist"
.\src-tauri\target\debug\ai-switch-server.exe
```

Release binary path:

```text
src-tauri/target/release/ai-switch-server.exe
```

Optional environment variables:

- `AI_SWITCH_HOST` default `127.0.0.1`
- `AI_SWITCH_PORT` default `19527`
- `AI_SWITCH_TOKEN` required for API and WebSocket access, at least 16 characters; the server refuses to start without it
- `AI_SWITCH_STATIC_DIR` frontend `dist` directory for browser UI (only needed if you moved it)

The release archive `ai-switch-server_<tag>_<platform>.zip` already contains the binary, the Tailscale sidecar and a sibling `web/` directory, so unzip-and-run serves the browser UI with no extra configuration. Installed desktop builds ship the same assets under `web/` next to the executable.

### Shared port and one-click Linux installation

The standalone server listens on `19527` by default, with the panel and compute-pool API on the same port. `/api/*`, `/ws/*`, and panel pages use `AI_SWITCH_TOKEN`; `/models`, `/v1/*`, `/v1beta/*`, `/messages`, and `/responses` are forwarded to the compute pool and use a separate route-proxy API key. The two credentials are not interchangeable.

Plain HTTP on non-loopback binds is disabled by default. With Nginx or Caddy terminating HTTPS and proxying to `127.0.0.1:19527`, built-in TLS is not needed. Only set `AI_SWITCH_ALLOW_INSECURE_HTTP=1` when a trusted network boundary explicitly permits plaintext; API authentication remains enabled and direct public exposure is unsafe.

On x86_64 Linux, install with one command:

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/ijry/ai-switch/main/scripts/install-server.sh)"
```

The installer creates the `ai-switch` system user, installs under `/opt/ai-switch`, persists `/etc/ai-switch/server.env`, and enables the systemd service. Re-running it preserves the existing token and data. It does not configure Nginx, Certbot, or firewall rules.

### Docker one-click server and SaaS startup

The Docker image reuses the standalone server already packaged in the GitHub Release, so no Rust compilation happens locally and desktop WebKitGTK is not required. After each stable release, CI publishes `linux/amd64` and `linux/arm64` images to `ijry/ai-switch`. By default, Redis is the log queue, PostgreSQL is the log store, and SaaS is enabled automatically:

```bash
docker compose -f deploy/docker-compose.yml up -d
```

Inside the container, the default log queue and store use `redis://redis:6379` and `postgresql://ai_switch:change-me@postgres:5432/ai_switch_logs?sslmode=disable`, via `SAAS_LOGS_REDIS_URL` and `SAAS_LOGS_POSTGRES_URL`. SaaS settings store only environment-name references and do not persist connection strings in the database.

Common overrides:

- `AI_SWITCH_PORT`: host port mapping, default `19527`.
- `AI_SWITCH_DOCKER_IMAGE`: image reference, default `ijry/ai-switch:latest`; pin a release with `ijry/ai-switch:0.9.0` or `ijry/ai-switch:0.9`.
- `AI_SWITCH_TOKEN`: when unset, the entrypoint generates and prints a container-local token; set it explicitly for restarts.
- `AI_SWITCH_SAAS_ENABLE`: default `1`; set `0` to run only the standalone server.
- `AI_SWITCH_SAAS_ACTIVATION_CODE`, `AI_SWITCH_SAAS_INSTANCE_ID`, `AI_SWITCH_SAAS_SITE_NAME`, `AI_SWITCH_SAAS_PUBLIC_BASE_URL`.
- `AI_SWITCH_SAAS_LOGS_QUEUE`, `AI_SWITCH_SAAS_LOGS_STORE`, defaulting to `redis` and `postgres`.
- `AI_SWITCH_SAAS_LOGS_REDIS_URL_ENV`, `AI_SWITCH_SAAS_LOGS_POSTGRES_URL_ENV`, defaulting to references to `SAAS_LOGS_REDIS_URL` and `SAAS_LOGS_POSTGRES_URL`.

Data is stored in named volumes: `ai-switch-data`, `redis-data`, and `postgres-data`. For production, replace the default PostgreSQL password and terminate HTTPS at your reverse proxy.

To build the image locally, the Dockerfile also downloads and verifies the release archive instead of compiling source:

```bash
docker build --build-arg AI_SWITCH_VERSION=v0.9.0 .
```

### Security notes

- Every `/api/*` and `/ws/events` request requires the access token
- Tailscale login is manual; the app does not auto-login on startup
- Web access still requires the AI Switch token even over Tailscale
- Mobile pairing creates an independent mobile token; pairing codes are single-use and expire

## Clean-Room Boundary

This project may study public behavior, public documentation, and public file formats from related tools.

## License

The repository is generally available under the MIT License in the root LICENSE. The SaaS plugin has a separate license scope: src/saas/ and src-tauri/src/saas/ are licensed under the GNU General Public License v3.0 only (GPL-3.0-only), as stated in each directory's LICENSE. See docs/saas-license.md for the scope, distribution notes, and bilingual explanation.
