# AI Switch Web Transport Server Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a shared Rust core with desktop and Web transports, a standalone server binary, Web service settings, and Tailscale OAuth access.

**Architecture:** Keep one Rust business core and two runtime adapters. Desktop continues to use Tauri IPC and events, while server mode uses Axum HTTP and WebSocket. Frontend calls go through a transport abstraction so the same React screens work in both runtimes.

**Tech Stack:** Rust 2021, Tauri 2, Axum 0.7, portable-pty, SQLite, React 18, TypeScript, TanStack Query, Vite

## Global Constraints

- Do not replace `portable-pty` with tmux or another multiplexer in this phase.
- The server is local/self-hosted.
- Every `/api/*` and `/ws/events` request requires the access token.
- Web server binds to `127.0.0.1` by default.
- Binding to `0.0.0.0` must be explicit in settings or environment.
- No credentials are logged.
- Do not silently run network login on startup.
- User must click the OAuth login button at least once.

---

### Task 1: Frontend transport abstraction

**Files:**
- Create: `src/lib/transport/types.ts`
- Create: `src/lib/transport/detect.ts`
- Create: `src/lib/transport/index.ts`
- Create: `src/lib/transport/tauri-transport.ts`
- Create: `src/lib/transport/web-transport.ts`
- Modify: `src/lib/api/client.ts`
- Modify: `src/components/terminal/XtermPane.tsx`
- Test: `tests/transport/transport.test.ts`
- Test: `tests/terminal/XtermPane.test.tsx`

**Interfaces:**
- Consumes: `Transport.call()`, `Transport.subscribe()`, `isDesktop()`
- Produces: `getTransport()`, `TauriTransport`, `WebTransport`

- [x] **Step 1: Write the failing tests**

```ts
import { describe, expect, it, vi } from "vitest";
import { __resetTransportForTests, getTransport, isDesktop } from "@/lib/transport";

describe("transport detection", () => {
  it("uses web transport outside Tauri", () => {
    expect(isDesktop()).toBe(false);
    expect(getTransport().isDesktop()).toBe(false);
  });
});
```

- [x] **Step 2: Run the tests to verify they fail**

Run: `pnpm test:run tests/transport/transport.test.ts tests/terminal/XtermPane.test.tsx`

Expected: fail because `src/lib/transport/*` does not exist yet and `XtermPane` still listens directly through Tauri.

- [x] **Step 3: Implement the transport layer**

```ts
export interface Transport {
  call<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  subscribe<T>(event: string, handler: (payload: T) => void): Promise<() => void>;
  isDesktop(): boolean;
  destroy?(): void;
}
```

Implement `getTransport()` so desktop uses `invoke()` and browser uses `fetch('/api/:command')` plus WebSocket events. Replace direct `invoke()` calls in `src/lib/api/client.ts` with `getTransport().call(...)`, and replace direct `listen()` usage in `XtermPane` with `getTransport().subscribe(...)`.

- [x] **Step 4: Run the tests to verify they pass**

Run: `pnpm test:run tests/transport/transport.test.ts tests/terminal/XtermPane.test.tsx && pnpm typecheck`

Expected: pass.

- [x] **Step 5: Commit**

```bash
git add src/lib/transport src/lib/api/client.ts src/components/terminal/XtermPane.tsx tests/transport/transport.test.ts tests/terminal/XtermPane.test.tsx
git commit -m "feat: add web transport abstraction"
```

### Task 2: Shared Rust core and event emitter

**Files:**
- Create: `src-tauri/src/core/mod.rs`
- Create: `src-tauri/src/core/settings.rs`
- Create: `src-tauri/src/core/sessions.rs`
- Create: `src-tauri/src/core/terminals.rs`
- Create: `src-tauri/src/web/event_bridge.rs`
- Modify: `src-tauri/src/app_state.rs`
- Modify: `src-tauri/src/terminal_manager.rs`
- Modify: `src-tauri/src/commands/settings_commands.rs`
- Modify: `src-tauri/src/commands/session_commands.rs`
- Modify: `src-tauri/src/commands/terminal_commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/terminal_manager.rs`

**Interfaces:**
- Consumes: `AppState`, `TerminalManager`, `EventEmitter`, `SqlitePool`
- Produces: `get_settings_core()`, `save_settings_core()`, `list_sessions_core()`, `get_session_messages_core()`, `create_terminal_session_core()`

- [x] **Step 1: Write the failing tests**

```rust
#[test]
fn create_session_emits_through_shared_emitter() {
    let manager = TerminalManager::default();
    let emitter = EventEmitter::test_web_only(Arc::new(WebEventBroadcaster::new()));
    let input = base_input();
    let session = create_terminal_session_core(&manager, &emitter, input).unwrap();
    assert_eq!(session.status, TerminalStatus::Running);
}
```

- [x] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test --features test-utils terminal_manager::tests::list_sessions_starts_empty`

Expected: fail because the new core entry points and emitter wiring are not implemented yet.

- [x] **Step 3: Extract the core functions**

```rust
pub fn create_terminal_session_core(
    manager: &TerminalManager,
    emitter: &EventEmitter,
    input: CreateTerminalSessionInput,
) -> Result<TerminalSession, String>;
```

Move business logic into the `core/*` modules. Keep the `#[tauri::command]` functions as thin wrappers that call the shared core functions. Change `TerminalManager` so it emits through `EventEmitter` instead of depending on `tauri::AppHandle` directly.

- [x] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test --features test-utils && cargo check`

Expected: pass.

- [x] **Step 5: Commit**

```bash
git add src-tauri/src/core src-tauri/src/web/event_bridge.rs src-tauri/src/app_state.rs src-tauri/src/terminal_manager.rs src-tauri/src/commands src-tauri/src/lib.rs
git commit -m "refactor: share rust core across runtimes"
```

### Task 3: Standalone server binary and HTTP/WebSocket runtime

**Files:**
- Create: `src-tauri/src/bin/ai_switch_server.rs`
- Create: `src-tauri/src/web/mod.rs`
- Create: `src-tauri/src/web/router.rs`
- Create: `src-tauri/src/web/auth.rs`
- Create: `src-tauri/src/web/ws.rs`
- Create: `src-tauri/src/web/handlers/mod.rs`
- Create: `src-tauri/src/web/handlers/settings.rs`
- Create: `src-tauri/src/web/handlers/sessions.rs`
- Create: `src-tauri/src/web/handlers/terminals.rs`
- Create: `src-tauri/src/web/handlers/tailscale.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `package.json`
- Modify: `src/lib/api/client.ts`

**Interfaces:**
- Consumes: `Arc<AppState>`, `EventEmitter::Web`, `build_router()`, `AuthState`
- Produces: `POST /api/:command`, `GET /ws/events`, `GET /health`, `POST /api/health`

- [x] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn api_requires_token() {
    let router = build_router(state, static_dir, "secret".to_string());
    let res = router.oneshot(post("/api/get_settings")).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
```

- [x] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test --no-default-features --bin ai-switch-server`

Expected: fail because the server binary, router, auth, and WS handler do not exist yet.

- [x] **Step 3: Implement the server runtime**

```rust
pub fn build_router(
    state: Arc<AppState>,
    token: String,
    static_dir: PathBuf,
) -> Router;
```

Add an `ai_switch_server` binary that loads paths, opens the database, initializes the shared core state, and serves both JSON APIs and WebSocket events. Use the same command names as Tauri so the frontend transport can switch without changing feature code.

- [x] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test --no-default-features --bin ai-switch-server && cargo check --no-default-features --bin ai-switch-server`

Expected: pass.

- [x] **Step 5: Commit**

```bash
git add src-tauri/src/bin/ai_switch_server.rs src-tauri/src/web src-tauri/Cargo.toml package.json src/lib/api/client.ts
git commit -m "feat: add standalone web server runtime"
```

### Task 4: Web service settings and Tailscale access

**Files:**
- Create: `src/components/settings/web-service-settings.tsx`
- Create: `src/components/settings/tailscale-settings.tsx`
- Modify: `src/screens/SettingsScreen.tsx`
- Modify: `src/lib/i18n.tsx`
- Modify: `src/lib/api/types.ts`
- Modify: `src/lib/api/client.ts`
- Create: `src-tauri/src/services/tailscale_service.rs`
- Create: `src-tauri/src/commands/web_service_commands.rs`
- Modify: `src-tauri/src/commands/mod.rs`

**Interfaces:**
- Consumes: `get_web_service_config`, `save_web_service_config`, `get_tailscale_status`, `start_tailscale_login`, `disconnect_tailscale`
- Produces: compact settings card, remote access state, OAuth login button

- [x] **Step 1: Write the failing tests**

```tsx
it("shows the web service entry and Tailscale login button", async () => {
  render(<SettingsScreen />);
  expect(await screen.findByRole("button", { name: /web service/i })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: /login with tailscale/i })).toBeInTheDocument();
});
```

- [x] **Step 2: Run the tests to verify they fail**

Run: `pnpm test:run tests/SettingsScreen.test.tsx`

Expected: fail because the new settings section and Tailscale API are not implemented yet.

- [x] **Step 3: Implement the settings UI and Tailscale service facade**

```ts
export type WebServiceConfig = {
  host: string;
  port: number;
  token?: string | null;
  autoStart: boolean;
  tailscaleEnabled: boolean;
};
```

Add a compact settings card that controls host, port, token, auto-start, and Tailscale access. The Tailscale backend should use a service facade so the implementation can start with CLI-sidecar integration and later swap to a different provider without changing the UI or command names.

- [x] **Step 4: Run the tests to verify they pass**

Run: `pnpm test:run tests/SettingsScreen.test.tsx && pnpm typecheck && cd src-tauri && cargo test --features test-utils`

Expected: pass.

- [x] **Step 5: Commit**

```bash
git add src/components/settings/web-service-settings.tsx src/components/settings/tailscale-settings.tsx src/screens/SettingsScreen.tsx src/lib/i18n.tsx src/lib/api/types.ts src/lib/api/client.ts src-tauri/src/services/tailscale_service.rs src-tauri/src/commands/web_service_commands.rs src-tauri/src/commands/mod.rs
git commit -m "feat: add web service and tailscale settings"
```

### Task 5: Packaging, docs, and end-to-end verification

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Modify: `package.json`
- Modify: `README.md`
- Modify: `docs/superpowers/specs/2026-07-14-ai-switch-web-transport-server-design.md` only if the implementation forces a spec correction

**Interfaces:**
- Consumes: `pnpm tauri:build`, `pnpm build`, `pnpm test:run`, `cargo check --no-default-features --bin ai-switch-server`
- Produces: desktop installer and server binary build path

- [x] **Step 1: Write the failing verification script**

```powershell
pnpm build
pnpm test:run
pnpm tauri:build
cd src-tauri
cargo check --no-default-features --bin ai-switch-server
```

- [x] **Step 2: Run the script to see the current gaps**

Run the four commands above.

Expected: the new server path and settings/Tailscale work may still be incomplete until Tasks 1-4 are done.

- [x] **Step 3: Update packaging and docs**

Add the server build script, bundle the server binary path, and document how to start the standalone server, how to open the Web settings page, and how to complete Tailscale login.

- [x] **Step 4: Run the full verification matrix**

Run:

```powershell
pnpm test:run
pnpm typecheck
pnpm build
pnpm tauri:build
cd src-tauri
cargo check
cargo test --features test-utils
cargo check --no-default-features --bin ai-switch-server
cargo test --no-default-features --bin ai-switch-server
```

Expected: pass.

- [x] **Step 5: Commit**

```bash
git add package.json src-tauri/tauri.conf.json README.md
git commit -m "chore: finish web server packaging"
```
