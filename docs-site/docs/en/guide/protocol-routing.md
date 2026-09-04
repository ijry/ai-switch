---
title: Protocol Routing and Bridging
description: How the AI Switch local proxy bridges native Codex, Claude Code, and Gemini CLI requests onto four upstream dialects — Chat Completions, Responses, Anthropic Messages, and Gemini generateContent.
---

# Protocol Routing and Bridging

Most account switchers solve a config-file problem: swap the CLI's base URL and key for another provider's. That approach has a low ceiling — **Codex only speaks the Responses API, Claude Code only speaks the Anthropic Messages API**. If the third-party key in your hand is OpenAI Chat Completions shaped, editing config gets you nowhere; the upstream simply won't recognize what the CLI sends.

AI Switch inserts a local HTTP proxy in the middle instead. The CLI keeps speaking its native tongue; the proxy rewrites the request into whatever dialect the upstream account understands, then rewrites the answer back into the shape the CLI expects. **One Codex client can therefore reach a Chat Completions gateway, a Responses gateway, an Anthropic gateway, or even Gemini.**

## The local proxy

The proxy binds loopback only:

```rust
const BIND_HOST: &str = "127.0.0.1";
const DEFAULT_ROUTE_PROXY_PORT: u16 = 19527;
```

Default port **19527**. If that port is taken, the bind logic scans upward from 19527 until it finds a free one, so the authoritative port is the `base_url` in runtime state, not the constant.

When local HTTPS is on, HTTPS **does not share the HTTP port**: HTTP keeps serving where it was, and HTTPS binds the next free port up from it (normally 19527/19528). Both listeners share the same routing logic and credential pool. Client configs always receive the HTTP address — clients that ship their own CA bundle (curl on macOS/Linux, Node-based CLIs) cannot see the root certificate in the system trust store, so writing `https://` would break them outright. The HTTPS address (`https_base_url` in runtime state) is there to be pasted by hand when a client genuinely requires TLS. If HTTPS cannot start, the reason is recorded and HTTP is unaffected.

### Platform identification

Every platform shares one port, so each inbound request first has to answer "which CLI is this?". The order is:

1. **The local proxy key.** The inbound key is pulled from `Authorization: Bearer`, `x-api-key`, or a query parameter and looked up in the platform mapping table. That table is cached in memory with a 30-second TTL; a cache miss falls through to the database, so a key you just created works without waiting for the TTL to lapse.
2. **The `x-ai-switch-platform` header.** Manual debugging and custom clients can name the platform outright.
3. **Neither present is an error.** A key with no mapping returns `route_proxy.key_invalid`; no key at all returns `route_proxy.platform_unresolved`. Both are 401 with `WWW-Authenticate: Bearer`.

The inbound key is **for local authentication only and is never forwarded upstream**: before forwarding, proxy auth headers and query parameters are explicitly stripped, then real credentials are attached according to the upstream account's dialect.

### How each CLI connects

When writing CLI config, AI Switch touches only its own section and leaves everything else intact (TOML goes through `toml_edit` to preserve comments and unmanaged blocks; JSON keeps the existing indentation style).

| Platform | Config file | What gets written |
| --- | --- | --- |
| Codex | `~/.codex/config.toml` | `model_provider = "ai-switch"`, `model_catalog_json = "ai-switch-model-catalog.json"`, plus a `[model_providers.ai-switch]` block: `base_url = "<proxy>/v1"`, `wire_api = "responses"`, `experimental_bearer_token = "<proxy key>"` |
| Claude Code | `~/.claude/settings.json` | `env.ANTHROPIC_BASE_URL`, `env.AI_SWITCH_ROUTE_PROXY`, `env.AI_SWITCH_ROUTE_PROXY_API_KEY`, plus `aiSwitch.routeProxy.{enabled,baseUrl,platform,apiKey}` |
| Gemini CLI | `~/.gemini/settings.json` | Same structure; the base URL variables are `GEMINI_API_BASE_URL` and `GOOGLE_GEMINI_BASE_URL` |
| Grok | `~/.grok/settings.json` | Same structure; the base URL variables are `XAI_API_BASE_URL` and `GROK_API_BASE_URL` |

The Codex provider block deliberately uses `experimental_bearer_token` rather than `api_key`, and rendering actively deletes any leftover `api_key` key so the two forms never coexist.

OpenCode, OpenClaw, and Hermes have **no config-write adapter**. They participate through route credentials only; you point the client at the proxy address yourself. See [Platform Support Matrix](/en/guide/platform-support).

## Four upstream dialects

The upstream protocol dialect comes from the credential's `interface_format` field, and there are exactly four legal values:

| Dialect | Protocol | Upstream path | Auth |
| --- | --- | --- | --- |
| `openai` | Chat Completions | `/chat/completions` | `Authorization: Bearer <key>` |
| `openai-responses` | Responses API | `/responses` | `Authorization: Bearer <key>` |
| `anthropic` | Messages API | `/v1/messages` | `x-api-key` or `Authorization: Bearer` (per `api_key_field`) |
| `gemini` | generateContent | `/v1beta/models/{model}:generateContent` | `?key=<key>` query parameter |

Each dialect carries extra handling of its own:

- **`anthropic`**: fills in `anthropic-version: 2023-06-01` when the client hasn't supplied it; appends `?beta=true` to the messages path; applies Claude Code's client identity so gateways that fingerprint clients don't reject the request as unknown.
- **`openai` / `openai-responses`**: applies Codex CLI's client identity for the same fingerprinting reason; the request path has its leading version segment stripped (`/v1/...` → `/...`) before the base URL re-attaches it.
- **`gemini`**: the key goes in a query parameter, never a header. Streaming requests use `:streamGenerateContent` with `alt=sse`.

Whatever the dialect, outbound requests are forced to carry `accept-encoding: identity`. The reason is in a code comment: the outbound HTTP client is compiled without any decompression features, so a gzip/br/zstd response upstream would arrive as garbage at the relay and parse stages.

## Seven bridge links

Bridge kinds are enumerated exhaustively, and there are only seven:

```rust
pub enum ProtocolBridgeKind {
    ResponsesToChat,
    ResponsesToResponses,
    ResponsesToAnthropic,
    ResponsesToGemini,
    ClaudeToChat,
    ClaudeToResponses,
    ClaudeToGemini,
}
```

Laid out as "local entry × upstream dialect":

| Local entry | Upstream `openai` | Upstream `openai-responses` | Upstream `anthropic` | Upstream `gemini` |
| --- | --- | --- | --- | --- |
| Codex `/responses` | `ResponsesToChat` | `ResponsesToResponses` | `ResponsesToAnthropic` | `ResponsesToGemini` |
| Claude `/v1/messages` | `ClaudeToChat` | `ClaudeToResponses` | Passthrough (no bridge) | `ClaudeToGemini` |
| Any other entry / path | Passthrough | Passthrough | Passthrough | Passthrough |

Every link converts in both directions: the request side rewrites the entry format into the upstream format, and the response side rewrites the upstream answer back. Response conversion dispatches on `kind`, so each link has its own matched pair of implementations.

### When bridging actually happens

The condition is narrow — only two branches ever reach a bridge:

```rust
if platform == PlatformId::Codex && is_responses { /* … */ }
if platform == PlatformId::Claude && is_messages { /* … */ }
```

Everything else lands in `passthrough_request`: body forwarded verbatim, path normalized. Which means:

- **Entry-path matching ignores version segments.** `/responses`, `/v1/responses`, and `/v1/v1/responses` all count as the Responses creation path; same for messages.
- **Only creation endpoints get bridged.** A Codex request to something like `/v1/chat/completions` is not bridged.
- **Claude → `anthropic` is pure passthrough**, with an empty bridge kind. Same protocol on both sides needs no translation.
- **Codex → `openai-responses` still counts as a bridge** (`ResponsesToResponses`). Both sides are the Responses API, but third-party Responses gateways differ enough in implementation to need a normalization pass, so this is not a plain passthrough.
- **Traffic entering from Gemini CLI is never bridged.** `PlatformId::Gemini` does not appear in either branch, so Gemini CLI requests always pass through. That is the current capability boundary: Gemini CLI can only route to `gemini`-dialect accounts. The model test's dialect validation hardcodes the same rule — platform `gemini` permits only the `gemini` dialect.

### Streaming and non-streaming

Bridging handles both response shapes. Whether a request is streaming comes from the `stream` field in the body (absent means `false`).

Non-streaming goes through JSON structure conversion; streaming goes through SSE event-stream conversion, replaying the upstream event sequence as the entry protocol's event sequence. Bridging into a Responses entry, for example, emits `response.created`, `response.in_progress`, `response.output_text.delta`, `response.output_text.done`, and `response.completed`, with `response.function_call_arguments.delta` / `.done` for tool calls and `response.reasoning_summary_text.delta` / `.done` for reasoning summaries. The upstream `[DONE]` marker is stripped, because the Responses protocol does not use that terminator.

**Non-2xx responses are not converted** — they pass through verbatim. When the upstream errors, you see the upstream's original error body rather than a rewritten shape, which matters a great deal for diagnosis.

## Rewrites beyond the bridge

Bridging handles protocol shape. Several protocol-orthogonal rewrites stack on the same forwarding path.

### Model mapping

A credential's `model_mappings` applies before bridging: the model name in the body is substituted `from` → `to`. The `gpt-5` you selected in the CLI can become whatever model ID the upstream actually serves, with no CLI config change.

### Tool namespace flattening

The Responses protocol lets tools be organized into `namespace` groups; protocols like Chat Completions only have a flat function list. The bridge flattens groups into `namespace__tool` names and carries the mapping in the request context; on the way back it restores the grouped structure from the same table, so the client never notices anything happened.

### Custom-tool compat and hosted-tool stripping

Third-party Responses gateways frequently don't support Codex's custom-tool form, nor OpenAI's own hosted tools. Two layers handle this:

- With `config_json.responses_custom_tool_compat` on, custom tools are rewritten as ordinary function tools and restored on the response side.
- Under the same condition, hosted tool types the gateway can't run are stripped. The code enumerates seven: `web_search`, `web_search_preview`, `file_search`, `computer_use_preview`, `code_interpreter`, `image_generation`, `container_file_citation`. If `tool_choice` happened to pin a tool that got stripped, it is relaxed to `"auto"`.

**Codex + the `openai` dialect + a Responses path turns both layers on automatically** — no checkbox needed, because a Responses→Chat bridge by definition means the upstream is a Chat gateway and certainly won't accept those forms.

### Reasoning content back-fill

Chat-family reasoning models (DeepSeek, MiMo, and similar) require `reasoning_content` on assistant messages that carry tool calls, or they return 400 outright. Clients routinely drop that field across multi-turn tool calls. The proxy keeps a reasoning-content cache and back-fills the real `reasoning_content` — and the whole `function_call` when necessary — onto tool-call turns before the Responses→Chat conversion, so the model doesn't lose its own plan between tool calls and stall.

### Model list aggregation

The `/models` and `/v1/models` paths are not forwarded upstream. The proxy aggregates and de-duplicates the outward-facing model IDs of every usable credential in the pool and returns them directly in OpenAI model-list format, with `owned_by` fixed to `ai-switch`. The Codex platform additionally gets `supported_reasoning_levels` and `default_reasoning_level`. This path accepts `GET` only; other methods return 405 with `route_proxy.method_not_allowed`.

Fetching an **upstream** account's real model list is a different thing entirely — see [Model Connectivity Tests](/en/guide/model-test).

## The full order of one forward

```text
CLI request
  ├─ identify platform (proxy key → x-ai-switch-platform)
  ├─ /models path? → aggregate the pool's model list and return directly
  ├─ read the body (32 MiB cap), parse the requested model name
  ├─ select an account: in pool, status=ok, not archived, not cooling, quota not exhausted,
  │  round-robin within priority groups
  ├─ filter again by platform capability rules and the model name
  ├─ strip proxy auth headers/query params, strip hop-by-hop headers,
  │  force accept-encoding: identity
  └─ retry-queue loop, for each candidate account:
        ├─ take a concurrency lease (max_concurrency); skip to the next if unavailable
        ├─ refresh the access token for official credentials if needed
        ├─ model mapping → custom-tool compat → hosted-tool stripping → reasoning back-fill
        ├─ protocol bridge (one of the seven links, or passthrough)
        ├─ assemble auth and client identity per dialect, build the target URL
        ├─ send the upstream request
        ├─ return the response to the client after reverse bridge conversion
        └─ record the usage event, update account state, push the live log
```

Selection details are in [Accounts and the Pool](/en/guide/accounts); failure and backoff rules are in [Reliability and Auto Recovery](/en/guide/reliability).

## The live request log

Debugging a bridging problem from what the client received is not enough — you need the three steps in between. The proxy has a built-in live log capturing **four stages** of every forwarded request:

1. The raw client request
2. The rewritten upstream request
3. The raw upstream response
4. The final response returned to the client

Entries also carry the trace ID, the account ID and name that were selected, the attempt number, the path, the target URL, the requested and upstream model names, the status code, success, the error message, the duration, the bridge kind that matched, plus diagnostic notes and truncation flags.

Two important limits:

- **Nothing is ever written to disk.** This is an in-memory ring buffer with a capacity of 100 entries (shared across the whole proxy, not 100 per platform), and a 64 KiB cap on each stage's body, truncated beyond that.
- **Events are only pushed while someone is watching.** At least one subscriber must be present; subscribing to a platform first delivers that platform's existing entries.

Long strings and sensitive fields in bodies are redacted before they reach the log.

## Next

- [Accounts and the Pool](/en/guide/accounts) — credential fields and selection rules
- [Model Connectivity Tests](/en/guide/model-test) — verify the whole bridge chain with one real request
- [Usage and Request Stats](/en/guide/usage-stats) — what each forward records
- [Architecture Overview](/en/dev/architecture) — where the proxy sits in the system
