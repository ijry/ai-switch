# AI Switch Provider Switching B2.3 Gemini CLI Real Adapter Implementation Plan

> **For agentic workers:** Use the existing provider-switching B2 pattern. Keep checkboxes updated as implementation and verification progress.

**Goal:** Add explicit real provider switching for Gemini CLI by writing Gemini settings JSON through backend path resolution, atomic backup-aware writes, config snapshots, target state updates, Providers UI, tray switching, and rollback-compatible metadata.

**Architecture:** B2.3 adds `adapters::gemini_config` and extends provider switch real-mode dispatch from Codex/OpenCode to Codex/Gemini CLI/OpenCode. The frontend keeps sandbox switching for every target and shows real config buttons only for targets with implemented real adapters.

## Constraints

- B2.3 must keep sandbox switching unchanged.
- B2.3 must keep Codex and OpenCode real switching unchanged.
- B2.3 must accept `mode = "real"` for `codex`, `gemini_cli`, and `opencode`.
- B2.3 must not write raw API keys, resolved secrets, or `secret_ref` into Gemini settings.
- B2.3 must resolve real Gemini settings paths in the backend only.
- B2.3 must write Gemini settings through `ConfigWriter::write_atomic_with_backup`.
- B2.3 must preserve unrelated existing Gemini settings JSON.
- B2.3 must not add a schema migration.

## Tasks

- [x] Add `src-tauri/src/adapters/gemini_config.rs`.
- [x] Implement Gemini settings path resolution with `GEMINI_CLI_SETTINGS` override and `~/.gemini/settings.json` default.
- [x] Implement Gemini JSON parse, merge, and render logic.
- [x] Add adapter tests for path resolution, JSON preservation, no secret output, target-specific model/env metadata, malformed config rejection, malformed target options rejection, and missing model id.
- [x] Register `gemini_config` in `src-tauri/src/adapters/mod.rs`.
- [x] Extend provider switch real-mode dispatch to select Gemini by `target.key = "gemini_cli"`.
- [x] Add test-only Gemini settings path injection.
- [x] Add backend tests for Gemini real success and failure snapshot/state recording.
- [x] Ensure Gemini real writes use backup-aware atomic writes for rollback compatibility.
- [x] Update Providers UI to show dynamic real config buttons for Codex, Gemini CLI, and OpenCode.
- [x] Add Providers UI test for Gemini CLI real action.
- [x] Add Gemini CLI to tray real-target filtering and tray item-count test.
- [x] Add README Gemini real-mode smoke notes.
- [x] Update README tray and rollback notes to include Gemini CLI.
- [x] Run `cargo fmt`.
- [x] Run `cargo test`.
- [x] Run `pnpm typecheck`.
- [x] Run `pnpm test:run`.

## Verification Commands

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
pnpm typecheck
pnpm test:run
```

## Safe Smoke

Use a temporary Gemini CLI settings path when manually smoking real mode:

```powershell
$env:GEMINI_CLI_SETTINGS = Join-Path $env:TEMP "ai-switch-gemini-smoke\settings.json"
pnpm tauri:dev
```

Expected:

- Providers screen shows `Switch Gemini CLI config` only when Gemini CLI is selected.
- Clicking it writes the temporary `GEMINI_CLI_SETTINGS` path.
- The file contains `model.name` and `aiSwitch.activeProvider`.
- No raw secret or `secret_ref` is written.
- Targets screen exposes rollback for the successful real switch snapshot.
