# AI Switch Route Proxy Provider Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the first real local route/proxy execution path and write Codex/Claude/Gemini config files to point at it.

**Architecture:** Rust owns the proxy runtime and config writes. Tauri commands expose proxy lifecycle, status, and config-write operations. The UI adds controls to start/stop proxy and write configs from the existing Accounts router screen.

**Tech Stack:** Tauri 2, Rust, Tokio, Axum, Reqwest, SQLite/sqlx, React, TanStack Query.

## Global Constraints

- Do not touch unrelated dirty files.
- Use `ConfigWriter::write_atomic` for all target config writes.
- Bind proxy only to `127.0.0.1`.
- First slice uses provider API credentials, not official account private token extraction.
- Keep config rendering conservative and testable.

---

### Task 1: Rust Proxy Runtime

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/app_state.rs`
- Create: `src-tauri/src/services/route_proxy_service.rs`
- Create: `src-tauri/src/commands/route_proxy_commands.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `RouteProxyService::start`, `RouteProxyService::stop`, `RouteProxyService::status`
- Produces Tauri commands: `start_route_proxy`, `stop_route_proxy`, `get_route_proxy_status`

- [ ] Add Axum/Reqwest/Tokio features.
- [ ] Add proxy runtime state to `AppState`.
- [ ] Implement local bind, lifecycle state, request forwarding, provider selection, and usage logging.
- [ ] Expose lifecycle commands.
- [ ] Run `pnpm rust:check`.

### Task 2: Config Writers

**Files:**
- Create: `src-tauri/src/services/route_config_service.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Modify: `src-tauri/src/commands/route_proxy_commands.rs`

**Interfaces:**
- Produces: `write_route_proxy_configs`
- Produces: `RouteConfigWriteOutcome { target_key, path, status }`

- [ ] Render Codex TOML pointing at local proxy.
- [ ] Render Claude settings JSON with route metadata.
- [ ] Render Gemini settings JSON with route metadata.
- [ ] Write files atomically and return outcomes.
- [ ] Add render unit tests.
- [ ] Run `cargo test route_config`.

### Task 3: Frontend API and Controls

**Files:**
- Modify: `src/lib/api/types.ts`
- Modify: `src/lib/api/client.ts`
- Modify: `src/screens/AccountsScreen.tsx`
- Modify: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes: `start_route_proxy`, `stop_route_proxy`, `get_route_proxy_status`, `write_route_proxy_configs`

- [ ] Add TypeScript route proxy types and client calls.
- [ ] Add AccountsScreen buttons for start/stop proxy and write configs.
- [ ] Show proxy base URL and config write results.
- [ ] Add frontend tests for command calls.
- [ ] Run `pnpm typecheck` and `pnpm test:run`.

### Task 4: Full Verification

- [ ] Run `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`.
- [ ] Run `pnpm rust:check`.
- [ ] Run `pnpm rust:test`.
- [ ] Run `git diff --check`.
