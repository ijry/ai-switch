# AI Switch IDE Account Importers E4 Implementation Plan

**Goal:** Extend metadata-only official account JSON import to more IDE account
platforms.

**Architecture:** Reuse the C4 official account JSON importer and import
service. Expand platform normalization, TypeScript platform types, the Imports
screen selector, tests, and documentation. E4 does not read real credential
stores or perform OAuth.

## Guardrails

- E4 must not read IDE credential stores.
- E4 must not extract tokens or passwords.
- E4 must not perform OAuth or token refresh.
- E4 must not make network calls.
- E4 must not store raw secret values.

## Steps

- [x] Add `cursor`, `windsurf`, `zed`, and `vscode` to backend platform normalization.
- [x] Add Rust import service test coverage for a new IDE platform.
- [x] Update TypeScript import request platform type.
- [x] Update Imports screen platform selector.
- [x] Update Imports screen tests for a new IDE platform.
- [x] Update README E4 notes.
- [x] Run `cargo fmt`, `pnpm typecheck`, `pnpm test:run`, `pnpm rust:check`, and `pnpm rust:test`.
