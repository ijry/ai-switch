# AI Switch Provider Switching B4 Tray Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans or equivalent task tracking. Keep checkboxes updated as implementation and verification progress.

**Goal:** Add a system tray menu for quick provider switching while preserving existing provider switch behavior.

**Architecture:** B4 builds a Tauri tray menu from current providers and targets, routes menu actions through `ProviderSwitchService`, exposes a `refresh_tray_menu` command, and documents manual smoke coverage.

## Constraints

- No schema migration.
- No new provider switch semantics.
- No tray rollback action.
- No real-mode actions for targets without implemented real adapters.
- No raw secret storage or secret resolution.

## Tasks

- [x] Add tray status model.
- [x] Add tray setup during app startup.
- [x] Build tray menu with open, provider switch, refresh, and quit actions.
- [x] Generate sandbox switch actions for enabled targets.
- [x] Generate real switch actions for Codex, Gemini CLI, and OpenCode.
- [x] Parse tray switch menu ids into typed actions.
- [x] Route tray switch actions through `ProviderSwitchService`.
- [x] Refresh tray menu after tray switch actions.
- [x] Add Tauri command `refresh_tray_menu`.
- [x] Register tray command in the invoke handler.
- [x] Add TypeScript API type and client wrapper.
- [x] Add API client test for tray refresh.
- [x] Add Rust tray parsing and switch-count tests.
- [x] Add README B4 notes.
- [x] Run `cargo fmt`.
- [x] Run `pnpm typecheck`.
- [x] Run `pnpm test:run`.
- [x] Run `pnpm rust:check`.
- [x] Run `pnpm rust:test`.

## Verification Commands

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml
pnpm typecheck
pnpm test:run
pnpm rust:check
pnpm rust:test
```

## Manual Smoke

1. Start the app with `pnpm tauri:dev`.
2. Create or import at least one provider.
3. Use the tray menu to refresh entries.
4. Choose a sandbox target switch from the tray.
5. Open `Targets` and confirm the target state updated.
6. For real mode, set temporary `CODEX_HOME`, `GEMINI_CLI_SETTINGS`, or `OPENCODE_CONFIG` paths before choosing real config actions.
