# AI Switch Provider Switching B1 Design

Date: 2026-07-13
Status: Approved for implementation planning

## Context

Phase A created the Tauri 2, React, TypeScript, Rust, SQLite, batch, provider, target, import, and atomic config writer foundation for `ai-switch`.

Phase B1 adds the first provider switching loop. The loop must be useful and testable, but it must not write real Claude, Codex, Gemini, OpenCode, OpenClaw, or Hermes configuration files yet. B1 uses sandbox target paths under the app data directory so rendering, atomic writes, database state, snapshots, and UI feedback can be verified without changing a user's external tool setup.

The long-term clean-room rule still applies: public behavior, public documentation, and public file formats may be studied, but non-commercial source code from `cockpit-tools` must not be copied or translated.

## Product Scope

B1 implements sandbox-first provider switching for the seven default target apps:

- Claude Code
- Claude Desktop
- Codex
- Gemini CLI
- OpenCode
- OpenClaw
- Hermes

The user can pick an existing provider, switch one target app to that provider, and see the resulting sandbox write status. The backend renders deterministic target-specific sandbox config, writes it atomically, records a config snapshot, and updates target activation state.

## Goals

- List providers in the frontend so the user can select a provider for switching.
- Show target apps with active provider state, last write status, last error code, last write time, and last sandbox output path.
- Add a backend provider switch command for `provider` items only.
- Render target-specific sandbox config for all seven default target keys.
- Write sandbox configs under `~/.ai-switch/targets/<target_key>/`.
- Use `ConfigWriter` for all sandbox file writes.
- Record every switch attempt in `config_snapshots`.
- Update `target_app_states` after each switch attempt.
- Keep secret handling safe by rendering secret references or redacted markers only; B1 does not resolve raw API keys.
- Add Rust and frontend tests for the successful path and representative validation/failure paths.

## Non-Goals

- No writes to real external tool config paths.
- No automatic detection of installed target apps.
- No real Claude, Codex, Gemini, OpenCode, OpenClaw, or Hermes account switching.
- No OAuth, token refresh, official account quota lookup, or provider quota lookup.
- No tray switching.
- No provider preset library.
- No provider import/export beyond the existing Phase A example import.
- No rollback UI.
- No encrypted secret fallback changes.
- No MCP, prompts, skills, proxy, cloud sync, usage tracking, sessions, updater, multi-instance management, or wakeup tasks.

## Architecture

B1 extends the existing backend layers instead of replacing them.

Frontend:

- `src/lib/api` exposes provider listing, target status listing, and provider switching commands.
- `src/screens/ProvidersScreen.tsx` lists providers and offers a switch action.
- `src/screens/TargetsScreen.tsx` shows target app status and the latest sandbox output path.
- UI state is refreshed through existing React Query invalidation patterns.

Backend:

- `commands` remains a thin Tauri IPC layer.
- `services::provider_switch_service` owns validation, orchestration, state updates, and snapshot recording.
- `adapters::provider_renderers` owns target-specific sandbox rendering.
- `database::repositories` adds focused repositories for provider listing, target state upsert, and config snapshot inserts/queries.
- `config_writer::ConfigWriter` remains the only file writing primitive.

The service boundary is intentional: frontend code never chooses filesystem paths or serializes target configs, and renderer code never mutates the database.

## Core Domain Changes

### ProviderSwitchRequest

The frontend sends:

- `target_app_id`
- `provider_id`
- `mode`

B1 accepts only `mode = "sandbox"`. Any other mode returns a validation error with code `validation.switch_mode`.

### ProviderSwitchOutcome

The backend returns:

- `target_app_id`
- `target_key`
- `provider_id`
- `provider_name`
- `mode`
- `path`
- `status`
- `before_hash`
- `after_hash`
- `snapshot_id`
- `state_id`
- `written_at`

Successful writes use `status = "written"`. Failed attempts use `status = "failed"` in database records and return an `ApiError`.

### TargetSwitchStatus

The target status list returns one row per target app:

- `target`
- `active_provider`
- `last_write_status`
- `last_error_code`
- `last_written_at`
- `last_snapshot_path`
- `last_snapshot_id`

This can be produced by joining `target_apps`, `target_app_states`, `providers`, and the latest `config_snapshots` row for each target. B1 does not need a schema migration for this shape.

## Sandbox Path Design

All B1 writes stay inside `AppPaths.data_dir`.

The canonical sandbox path is:

```text
~/.ai-switch/targets/<target_key>/provider.json
```

The backend must derive this path from `AppPaths.data_dir` and the target app key stored in SQLite. The request must not accept a path from the frontend.

Before writing, the service validates that the resolved path still starts with:

```text
~/.ai-switch/targets/
```

If path validation fails, the command returns `filesystem.sandbox_path_invalid` and records a failed snapshot when the target app can be resolved.

## Sandbox Config Rendering

Each renderer returns a deterministic JSON string. The JSON is target-specific enough for B1 tests and future extension, but it is explicitly an `ai-switch` sandbox schema, not a promise that the file can be copied directly into a real tool config.

The top-level shape is:

```json
{
  "schema": "ai-switch.provider-switch.sandbox.v1",
  "target": {
    "key": "codex",
    "display_name": "Codex"
  },
  "provider": {
    "id": "provider-id",
    "name": "Acme Provider",
    "kind": "openai_compatible",
    "base_url": "https://api.example.com/v1",
    "secret_ref": "secret://provider/acme",
    "secret_value": "[redacted]"
  },
  "model_config": {},
  "target_options": {},
  "rendered_for": "codex"
}
```

Renderer rules:

- Parse `model_config_json` as JSON object; malformed JSON returns `validation.provider_model_config_json`.
- Parse `target_options_json` as JSON object; malformed JSON returns `validation.provider_target_options_json`.
- Include only `target_options_json[target_key]` when the object has a matching key.
- Include the full `target_options_json` only when it has no target-specific key.
- Never resolve `secret_ref` into a raw secret.
- Always include `secret_value = "[redacted]"` when `secret_ref` is present.
- Sort or serialize keys deterministically so snapshot hashes are stable across repeated writes.

The seven renderers may share a common helper, but each target key must have an explicit match branch. Unsupported keys return `adapter.target_not_supported`.

## Write Flow

The provider switch service performs these steps:

1. Load and validate the target app.
2. Load and validate the provider.
3. Reject disabled targets with `validation.target_disabled`.
4. Reject non-sandbox mode with `validation.switch_mode`.
5. Resolve the sandbox path from `AppPaths.data_dir` and `target.key`.
6. Render the sandbox config for `target.key`.
7. Call `ConfigWriter::write_atomic`.
8. Insert a `config_snapshots` row with operation `switch_provider:sandbox`.
9. Upsert `target_app_states` with active item type `provider`, active provider id, status, error code, and timestamp.
10. Return `ProviderSwitchOutcome`.

If rendering or writing fails after the target app is known, the service still attempts to record:

- `config_snapshots.status = "failed"`
- `config_snapshots.error_code = <stable code>`
- `target_app_states.last_write_status = "failed"`
- `target_app_states.last_error_code = <stable code>`

The original error remains the command response.

## Database Use

B1 reuses existing tables.

Required repository additions:

- `ProviderRepository::list(pool) -> Result<Vec<Provider>, AppError>`
- `ProviderRepository::get(pool, id) -> Result<Provider, AppError>` already exists and should be reused.
- `TargetRepository::get(pool, id) -> Result<TargetApp, AppError>`
- `TargetStateRepository::upsert_provider_state(pool, target_app_id, provider_id, status, error_code, written_at) -> Result<TargetAppState, AppError>`
- `TargetStateRepository::list_switch_statuses(pool) -> Result<Vec<TargetSwitchStatus>, AppError>`
- `ConfigSnapshotRepository::insert(pool, input) -> Result<ConfigSnapshot, AppError>`
- `ConfigSnapshotRepository::latest_for_target(pool, target_app_id) -> Result<Option<ConfigSnapshot>, AppError>`

B1 must not add a schema migration. The implementation plan should use query-level joins and the existing `target_app_states` and `config_snapshots` tables.

## Frontend UX

### Providers Screen

The providers screen becomes the primary starting point for B1.

It shows:

- Provider name
- Provider kind
- Base URL
- Status
- A target selector populated from target switch statuses
- A `Switch in sandbox` button
- Success or failure message from the last switch action

The screen should remain usable when there are no providers by showing an empty state that points the user to the existing import flow.

### Targets Screen

The targets screen shows the result of switching.

Each target card shows:

- Display name
- Key
- Enabled/disabled state
- Active provider name or `No provider selected`
- Last write status
- Last error code when present
- Last write time when present
- Last sandbox output path when present

The target card does not offer real-write actions in B1.

## API Surface

New frontend API wrappers:

- `listProviders(): Promise<Provider[]>`
- `listTargetSwitchStatuses(): Promise<TargetSwitchStatus[]>`
- `switchTargetProvider(request: ProviderSwitchRequest): Promise<ProviderSwitchOutcome>`

New Tauri commands:

- `list_providers`
- `list_target_switch_statuses`
- `switch_target_provider`

Command errors must preserve the existing `ApiError` shape:

- `code`
- `message`
- `details`
- `recoverable`
- `operation_id`

## Error Handling

Stable error codes required in B1:

- `validation.switch_mode`
- `validation.target_disabled`
- `validation.provider_model_config_json`
- `validation.provider_target_options_json`
- `database.provider_list`
- `database.target_get`
- `database.target_state_upsert`
- `database.config_snapshot_insert`
- `filesystem.sandbox_path_invalid`
- `adapter.target_not_supported`

User-facing messages must be short and actionable. Technical details can be placed in `details`.

## Testing Strategy

Rust tests:

- Provider list returns created providers ordered by `sort_order`, then `created_at`.
- Target get returns a default target by id after `ensure_defaults`.
- Renderer produces deterministic JSON for each of the seven default target keys.
- Renderer rejects malformed `model_config_json`.
- Renderer rejects malformed `target_options_json`.
- Provider switch writes `provider.json` under the target sandbox directory.
- Provider switch records a successful `config_snapshots` row.
- Provider switch upserts `target_app_states` with active item type `provider`.
- Provider switch rejects unsupported modes.
- Provider switch records failure state when rendering fails after target resolution.

Frontend tests:

- Providers screen lists providers and triggers sandbox switching with selected target id.
- Providers screen shows an empty state when there are no providers.
- Targets screen shows active provider, write status, and sandbox output path.
- API wrappers invoke the expected Tauri command names.

Smoke test:

1. Start the app.
2. Import or create an example provider.
3. Open Providers.
4. Select `Codex`.
5. Click `Switch in sandbox`.
6. Open Targets.
7. Verify Codex shows the selected provider and status `written`.
8. Verify `~/.ai-switch/targets/codex/provider.json` exists.
9. Verify no real external tool config file was modified by this workflow.

## Acceptance Criteria

B1 is complete when:

- The app can list providers from SQLite.
- The app can list target switch statuses for the seven default targets.
- Switching a provider to any default target writes a sandbox config under `~/.ai-switch/targets/<target_key>/provider.json`.
- All switch writes go through `ConfigWriter`.
- Successful switch attempts create `config_snapshots` rows.
- Successful switch attempts update `target_app_states` to active item type `provider`.
- Failed switch attempts after target resolution record failed snapshot/state metadata when possible.
- The frontend exposes sandbox switching from Providers and switch status from Targets.
- Rust tests and frontend tests cover the B1 behavior.
- No B1 command writes to a real external tool config path.

## Later Phase Breakdown

B2 should convert sandbox renderers into real target adapters one target at a time, starting with the target whose public config format is best documented and easiest to verify.

B3 should add provider presets and import/export once the switch path is stable.

B4 should add tray switching after provider switching has a reliable backend API and state model.

B5 should add rollback UI once real-write mode exists and snapshots include enough backup data for user-facing recovery.
