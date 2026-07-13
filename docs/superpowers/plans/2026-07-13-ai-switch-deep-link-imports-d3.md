# AI Switch Deep-Link Imports D3 Implementation Plan

**Goal:** Add pasted deep-link import support for existing import formats.

**Architecture:** Add a parser inside `ImportService` that validates `ai-switch://import/...` links, decodes a base64url JSON payload, and dispatches to existing `example_json` and `official_account_json` import methods. Expose it through a Tauri command and Imports screen panel.

## Guardrails

- D3 must not register an OS protocol handler.
- D3 must not open external links or perform network calls.
- D3 must not execute imported content.
- D3 must preserve existing import guardrails for raw credentials.

## Steps

- [x] Add deep-link request type and parser.
- [x] Add service dispatch to existing import methods.
- [x] Add Tauri command and invoke registration.
- [x] Add Rust tests for supported links and malformed links.
- [x] Add frontend API types/functions.
- [x] Add Imports screen deep-link panel.
- [x] Add frontend/API tests.
- [x] Update README D3 notes.
- [x] Run `cargo fmt`, `pnpm typecheck`, `pnpm test:run`, `pnpm rust:check`, and `pnpm rust:test`.
