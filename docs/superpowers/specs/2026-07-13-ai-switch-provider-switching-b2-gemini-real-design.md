# AI Switch Provider Switching B2.3 Gemini CLI Real Adapter Design

## Context

B2.1 added Codex real provider switching and B2.2 added OpenCode real provider switching. Both use the same guardrails: backend-resolved paths, explicit `mode = "real"`, atomic writes with backups, no raw secret material in rendered configs, config snapshots, target state updates, and frontend actions only for implemented real targets.

B2.3 adds the next real adapter for Gemini CLI. Gemini CLI has a user settings file at `~/.gemini/settings.json`; AI Switch also supports `GEMINI_CLI_SETTINGS` as an explicit override for safe smoke testing and isolated automated tests.

## Goals

- Add explicit real provider switching for the `gemini_cli` target.
- Keep sandbox switching unchanged for all targets.
- Keep Codex and OpenCode real switching unchanged.
- Resolve Gemini CLI settings paths in the backend only.
- Preserve unrelated JSON settings when rendering Gemini CLI config.
- Record successful and failed Gemini CLI real attempts in `config_snapshots`.
- Update `target_app_states` on successful Gemini CLI real writes.
- Expose Gemini CLI real switching from Providers UI and tray only after the adapter exists.
- Include rollback support through the existing real-write backup pipeline.

## Non-Goals

- Do not read or write raw API keys.
- Do not resolve `secret_ref` values.
- Do not implement OAuth, account import, or quota lookup for Gemini accounts.
- Do not launch Gemini CLI or verify the external CLI can use the rendered settings.
- Do not add a schema migration.

## Path Resolution

The backend resolves the Gemini CLI real settings path in this order:

1. `GEMINI_CLI_SETTINGS` when set and non-empty.
2. `~/.gemini/settings.json`.

The resolved path must be absolute. Tests use an injected path helper so automated tests never write user config files.

## Rendering

The Gemini renderer reads existing JSON settings if present. Empty or missing files are treated as an empty object.

The renderer:

- Requires `model_config_json` and `target_options_json` to be JSON objects.
- Resolves the model from `target_options_json.gemini_cli.model`, `target_options_json.model`, `model_config_json.gemini_cli.model`, `model_config_json.default`, then `model_config_json.model`.
- Writes the selected model to `model.name`.
- Writes non-secret AI Switch metadata to `aiSwitch.activeProvider`.
- Uses `target_options_json.gemini_cli.env_key`, `target_options_json.env_key`, or `GEMINI_API_KEY` as metadata only.
- Preserves unrelated top-level JSON keys and nested `model` settings.
- Serializes standard pretty JSON with a trailing newline.

The rendered settings must not contain raw secret values or provider `secret_ref` values.

## Service Integration

`ProviderSwitchService` dispatches real-mode writes by `target.key`:

- `codex` -> Codex adapter.
- `gemini_cli` -> Gemini CLI adapter.
- `opencode` -> OpenCode adapter.

Gemini real writes use `ConfigWriter::write_atomic_with_backup`, which records the previous file state under the app backup directory before writing. Successful writes insert `config_snapshots.operation = "switch_provider:real"` with backup metadata and update the active provider state. Failed render/write attempts after path resolution insert failed snapshot/state metadata when possible.

Rollback uses the existing B5 rollback flow because Gemini real snapshots store the same backup metadata as Codex and OpenCode.

## Frontend And Tray

The Providers screen shows a real config action for Gemini CLI only when the selected target key is `gemini_cli`. The system tray includes Gemini CLI in the real-config submenu alongside Codex and OpenCode.

Frontend code sends only `target_app_id`, `provider_id`, and `mode = "real"`; it never sends filesystem paths or serialized Gemini settings.

## Verification

Automated coverage should include:

- Gemini path resolution.
- JSON rendering that preserves unrelated settings.
- target-specific model and env metadata.
- malformed existing settings rejection.
- malformed provider JSON rejection.
- missing model rejection.
- service success path with snapshot, backup, and target state.
- service failure path after path resolution.
- Providers UI real Gemini action.
- tray real target count including Gemini.
- existing Codex, OpenCode, sandbox, and rollback tests continuing to pass.

Manual smoke should use a temporary `GEMINI_CLI_SETTINGS` path before starting the app.
