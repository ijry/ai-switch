# CC-Switch Deep-Link Account Import Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Support `ccswitch://` and `aiswitch://` deep links that open a confirmation dialog and create one API route credential after the user confirms.

**Architecture:** Add a pure Rust parser/mapper that turns provider deep-link URLs into `CreateApiRouteCredentialInput`. Wire desktop cold/warm start through `tauri-plugin-deep-link` and `tauri-plugin-single-instance`, emit frontend events, and reuse the existing create-account path from a confirmation dialog mounted in `App`.

**Tech Stack:** Rust 2021, Tauri 2, url crate, tauri-plugin-deep-link, tauri-plugin-single-instance, React 18, TypeScript, existing transport subscribe API.

## Global Constraints

- Work directly on `main`. Do not create branches/worktrees unless the user asks.
- v1 supports only `resource=provider` API account create.
- Do not auto-join the route pool.
- Do not implement official CPA/OAuth deep-link import, MCP/prompt/skill, or config merge.
- Logs/toasts must never show full API keys; use masked values.
- Do not commit unrelated dirty files currently in the worktree:
  - settings/web/sessions/tailscale/transport files
  - `.codex-run/`
  - `src-tauri/src/bin/proxy_smoke.rs`
  - `src-tauri/target-codex-test/`
- Reuse `RouteCredentialService::create_api` / `createApiRouteCredential`. Do not invent a second create path.

---

## File Structure

- Create `src-tauri/src/services/deeplink_service.rs`: pure URL parse + mapping + unit tests.
- Modify `src-tauri/src/services/mod.rs`: export `deeplink_service`.
- Modify `src-tauri/Cargo.toml`: add `url`, `tauri-plugin-deep-link`, `tauri-plugin-single-instance`.
- Modify `src-tauri/tauri.conf.json`: register schemes `ccswitch`, `aiswitch`.
- Modify `src-tauri/src/lib.rs`: single-instance + deep-link plugin wiring, cold/warm handlers, emit events.
- Create `src/components/deeplink/DeepLinkImportDialog.tsx`: confirmation dialog + event listeners.
- Modify `src/App.tsx`: mount dialog and navigate to platform screen on success.

### Task 1: Pure parser and mapper

**Files:**
- Create: `src-tauri/src/services/deeplink_service.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Modify: `src-tauri/Cargo.toml` (add `url = "2"` if missing)

**Interfaces:**
- Consumes: `CreateApiRouteCredentialInput`, `ModelMapping` from `crate::models::route_credential`.
- Produces:
  - `DeepLinkProviderImport`
  - `DeepLinkErrorPayload`
  - `parse_deeplink_url(url: &str) -> Result<DeepLinkProviderImport, String>`
  - `to_create_api_input(parsed: &DeepLinkProviderImport) -> CreateApiRouteCredentialInput`
  - `mask_api_key(api_key: &str) -> String`
  - `sanitize_source_url(url: &str) -> String`

- [ ] **Step 1: Add unit tests and service skeleton**

Create `src-tauri/src/services/deeplink_service.rs` with types, helpers, and tests covering:
- Claude provider with `sonnetModel`
- `aiswitch` Codex with `model`
- `grok` / `xai` aliases
- first valid endpoint from CSV
- empty model fields => `[]`
- reject bad scheme/version/path/resource/app/endpoint/apiKey
- `to_create_api_input` field mapping

Exact mapping rules from approved design:
- Claude: `haikuModel` -> from `claude-haiku-4-5` label Haiku; `sonnetModel` -> `claude-sonnet-5` Sonnet; `opusModel` -> `claude-opus-4-8` Opus
- Other platforms with `model`:
  - codex from `gpt-5`
  - gemini from `gemini-2.5-flash`
  - grok from `grok-3`
- Platform/interface:
  - claude/anthropic
  - codex/openai-responses
  - gemini/gemini
  - grok|xai / openai
  - opencode/openclaw unsupported

Append `pub mod deeplink_service;` to `src-tauri/src/services/mod.rs`.

- [ ] **Step 2: Run tests, confirm fail before full implementation if using TDD stubs**

```powershell
cd src-tauri
cargo test deeplink_service -- --nocapture
```

- [ ] **Step 3: Implement parser/mapper**

`parse_deeplink_url` must:
1. Parse URL
2. scheme in {ccswitch, aiswitch}
3. host == v1
4. path == /import
5. resource == provider else error containing `暂不支持`
6. require app, name, endpoint, apiKey
7. first valid http(s) endpoint from comma-separated list
8. build model mappings JSON as above
9. mask api key and sanitize source URL (redact apiKey query)

`to_create_api_input` returns `CreateApiRouteCredentialInput` with platform/display_name/api_key/base_url/interface_format/model_mappings_json and no api_key_field/preview/batch.

- [ ] **Step 4: Re-run unit tests**

```powershell
cd src-tauri
cargo test deeplink_service -- --nocapture
```

Expected: PASS

- [ ] **Step 5: Commit parser only**

```powershell
git add src-tauri/src/services/deeplink_service.rs src-tauri/src/services/mod.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat: parse ccswitch/aiswitch provider deep links"
```

### Task 2: Desktop deep-link runtime wiring

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `parse_deeplink_url`, `DeepLinkErrorPayload`
- Produces events:
  - `deeplink-import` => `DeepLinkProviderImport`
  - `deeplink-error` => `{ message, source }`

- [ ] **Step 1: Add deps**

```toml
url = "2"
tauri-plugin-deep-link = "2"
tauri-plugin-single-instance = "2"
```

- [ ] **Step 2: Register schemes in tauri.conf.json plugins**

```json
"deep-link": {
  "desktop": {
    "schemes": ["ccswitch", "aiswitch"]
  }
}
```

- [ ] **Step 3: Wire plugins/handlers in lib.rs**

Add helpers:
- `is_deeplink_url`
- `focus_main_window` via `get_webview_window("main")`
- `handle_deeplink_url` parse + emit + focus

Wire:
- `tauri-plugin-single-instance` on desktop OS: forward argv deep links and focus window
- `tauri-plugin-deep-link::init()`
- setup: `register_all` on linux and windows debug
- setup: `on_open_url` callback
- setup: cold-start `std::env::args()` scan

Do not alter existing invoke handler list.

- [ ] **Step 4: Compile check**

```powershell
cd src-tauri
cargo check
cargo test deeplink_service -- --nocapture
```

- [ ] **Step 5: Commit runtime**

```powershell
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json src-tauri/src/lib.rs
git commit -m "feat: wire desktop deep-link and single-instance import events"
```

### Task 3: Confirmation dialog and App integration

**Files:**
- Create: `src/components/deeplink/DeepLinkImportDialog.tsx`
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `getTransport().subscribe`, `createApiRouteCredential`, `agentScreenByPlatform`
- Props: `onImported(platform: string)`

- [ ] **Step 1: Implement dialog**

Dialog requirements:
- Subscribe only on desktop
- Listen `deeplink-import` and `deeplink-error`
- Show: type API账号, platform, name, base URL, masked key, mapping summary, scheme
- Actions: 取消 / 确认导入
- Confirm calls `createApiRouteCredential` with existing input shape
- Create failure keeps dialog open with error text
- Cancel drops in-memory payload
- Never display full api key in UI beyond masked field already provided

- [ ] **Step 2: Mount in App**

- Import dialog
- On success, `setScreen(agentScreenByPlatform[platform])` when known
- Mount dialog under providers so it works outside Vibe and during account screens
- Rely on AccountsScreen mount refetch; no pool membership changes

- [ ] **Step 3: Typecheck**

```powershell
pnpm typecheck
```

- [ ] **Step 4: Commit frontend**

```powershell
git add src/components/deeplink/DeepLinkImportDialog.tsx src/App.tsx
git commit -m "feat: confirm deep-link provider imports in desktop UI"
```

### Task 4: Verification

- [ ] **Step 1: Backend tests + check**

```powershell
cd src-tauri
cargo test deeplink_service -- --nocapture
cargo check
```

- [ ] **Step 2: Manual checklist if desktop can launch**

1. Cold start aiswitch codex link
2. Warm start claude link
3. Confirm creates account and focuses platform
4. Cancel no write
5. Invalid link shows error

If desktop launch is impractical, note manual OS scheme verification pending and rely on unit tests + compile.

---

## Self-Review

1. Spec coverage: dual scheme, confirm-before-import, provider-only, no pool join, model maps, cold/warm start, error UX covered.
2. No TBD placeholders.
3. Event names and snake_case payload fields match Rust serde; create path reuses existing API.
