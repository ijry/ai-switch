# AI Switch Wakeup Tasks E2 Implementation Plan

**Goal:** Add safe local wakeup task definitions and manual run records.

**Architecture:** Add wakeup task/run tables plus model, repository, service,
commands, TypeScript API, and a `Wakeups` screen. E2 validates JSON and secret
references but never schedules jobs or starts external processes.

## Guardrails

- E2 must not launch external processes.
- E2 must not monitor PIDs.
- E2 must not call OS wake or task scheduler APIs.
- E2 must not write target app configs.
- E2 must not store raw secret values.

## Steps

- [x] Add wakeup task/run migration.
- [x] Add wakeup models.
- [x] Add wakeup repository and Rust repository tests.
- [x] Add wakeup service validation and Rust service tests.
- [x] Add Tauri commands and command registration.
- [x] Add TypeScript API types/functions and API tests.
- [x] Add `Wakeups` screen and navigation entry.
- [x] Add Wakeups screen tests.
- [x] Update README E2 notes.
- [x] Run `cargo fmt`, `pnpm typecheck`, `pnpm test:run`, `pnpm rust:check`, and `pnpm rust:test`.
