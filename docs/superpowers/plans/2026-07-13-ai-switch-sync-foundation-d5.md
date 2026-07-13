# AI Switch Sync Foundation D5 Implementation Plan

**Goal:** Add safe cloud sync foundation records and local snapshot manifests.

**Architecture:** Add sync profile and sync snapshot tables, a sync service that validates metadata and computes local item counts, Tauri commands, TypeScript API wrappers, and a `Sync` screen. D5 never contacts remote services and never stores raw credentials.

## Guardrails

- D5 must not perform network calls.
- D5 must not upload or download files.
- D5 must not store raw sync credentials.
- D5 must not run background sync tasks.
- D5 must not implement conflict resolution yet.

## Steps

- [x] Add sync migration.
- [x] Add sync models.
- [x] Add sync repository and Rust repository tests.
- [x] Add sync service validation, snapshot count generation, and Rust service tests.
- [x] Add Tauri commands and command registration.
- [x] Add TypeScript API types/functions and API tests.
- [x] Add `Sync` screen and navigation entry.
- [x] Add Sync screen tests.
- [x] Update README D5 notes.
- [x] Run `cargo fmt`, `pnpm typecheck`, `pnpm test:run`, `pnpm rust:check`, and `pnpm rust:test`.
