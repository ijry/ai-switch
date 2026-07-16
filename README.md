# ai-switch

AI Switch is a desktop and self-hosted Web app for AI provider and official account switching.

Current foundation includes:

- Tauri 2 + React + TypeScript desktop shell
- Shared Rust core with desktop and Web transports
- Standalone `ai-switch-server` binary for browser/mobile access
- SQLite foundation schema
- Account, session, terminal, and route-proxy workflows
- Settings stored in `~/.ai-switch/settings.json`
- Web Service settings with token-protected HTTP access
- Tailscale login entry for private remote access

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

GitHub Actions can build cross-platform release assets manually from the **Release** workflow.

Required repository secret:

- `TAURI_SIGNING_PRIVATE_KEY`

Optional repository secret:

- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

Run the workflow from GitHub Actions with:

- `tag`: release tag such as `v0.1.0`
- `release_name`: optional display name
- `draft`: keep `true` for review before publishing
- `prerelease`: set `true` for prerelease builds

The workflow builds signed Tauri desktop bundles, `ai-switch-server`, `ai-switch-tsnet`, and `latest.json` updater metadata for GitHub Releases.

## Web Service And Server Mode

Desktop and browser share one React UI. Desktop uses Tauri IPC. Browser mode uses:

- `POST /api/:command`
- `GET /ws/events`
- token auth on both endpoints

### Configure from desktop

1. Open Settings
2. Choose **Web Service**
3. Set host, port, and access token
4. Start the service
5. Optionally enable Tailscale and click **Login with Tailscale**

Default bind is `127.0.0.1:3090`. Binding to `0.0.0.0` must be explicit.

### Standalone server

Build:

```powershell
pnpm build
pnpm server:build
```

Run:

```powershell
$env:AI_SWITCH_HOST = "127.0.0.1"
$env:AI_SWITCH_PORT = "3090"
$env:AI_SWITCH_TOKEN = "replace-me"
$env:AI_SWITCH_STATIC_DIR = "$PWD\dist"
.\src-tauri\target\debug\ai-switch-server.exe
```

Release binary path:

```text
src-tauri/target/release/ai-switch-server.exe
```

Optional environment variables:

- `AI_SWITCH_HOST` default `127.0.0.1`
- `AI_SWITCH_PORT` default `3090`
- `AI_SWITCH_TOKEN` required for API and WebSocket access
- `AI_SWITCH_DATA_DIR` optional data directory override
- `AI_SWITCH_STATIC_DIR` frontend `dist` directory for browser UI

Installed desktop builds ship web assets next to the executable under web/. Standalone server mode can also use AI_SWITCH_STATIC_DIR or a sibling web/ / dist/ folder.

### Security notes

- Every `/api/*` and `/ws/events` request requires the access token
- Tailscale login is manual; the app does not auto-login on startup
- Web access still requires the AI Switch token even over Tailscale

## Clean-Room Boundary

This project may study public behavior, public documentation, and public file formats from related tools. It must not copy or translate non-commercial source code from `cockpit-tools`.
