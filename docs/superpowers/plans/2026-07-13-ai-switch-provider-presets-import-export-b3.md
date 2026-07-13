# AI Switch Provider Presets And Import/Export B3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans or equivalent task tracking. Keep checkboxes updated as implementation and verification progress.

**Goal:** Add built-in provider presets and re-importable example JSON export while preserving existing import and switching behavior.

**Architecture:** B3 adds static backend provider presets, creates normal provider rows from presets, and exports current providers/accounts in the existing `example_json` shape. The Imports screen becomes the entry point for paste import, preset seeding, and export.

## Constraints

- No schema migration.
- No raw API key storage.
- Existing `import_example_json` behavior remains unchanged.
- Export must be compatible with `parse_example_json`.
- Preset-created providers should be ordinary providers usable by existing switching flows.

## Tasks

- [x] Add provider preset models.
- [x] Add account repository list support.
- [x] Add `ProviderPresetService` with built-in OpenAI-compatible presets.
- [x] Add preset creation tests.
- [x] Add `ImportService::export_example_json`.
- [x] Add export tests proving re-importable JSON.
- [x] Add Tauri commands for `list_provider_presets`, `create_provider_from_preset`, and `export_example_json`.
- [x] Register commands in the invoke handler.
- [x] Add TypeScript API types and client functions.
- [x] Add Imports screen preset and export UI.
- [x] Add frontend fixtures and tests.
- [x] Add README B3 notes.
- [x] Run `cargo fmt`.
- [x] Run `pnpm test:run`.
- [x] Run `pnpm typecheck`.
- [x] Run `pnpm rust:check`.
- [x] Run `pnpm rust:test`.
- [x] Update this plan with final verification results.

## Verification Commands

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml
pnpm test:run
pnpm typecheck
pnpm rust:check
pnpm rust:test
```

## Manual Smoke

1. Start the app with `pnpm tauri:dev`.
2. Open `Imports`.
3. Confirm provider presets load.
4. Click `Create OpenAI Compatible`.
5. Open `Batches` or `Providers` and confirm the provider exists.
6. Click `Export example JSON`.
7. Confirm the textarea contains `providers` and `accounts`.
8. Paste the exported JSON into the import panel with a new batch name and import it.
