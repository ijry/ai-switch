# AI Switch Updater Foundation D7 Implementation Plan

**Goal:** Add safe updater metadata records and manual update check results.

**Architecture:** Add update channel and update check tables plus model, repository, service, commands, TypeScript API, and an `Updates` screen. D7 validates metadata only and never performs network or installer actions.

## Guardrails

- D7 must not perform network calls.
- D7 must not download update packages.
- D7 must not execute installers.
- D7 must not modify the running app.
- D7 must not claim automatic update support.

## Steps

- [x] Add updater migration.
- [x] Add updater models.
- [x] Add updater repository and Rust repository tests.
- [x] Add updater service validation and Rust service tests.
- [x] Add Tauri commands and command registration.
- [x] Add TypeScript API types/functions and API tests.
- [x] Add `Updates` screen and navigation entry.
- [x] Add Updates screen tests.
- [x] Update README D7 notes.
- [x] Run `cargo fmt`, `pnpm typecheck`, `pnpm test:run`, `pnpm rust:check`, and `pnpm rust:test`.
