# AI Switch Provider Switching B2.3 Gemini CLI Real Adapter Design

## Context

B2.1 added real provider switching for Codex, and B2.2 added real provider switching for OpenCode. B2.3 continues Phase B by adding the next real target adapter for Gemini CLI.

Clean-room rule remains unchanged: use public documentation and public package contents only. Do not copy or translate non-commercial source from `cockpit-tools`.

Public Gemini CLI facts used by B2.3:

- User settings path: `~/.gemini/settings.json`.
- Project settings path: `.gemini/settings.json`, but B2.3 writes only user settings.
- `settings.json` is JSON.
- Model selection can be configured as `model.name`.
- Model precedence is `--model`, `GEMINI_MODEL`, `settings.json` `model.name`, local model router, then default model.
- Authentication is done through Google login, `GEMINI_API_KEY`, `GOOGLE_API_KEY`, and Vertex-related environment variables; B2.3 must not write raw credentials.

## Product Scope

B2.3 adds explicit real switching for the `gemini_cli` target. The user can switch a provider to Gemini CLI in sandbox mode as before, or choose a real write action that updates Gemini CLI user `settings.json` through the existing atomic writer, backup, snapshot, target-state, tray, and rollback pipeline.

Gemini CLI public settings do not define an OpenAI-compatible provider block equivalent to OpenCode. Therefore B2.3 switches the Gemini CLI default model and records safe `aiSwitch` metadata only. It does not attempt arbitrary `base_url` provider injection.

## Goals

- Add `mode = "real"` support for target key `gemini_cli`.
- Keep sandbox, Codex real, and OpenCode real switching unchanged.
- Resolve Gemini CLI settings path in the backend only.
- Render Gemini CLI JSON settings by setting `model.name`.
- Preserve unrelated existing Gemini CLI settings after parse/serialize.
- Store safe `aiSwitch.activeProvider` metadata for auditability.
- Never write raw API keys, resolved secrets, or `secret_ref` into Gemini CLI settings.
- Record successful and failed Gemini CLI real attempts in `config_snapshots`.
- Save real-write backups so B5 rollback works for Gemini CLI.
- Expose Gemini CLI real switch action in Providers UI and tray menus.

## Non-Goals

- No arbitrary OpenAI-compatible provider injection for Gemini CLI.
- No writes to project `.gemini/settings.json`.
- No OAuth, browser login, token refresh, or credential import.
- No raw API key storage.
- No network calls.
- No official account switching.
- No changes to Codex/OpenCode rendering.

## Gemini CLI Path Resolution

The backend resolves the Gemini CLI real config path in this order:

1. If `GEMINI_CLI_SETTINGS` is set and non-empty, write that exact file path. This is an ai-switch test/smoke override, not a public Gemini CLI variable.
2. Otherwise write `<home>/.gemini/settings.json`.

The resolved path must be absolute. Invalid paths return `filesystem.gemini_config_path_invalid`. Tests inject a temporary path so real user config is not modified.

## Gemini CLI Rendering

The adapter reads existing JSON settings if present. Empty or missing settings start as `{}`. Malformed settings return `validation.gemini_config_json`.

The adapter resolves a model id from:

1. `target_options_json.gemini_cli.model`
2. `target_options_json.model`
3. `model_config_json.gemini_cli.model`
4. `model_config_json.default`
5. `model_config_json.model`

Missing or empty model id returns `validation.provider_model_required`.

The rendered settings set:

```json
{
  "model": {
    "name": "<model_id>"
  },
  "aiSwitch": {
    "activeProvider": {
      "id": "<provider.id>",
      "name": "<provider.name>",
      "kind": "<provider.kind>",
      "envKey": "<env_key>"
    }
  }
}
```

`env_key` is read from `target_options_json.gemini_cli.env_key`, then `target_options_json.env_key`, else defaults to `GEMINI_API_KEY`. The env key is metadata only. The adapter does not write an `apiKey` value or `secret_ref`.

The adapter overwrites only `model.name` and `aiSwitch.activeProvider`. It preserves unrelated root keys and nested `model` keys after JSON parse/serialize.

## Write Flow

For `mode = "sandbox"`, existing behavior remains unchanged.

For `mode = "real"`:

1. Load target app and provider.
2. Reject disabled targets with `validation.target_disabled`.
3. Dispatch by target key.
4. For `codex` and `opencode`, use existing flows.
5. For `gemini_cli`, resolve Gemini CLI settings path.
6. Read and render Gemini CLI settings.
7. Write through `ConfigWriter::write_atomic_with_backup`.
8. Insert `config_snapshots.operation = "switch_provider:real"`.
9. Upsert `target_app_states` with active provider state.
10. Return `ProviderSwitchOutcome.mode = "real"`.

If rendering or writing fails after path resolution, record a failed snapshot/state when possible and return the original error.

## Frontend UX

Providers screen:

- Keep target selector.
- Keep `Switch in sandbox`.
- Show `Switch Codex config`, `Switch OpenCode config`, or `Switch Gemini CLI config` only when that target is selected.
- Hide real config actions for unsupported targets.
- Success copy uses the selected target display name.

Tray:

- Add Gemini CLI to the real config submenu.
- Keep sandbox entries for all targets.

## Error Codes

- `validation.real_target_not_supported`
- `validation.provider_model_required`
- `validation.provider_model_config_json`
- `validation.provider_target_options_json`
- `validation.gemini_config_json`
- `filesystem.gemini_config_path_invalid`
- `filesystem.gemini_config_read`

## Acceptance Criteria

- Gemini CLI real mode writes backend-resolved `settings.json`.
- Gemini CLI real mode uses `ConfigWriter::write_atomic_with_backup`.
- Gemini CLI config sets `model.name` from provider metadata.
- Gemini CLI config stores safe `aiSwitch.activeProvider` metadata only.
- Gemini CLI config does not store raw keys or `secret_ref`.
- Existing unrelated JSON keys are preserved after parse/serialize.
- Successful real writes create restorable `switch_provider:real` snapshots and active provider state.
- Failed Gemini CLI real attempts after path resolution record failed snapshot/state metadata.
- Providers UI and tray expose Gemini CLI real action only for Gemini CLI.
- Existing Codex/OpenCode real, rollback, and sandbox tests continue to pass.
