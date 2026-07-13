# AI Switch Web Transport And Server Mode Design

## Context

AI Switch currently runs as a Tauri desktop app. The frontend calls Rust through `@tauri-apps/api/core.invoke`, and terminal output is delivered through Tauri events. The Rust side already has useful core pieces such as SQLite repositories, session scanning, `portable-pty` terminal management, and an Axum route proxy, but command handlers and event delivery are still tied to the desktop runtime.

The requested target is the same broad architecture used by `xintaofei/codeg`: two transports over one shared Rust core. Desktop keeps Tauri IPC. Web/server mode exposes the same app behavior through Axum HTTP plus WebSocket events, with settings UI for local Web service configuration and built-in Tailscale/OAuth access management.

## Goals

- Add a standalone `ai-switch-server` binary for browser/mobile access.
- Keep a shared Rust core so desktop and Web do not fork business logic.
- Add a frontend transport abstraction that automatically uses Tauri IPC in desktop and HTTP/WebSocket in Web mode.
- Deliver terminal/session events through a common event emitter that supports both Tauri events and WebSocket broadcast.
- Add a compact settings entry and page for Web service configuration.
- Integrate Tailscale access as a managed local capability with an OAuth login button.
- Preserve current desktop behavior while enabling future mobile clients over LAN/Tailscale.

## Non-Goals

- Do not replace `portable-pty` with tmux or another multiplexer in this phase.
- Do not implement multi-user authorization or role-based access control.
- Do not require cloud infrastructure. The server is local/self-hosted.
- Do not expose the Web service without an access token or explicit user action.
- Do not redesign all settings screens; only add the Web service entry/page.

## Architecture

The application will have three layers:

1. Shared Rust core: pure services and managers that accept plain state and return typed results.
2. Runtime adapters: Tauri commands for desktop and Axum handlers for Web/server mode.
3. Frontend transport: `Transport.call()` and `Transport.subscribe()` hide whether the app is using `invoke()` or HTTP/WebSocket.

Desktop flow:

```text
React UI -> TauriTransport.call() -> tauri command -> shared Rust core
Rust event -> EventEmitter::Tauri -> Tauri event -> React subscriber
```

Server flow:

```text
Browser UI -> WebTransport.call() -> POST /api/:command -> shared Rust core
Rust event -> EventEmitter::Web -> WebSocket /ws/events -> React subscriber
```

## Rust Core Refactor

Move command business logic into core functions that do not depend on Tauri types.

Initial core modules:

- `core/settings.rs`: `get_settings_core`, `save_settings_core`.
- `core/sessions.rs`: `list_sessions_core`, `get_session_messages_core`.
- `core/terminals.rs`: `create_terminal_session_core`, `write_terminal_input_core`, `resize_terminal_core`, `kill_terminal_session_core`, `list_terminal_sessions_core`.
- `core/route_proxy.rs`: wraps current route proxy service calls.
- `web/event_bridge.rs`: common event emitter and WebSocket broadcaster.

Existing Tauri command modules stay, but become thin wrappers around core functions. Axum handlers call the same core functions through `Arc<AppState>`.

Terminal manager changes:

- Replace the direct `tauri::AppHandle` dependency in `TerminalManager::create_session` with an `EventEmitter`.
- Keep `portable-pty` as the single terminal process backend.
- Keep terminal session IDs process-local in phase one.
- Emit `terminal://output`, `terminal://exit`, and `terminal://error` through the common event emitter.

## Server Runtime

Add `src-tauri/src/bin/ai_switch_server.rs`.

Server responsibilities:

- Initialize app paths, SQLite migrations, settings, route proxy runtime state, terminal manager, and event broadcaster.
- Serve API at `POST /api/:command`.
- Serve WebSocket events at `GET /ws/events`.
- Serve built frontend assets when `AI_SWITCH_STATIC_DIR` is configured or a bundled `web` directory exists next to the binary.
- Expose health at `GET /health` and authenticated `POST /api/health`.

Environment variables:

- `AI_SWITCH_HOST`: default `127.0.0.1`.
- `AI_SWITCH_PORT`: default `3090`.
- `AI_SWITCH_TOKEN`: optional; if absent, generate and persist a token in settings.
- `AI_SWITCH_DATA_DIR`: optional app data override.
- `AI_SWITCH_STATIC_DIR`: optional static frontend path.

Cargo setup:

- Add feature `tauri-runtime` as the default feature.
- Desktop binary uses default features.
- `ai-switch-server` builds with `--no-default-features`.
- Tauri-specific imports and plugins are guarded by `#[cfg(feature = "tauri-runtime")]`.

## Web API

HTTP command format mirrors Tauri invoke:

```text
POST /api/create_terminal_session
Authorization: Bearer <token>
Content-Type: application/json

{ "input": { ... } }
```

Response format:

- Success returns JSON result directly.
- Failure returns `{ "code": string, "message": string, "details": string | null, "recoverable": boolean }`.

Initial API commands:

- settings: `get_settings`, `save_settings`.
- sessions: `list_sessions`, `get_session_messages`.
- terminals: `create_terminal_session`, `write_terminal_input`, `resize_terminal`, `kill_terminal_session`, `list_terminal_sessions`.
- route proxy: existing route proxy commands.
- web service: `get_web_service_config`, `save_web_service_config`, `get_web_server_status`, `start_web_server`, `stop_web_server`.
- tailscale: `get_tailscale_status`, `start_tailscale_login`, `disconnect_tailscale`.

## WebSocket Events

WebSocket endpoint:

```text
GET /ws/events
Authorization via Sec-WebSocket-Protocol or token query fallback
```

Frame shape:

```json
{
  "channel": "terminal://output",
  "payload": {
    "sessionId": "abc",
    "data": "..."
  }
}
```

The server sends an initial ready frame:

```json
{ "channel": "__ready__", "payload": {} }
```

The frontend waits for ready before considering subscriptions reliable.

## Frontend Transport

Add `src/lib/transport`.

Transport interface:

```ts
type Unsubscribe = () => void;

interface Transport {
  call<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  subscribe<T>(event: string, handler: (payload: T) => void): Promise<Unsubscribe>;
  isDesktop(): boolean;
  destroy?(): void;
}
```

Implementations:

- `TauriTransport`: dynamic imports `@tauri-apps/api/core` and `@tauri-apps/api/event`.
- `WebTransport`: uses `fetch('/api/:command')` and a reconnecting WebSocket.
- `detect.ts`: chooses Tauri when `window.__TAURI_INTERNALS__` exists, otherwise Web.

Change `src/lib/api/client.ts` to use `getTransport().call()` instead of direct `invoke`. Change terminal components to subscribe through transport, not direct Tauri event listeners.

## Web Service Settings

Add `Web Service` to the settings feature grid.

Screen layout:

- Compact card matching current macOS-style settings.
- Status row: running/stopped, runtime mode, WebSocket status.
- Host/port/token controls.
- Start/stop actions.
- Copy/open address actions.
- Auto-start toggle.
- Tailscale section with status, device name, tailnet address, OAuth login button, disconnect button.

UI copy must avoid developer-only wording. For example:

- Use `Remote access` instead of `server binary`.
- Use `Secure network` instead of `Tailscale daemon`.
- Use `Login with Tailscale` for the OAuth action.

## Tailscale Integration

Use a pragmatic sidecar/system integration first.

Backend behavior:

- Detect an installed `tailscale` CLI first.
- If bundled sidecar support is added later, prefer the bundled binary when present.
- `get_tailscale_status` runs a bounded status probe and returns disconnected, needs_login, connected, or error.
- `start_tailscale_login` starts `tailscale up` with a generated auth flow and returns the login URL when available.
- `disconnect_tailscale` calls `tailscale logout` or `tailscale down` based on platform behavior.

Security posture:

- Do not store Tailscale OAuth secrets in plaintext.
- Do not silently run network login on startup.
- User must click the OAuth login button at least once.
- Web access still requires the AI Switch token even on Tailscale.

Future upgrade path:

- If `tailscale.com/tsnet` becomes practical for this app, replace CLI sidecar internals behind the same `TailscaleService` interface without changing UI or frontend API.

## Persistence

Add migration for Web service settings:

```sql
CREATE TABLE web_service_settings (
  id TEXT PRIMARY KEY,
  host TEXT NOT NULL,
  port INTEGER NOT NULL,
  token TEXT,
  auto_start INTEGER NOT NULL DEFAULT 0,
  tailscale_enabled INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

Use a singleton row id such as `default`.

Token handling:

- Generate a random token when server mode starts without a configured token.
- Persist the generated token so restart does not rotate access unexpectedly.
- Mask token in UI by default.

## Packaging

Desktop installer:

- Continues to package the Tauri app.
- May optionally include `ai-switch-server` as a sibling binary after server mode is stable.

Server release:

- Adds `pnpm server:build`.
- Produces `src-tauri/target/release/ai-switch-server.exe` on Windows.
- Later release automation can package zip/tar artifacts with `web` static assets.

Frontend build:

- Current Vite build remains valid for desktop.
- Server mode uses the same static `dist` output.

## Security

- Web server binds to `127.0.0.1` by default.
- Binding to `0.0.0.0` must be explicit in settings or environment.
- Every `/api/*` and `/ws/events` request requires the access token.
- Health endpoint without auth exposes only minimal liveness.
- Terminal creation validates working directory exists and rejects empty commands.
- WebSocket reconnect must not bypass token validation.
- No credentials are logged. Generated tokens are shown only in UI or first-run stderr for standalone server.

## Testing

Rust tests:

- Core functions compile with and without `tauri-runtime`.
- `cargo check` for desktop default features.
- `cargo check --no-default-features --bin ai-switch-server`.
- Terminal manager tests cover emitter-free command resolution and validation.
- Web handler tests cover auth required, command dispatch, and WebSocket ready frame.
- Tailscale service tests use a fake command runner.

Frontend tests:

- Transport detection tests.
- WebTransport call success, error, unauthorized, and timeout tests.
- WebSocket event subscription and reconnect tests.
- API client tests confirm command names and payload shapes remain stable.
- Web service settings screen tests cover start/stop, token masking, and Tailscale login button.

End-to-end smoke:

- Desktop app can create and use a terminal.
- Server binary can serve the frontend and create a terminal through Web transport.
- Terminal output appears in browser through WebSocket.
- Existing route proxy commands still work from desktop.

## Rollout Plan

1. Add transport abstraction on the frontend while still backed by Tauri.
2. Refactor terminal/session/settings commands into shared Rust core.
3. Add Web event bridge and remove direct Tauri dependency from terminal manager.
4. Add Axum router and `ai-switch-server`.
5. Add WebTransport and WebSocket subscriptions.
6. Add Web service settings screen and persistence.
7. Add Tailscale service facade and OAuth login entry.
8. Build desktop and server artifacts, then run full test matrix.

## Risks

- Conditional compilation can cause desktop-only code to leak into server builds. Mitigation: add `cargo check --no-default-features --bin ai-switch-server` to verification.
- Terminal events can be lost during WebSocket reconnect. Mitigation: send ready frame and refresh terminal session status after reconnect.
- Tailscale CLI behavior differs by OS. Mitigation: isolate command execution behind `TailscaleService` and test with fake runners.
- Running a Web server increases attack surface. Mitigation: localhost default, token auth, explicit external bind, no unauthenticated APIs.
- Large refactor can regress desktop features. Mitigation: refactor one command group at a time and keep Tauri wrappers thin.

## Acceptance Criteria

- `pnpm build`, `pnpm test:run`, `pnpm rust:check`, and `pnpm rust:test` pass.
- `cargo check --no-default-features --bin ai-switch-server` passes.
- Desktop mode still uses Tauri transport and existing screens work.
- Browser mode uses Web transport without importing Tauri APIs.
- Server mode serves the frontend and supports terminal create/input/output/resize/kill.
- Settings has a Web service entry with service configuration and Tailscale login controls.
- Web APIs and WebSocket require the configured token.
