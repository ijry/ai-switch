# AI Switch Provider Switching B2.1 Codex Real Adapter Design

Date: 2026-07-13
Status: Approved for implementation planning

## Context

Phase B1 implemented sandbox provider switching for the seven default targets. It verified provider selection, deterministic rendering, atomic writes, config snapshots, target state updates, and frontend status feedback without touching real external tool configuration files.

B2 starts converting sandbox-only renderers into real target adapters one target at a time. B2.1 implements the first real adapter for Codex because its public configuration model is documented and testable: Codex reads user-level configuration from `~/.codex/config.toml` or `CODEX_HOME/config.toml`, selects providers with `model_provider`, and defines custom providers under `[model_providers.<id>]`.

The clean-room rule still applies: public behavior and public documentation may be used, but non-commercial source code from `cockpit-tools` must not be copied or translated.

## Product Scope

B2.1 adds explicit real provider switching for the Codex target only. The user can switch a provider to Codex in sandbox mode as before, or choose a real write action that updates the Codex user config file through the same atomic write and snapshot pipeline.

The feature does not make real writes for Claude Code, Claude Desktop, Gemini CLI, OpenCode, OpenClaw, or Hermes. Those remain sandbox-only until later B2 subphases.

## Goals

- Add `mode = "real"` support for provider switching when the selected target key is `codex`.
- Keep `mode = "sandbox"` behavior unchanged for every B1 target.
- Resolve the Codex user config path in the backend only.
- Render a minimal Codex TOML provider configuration using documented user-level keys.
- Preserve unrelated existing Codex config content where practical.
- Never write raw API keys into Codex config.
- Continue using `ConfigWriter` for all writes.
- Record successful and failed real switch attempts in `config_snapshots`.
- Update `target_app_states` with the active provider after a successful real switch.
- Show frontend affordances that distinguish sandbox and real Codex switching.
- Add backend and frontend tests for the real Codex path and unsupported-target validation.

## Non-Goals

- No real writes for any target except Codex.
- No automatic migration of existing Codex provider definitions beyond the selected provider block.
- No TOML comment preservation guarantee.
- No secret resolution, keychain export, or raw API key storage in config.
- No official account switching.
- No quota lookup.
- No tray switching.
- No rollback UI.
- No provider preset library or import/export expansion.
- No MCP, prompts, skills, proxy, cloud sync, usage tracking, sessions, updater, multi-instance management, or wakeup tasks.

## Architecture

B2.1 extends the B1 switching service instead of creating a parallel workflow.

Frontend:

- `src/lib/api` expands `ProviderSwitchRequest.mode` and `ProviderSwitchOutcome.mode` to `"sandbox" | "real"`.
- `ProvidersScreen` keeps the existing sandbox action for all targets.
- `ProvidersScreen` adds a real Codex action only when the selected target is Codex.
- `TargetsScreen` displays real write paths and statuses through the existing target switch status shape.

Backend:

- `services::provider_switch_service` dispatches by request mode.
- `adapters::codex_config` owns Codex path resolution and TOML rendering.
- `config_writer::ConfigWriter` remains the only file-writing primitive.
- Existing snapshot and target state repositories continue to record results.

The service remains the orchestration boundary. Frontend code does not send filesystem paths or serialize Codex config, and adapter code does not mutate the database.

## Codex Config Path Resolution

The backend resolves the real Codex config path in this order:

1. If `CODEX_HOME` is set and non-empty, use `<CODEX_HOME>/config.toml`.
2. Otherwise use `<home>/.codex/config.toml`.

B2.1 adds an injectable environment/path resolver for tests so unit tests do not modify the developer's real Codex config.

The service creates the parent directory when needed. If the resolved path is empty, relative, or cannot be normalized safely, the command returns `filesystem.codex_config_path_invalid`.

## Codex TOML Rendering

The adapter reads the current config if it exists and parses it as TOML. If the file is missing, it starts from an empty document. If parsing fails, the command returns `validation.codex_config_toml`.

The rendered config sets:

```toml
model_provider = "<provider_slug>"

[model_providers.<provider_slug>]
name = "<provider.name>"
base_url = "<provider.base_url>"
wire_api = "responses"
env_key = "<env_key>"
```

Provider slug rules:

- Use `ai_switch_<safe_provider_id>`.
- Lowercase ASCII letters and digits are preserved.
- Any other character becomes `_`.
- Collapse repeated underscores.
- Trim leading and trailing underscores.
- If the result is empty, use `ai_switch_provider`.

`base_url` is required for B2.1 real Codex writes. Missing or empty `base_url` returns `validation.provider_base_url_required`.

`env_key` is resolved from provider metadata:

1. Parse `target_options_json.codex.env_key` if it exists and is a non-empty string.
2. Else parse `target_options_json.env_key` if it exists and is a non-empty string.
3. Else default to `OPENAI_API_KEY`.

If `target_options_json` is malformed, return `validation.provider_target_options_json`. The adapter must not read `secret_ref`, resolve a secret, or write a token value.

The adapter overwrites only:

- top-level `model_provider`
- `[model_providers.<provider_slug>]`

It preserves other top-level keys and other provider blocks after TOML parse/serialize. Exact formatting and comments are not guaranteed.

## Write Flow

For `mode = "sandbox"`, B1 behavior remains unchanged.

For `mode = "real"`:

1. Load and validate target app.
2. Load and validate provider.
3. Reject disabled targets with `validation.target_disabled`.
4. Reject non-Codex targets with `validation.real_target_not_supported`.
5. Resolve the Codex config path from `CODEX_HOME` or home directory.
6. Read the existing config if present.
7. Render the updated Codex TOML.
8. Write through `ConfigWriter::write_atomic`.
9. Insert a `config_snapshots` row with operation `switch_provider:real`.
10. Upsert `target_app_states` with active item type `provider`, active provider id, status, error code, and timestamp.
11. Return `ProviderSwitchOutcome` with `mode = "real"`.

If rendering or writing fails after target resolution, the service still attempts to record:

- `config_snapshots.status = "failed"`
- `config_snapshots.operation = "switch_provider:real"`
- `config_snapshots.error_code = <stable code>`
- `target_app_states.last_write_status = "failed"`
- `target_app_states.last_error_code = <stable code>`

The original error remains the command response.

## API Surface

Existing command remains:

- `switch_target_provider`

Updated request:

- `target_app_id`
- `provider_id`
- `mode: "sandbox" | "real"`

Updated outcome:

- existing B1 fields
- `mode` can be `"sandbox"` or `"real"`
- `path` points to sandbox output for sandbox mode and Codex config for real mode

## Frontend UX

Providers screen:

- Keep target selector.
- Keep `Switch in sandbox`.
- Add `Switch Codex config` when the selected target has key `codex`.
- Disable or hide the real switch action for non-Codex targets.
- Show success copy that includes whether the write was sandbox or real.
- Show failure copy using existing mutation error handling.

Targets screen:

- Existing status cards are enough for B2.1.
- A real write path displays as the last snapshot path.
- Status remains `written` or `failed`.

No destructive-looking UI language should imply rollback exists in B2.1.

## Error Handling

Stable error codes required in B2.1:

- `validation.real_target_not_supported`
- `validation.provider_base_url_required`
- `validation.provider_target_options_json`
- `validation.codex_config_toml`
- `filesystem.codex_config_path_invalid`
- `filesystem.codex_config_read`
- existing B1 switch, database, and config writer codes

User-facing messages must be short and actionable. Technical parse errors can be placed in `details`.

## Testing Strategy

Rust tests:

- Codex adapter resolves `<CODEX_HOME>/config.toml` when `CODEX_HOME` is set.
- Codex adapter renders a new config with `model_provider` and `[model_providers.<slug>]`.
- Codex adapter preserves unrelated TOML keys and provider blocks.
- Codex adapter rejects malformed existing TOML.
- Codex adapter rejects providers without `base_url`.
- Codex adapter uses `target_options_json.codex.env_key` when present.
- Provider switch real mode writes to a temp Codex config path.
- Provider switch real mode records `switch_provider:real` snapshot.
- Provider switch real mode updates active provider state.
- Provider switch real mode rejects non-Codex targets.
- Sandbox mode behavior remains covered by existing B1 tests.

Frontend tests:

- API types allow `mode = "real"`.
- Providers screen shows real Codex action when Codex is selected.
- Providers screen does not show real action for non-Codex target selection.
- Providers screen sends `mode: "real"` for the real Codex action.
- Success copy distinguishes real writes from sandbox writes.

Smoke test:

1. Set `CODEX_HOME` to a temporary directory.
2. Start the app.
3. Import or create an example provider with `base_url`.
4. Open Providers.
5. Select `Codex`.
6. Click `Switch Codex config`.
7. Verify `<CODEX_HOME>/config.toml` exists.
8. Verify it contains `model_provider` and `[model_providers.ai_switch_<id>]`.
9. Verify no real config outside the temporary `CODEX_HOME` was modified.

## Acceptance Criteria

B2.1 is complete when:

- Sandbox provider switching remains unchanged.
- Real provider switching is accepted only for Codex.
- Real Codex writes target backend-resolved `config.toml`.
- Real Codex writes use `ConfigWriter`.
- Real Codex writes never store raw API keys.
- Successful real writes create `config_snapshots` rows with operation `switch_provider:real`.
- Successful real writes update `target_app_states` to the active provider.
- Failed real attempts after target resolution record failed snapshot/state metadata when possible.
- Providers UI exposes a real Codex switch action without exposing real actions for other targets.
- Rust and frontend tests cover the behavior.
- Manual smoke can be run safely with temporary `CODEX_HOME`.

## Later Phase Breakdown

B2.2 should implement the next real target adapter after Codex, using the same explicit-mode, backend-path, atomic-write, and snapshot rules.

B3 should add provider presets and import/export once real switching has at least one stable target adapter.

B4 should add tray switching after provider switching has reliable real-mode state.

B5 should add rollback UI once real-write snapshots include enough backup metadata for user-facing recovery.
