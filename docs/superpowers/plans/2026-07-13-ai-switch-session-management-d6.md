# AI Switch Session Management D6 Implementation Plan

**Goal:** Add local session records and event notes for grouping AI tool context.

**Architecture:** Add session tables plus model, repository, service, commands, TypeScript API, and a `Sessions` screen. The backend validates status values and JSON shapes before persisting records.

## Guardrails

- D6 must not launch target apps or external processes.
- D6 must not write target app configs.
- D6 must not perform network calls.
- D6 must not capture transcripts automatically.
- D6 must not store raw secret values in event metadata.

## Steps

- [x] Add session migration.
- [x] Add session models.
- [x] Add session repository and Rust repository tests.
- [x] Add session service validation and Rust service tests.
- [x] Add Tauri commands and command registration.
- [x] Add TypeScript API types/functions and API tests.
- [x] Add `Sessions` screen and navigation entry.
- [x] Add Sessions screen tests.
- [x] Update README D6 notes.
- [x] Run `cargo fmt`, `pnpm typecheck`, `pnpm test:run`, `pnpm rust:check`, and `pnpm rust:test`.
