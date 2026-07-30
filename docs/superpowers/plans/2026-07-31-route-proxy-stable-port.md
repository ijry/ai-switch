# Route Proxy Stable Port Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the capacity-pool route proxy prefer port `19527` and sequentially fall back only when that port is unavailable.

**Architecture:** Keep listener selection inside `RouteProxyService` so HTTP and HTTPS startup paths share the same chosen listener. Add a constant for the default port, use the `u16` port range as the upper bound, and replace the random OS port bind with a small sequential bind helper.

**Tech Stack:** Rust, Tokio `TcpListener`, existing Tauri service tests.

## Global Constraints

- Work directly on `main`; do not create branches or worktrees.
- Default route proxy bind host remains `127.0.0.1`.
- Default route proxy port is `19527`.
- Fallback ports increment by `1` until a bind succeeds or the `u16` port range is exhausted.

---

### Task 1: Stable Route Proxy Listener

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs`

**Interfaces:**
- Consumes: existing `RouteProxyService::start(state, pool, transport)`.
- Produces: unchanged `RouteProxyStatus` with the actual selected port and base URL.

- [x] Add route proxy port constants next to `BIND_HOST`.
- [x] Add `bind_route_proxy_listener()` that loops from `19527` through `65535` and returns the first successful `TcpListener`.
- [x] Change `RouteProxyService::start` to call the helper instead of binding port `0`.
- [x] Preserve the existing error code `filesystem.route_proxy_bind` if no candidate can bind.

### Task 2: Fallback Test

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs`

**Interfaces:**
- Consumes: `RouteProxyService::start` and `RouteProxyRuntimeState`.
- Produces: a regression test proving default-port occupation triggers sequential fallback.

- [x] Add a Tokio test that binds `127.0.0.1:19527` before starting the route proxy.
- [x] Assert the route proxy is running on a port greater than `19527` and that `base_url` contains the reported port.
- [x] Stop the proxy at the end of the test.

### Task 3: Verification

**Files:**
- Test: `src-tauri/src/services/route_proxy_service.rs`

- [x] Run `cargo test route_proxy_service --lib` from `src-tauri`.
- [x] Fix any failures caused by the port-selection change.
