# Account User-Agent Header Design

Date: 2026-07-24  
Status: Approved for planning

## Goal

Allow each route credential (API and official) to configure a custom `User-Agent` header from create/edit UI, with presets. The custom value must override built-in forced headers, including the Grok `cli-chat-proxy` forced UA, so users can fix proxy 407 / client-identity issues per account.

## Non-Goals

- Full multi-header editor UI
- Platform-global UA setting
- Changing non-UA forced Grok identity headers in this iteration (`x-grok-client-version`, `x-xai-token-auth`)
- Changing inbound client request header precedence beyond UA override

## Confirmed Decisions

1. Scope: all platforms, not only Grok
2. Priority: account custom UA wins over built-in forced UA
3. UI: create + edit account forms
4. Storage: reuse `config_json.headers["User-Agent"]`

## Current Behavior

- `apply_config_headers()` applies `config.headers` with `or_insert` only (missing keys)
- For official Grok + `cli-chat-proxy.grok.com`, `apply_official_grok_cli_headers()` force-sets:
  - `User-Agent: xai-grok-workspace/0.2.93`
  - `x-grok-client-version: 0.2.93`
  - `x-xai-token-auth: xai-grok-cli`
- CPA import normalizes outdated Grok UA (`grok-cli`) into the workspace UA for storage defaults
- Model test and proxy both use `build_upstream_request()`, so one header-priority fix covers both paths

## Design

### Storage

- Persist custom UA under `config.headers["User-Agent"]`
- Empty / default: omit the key entirely
- Preserve any other existing header keys
- When reading, accept either `User-Agent` or `user-agent` and normalize display/save to `User-Agent`

### UI

Add a first-class User-Agent control to:

- Create API account form
- Edit API account form
- Edit official account structured form

Control shape:

1. Preset select
2. Free-text input (editable after preset selection)

Presets:

| Preset | Value written |
|---|---|
| 默认（空） | omit `headers.User-Agent` |
| Grok Workspace | `xai-grok-workspace/0.2.93` |
| Grok CLI (legacy) | `grok-cli` |
| Browser | a fixed modern desktop Chrome UA string |
| 自定义 | keep current text / free input |

Behavior:

- Selecting a non-custom preset fills the text input
- Editing text after preset selection may switch the select to `自定义` when value no longer matches a preset
- Edit mode hydrates from existing `config.headers`
- Advanced JSON editor remains available; structured UA field and JSON preview stay consistent with existing form sync patterns

Official create via CPA/Sub2API import does not require a separate UA form field at import time. Users can adjust UA after import in edit form.

### Proxy / Request Header Priority

Final outbound header construction for official and API paths:

1. Start from inbound/forwarded headers (existing)
2. Apply credential auth and platform-specific headers (existing)
3. Apply `config.headers` fill-missing behavior where currently used
4. Apply Grok chat-proxy forced identity headers when applicable (existing)
5. **If credential has non-empty custom User-Agent, force-set `User-Agent` last**

Rules:

- Custom UA present => always wins
- Custom UA absent => keep current defaults, including Grok forced UA
- Other Grok identity headers remain forced by existing logic in this iteration
- Model connectivity test must inherit the same final UA behavior through shared request builder

Implementation note:

- Prefer a small helper such as `apply_credential_user_agent(headers, config)` called at the end of both API and official upstream builders, rather than special-casing only Grok
- Helper should read `config.headers` case-insensitively for the UA key

### Import Compatibility

- CPA / Sub2API imports continue to store headers into `config.headers`
- Existing Grok import normalization may still upgrade missing/outdated stored defaults
- After import, user edits via the new UA field override stored UA and remain effective at request time because custom UA is applied last

### Error / Empty Handling

- Whitespace-only UA is treated as empty (omit key)
- Invalid header characters should fail save or request build with a clear validation error, consistent with existing header insert failures
- No automatic network retry based on 407 in this design; user configures UA and retests

## Testing

### Rust

1. Custom UA overrides Grok chat-proxy forced UA
2. Empty UA keeps Grok forced UA on chat-proxy base URL
3. Custom UA applies on non-Grok platforms when present
4. Model-test path uses the same final UA (shared builder coverage is enough if builder is shared)

### Frontend

1. Create API account saves non-empty UA into `config.headers.User-Agent`
2. Create with default/empty omits UA key
3. Edit hydrates existing UA and can clear it back to default
4. Preset selection fills input; free edit remains possible

## Files Likely Touched

- `src/screens/AccountsScreen.tsx`
- optional small helper for presets / header read-write
- `src-tauri/src/services/route_proxy_service.rs`
- existing account/proxy tests nearby

## Acceptance Criteria

1. Any platform account can set a custom User-Agent in create/edit UI
2. Presets are available and editable
3. Saved value persists across refresh via `config.headers`
4. Custom UA overrides built-in forced UA, including Grok chat-proxy
5. Empty custom UA preserves previous default/forced behavior
6. Real proxy requests and account model tests use the same UA resolution

## Out of Scope Follow-ups

- Generic custom headers editor
- Per-platform default UA templates beyond presets
- Automatic 407 detection suggesting UA presets
