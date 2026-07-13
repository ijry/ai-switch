# AI Switch Prompts And Skills D2 Implementation Plan

**Goal:** Add local prompt and skill library management.

**Architecture:** Add a `prompt_assets` migration, model, repository, service, Tauri commands, TypeScript API wrappers, and a Library screen. D2 manages records only; future phases can export or render them into target-specific configs.

## Guardrails

- D2 must not execute skills.
- D2 must not install packages.
- D2 must not perform network calls.
- D2 must not write target app prompt/skill config files.
- D2 must not store raw token, password, API key, or secret values in metadata.

## Steps

- [x] Add `prompt_assets` migration and model.
- [x] Add repository create/list/toggle helpers and tests.
- [x] Add service validation for item type, JSON fields, and metadata secret references.
- [x] Add Tauri commands and invoke registration.
- [x] Add frontend API types/functions.
- [x] Add Library screen for prompt/skill list/create/toggle.
- [x] Add frontend/API tests.
- [x] Update README D2 notes.
- [x] Run `cargo fmt`, `pnpm typecheck`, `pnpm test:run`, `pnpm rust:check`, and `pnpm rust:test`.
