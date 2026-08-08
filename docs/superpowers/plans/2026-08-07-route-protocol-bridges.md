# Route Protocol Bridges Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Codex Responses requests work through route credentials whose upstream API uses OpenAI Chat Completions.

**Architecture:** Add a pure protocol-bridge module that owns Responses↔Chat payload conversion and buffered SSE conversion. The existing route proxy remains responsible for credential selection, auth, retries, usage logging, and response delivery; it only asks the bridge to rewrite request and response payloads when the inbound path is Responses and the selected API dialect is `openai`.

**Tech Stack:** Rust 2021, serde_json, Axum HTTP types, existing reqwest route proxy, built-in Rust unit/integration tests.

## Global Constraints

- Work directly on `main`; do not create a branch or worktree.
- Preserve all existing uncommitted changes, especially unrelated frontend edits.
- Do not create a git commit unless the user explicitly asks.
- Enable conversion only for Codex Responses entry requests routed to API credentials with `interface_format = "openai"`.
- Keep direct `openai-responses`, Anthropic, Gemini, official-account, models-list, and Responses compact behavior unchanged.
- Do not add dependencies unless the standard library and existing crates cannot express the conversion.

---

### Task 1: Define Bridge Selection and Request Conversion

**Files:**
- Create: `src-tauri/src/services/route_protocol_bridge/mod.rs`
- Create: `src-tauri/src/services/route_protocol_bridge/responses_chat.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Test: `src-tauri/src/services/route_protocol_bridge/responses_chat.rs`

**Interfaces:**
- Produces `ProtocolBridgeKind::ResponsesToChat`.
- Produces `prepare_request(platform, upstream_dialect, path, body) -> Result<PreparedBridgeRequest, String>`.
- Produces `PreparedBridgeRequest { kind: Option<ProtocolBridgeKind>, upstream_path: String, body: Vec<u8>, streaming: bool }`.
- Produces `transform_response(kind, status, headers, body) -> Result<TransformedBridgeResponse, String>` in Task 2.

- [x] **Step 1: Write failing bridge-selection tests**

Cover `/responses`, `/v1/responses`, and repeated leading version segments for `PlatformId::Codex + ApiDialect::OpenAi`. Assert no bridge for Chat entry paths, `OpenAiResponses`, official/non-API callers, Claude, or `/responses/compact`.

- [x] **Step 2: Run focused tests and confirm failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml route_protocol_bridge -- --nocapture`

Expected: FAIL because the new module and interfaces do not exist.

- [x] **Step 3: Implement request classification**

Normalize the entry path without losing endpoint identity. Return `/chat/completions` only for the supported bridge and preserve the normalized original path otherwise.

- [x] **Step 4: Write failing request-conversion tests**

Use concrete fixtures covering:

```json
{
  "model": "gpt-5",
  "instructions": "Be concise",
  "input": "Hello",
  "max_output_tokens": 128,
  "stream": true,
  "tools": [{
    "type": "function",
    "name": "lookup",
    "description": "Lookup a value",
    "parameters": {"type":"object","properties":{"key":{"type":"string"}},"required":["key"]}
  }]
}
```

Assert a leading system message, user message, Chat function tool, `max_tokens`, and `stream`. Add o-series coverage asserting `max_completion_tokens` instead of `max_tokens`. Add multi-turn fixtures for Responses message items, `function_call`, and `function_call_output`.

- [x] **Step 5: Implement minimal request conversion**

Parse JSON, reject unsupported top-level shapes with a descriptive conversion error, map lossless fields, and preserve common controls. Keep model mapping outside this module by accepting the already-mapped body.

- [x] **Step 6: Run request bridge tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml route_protocol_bridge -- --nocapture`

Expected: PASS for selection and request conversion tests.

### Task 2: Convert Chat JSON and SSE to Responses

**Files:**
- Modify: `src-tauri/src/services/route_protocol_bridge/mod.rs`
- Modify: `src-tauri/src/services/route_protocol_bridge/responses_chat.rs`
- Test: `src-tauri/src/services/route_protocol_bridge/responses_chat.rs`

**Interfaces:**
- Consumes `ProtocolBridgeKind::ResponsesToChat` and `PreparedBridgeRequest.streaming`.
- Produces `TransformedBridgeResponse { body: Vec<u8>, content_type: Option<&'static str> }`.

- [x] **Step 1: Write failing non-stream response tests**

Cover assistant text, multiple content parts, tool calls, finish reasons, model/id propagation, and usage mapping from `prompt_tokens`, `completion_tokens`, cached tokens, and reasoning tokens. Assert the result contains `object = "response"`, `status`, `output`, `output_text`, and normalized usage.

- [x] **Step 2: Implement non-stream response conversion**

Map Chat assistant content to a Responses message output item and Chat tool calls to Responses `function_call` output items. Use stable generated fallback IDs only when upstream IDs are absent. Preserve upstream errors unchanged when the status is not successful.

- [x] **Step 3: Write failing SSE conversion tests**

Use representative buffered SSE containing:

```text
data: {"id":"chatcmpl-1","model":"deepseek-chat","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}

data: {"id":"chatcmpl-1","model":"deepseek-chat","choices":[{"index":0,"delta":{"content":"hello"},"finish_reason":null}]}

data: {"id":"chatcmpl-1","model":"deepseek-chat","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":1,"total_tokens":4}}

data: [DONE]
```

Assert Responses lifecycle events, text delta/done events, completion usage, and no invalid JSON. Add interleaved tool-call delta coverage and an upstream error event fixture.

- [x] **Step 4: Implement buffered SSE state machine**

Track response ID/model, output indexes, accumulated text, tool-call IDs/names/arguments, finish reason, and usage. Emit `response.created`, `response.in_progress`, output item/content part start and delta events, item completion events, and exactly one terminal `response.completed` or `response.failed` event.

- [x] **Step 5: Run all bridge tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml route_protocol_bridge -- --nocapture`

Expected: PASS for JSON and SSE conversion.

### Task 3: Integrate the Bridge into Route Proxy

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs`
- Test: `src-tauri/src/services/route_proxy_service.rs`

**Interfaces:**
- Consumes `prepare_request` before URL construction.
- Consumes `transform_response` after the upstream body is read.
- Leaves existing auth, retry, quota, usage, custom-tool compatibility, and trace interfaces intact.

- [x] **Step 1: Write failing request-build integration test**

Create a Codex API credential with `interface_format = "openai"`, call `build_upstream_request` with `/v1/responses`, and assert target URL `/v1/chat/completions` plus a Chat request body. Assert a credential with `openai-responses` still targets `/v1/responses` unchanged.

- [x] **Step 2: Extend the built request metadata**

Return the bridge kind and streaming flag alongside URL, headers, and body through a focused internal request struct, or add a separate helper that allows `forward_request` to retain bridge metadata without changing public command DTOs.

- [x] **Step 3: Apply response conversion before analysis**

After reading the upstream bytes, transform successful bridged responses before custom-tool restoration, quota parsing, semantic-failure detection, usage extraction, trace recording, and proxy response construction. Replace content type with `application/json` or `text/event-stream` when the bridge produces one and remove stale content length.

- [x] **Step 4: Add route-level proxy test**

Start a fixed local Chat upstream that records the request and returns either Chat JSON or Chat SSE. Route a Codex `/v1/responses` request through an `openai` credential and assert the upstream receives `/v1/chat/completions` while the client receives Responses-compatible output.

- [x] **Step 5: Run focused proxy tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml route_proxy_service -- --nocapture`

Expected: PASS, including existing auth, retry, path, quota, and custom-tool tests.

### Task 4: Verify Config Authentication and Document Scope

**Files:**
- Modify only if tests expose defects: `src-tauri/src/adapters/route_config/codex.rs`
- Modify only if tests expose defects: `src-tauri/src/adapters/route_config/mod.rs`
- Modify: `README.md`
- Test: existing route-config tests

**Interfaces:**
- Keeps generated Codex `wire_api = "responses"`.
- Keeps the local route proxy key in a Codex-supported bearer-token field so it reaches `Authorization: Bearer`.

- [x] **Step 1: Run focused route-config tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml codex_render route_config_service -- --nocapture`

Expected: PASS with `experimental_bearer_token` and no managed `api_key` field.

- [x] **Step 2: Document protocol routing behavior**

Add a concise README note: Codex always points locally at Responses; AI Switch translates to Chat only when the selected API account is configured as OpenAI Chat. Note that Claude Messages conversion is planned separately and that CC-Switch's Codex Responses route is passthrough.

- [x] **Step 3: Format and run focused verification**

Run:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml route_protocol_bridge -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml route_proxy_service -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml codex_render route_config_service -- --nocapture
```

- [x] **Step 4: Run broader Rust validation**

Run: `pnpm rust:test`

Expected: PASS. If unrelated pre-existing failures occur, report them without modifying unrelated code.

- [x] **Step 5: Inspect the working tree**

Run: `git diff --check` and `git status --short`. Confirm existing frontend edits are preserved and protocol work is limited to the planned files.
