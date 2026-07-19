# Route Model Connectivity Test Design

## Context

The current "测试路由" action calls `route_pool_route_once`. That service only selects the
next route-pool credential, advances the cursor, and writes synthetic usage events. It does
not send an HTTP request to an upstream model provider, so it cannot prove that base URL,
API key, access token, interface format, model mapping, or provider response handling works.

The route proxy path already has most of the real behavior: it loads route-pool credentials,
selects one by cursor, builds the upstream request, injects provider authentication, applies
model mappings for JSON bodies, sends the request with `reqwest`, reads the response, extracts
token/cost usage when possible, records usage events, and advances the cursor.

## Goal

Replace the misleading UI-level test with a real model connectivity test.
The user can click a button, the app sends a minimal prompt to the selected upstream route
credential, and the result shows the exact fixed test input plus upstream status, extracted
model output, raw response snippet, latency, and selected account.

## Non-Goals

- Do not log normal user proxy request bodies or response bodies.
- Do not store API keys, access tokens, refresh tokens, `Authorization`, or `x-api-key` values.
- Do not remove `route_pool_route_once`; keep it for existing internal tests and compatibility.
- Do not implement a full prompt playground or arbitrary chat UI.
- Do not require users to type a prompt in the first version.

## User Experience

In the route pool panel, keep the button label as "测试路由" so the control stays compact.
The result card title and detail content should clarify that this is a model connectivity test.
The button must call the app's internal backend test command directly. It must not require the
local route proxy to be running, and it must not require route config files to be written first.

When clicked:

1. Select the next enabled credential from the current platform pool using the same cursor
   behavior as proxy routing.
2. Send a fixed low-token prompt:
   `Reply with exactly: ai-switch-ok`
3. Show a compact result card:
   - success or failure
   - selected account name
   - interface format
   - upstream path
   - HTTP status when available
   - latency in milliseconds
   - extracted model output when available
4. Provide expandable details showing:
   - sanitized request body JSON
   - response body text or JSON, truncated to a safe size
   - error message when the request failed before a response

The request statistics panel should show these manual tests as request rows. Expanding a test
row shows the same safe input/output metadata in `metadata_json`.

## Backend API

Add a new command instead of changing `route_pool_route_once`:

- Tauri command: `route_pool_test_model`
- Web handler command: `route_pool_test_model`
- Frontend client: `routePoolTestModel(request)`

Input:

```ts
type RoutePoolModelTestRequest = {
  platform: string;
};
```

Output:

```ts
type RoutePoolModelTestOutcome = {
  platform: string;
  selected_account_id: string;
  selected_account_name: string;
  interface_format: string;
  request_path: string;
  request_body_json: string;
  response_status?: number | null;
  response_body: string;
  response_text?: string | null;
  error_message?: string | null;
  success: boolean;
  duration_ms: number;
  stats: RoutePoolStats;
};
```

`success` is `true` when an upstream response is received with a 2xx status. Non-2xx responses
are successful transport but failed connectivity, so they return `success: false` with status
and response body available.

Network/send/build errors return an outcome with `success: false` when a credential was selected,
so the UI can show which account failed. Validation errors before credential selection, such as
an empty pool, continue to return normal API errors.

## Request Construction

Use the existing selected credential and upstream request-building logic where possible, so the
test exercises the same auth and mapping behavior as real proxy traffic.

The fixed test prompt is:

```text
Reply with exactly: ai-switch-ok
```

Derive the interface format:

- API credentials: read `config_json.interface_format`, defaulting to `openai`.
- Official credentials: derive from platform: `claude` uses `anthropic`, `gemini` uses `gemini`,
  all other current platforms use `openai`.

Derive model:

- Use the first `model_mappings[].from` as the request-side model when present.
- If there is no mapping, use a platform/interface default:
  - OpenAI-style: `gpt-5`
  - Anthropic-style: `claude-sonnet-4-20250514`
  - Gemini: `gemini-2.5-flash`

Build request by interface:

- `openai`: `POST /chat/completions` with `model`, one user message, `temperature: 0`,
  and `max_tokens: 16`.
- `openai-responses`: `POST /responses` with `model`, `input`, `temperature: 0`,
  and `max_output_tokens: 16`.
- `anthropic` and `anthropic-messages`: `POST /v1/messages` with `model`, one user message,
  and `max_tokens: 16`.
- `gemini`: `POST /v1beta/models/<model>:generateContent` with one text part and
  `generationConfig.maxOutputTokens: 16`.

For Gemini, model names are part of the URL. If mappings are present, use the first
`model_mappings[].to` for the Gemini URL because the existing JSON-body model rewriting cannot
rewrite a model embedded in a path.

## Response Parsing

Store the raw response body as text, truncated to 16 KiB.

Also extract a best-effort `response_text` from common provider shapes:

- OpenAI chat completions: `choices[0].message.content`
- OpenAI responses: `output_text` or `output[*].content[*].text`
- Anthropic: `content[0].text`
- Gemini: `candidates[0].content.parts[0].text`

If extraction fails, `response_text` is `null` and the raw response snippet remains available.

## Usage Event Metadata

Record one request usage event for each selected-account test attempt with source
`route_pool_model_test`.

Metadata includes:

- `source`: `ui_model_connectivity_test`
- `request_kind`: `model_connectivity`
- `platform`
- `route_credential_id`
- `route_credential_name`
- `interface_format`
- `path`
- `status`
- `success`
- `duration_ms`
- `request_body_json`
- `response_body`
- `response_text`
- `error_message`

Record token and cost events when the provider response exposes supported usage fields.

## Privacy And Safety

This feature stores input/output only for the fixed manual test prompt. It must not capture or
store arbitrary user prompts from normal proxy traffic.

The stored request body contains no credentials. The stored response is truncated to 16 KiB.
Headers are not stored. Query strings containing API keys are not stored; metadata keeps only the
path, not the full target URL.

Use a request timeout of 30 seconds so a bad endpoint does not hang the UI indefinitely.

## UI Error Handling

The result card should distinguish:

- Empty route pool: validation error from the command.
- Request build error: selected account is known, no upstream status.
- Network timeout/connect error: selected account is known, no upstream status.
- HTTP error status: selected account and response body are available.
- 2xx response without extracted text: connectivity succeeds, but output extraction is unavailable.

## Testing

Rust tests cover:

- fixed request body/path generation for OpenAI chat, OpenAI responses, Anthropic, and Gemini.
- Gemini model mapping uses `to` in the URL.
- response text extraction for OpenAI, Anthropic, and Gemini response shapes.
- model test records failure metadata when upstream returns non-2xx.
- model test records request/token/cost usage when upstream returns usage fields.

Frontend tests cover:

- clicking "测试路由" calls `routePoolTestModel`.
- "测试路由" remains available without starting the local proxy or writing route config files
  when the pool has at least one enabled account.
- success result shows account, status, latency, extracted output, request JSON, and response body.
- failure result shows selected account and error/status details.
- request statistics detail view displays model connectivity metadata.
