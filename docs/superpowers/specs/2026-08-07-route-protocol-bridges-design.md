# Route Protocol Bridges Design

## Goal

Allow a Codex client that sends OpenAI Responses requests through AI Switch to use an API route credential whose upstream contract is OpenAI Chat Completions, without requiring the user to hand-edit the generated Codex configuration.

## Confirmed Context

- Codex enters the local route proxy through `/v1/responses` and expects Responses JSON or Responses SSE.
- A route API credential with `interface_format = "openai"` targets `/chat/completions` and expects Chat Completions JSON or SSE.
- The current route proxy forwards the request path/body and buffers the upstream response, but has no protocol conversion layer.
- CC-Switch converts Claude Messages requests from `/v1/messages` to OpenAI Chat or Responses when its Claude provider is configured for those upstream formats. Its Codex `/v1/responses` handler is passthrough, so it is not a reference for Responses→Chat.

## Scope

### In scope

- Detect the Codex Responses entry path (`/responses`, `/v1/responses`, and equivalent leading version forms) before choosing the upstream path.
- Convert Responses request JSON to Chat Completions request JSON when the selected API credential uses `openai`.
- Convert non-streaming Chat Completions JSON back to a Responses response object.
- Convert buffered Chat Completions SSE back to Responses SSE event sequences, preserving text, tool calls, usage, finish state, and upstream errors where representable.
- Keep `openai-responses` credentials on the existing direct Responses path.
- Keep errors and unsupported payloads explicit and recoverable rather than silently forwarding a mismatched protocol.

### Out of scope for this slice

- Anthropic Messages ↔ OpenAI Chat/Responses conversion.
- Claude legacy `/v1/complete` conversion.
- Responses `/compact` semantic conversion.
- True chunk-by-chunk streaming from the local proxy; the existing proxy buffers upstream bodies, so this slice converts the complete buffered SSE payload before returning it.
- Automatic conversion between every pair of dialects.

## Architecture

Add a focused `services::route_protocol_bridge` module. It exposes a small, pure API:

- classify whether an inbound path and upstream dialect require a bridge;
- rewrite the outbound path and request body;
- rewrite a successful upstream response body based on content type/stream markers.

`route_proxy_service::build_api_upstream_request` calls the request-side bridge after model mapping is selected. `forward_request` calls the response-side bridge immediately after reading the upstream body and before usage extraction, semantic-failure detection, custom-tool restoration, and response construction. Non-bridged credentials retain the current behavior.

The bridge must not modify authentication headers. Authentication remains determined solely by the configured upstream dialect.

## Data Flow

1. Resolve the local proxy key to the Codex platform.
2. Select an enabled route credential.
3. Read its `interface_format`.
4. If the entry is Responses and the dialect is `openai`, convert:
   - `/v1/responses` → `/chat/completions`;
   - `input`/`instructions`/Responses tools → `messages`/system message/Chat tools;
   - `max_output_tokens` → `max_tokens` (or `max_completion_tokens` for o-series models).
5. Send the authenticated Chat request upstream.
6. If the request is non-streaming, convert `choices` to a Responses object.
7. If the request is streaming, parse each buffered `data:` SSE record and emit Responses lifecycle, text-delta, tool-call, usage, completion, or failure events.
8. Record usage from the converted Responses-compatible body and return it to Codex.

## Conversion Rules

- Preserve the requested model after the existing model-mapping pass.
- Accept Responses `input` as a string or array. Convert message items to Chat messages and convert `input_text`, `output_text`, image URLs, and file references only when a lossless Chat representation is available.
- Convert `instructions` to a leading `system` message.
- Convert Responses function tools to Chat `{type: "function", function: ...}` tools.
- Convert assistant `function_call` output items to Chat assistant `tool_calls`; convert `function_call_output` items to Chat tool messages.
- Copy common generation controls (`temperature`, `top_p`, `stream`, `stop`, `parallel_tool_calls`, and token limits) when the target dialect supports them.
- Build a stable Responses object with `output` message/function-call items, `output_text`, `status`, `model`, `id`, and normalized `usage`.
- For streaming, emit a valid Responses event envelope even when the Chat provider omits optional IDs or usage. Never emit malformed JSON or an invented successful completion after an upstream error.

## Error Handling

- Invalid JSON or an unsupported Responses item returns a request-build error and allows the existing credential retry/error reporting path to operate.
- A non-JSON upstream error body is passed through unchanged.
- A successful upstream status with an unconvertible JSON body is treated as a gateway conversion failure, not as a successful model response.
- Existing retry, quota, usage, and trace behavior remains unchanged outside the bridge.

## Testing

- Unit tests for path/dialect bridge selection.
- Unit tests for string input, multi-turn input, instructions, tools, tool results, token controls, and o-series token naming.
- Unit tests for non-streaming text, tool-call, usage, and error responses.
- Unit tests for representative Chat SSE text, tool-call, usage, `[DONE]`, and upstream error records.
- Existing route proxy and model-test tests must continue to pass.

## Follow-up: Claude

Claude Code's current native entry is Anthropic Messages `/v1/messages`; the old Anthropic Text Completions `/v1/complete` endpoint is a separate legacy API and is not the Claude Code route used by CC-Switch. A later slice can reuse the same bridge boundary to support `anthropic` → `openai` and `anthropic` → `openai-responses`, including reverse SSE conversion, following CC-Switch's provider/transform modules. It should be a separate spec because its content blocks, tool-use lifecycle, thinking blocks, and SSE state machine are independent of Codex Responses conversion.
