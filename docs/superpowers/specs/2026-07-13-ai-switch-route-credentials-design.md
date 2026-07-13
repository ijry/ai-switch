# AI Switch Route Credentials Design

## Goal

Replace the simple official-account create form with a unified routing credential model that supports:

1. Official accounts imported only via CLIProxyAPI-style CPA credentials.
2. API accounts configured with `apiKey` + `baseUrl`, advanced interface format, and model mappings.
3. Account-list and route-pool selection that feed real local proxy routing.
4. Editable config preview drafts for Codex / Claude / Gemini without writing disk until an explicit write action.

This design keeps the product IA agent-first:

- Left primary tabs: Codex / Claude / Gemini / OpenCode / OpenClaw / Hermes
- Settings remains the only secondary hub for non-core features such as MCP

User-facing copy must not mention competitor product names.

## Scope

In scope:

- New `route_credentials` storage and APIs
- Official CPA import: single paste + multi-file select
- API credential create/edit with advanced options
- Config preview draft generation and manual edit, saved to DB only
- Route pool membership bound to `route_credentials`
- Local route proxy credential selection from the pool
- Explicit "write route config files" action that points target CLIs at the local proxy

Out of scope:

- Email/password official login
- pro-api style `{accounts:[{provider,email,credentials}]}` wrapper import
- Legacy data migration from `official_accounts` / `providers` / old pool members
- Secure secret encryption beyond v1 plaintext DB storage
- Direct "switch current local official auth file" as a separate product mode
- OpenCode / OpenClaw / Hermes CPA formats in v1 (tabs remain, credential create for them can follow the same model later)

## Decisions Locked

| Topic | Decision |
| --- | --- |
| Create modes | Official account + API config |
| Type mutability | Locked at create time |
| Official create | CPA only; no email/password |
| CPA single | Paste JSON text |
| CPA bulk | Open/select one or more `.json` files and parse |
| CPA accepted formats | CLIProxyAPI single-auth JSON object, JSON array, nested `tokens`, camelCase aliases |
| CPA rejected formats | `{accounts:[{provider,email,credentials}]}` wrapper |
| API advanced options | `interface_format` enum with 5 values + model mappings array |
| Model mapping shape | `[{from,to,label?}]` |
| Preview targets | Codex + Claude + Gemini |
| Preview files | Codex: `auth.json` + `config.toml`; Claude: `settings.json`; Gemini: `settings.json` |
| Preview persistence | Editable draft stored in DB only |
| Real disk write | Existing explicit route config write action |
| Storage | Unified `route_credentials` |
| Secret storage v1 | Plaintext JSON in DB |
| Route pool source | Only `route_credentials` |
| Legacy migration | None; app not released yet |

## Architecture

### Unified credential model

Introduce `route_credentials` as the single source of truth for account list rows, route-pool members, and outbound proxy credentials.

```text
UI Account List (platform tab)
  -> route_credentials(platform)
  -> checkbox membership
  -> route_pool_members(route_credential_id)

Local Route Proxy
  -> resolve platform
  -> pick enabled pool member
  -> load route_credentials
  -> official tokens OR api key/base_url/format/mappings
  -> forward upstream
  -> write usage_events
```

### Why not keep official_accounts + providers split

The agent-first UI already removed Providers as a primary surface. Keeping official and API credentials in separate tables forces dual list/pool/proxy paths and recreates the old IA. A single `route_credentials` table matches the product model: every row is a routable account-like credential under an agent tab.

### Relationship to existing tables

- `providers` may remain temporarily for older provider-switch experiments, but account list, pool, and proxy routing in this slice ignore it.
- `official_accounts` is not the write path for new creates. No migration is required.
- `route_pool_members` is reshaped to reference `route_credential_id`.
- `usage_events` should record `route_credential_id` for pool stats; provider_id may remain nullable for compatibility.

## Data Model

### `route_credentials`

| Column | Type | Notes |
| --- | --- | --- |
| `id` | TEXT PK | UUID |
| `platform` | TEXT NOT NULL | `codex` / `claude` / `gemini` / later agents |
| `kind` | TEXT NOT NULL | `official` or `api` |
| `display_name` | TEXT NOT NULL | UI label |
| `email` | TEXT NULL | From CPA when present |
| `status` | TEXT NOT NULL | `ok` / `invalid` / `disabled` |
| `sort_order` | INTEGER NOT NULL | List ordering |
| `batch_id` | TEXT NULL | Optional group for bulk imports |
| `secret_payload_json` | TEXT NOT NULL | Plaintext secrets v1 |
| `config_json` | TEXT NOT NULL | Non-secret structured config |
| `preview_json` | TEXT NOT NULL | Editable draft files |
| `created_at` | TEXT NOT NULL | ISO timestamp |
| `updated_at` | TEXT NOT NULL | ISO timestamp |

#### `secret_payload_json`

Official:

```json
{
  "id_token": "...",
  "access_token": "...",
  "refresh_token": "...",
  "account_id": "ac_..."
}
```

API:

```json
{
  "api_key": "..."
}
```

#### `config_json`

Official:

```json
{
  "type": "codex",
  "account_id": "ac_...",
  "last_refresh": "...",
  "expired": "...",
  "raw_type": "codex"
}
```

API:

```json
{
  "base_url": "https://api.example.com/v1",
  "interface_format": "openai",
  "model_mappings": [
    {"from": "gpt-5", "to": "upstream-gpt", "label": "default"}
  ]
}
```

`interface_format` enum:

- `openai`
- `openai-responses`
- `anthropic`
- `anthropic-messages`
- `gemini`

#### `preview_json`

Codex:

```json
{
  "auth_json": "{...}",
  "config_toml": "..."
}
```

Claude / Gemini:

```json
{
  "settings_json": "{...}"
}
```

Preview content is generated on create/edit and can be manually overridden before save. Saving preview never writes target app files.

### `route_pool_members`

Replace account foreign key usage with:

| Column | Type | Notes |
| --- | --- | --- |
| `id` | TEXT PK | UUID |
| `platform` | TEXT NOT NULL | Agent platform |
| `route_credential_id` | TEXT NOT NULL | FK-like reference |
| `enabled` | INTEGER NOT NULL | 1/0 |
| `sort_order` | INTEGER NOT NULL | Round-robin order |
| `created_at` | TEXT NOT NULL | ISO timestamp |

Unique on `(platform, route_credential_id)`.

### Batches

Bulk official imports may create or attach a `batches` row and set `route_credentials.batch_id`. Account list expands batch groups. Ungrouped credentials render as flat rows.

## CPA Import Contract

### Source of truth

CPA means CLIProxyAPI auth-file JSON as used by cockpit-tools / CLIProxyAPI token storage, not the pro-api wrapper export.

### Accepted official formats

1. Single object:

```json
{
  "type": "codex",
  "email": "user@example.com",
  "id_token": "...",
  "access_token": "...",
  "refresh_token": "rt_...",
  "account_id": "ac_...",
  "last_refresh": "...",
  "expired": "..."
}
```

```json
{
  "type": "claude",
  "email": "user@example.com",
  "id_token": "...",
  "access_token": "sk-ant-oat01-...",
  "refresh_token": "sk-ant-ort01-...",
  "last_refresh": "...",
  "expired": "..."
}
```

2. JSON array of the above objects.
3. Nested token objects:

```json
{
  "type": "codex",
  "email": "user@example.com",
  "tokens": {
    "id_token": "...",
    "access_token": "...",
    "refresh_token": "rt_..."
  },
  "account_id": "ac_..."
}
```

4. CamelCase aliases: `accessToken`, `refreshToken`, `idToken`, `accountId`.

### Import UX

- Single mode: one multiline paste box.
- Bulk mode: native multi-file picker for `.json` files.
- Current agent tab supplies expected platform.
- Parser normalizes into `route_credentials`.
- Result shape:

```json
{
  "imported": [{"id":"...","display_name":"..."}],
  "failed": [{"label":"file-or-email","error":"..."}]
}
```

### Validation rules

- Reject non-JSON.
- Reject wrapper `{accounts:[{provider,email,credentials}]}`.
- Reject credentials whose `type`/token shape does not match current platform.
- Require at least one routable secret:
  - Codex official: `access_token` or `refresh_token`
  - Claude official: `access_token` or `refresh_token`
  - Gemini official: only if a future CPA shape is added; v1 Gemini create focuses on API mode unless a clear CPA object is supplied later
- Empty paste / no selected files fails fast.
- Bulk failures are per item; successful items still save.

### Explicit non-goals for official create

- No email field + password field create form.
- No browser OAuth capture in this slice.
- No auto disk import from `~/.codex/auth.json` unless later requested.

## API Credential Contract

### Required fields

- `display_name`
- `api_key`
- `base_url`

### Advanced fields

- `interface_format`: one of the 5 enum values
- `model_mappings`: array of `{from, to, label?}`
  - `from` and `to` required when a row exists
  - empty array allowed

### Defaults by platform

| Platform | Default `interface_format` |
| --- | --- |
| codex | `openai` |
| claude | `anthropic` |
| gemini | `gemini` |

User can override.

## Config Preview

### Generation

On create/edit, backend generates draft files for the current platform from credential data.

Codex draft intent:

- `auth.json`: official tokens or API-key style auth representation for inspection
- `config.toml`: model provider block / base URL hints aligned with how route writes will later target the local proxy

Claude / Gemini draft intent:

- `settings.json`: routing-oriented settings draft derived from credential + platform defaults

### Edit semantics

- Preview editors are free-text.
- Save stores exact edited draft into `preview_json`.
- Invalid JSON/TOML in preview is allowed to be stored as draft text, but UI should warn.
- Preview save does not touch filesystem.
- Explicit route config write uses the live proxy endpoint and current write templates; it does not dump arbitrary preview text to disk by default.

This keeps preview useful for inspection/adjustment without turning the create dialog into an uncontrolled file writer.

## UI Behavior

### Account list

Top-level agent tab content:

1. Fixed route-pool header row with stats icon
2. Account rows for current platform
3. Batch groups expandable when `batch_id` is present
4. Left checkbox joins/leaves pool
5. Right edit action opens credential editor
6. Top-right `+` opens create dialog

### Create dialog

Steps:

1. Choose kind: Official / API
2. Kind locked after first save
3. Official shows CPA paste or multi-file import
4. API shows base form + advanced options + live preview panels
5. Save creates one or many `route_credentials`

### Route pool header

- Shows member count and quick health
- Stats icon opens request logs / token / cost summary for current platform pool
- Pool membership changes are immediate

## Proxy Routing Behavior

### Selection

For an inbound request:

1. Detect platform from path/header/route metadata.
2. Load enabled `route_pool_members` for that platform ordered by `sort_order`.
3. Round-robin via existing cursor mechanism, updated to credential ids.
4. Skip credentials with `status != ok` or missing secrets.
5. If none available, return JSON 502 with stable error code such as `route_pool.empty`.

No fallback to unrelated `providers`.

### Outbound official

- Build upstream auth from `secret_payload_json` tokens.
- Use platform-native upstream endpoints / headers.
- Official credentials are first-class routable members.

### Outbound API

- Use `config_json.base_url`
- Attach API key according to `interface_format`
- Rewrite request model field(s) using first matching `model_mappings.from -> to`
- Forward remaining path/body conservatively

### Usage

Record:

- request count always
- token/cost when response JSON exposes them
- selected `route_credential_id`
- platform
- success/failure metadata

Stats icon reads these events filtered by current platform pool.

## Commands / Services

Suggested backend surface:

- `list_route_credentials(platform)`
- `get_route_credential(id)`
- `create_api_route_credential(input)`
- `import_official_route_credentials_from_text(platform, text, batch_name?)`
- `import_official_route_credentials_from_files(platform, paths, batch_name?)`
- `update_route_credential(id, input)`
- `delete_route_credential(id)`
- `set_route_pool_members(platform, credential_ids)`
- `get_route_pool(platform)`
- existing proxy start/stop/status/write-config commands, updated to select credentials

Services:

- `route_credential_service`
- `cpa_import_service`
- `route_preview_service`
- update `route_pool_service`
- update `route_proxy_service`

## Error Handling

| Case | Behavior |
| --- | --- |
| Invalid CPA JSON | Item/file fails with parse error |
| Platform mismatch | Item fails; others continue in bulk |
| Missing official tokens | Reject save / mark invalid |
| Missing API key or base URL | Form validation error |
| Invalid interface format | Validation error |
| Mapping row missing from/to | Validation error |
| Join pool with invalid credential | Reject membership update for that id |
| Empty pool during proxy | 502 + `route_pool.empty` |
| Upstream failure | Return upstream-mapped error; store failed usage event |
| Route config write failure | Return path + reason; DB preview unchanged |

Bulk import never rolls back already-saved successful items because of a later item failure.

## Testing

### Unit

- CPA parse: single object, array, nested tokens, camelCase
- CPA reject: wrapper accounts export, wrong platform type, empty secrets
- API validation: required fields, enum, mappings
- Model rewrite helper
- Preview generators for Codex / Claude / Gemini
- Pool selection: round-robin, skip invalid/disabled, empty pool

### Service / command

- Create API credential
- Import official text and files with mixed success/failure
- Update preview draft without filesystem writes
- Set pool members by credential ids
- Proxy selects official and API credentials correctly

### UI smoke

- Agent tab list renders credentials only for current platform
- `+` supports official and API modes
- Bulk official import shows imported/failed summary
- Checkbox updates pool
- Stats drawer shows usage summary
- Preview editors editable; save does not write disk
- Explicit write-config action still required for target CLI files

## Acceptance Criteria

1. Official accounts can be created only through CPA paste or multi-file JSON import.
2. API accounts support `apiKey`, `baseUrl`, 5 interface formats, and `[{from,to,label?}]` mappings.
3. Account list, route pool, and proxy all use `route_credentials`.
4. Official and API credentials can both join the pool and be selected for real routing.
5. Preview drafts exist for Codex (`auth.json` + `config.toml`), Claude (`settings.json`), and Gemini (`settings.json`), saved only to DB.
6. Real CLI config writes remain an explicit separate action that points apps at the local proxy.
7. No legacy migration path is implemented.
8. No competitor names appear in user-facing copy.

## Implementation Notes

- Prefer one migration that creates `route_credentials` and reshapes `route_pool_members`.
- Keep create-kind immutable to avoid secret/config shape ambiguity.
- For v1 Gemini official CPA, implement parser hooks only if a concrete CLIProxyAPI gemini auth object is available; otherwise ship Gemini API mode first without blocking Codex/Claude official routing.
- Delete or stop using the current simple `createOfficialAccount` metadata form on Accounts screen as part of this slice.
- Continue using Settings as the only place for MCP and other non-core entries.

## Non-Goals Recap

- Email/password official onboarding
- Wrapper `cliproxy_export` accounts array compatibility
- Migrating unreleased old rows
- Encrypting secrets in v1
- Making preview editors directly authoritative for disk writes
