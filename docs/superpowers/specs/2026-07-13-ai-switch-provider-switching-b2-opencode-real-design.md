# AI Switch Provider Switching B2.2 OpenCode Real Adapter Design

Date: 2026-07-13
Status: In implementation

## Context

B2.1 added real provider switching for Codex while keeping sandbox switching unchanged for every default target. B2.2 adds the next real adapter for OpenCode because its public docs define a JSON/JSONC config file, global config path, provider section, model selector, base URL, API key environment substitution, and custom OpenAI-compatible provider shape.

Clean-room rule remains unchanged: use public documentation and observed public behavior only. Do not copy or translate non-commercial source from `cockpit-tools`.

Public OpenCode config facts used by B2.2:

- Global config: `~/.config/opencode/opencode.json`.
- Custom config override: `OPENCODE_CONFIG=/path/to/config.json`.
- Config supports JSON and JSONC.
- Main model key: `model`.
- Providers are configured under `provider`.
- Provider options include `options.baseURL` and `options.apiKey`.
- Environment variables can be referenced as `{env:VARIABLE_NAME}`.
- OpenAI-compatible custom providers use `npm: "@ai-sdk/openai-compatible"`, `name`, `options`, and `models`.

## Product Scope

B2.2 adds explicit real provider switching for the OpenCode target. The user can switch a provider to OpenCode in sandbox mode as before, or choose a real write action that updates the OpenCode user config file through the existing atomic writer and snapshot pipeline.

Codex real mode remains supported. Claude Code, Claude Desktop, Gemini CLI, OpenClaw, and Hermes remain sandbox-only.

## Goals

- Add `mode = "real"` support for target key `opencode`.
- Keep sandbox switching unchanged.
- Keep Codex real switching unchanged.
- Resolve OpenCode config paths in the backend only.
- Render a minimal OpenCode custom provider config using documented JSON keys.
- Preserve unrelated existing OpenCode config keys and provider blocks after parse/serialize.
- Never write raw API keys or resolved secrets.
- Record successful and failed OpenCode real attempts in `config_snapshots`.
- Update `target_app_states` on successful OpenCode real writes.
- Expose a real OpenCode switch action in the Providers UI only when OpenCode is selected.

## Non-Goals

- No real writes for Claude Code, Claude Desktop, Gemini CLI, OpenClaw, or Hermes.
- No official provider mapping beyond a custom OpenAI-compatible provider entry.
- No secret resolution, keychain export, or raw API key storage in OpenCode config.
- No comment preservation guarantee for JSONC input.
- No project config writes.
- No rollback UI, tray switching, presets, quota lookup, or official account switching.

## OpenCode Path Resolution

The backend resolves the OpenCode real config path in this order:

1. If `OPENCODE_CONFIG` is set and non-empty, write that file path.
2. Otherwise write `<home>/.config/opencode/opencode.json`.

The resolved path must be absolute. Invalid paths return `filesystem.opencode_config_path_invalid`. Tests inject a temporary path so real user config is not modified.

## OpenCode Rendering

The adapter reads the existing config if present. It accepts JSON and basic JSONC comments/trailing commas, then serializes standard pretty JSON. If parsing fails, the command returns `validation.opencode_config_json`.

The rendered config sets:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "model": "<provider_slug>/<model_id>",
  "provider": {
    "<provider_slug>": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "<provider.name>",
      "options": {
        "baseURL": "<provider.base_url>",
        "apiKey": "{env:<env_key>}"
      },
      "models": {
        "<model_id>": {
          "name": "<model_name>"
        }
      }
    }
  }
}
```

Provider slug rules:

- Use `ai-switch-<safe_provider_id>`.
- Lowercase ASCII letters and digits are preserved.
- Any other character becomes `-`.
- Collapse repeated separators.
- Trim leading and trailing separators.
- If the result is empty, use `ai-switch-provider`.

Required provider metadata:

- `base_url` is required. Missing or empty returns `validation.provider_base_url_required`.
- `model_id` is required. It is read from `target_options_json.opencode.model`, `target_options_json.model`, `model_config_json.opencode.model`, `model_config_json.default`, or `model_config_json.model`. Missing returns `validation.provider_model_required`.

Optional provider metadata:

- `env_key` is read from `target_options_json.opencode.env_key`, then `target_options_json.env_key`, else defaults to `OPENAI_API_KEY`.
- `npm` is read from `target_options_json.opencode.npm`, else defaults to `@ai-sdk/openai-compatible`.
- `model_name` is read from `target_options_json.opencode.model_name`, then `model_config_json.model_name`, else defaults to `model_id`.
- `provider_name` is read from `target_options_json.opencode.provider_name`, else defaults to `provider.name`.

The adapter overwrites only top-level `model` and `provider.<provider_slug>`, and inserts `$schema` when missing. It preserves unrelated keys and other provider blocks after JSON parse/serialize.

## Write Flow

For `mode = "sandbox"`, B1 behavior remains unchanged.

For `mode = "real"`:

1. Load target app and provider.
2. Reject disabled targets with `validation.target_disabled`.
3. Dispatch by target key.
4. For `codex`, use the existing B2.1 Codex flow.
5. For `opencode`, resolve the OpenCode config path.
6. Read and render the OpenCode config.
7. Write through `ConfigWriter::write_atomic`.
8. Insert `config_snapshots.operation = "switch_provider:real"`.
9. Upsert `target_app_states` with active provider state.
10. Return `ProviderSwitchOutcome.mode = "real"`.

If rendering or writing fails after target/path resolution, record a failed snapshot/state when possible and return the original error.

## Frontend UX

Providers screen:

- Keep target selector.
- Keep `Switch in sandbox`.
- Show `Switch Codex config` when Codex is selected.
- Show `Switch OpenCode config` when OpenCode is selected.
- Hide real config actions for unsupported targets.
- Success copy uses the selected target display name: `Wrote OpenCode config for ... to OpenCode.`

## Error Codes

- `validation.real_target_not_supported`
- `validation.provider_base_url_required`
- `validation.provider_model_required`
- `validation.provider_model_config_json`
- `validation.provider_target_options_json`
- `validation.opencode_config_json`
- `filesystem.opencode_config_path_invalid`
- `filesystem.opencode_config_read`

## Acceptance Criteria

- OpenCode real mode writes backend-resolved `opencode.json`.
- OpenCode real mode uses `ConfigWriter`.
- OpenCode config contains provider/model fields documented above.
- OpenCode config stores `{env:...}` references, not raw keys or `secret_ref`.
- Existing unrelated JSON keys and provider blocks are preserved after parse/serialize.
- Successful real writes create `switch_provider:real` snapshots and active provider state.
- Failed OpenCode real attempts after path resolution record failed snapshot/state metadata.
- Providers UI exposes OpenCode real action only for OpenCode.
- Existing Codex real and sandbox tests continue to pass.
