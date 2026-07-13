# AI Switch Provider Switching B2.2 OpenCode Real Adapter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans or equivalent task tracking. Keep checkboxes updated as implementation and verification progress.

**Goal:** Add explicit real provider switching for OpenCode by writing documented OpenCode JSON config through the existing backend path resolution, atomic writer, snapshot, and target-state pipeline.

**Architecture:** B2.2 adds `adapters::opencode_config` and extends provider switch real-mode dispatch from Codex-only to Codex/OpenCode. The frontend keeps sandbox switching for every target and shows real config buttons only for targets with implemented real adapters.

**Tech Stack:** Tauri 2, React 18, TypeScript, Vite, Vitest, Testing Library, Rust, sqlx SQLite, serde, serde_json, chrono, uuid, tokio.

## Global Constraints

- B2.2 must keep sandbox switching unchanged.
- B2.2 must keep Codex real switching unchanged.
- B2.2 must accept `mode = "real"` for `codex` and `opencode` only.
- B2.2 must not write real configs for Claude Code, Claude Desktop, Gemini CLI, OpenClaw, or Hermes.
- B2.2 must resolve real OpenCode config paths in the backend only.
- B2.2 must write OpenCode config through `ConfigWriter`.
- B2.2 must not write raw API keys, resolved secrets, or `secret_ref` into OpenCode config.
- B2.2 may parse JSONC but serializes standard pretty JSON.

## Tasks

- [x] Confirm public OpenCode config facts: global path, `OPENCODE_CONFIG`, JSON/JSONC support, `model`, `provider`, `options.baseURL`, `options.apiKey`, and `{env:VAR}` substitution.
- [x] Add `src-tauri/src/adapters/opencode_config.rs`.
- [x] Implement OpenCode path resolution with injectable test helper.
- [x] Implement OpenCode JSON/JSONC parse, merge, and render logic.
- [x] Add adapter tests for path resolution, JSONC preservation of unrelated keys, no secret output, malformed config rejection, missing `base_url`, and missing model id.
- [x] Register `opencode_config` in `src-tauri/src/adapters/mod.rs`.
- [x] Refactor provider switch real-mode dispatch to select real adapters by `target.key`.
- [x] Add test-only OpenCode config path injection.
- [x] Add backend tests for OpenCode real success and failure snapshot/state recording.
- [x] Update Providers UI to show dynamic real config buttons for Codex and OpenCode.
- [x] Add frontend fixture target for OpenCode.
- [x] Add Providers UI test for OpenCode real action and unsupported-target hiding.
- [x] Add README OpenCode real-mode smoke notes.
- [x] Run `cargo fmt`.
- [x] Run `pnpm test:run`.
- [x] Run `pnpm typecheck`.
- [x] Run `pnpm rust:check`.
- [x] Run `pnpm rust:test`.
- [x] Update this plan with verification results.

## Verification Commands

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml
pnpm test:run
pnpm typecheck
pnpm rust:check
pnpm rust:test
```

## Safe Smoke

Use a temporary OpenCode config path when manually smoking real mode:

```powershell
$env:OPENCODE_CONFIG = Join-Path $env:TEMP "ai-switch-opencode-smoke\opencode.json"
pnpm tauri:dev
```

Expected:

- Providers screen shows `Switch OpenCode config` only when OpenCode is selected.
- Clicking it writes the temporary `OPENCODE_CONFIG` path.
- The file contains `$schema`, `model`, `provider.<ai-switch-id>.options.baseURL`, and `provider.<ai-switch-id>.options.apiKey`.
- The API key value is `{env:...}` and no raw secret or `secret_ref` is written.
