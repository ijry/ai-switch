# AI Switch Batch Quota Health C3 Implementation Plan

**Goal:** Make batch health reflect cached official account quota state.

**Architecture:** Extend `BatchRepository::children_for_batch` to left-join account quota snapshots and derive account child status from account status plus quota snapshot status.

## Guardrails

- C3 must not add a schema migration.
- C3 must not call external quota APIs.
- C3 must keep provider child health unchanged.
- C3 must keep the existing batch list UI structure.

## Steps

- [x] Join `quota_snapshots` in batch child queries.
- [x] Derive quota-aware official account child status.
- [x] Update repository tests for warning/error batch health.
- [x] Update frontend fixtures if needed.
- [x] Update README C3 notes.
- [x] Run `cargo fmt`, `pnpm typecheck`, `pnpm test:run`, `pnpm rust:check`, and `pnpm rust:test`.
