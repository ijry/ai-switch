---
title: Model Connectivity Tests
description: AI Switch model tests send a real generation request and record the post-bridge upstream body, the raw response, and the extracted text — plus how to fetch a provider's real model list.
---

# Model Connectivity Tests

Once a credential is configured, the thing you want to know is whether it actually produces output. AI Switch's model connectivity test answers that unambiguously: **it sends a real generation request, not a liveness probe.**

The request content is hardcoded:

```rust
pub const MODEL_TEST_PROMPT: &str = "Reply with exactly: ai-switch-ok";
```

The model is asked to echo `ai-switch-ok`, with a 16-token output cap and temperature 0. That is enough to confirm the whole chain works — auth, protocol bridging, model-name mapping, response parsing — while consuming almost no quota.

## What the request looks like

The request's shape is determined by the **platform**, not by the upstream dialect, because the test is simulating the local CLI's entry request and leaving the rest of the rewriting to protocol bridging.

| Platform | Entry path | Body |
| --- | --- | --- |
| `codex` | `/responses` | `{"model": …, "input": "<prompt>", "temperature": 0, "max_output_tokens": 16}` |
| `claude` | `/v1/messages` | `{"model": …, "messages": [{"role": "user", "content": "<prompt>"}], "max_tokens": 16}` |
| `gemini` | `/v1beta/models/{model}:generateContent` | `{"contents": [...], "generationConfig": {"temperature": 0, "maxOutputTokens": 16}}` |

The remaining platforms (Grok, OpenCode, OpenClaw, Hermes) have no fixed entry protocol, so the shape follows the credential's `interface_format`:

| `interface_format` | Path | Body shape |
| --- | --- | --- |
| `openai` | `/chat/completions` | `messages` + `temperature: 0` + `max_tokens: 16` |
| `openai-responses` | `/responses` | `input` + `temperature: 0` + `max_output_tokens: 16` |
| `anthropic` | `/v1/messages` | `messages` + `max_tokens: 16` |
| `gemini` | `/v1beta/models/{model}:generateContent` | `contents` + `generationConfig` |

Model name selection goes: an explicitly requested model → the credential's model mappings → the platform/dialect built-in default (`anthropic` → `claude-sonnet-4-20250514`, `gemini` → `gemini-2.5-flash`, the `grok` platform → `grok-4.5`, everything else → `gpt-5.5`). Placeholder mappings — empty values or the literal `upstream-model` — are filtered out and never selected.

### Tests and real traffic share one path

The key detail: once the entry request is assembled, the test calls **the same upstream-request builder the proxy uses for forwarding**. Model mapping, custom-tool compat, hosted-tool stripping, protocol bridging, and per-dialect auth assembly all run exactly as they would in production.

So the `request_body_json` in the result is **the post-bridge upstream body**, not the entry request you see described in the UI. That is precisely what you want when chasing a bridging problem — you can read what your Responses request actually became after being rewritten into Chat Completions.

The response likewise goes through the bridge's reverse conversion before text extraction.

### Dialect override

A test can temporarily specify a dialect, which is how you probe "what protocol does this gateway actually speak?" — but the allowed range is restricted:

| Platform | May be overridden to |
| --- | --- |
| `codex`, `claude` | `openai`, `openai-responses`, `anthropic`, `gemini` (all four) |
| `gemini` | `gemini` only |
| `grok`, `opencode`, `openclaw`, `hermes` | `openai` only |

Out-of-range values return `validation.route_model_test_interface_format`.

## Two test paths

### Direct to upstream

The default. After account selection, the app process sends an HTTP request straight to the upstream with a 30-second timeout.

Selection matches real forwarding: name an account ID and that one is tested; otherwise the pool candidates are used, round-robin within priority groups, taking a concurrency lease per candidate. If every account is saturated the call returns `route_pool.concurrency_exhausted`; if the pool has no usable account it returns `validation.route_pool_empty`.

### Through the local proxy

The other path sends the request to the **local proxy's entry address**, authenticates with the platform's local proxy key, and adds an `x-ai-switch-test-trace-id` header.

This path verifies "would a CLI request get through?", not merely "does the credential work" — proxy listening, platform identification, selection, and bridging are all in the chain. It only applies when no specific account is named; naming an account falls back to direct mode.

Because the proxy selects internally, the test side does not know in advance which account will be hit. So after the request completes it looks the account up by trace ID: it scans the most recent 50 request events with `source_label = 'route_proxy'`, finds the one whose `metadata_json.trace_id` matches, and reads the actual account ID, account name, and target URL out of it.

## Retries and failure classification

Retry count, interval, and semantic-failure threshold come from the account's failure policy (`config_json.failure_policy`), defaulting to 2 retries at 200 ms.

Two hard exceptions to the retry rules:

- **401 / 403 are never retried.** Retrying an auth failure accomplishes nothing except tripping upstream abuse controls sooner.
- **Deterministic quota exhaustion is not retried.** When a semantic failure is identified as quota exhaustion, the test short-circuits.

Otherwise, non-2xx status codes and semantic failures (the body is structurally a failure while the HTTP status is 200) both trigger a retry. A streaming request that disconnects before the completion event also counts as one semantic failure.

### What success and failure each do

**Success:**

- Clears the transient failure count and backoff window
- Pulls the account back to `ok` if it was `error` or `warning`
- For an explicit single-account test, additionally performs "explicit-test recovery", restoring the account fully into the pool

**Failure** routes by class:

| Classification | Result |
| --- | --- |
| Quota exhausted | Status written directly to `error` |
| Non-2xx HTTP | Records one `model_test_status` transient failure |
| Semantic failure | Records one `semantic_response_transient` transient failure |
| Permanent failure (e.g. revoked credential) | Status written to `revoked` |
| Any other retryable failure | Records one `model_test` transient failure |

Transient failures carry a backoff window; the exact thresholds and durations are in [Reliability and Auto Recovery](/en/guide/reliability).

**A `paused` account can be tested.** The code says so explicitly in a comment: an explicit test is exactly how a user determines whether a paused account has recovered, and success recovers it.

## What comes back in the result

Fields returned by one test:

| Field | Contents |
| --- | --- |
| `platform` | Platform |
| `selected_account_id` / `selected_account_name` | The account actually used |
| `via_route_proxy` | Whether the local-proxy path was taken |
| `route_proxy_entry_url` / `route_proxy_entry_path` / `route_proxy_trace_id` | Proxy-path specifics |
| `interface_format` | The upstream dialect actually used |
| `request_path` | Entry path |
| `base_url` / `target_url` | The credential's base URL and the full final URL |
| `request_body_json` | **The post-bridge upstream body**, pretty-printed |
| `response_status` | HTTP status code (empty on a transport-layer failure) |
| `response_body` | Raw response body, capped at 16 KiB |
| `response_text` | The model's reply text as extracted from the response |
| `error_message` | Error message |
| `success` / `duration_ms` | Whether it succeeded, and how long it took |
| `stats` | A full usage-stats snapshot for that platform |

`response_text` extraction differs by dialect:

| Dialect | JSON pointers (tried in order) |
| --- | --- |
| `openai` / `openai-responses` | `/choices/0/message/content` → `/output_text` → walk `/output[]/content[]/text` |
| `anthropic` | `/content/0/text` |
| `gemini` | `/candidates/0/content/parts/0/text` |

### Secret redaction

The response body and error message are redacted by substitution before they are written to the database: every sensitive key's value is pulled out of the credential's secret payload (`api_key`, `access_token`, `refresh_token`, `id_token`, `authorization`, `x-api-key`) and each occurrence in the text is replaced with `[redacted]`. So even if the upstream echoes your key back inside an error message, it does not reach storage.

### Every test records a usage event

Test results are written to the `usage_events` table with `source_label` set to `route_pool_model_test`, and `metadata_json` carrying:

```json
{
  "source": "ui_model_connectivity_test",
  "request_kind": "model_connectivity",
  "platform": "codex",
  "route_credential_id": "…",
  "route_credential_name": "…",
  "interface_format": "openai",
  "path": "/responses",
  "base_url": "…",
  "target_url": "…",
  "status": 200,
  "success": true,
  "duration_ms": 812,
  "request_body_json": "…",
  "response_body": "…",
  "response_text": "ai-switch-ok",
  "error_message": null
}
```

Any usage information the upstream returned is also parsed into token and price breakdown columns and stored alongside. Because test events and real forwarding events live in the same table, the request count on the stats page includes every test you clicked by hand — worth remembering when reading the numbers. See [Usage and Request Stats](/en/guide/usage-stats).

## Fetching the upstream model list

A model test needs a model name, and which models a third-party gateway serves is often known only to that gateway. So there is a separate model-list fetch that simply asks the upstream for its catalog.

It requires a non-empty base URL and API key, with a 15-second timeout. Candidate URLs are tried in dialect-specific order:

| Dialect | Candidate URLs (in order) | Auth |
| --- | --- | --- |
| `openai` / `openai-responses` (default) | `{base}/models` | `Authorization: Bearer` + Codex CLI client identity |
| `anthropic` | `{base}/v1/models` → `{base}/models` | `x-api-key` or `Authorization: Bearer`, plus `anthropic-version: 2023-06-01`, `anthropic-beta`, and Claude Code client identity |
| `gemini` | `{base}/models` when base already ends in `/v1beta` or `/v1`; otherwise `{base}/v1beta/models` → `{base}/v1/models` | Key in a query parameter; `Authorization` and `x-api-key` are actively removed |

Failure handling is deliberately restrained: **only 404 and 405 fall through to the next candidate.** Any other non-2xx returns `validation.route_models_http` immediately rather than retrying pointlessly. If every candidate fails, the result is `validation.route_models_all_failed`.

### Response normalization

Upstream response structures vary wildly, so parsing normalizes recursively:

- Container keys are recognized in the order `data`, `models`, `items`, and expanded recursively
- Model IDs are tried as `id`, `name`, `model`, `slug`
- Ownership info is tried as `owned_by`, `ownedBy`, `provider`, `display_name`, `displayName`
- Long-context flags are recognized as `supports_1m` / `supports1m`
- A Gemini-style `models/gemini-2.5-flash` prefix is stripped
- Plain string arrays parse too
- Results are sorted by ID and de-duplicated

The fetched list is written into the credential's `config_json.fetched_models`, so editing an account lets you pick a model from a dropdown without re-fetching each time.

## The live request log

A model test gives you a snapshot of one request. To watch **ongoing** traffic, the proxy side has a live request log capturing four stages of each forwarded request: the raw client request, the rewritten upstream request, the raw upstream response, and the final response returned to the client.

It is an in-memory ring buffer with a capacity of 100 entries and a 64 KiB cap per stage body, never written to disk, and it only pushes events while there is a subscriber. Details in [Protocol Routing and Bridging](/en/guide/protocol-routing).

## Next

- [Protocol Routing and Bridging](/en/guide/protocol-routing) — understand where that upstream body in `request_body_json` came from
- [Accounts and the Pool](/en/guide/accounts) — model mappings and credential fields
- [Reliability and Auto Recovery](/en/guide/reliability) — what happens to an account after a test fails
- [FAQ](/en/faq)
