# AI Switch Routing And Usage D4 Implementation Plan

**Goal:** Add safe local routing metadata for proxy profiles, failover policies, and manual usage events.

**Architecture:** Add one migration and a focused routing module across model, repository, service, commands, TypeScript API, and a `Routing` screen. The backend validates URL schemes, JSON shapes, secret references, and non-negative usage amounts before writing records.

## Guardrails

- D4 must not start a proxy process or bind ports.
- D4 must not execute failover automatically.
- D4 must not make network calls.
- D4 must not sync data to cloud services.
- D4 must not store raw proxy credentials.

## Steps

- [x] Add routing/usage migration.
- [x] Add routing models.
- [x] Add routing repository and Rust repository tests.
- [x] Add routing service validation and Rust service tests.
- [x] Add Tauri commands and command registration.
- [x] Add TypeScript API types/functions and API tests.
- [x] Add `Routing` screen and navigation entry.
- [x] Add Routing screen tests.
- [x] Update README D4 notes.
- [x] Run `cargo fmt`, `pnpm typecheck`, `pnpm test:run`, `pnpm rust:check`, and `pnpm rust:test`.
