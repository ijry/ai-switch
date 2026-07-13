# AI Switch Official Accounts C1 Implementation Plan

**Goal:** Replace the placeholder Accounts screen with metadata-only official account list/create support.

**Architecture:** Reuse the existing official account schema, repository, and `create_official_account` command. Add a list command and a React screen backed by typed API client calls.

## Guardrails

- C1 must not store raw tokens or API keys.
- C1 must not implement OAuth, token refresh, or quota network calls.
- C1 must not add a schema migration.
- C1 must preserve batch grouping behavior through the existing create command.

## Steps

- [x] Add `list_official_accounts` Tauri command.
- [x] Add API client/types for list/create official account.
- [x] Replace Accounts placeholder with list/create UI.
- [x] Add account fixture and Accounts screen tests.
- [x] Extend API client tests.
- [x] Add backend list command/repository coverage if needed.
- [x] Update README C1 notes.
- [x] Run `cargo fmt`, `pnpm typecheck`, `pnpm test:run`, `pnpm rust:check`, and `pnpm rust:test`.
