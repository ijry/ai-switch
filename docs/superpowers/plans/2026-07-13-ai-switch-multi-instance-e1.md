# AI Switch Multi-Instance Management E1 Implementation Plan

**Goal:** Add safe local multi-instance configuration and status records.

**Architecture:** Add managed instance tables plus model, repository, service, commands, TypeScript API, and an `Instances` screen. E1 validates JSON and secret references but never starts external processes.

## Guardrails

- E1 must not launch external processes.
- E1 must not monitor PIDs.
- E1 must not wake tasks.
- E1 must not write target app configs.
- E1 must not store raw secret environment values.

## Steps

- [x] Add managed instance migration.
- [x] Add managed instance models.
- [x] Add managed instance repository and Rust repository tests.
- [x] Add managed instance service validation and Rust service tests.
- [x] Add Tauri commands and command registration.
- [x] Add TypeScript API types/functions and API tests.
- [x] Add `Instances` screen and navigation entry.
- [x] Add Instances screen tests.
- [x] Update README E1 notes.
- [x] Run `cargo fmt`, `pnpm typecheck`, `pnpm test:run`, `pnpm rust:check`, and `pnpm rust:test`.
