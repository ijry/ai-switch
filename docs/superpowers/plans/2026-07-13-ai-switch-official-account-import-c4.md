# AI Switch Official Account Import C4 Implementation Plan

**Goal:** Add metadata-only official account bundle import for Codex, Claude, and Gemini.

**Architecture:** Add an importer module that parses a platform-scoped account bundle into `NewOfficialAccount` rows, validates credential safety, records an import job through `ImportService`, and exposes the flow in the Imports screen.

## Guardrails

- C4 must not store raw tokens, passwords, API keys, or secrets.
- C4 must not parse real app credential stores.
- C4 must not perform network calls.
- C4 must not add a schema migration.
- C4 must preserve existing `example_json` import/export behavior.

## Steps

- [x] Add `official_account_json` importer parser.
- [x] Add import service request and command.
- [x] Add Rust tests for successful import and credential-key rejection.
- [x] Add frontend API types/functions.
- [x] Add Imports screen account import panel.
- [x] Add frontend/API tests.
- [x] Update README C4 notes.
- [x] Run `cargo fmt`, `pnpm typecheck`, `pnpm test:run`, `pnpm rust:check`, and `pnpm rust:test`.
