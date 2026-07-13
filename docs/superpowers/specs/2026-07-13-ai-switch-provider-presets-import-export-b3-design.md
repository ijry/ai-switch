# AI Switch Provider Presets And Import/Export B3 Design

Date: 2026-07-13
Status: In implementation

## Context

Phase B1 added sandbox provider switching. B2.1 and B2.2 added real provider switching for Codex and OpenCode. B3 starts provider preset and import/export expansion without changing the database schema or introducing secret storage complexity.

The existing import format is `example_json`, which accepts `providers` and `accounts`. B3 reuses that format for export so exported data can be pasted back into the existing import flow.

## Product Scope

B3 adds:

- Built-in provider presets for common OpenAI-compatible configurations.
- A command to create a provider from a preset, optionally grouped under a new batch.
- A command to export current providers and official accounts as re-importable `example_json`.
- Imports UI controls for preset creation and export.

## Goals

- Keep existing paste-based import behavior unchanged.
- Avoid schema migrations.
- Avoid raw API key storage in presets.
- Preserve batch-first workflows by allowing preset-created providers to be added to a named batch.
- Produce export JSON compatible with `import_example_json`.
- Add backend and frontend tests for preset creation and export.

## Non-Goals

- No remote preset registry.
- No user-authored preset persistence.
- No encrypted export format.
- No file picker or clipboard integration.
- No conflict strategy expansion beyond current import behavior.
- No provider editing UI.
- No secret resolution or keychain writes.

## Backend

New models:

- `ProviderPreset`
- `CreateProviderFromPresetRequest`
- `CreateProviderFromPresetOutcome`
- `ExampleJsonExportOutcome`

New commands:

- `list_provider_presets`
- `create_provider_from_preset`
- `export_example_json`

Preset creation flow:

1. Find the preset by id.
2. Return `validation.provider_preset_not_found` if missing.
3. If `batch_name` is present and non-empty, create a batch with source `provider_preset`.
4. Create a normal provider row using preset metadata.
5. Store only an env reference such as `env://OPENAI_API_KEY` in `secret_ref`.
6. Attach the provider to the new batch when a batch was created.
7. Return provider and optional batch id.

Export flow:

1. List providers.
2. List official accounts.
3. Convert persisted rows to `NewProvider` and `NewOfficialAccount` shapes.
4. Serialize pretty JSON:

```json
{
  "providers": [],
  "accounts": []
}
```

The export intentionally includes `secret_ref` because it is already a non-sensitive reference. It does not resolve or include secret values.

## Frontend

Imports screen adds:

- Provider presets panel with a batch-name input and one create button per preset.
- Export panel with an `Export example JSON` button.
- Read-only textarea containing the export JSON.

Existing paste import remains on the same screen.

## Testing

Rust:

- Presets list includes built-ins and no raw secret-looking values.
- Creating a preset creates a provider and batch item.
- Unknown preset id returns `validation.provider_preset_not_found`.
- Export returns provider/account counts and re-importable JSON.

Frontend:

- API client invokes preset and export commands.
- Imports screen creates a provider from preset with the default batch name.
- Imports screen exports JSON and renders the read-only output.

## Acceptance Criteria

- Users can create a provider from a built-in preset.
- Users can place preset-created providers into a named batch.
- Users can export providers/accounts as valid `example_json`.
- Export JSON can be parsed by the existing importer.
- No raw API keys are introduced by presets or export.
- Existing provider switching behavior remains unchanged.
