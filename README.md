# ai-switch

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
| OpenCode | Partial: API credentials require an explicit base URL and API dialect | Not supported | Not supported |
| OpenClaw | Partial: API credentials require an explicit base URL and API dialect | Not supported | Not supported |
| Hermes | Partial: API credentials require an explicit base URL and API dialect | Not supported | Not supported |

OpenCode, OpenClaw, and Hermes remain visible for generic API routing, terminal launch, and session workflows, but AI Switch does not claim native configuration, official-account import, or quota support for them.

Native Codex, Claude Code, Gemini CLI, and Grok configuration changes use safe direct writes: AI Switch prepares a snapshot before mutation, writes atomically, detects concurrent changes, and supports guarded rollback. Phase A never resolves or modifies Hermes `config.yaml`.

### Protocol Routing

Codex 和 Claude 的 API 路由账号可以选择 `openai`、`openai-responses`、`anthropic`、`gemini` 四种上游协议。Codex 本地入口仍使用 OpenAI Responses；Claude 本地入口仍使用 Anthropic Messages。AI Switch 会在本地入口协议和上游账号协议不一致时进行桥接转换。Gemini CLI 本地入口目前保持 Gemini native，只路由到 Gemini 协议账号。

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
5. Optionally enable Tailscale, choose private or public access, and click **Login with Tailscale**

Default bind is `127.0.0.1:3090`. Binding to `0.0.0.0` must be explicit.

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
- `AI_SWITCH_TOKEN` required for API and WebSocket access; the server refuses to start without it
- `AI_SWITCH_STATIC_DIR` frontend `dist` directory for browser UI (only needed if you moved it)

The release archive `ai-switch-server_<tag>_<platform>.zip` already contains the binary, the Tailscale sidecar and a sibling `web/` directory, so unzip-and-run serves the browser UI with no extra configuration. Installed desktop builds ship the same assets under `web/` next to the executable.

### Security notes

- Every `/api/*` and `/ws/events` request requires the access token
- Tailscale login is manual; the app does not auto-login on startup
- Web access still requires the AI Switch token even over Tailscale
- Mobile pairing creates an independent mobile token; pairing codes are single-use and expire

## Clean-Room Boundary

This project may study public behavior, public documentation, and public file formats from related tools.
