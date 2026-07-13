# AI Switch Bulk Tags Plugins E3 Implementation Plan

**Goal:** Add safe local metadata for bulk operations, tags, and plugin links.

**Architecture:** Add tags, item-tag assignments, plugin links, and bulk
operation tables plus models, repository, service, commands, TypeScript API,
and a `Bulk` screen. E3 validates JSON and secret references but never executes
plugins or bulk actions.

## Guardrails

- E3 must not execute plugins.
- E3 must not launch external processes.
- E3 must not mutate target configs or external files.
- E3 must not perform network calls.
- E3 must not store raw secret values.

## Steps

- [x] Add tags, item tags, plugin links, and bulk operation migration.
- [x] Add E3 models.
- [x] Add E3 repository and Rust repository tests.
- [x] Add E3 service validation and Rust service tests.
- [x] Add Tauri commands and command registration.
- [x] Add TypeScript API types/functions and API tests.
- [x] Add `Bulk` screen and navigation entry.
- [x] Add Bulk screen tests.
- [x] Update README E3 notes.
- [x] Run `cargo fmt`, `pnpm typecheck`, `pnpm test:run`, `pnpm rust:check`, and `pnpm rust:test`.
