# AI Switch Official Account Quota C2 Implementation Plan

**Goal:** Add quota snapshot cache plumbing for official accounts without real network quota lookup.

**Architecture:** Introduce quota snapshot model/repository helpers, an account service for account statuses and manual snapshot recording, Tauri commands, and Accounts screen UI for cached quota display.

## Guardrails

- C2 must not perform external network calls.
- C2 must not implement OAuth or token refresh.
- C2 must not store raw secrets.
- C2 must not add a schema migration.
- C2 must preserve C1 account creation and batch grouping.

## Steps

- [x] Add quota snapshot models and repository.
- [x] Add account status and quota-recording service.
- [x] Add Tauri commands for account statuses and quota snapshot recording.
- [x] Add frontend types/API functions.
- [x] Update Accounts UI to display cached quota and record manual snapshots.
- [x] Add Rust tests.
- [x] Add frontend/API tests.
- [x] Update README C2 notes.
- [x] Run `cargo fmt`, `pnpm typecheck`, `pnpm test:run`, `pnpm rust:check`, and `pnpm rust:test`.
