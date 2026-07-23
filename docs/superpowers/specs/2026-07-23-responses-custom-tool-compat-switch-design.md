# Responses Custom Tool Compat Switch Design

## Goal

Make the existing Codex Responses `custom` tool rewrite a per-account configurable switch instead of always-on behavior for API Responses relays.

## Decisions

- Switch type: simple `on | off`
- Default: `off` (missing field treated as `off`)
- Scope: account-level API credentials only
- Storage: `config_json.responses_custom_tool_compat` boolean
- No database migration
- Official OAuth credentials are never rewritten

## Behavior

Rewrite is applied only when all conditions are true:

1. credential kind is API
2. request is Responses path (`interface_format == "openai-responses"` or path ends with `/responses`)
3. `config_json.responses_custom_tool_compat == true`

When enabled, keep current rewrite/restore logic:

- request: `tools[].type=custom` -> `function`
- request: `custom_tool_call` -> `function_call`
- request: `custom_tool_call_output` -> `function_call_output`
- response/SSE: restore original custom tools that were rewritten

When disabled or missing:

- leave request body tool types unchanged
- no response restore needed for this path

Model mapping remains independent from this switch.

## Data Model

Field:

```json
{
  "base_url": "...",
  "interface_format": "openai-responses",
  "model_mappings": [],
  "responses_custom_tool_compat": false
}
```

Rules:

- type: boolean
- create API account may write `true` or `false`
- edit API account reads/writes the same field through existing `config_json` update path
- invalid/non-boolean values are treated as `false`

## Backend Changes

### Create API credential

- extend `CreateApiRouteCredentialInput` with optional boolean:
  - `responses_custom_tool_compat?: boolean | null`
- persist into generated `config_json`

### Proxy request build

In `build_api_upstream_request`:

- read `responses_custom_tool_compat` from credential config
- replace hard-coded always-rewrite trigger with:
  - switch enabled AND Responses path

### Official path

- no change
- no rewrite

## Frontend Changes

Show only for API create/edit forms.

Placement:

- under interface format
- above model mappings

Control:

- checkbox
- label: `兼容 custom 工具（Responses 中转）`
- helper: `把 custom 工具改写成 function，给不支持 custom 的中转站用。默认关闭。`

Create form:

- state default `false`
- pass value into create API payload

Edit form:

- load from `config_json.responses_custom_tool_compat`
- missing => `false`
- save via existing `apiConfigJsonWithFields` / full `config_json` update

Official create/edit:

- hide control

Account list:

- no new column in this iteration

## Tests

Backend unit tests:

1. switch off + Responses path => keep `custom`
2. switch on + Responses path => rewrite custom tools
3. switch on + non-Responses path => no rewrite
4. missing field => treated as off
5. response restore still works when switch on

Frontend/type coverage:

- create/update payload includes boolean when present
- edit form loads false by default and true when config has true

## Non-goals

- global app setting
- DB schema migration
- list-page badge/column
- auto-detection of upstream custom-tool support
- changing model mapping behavior

## Acceptance

1. New API account defaults to no custom-tool rewrite
2. Enabling switch on a Xiaomi/OpenAI-Responses relay account allows Codex custom tools to pass
3. Disabling switch preserves raw `custom` tool payload for native-compatible upstreams
4. Official OAuth accounts remain unaffected
