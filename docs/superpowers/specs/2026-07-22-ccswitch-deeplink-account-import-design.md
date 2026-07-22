# CC-Switch Compatible Deep-Link Account Import

## Goal

Support opening AI Switch from a web page via a custom URL scheme and one-click adding an API route credential after user confirmation.

Compatibility target is the existing cc-switch provider deep-link format, while also registering an AI Switch native scheme.

## Confirmed Decisions

- Dual scheme support: `ccswitch://` and `aiswitch://`
- Confirm-before-import dialog
- First version supports only `resource=provider`
- Successful import only creates the account; it does not auto-add to the route pool
- Official CPA/OAuth deep-link import is out of scope for v1
- MCP / Prompt / Skill resources are out of scope for v1

## Non-Goals (v1)

- Auto-switch current provider/profile
- Auto-join route pool
- Multi-endpoint failover from comma-separated endpoints
- Full cc-switch `config` / `configUrl` merge
- Usage-script deep-link fields
- Web-only remote mode deep-link handling without desktop shell

## Success Criteria

1. Cold start and warm start both receive deep-link URLs.
2. Valid provider links open a confirmation dialog showing platform, name, endpoint, and masked API key.
3. Confirming creates an API account via the existing route-credential path.
4. Invalid links emit clear, user-visible errors.
5. Existing accounts page list refreshes and focuses the corresponding platform tab after success.

## Protocol

### Supported URL shape

```text
{ccswitch|aiswitch}://v1/import?resource=provider&app=claude&name=My%20Provider&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-xxx
```

Rules:

- scheme: `ccswitch` or `aiswitch`
- host/version: `v1`
- path: `/import`
- required query: `resource=provider`, `app`, `name`
- required for create: `endpoint`, `apiKey`

### Supported provider query fields

| Field | Required | Behavior |
|---|---|---|
| `resource` | yes | Must be `provider` |
| `app` | yes | Platform selector |
| `name` | yes | `display_name` |
| `endpoint` | yes for create | First valid URL becomes `base_url` |
| `apiKey` | yes for create | API secret |
| `model` | no | Non-Claude mapping source |
| `haikuModel` | no | Claude mapping |
| `sonnetModel` | no | Claude mapping |
| `opusModel` | no | Claude mapping |
| `homepage` | no | Preview only |
| `notes` | no | Preview only |
| `icon` | no | Preview only |
| `enabled` | no | Ignored in v1 |

Ignored but non-blocking in v1:

- `config`, `configFormat`, `configUrl`
- `usageScript`, `usageEnabled`, `usageApiKey`, `usageBaseUrl`, `usageAccessToken`, `usageUserId`, `usageAutoInterval`

### Platform mapping

| Deep-link `app` | AI Switch platform | Default interface format |
|---|---|---|
| `claude` | `claude` | `anthropic` |
| `codex` | `codex` | `openai-responses` |
| `gemini` | `gemini` | `gemini` |
| `grok` / `xai` | `grok` | `openai` |
| `opencode` / `openclaw` | unsupported | error |

Notes:

- Extra aliases `grok` / `xai` are intentional even if stock cc-switch docs emphasize claude/codex/gemini.
- Unsupported apps must fail with an explicit message, not silently coerce.

### Endpoint handling

- Accept comma-separated endpoints.
- Validate each candidate is `http` or `https`.
- Use the first non-empty valid endpoint as `base_url`.
- Extra endpoints are discarded in v1.

### Model mapping rules

Claude role keys must match the accounts UI templates:

| Deep-link field | Mapping `from` | Mapping `label` |
|---|---|---|
| `haikuModel` | `claude-haiku-4-5` | `Haiku` |
| `sonnetModel` | `claude-sonnet-5` | `Sonnet` |
| `opusModel` | `claude-opus-4-8` | `Opus` |

Rules:

- Include only fields that are present and non-empty.
- If none are present, use `[]`.
- Do not invent fallback Claude mappings in v1.

Other platforms:

- If `model` is present, create one mapping:
  - `from`:
    - `codex` -> `gpt-5`
    - `gemini` -> `gemini-2.5-flash`
    - `grok` -> `grok-3`
  - `to`: deep-link `model`
- If absent, use `[]`.

This matches the product rule that default model mappings stay empty unless explicitly provided.

## Architecture

### Components

1. **Deep-link registration**
   - `tauri-plugin-deep-link`
   - schemes in `tauri.conf.json`: `ccswitch`, `aiswitch`

2. **Single-instance bridge**
   - `tauri-plugin-single-instance`
   - second launches forward argv deep-link URLs into the running app

3. **Rust parser/mapper**
   - `src-tauri/src/services/deeplink_service.rs`
   - pure functions:
     - `parse_deeplink_url(url) -> DeepLinkProviderImport`
     - `to_create_api_input(parsed) -> CreateApiRouteCredentialInput`

4. **Rust runtime handler**
   - focus main window
   - parse URL
   - emit frontend events:
     - `deeplink-import`
     - `deeplink-error`

5. **Frontend confirmation dialog**
   - global listener in app shell
   - confirm calls existing `createApiRouteCredential`
   - success navigates/focuses Accounts page platform tab and refreshes list

### Data flow

```text
Web page link
  -> OS protocol handler
  -> AI Switch (cold or warm start)
  -> parse_deeplink_url
  -> emit deeplink-import payload
  -> confirmation dialog
  -> create_api_route_credential
  -> account list refresh
```

### Payload shape (frontend event)

```ts
type DeepLinkProviderImportPayload = {
  scheme: "ccswitch" | "aiswitch";
  version: "v1";
  resource: "provider";
  app: string;
  platform: "claude" | "codex" | "gemini" | "grok";
  display_name: string;
  base_url: string;
  api_key_masked: string;
  api_key: string; // only for confirmed create call path
  interface_format: string;
  model_mappings_json: string;
  homepage?: string | null;
  notes?: string | null;
  source_url_sanitized: string; // apiKey redacted
};
```

Security note: full `api_key` is held only in memory for the active confirmation session. Logs and toasts must use masked values only.

## UI / UX

### Confirmation dialog

Show:

- import type: API 账号
- platform
- display name
- base URL
- masked API key
- model mapping summary (`N` mappings or "空")
- source scheme

Actions:

- 取消
- 确认导入

After success:

- toast success
- switch to Accounts screen
- select matching platform tab
- reload credentials for that platform

After cancel:

- drop in-memory payload
- no DB write

### Error UX

- parse failures: toast or lightweight error dialog with message
- create failures: keep dialog open and show recoverable error text

## Scheme Contention

Windows can only bind one default handler per scheme.

Policy:

- Register both `ccswitch` and `aiswitch`.
- Document that if CC Switch and AI Switch are both installed, the OS decides which app owns `ccswitch://`.
- Prefer `aiswitch://` for AI Switch-native share links.
- Keep `ccswitch://` parsing so existing web links still work when AI Switch is the registered handler.

## Error Cases

| Case | Result |
|---|---|
| wrong scheme | `deeplink-error` |
| version != v1 | `deeplink-error` |
| path != /import | `deeplink-error` |
| resource != provider | `deeplink-error` with "暂不支持" |
| unsupported app | `deeplink-error` |
| missing name/app | `deeplink-error` |
| missing/invalid endpoint | `deeplink-error` |
| missing apiKey | `deeplink-error` |
| create API fails | dialog error |

## Testing

### Unit tests

- parse valid claude/codex/gemini/grok links
- accept both schemes
- reject bad scheme/version/path/resource
- multi-endpoint chooses first valid URL
- Claude model fields map correctly
- empty model fields produce `[]`
- unsupported app fails clearly

### Integration / manual

- cold start from browser link
- warm start while app already running
- confirm import creates account
- cancel does nothing
- success lands on correct platform tab

## Implementation Notes

- Reuse `RouteCredentialService::create_api`; do not invent a second create path.
- Keep parser pure and independent from Tauri runtime for easy testing.
- Prefer additive dependencies only:
  - `tauri-plugin-deep-link`
  - `tauri-plugin-single-instance`
- Do not expand into MCP/prompt/skill until those product surfaces exist.

## Rollout

1. Land parser + mapping + unit tests.
2. Wire desktop deep-link/single-instance runtime.
3. Add confirmation dialog and account refresh.
4. Document example links for both schemes.

## Open Follow-ups (not v1)

- Optional official CPA deep-link payload
- Optional confirm-dialog checkbox for route-pool membership
- Optional `config` Base64 merge for advanced provider packs
- Optional MCP/prompt/skill once those modules exist


