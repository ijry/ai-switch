# Route Usage Breakdown Design

## Context

The route statistics screen currently reads request rows from `usage_events`, while
input/output/cache token usage and cost are stored as separate metric rows. The request
list therefore cannot show those values on the same row, and the summary cost only works
when a separate USD cost event exists.

The project already has a stable route request path for real proxy traffic and model
connectivity tests. This change keeps that path and makes each request record carry its
own usage breakdown.

## Goal

Store and display input tokens, output tokens, cache tokens, and upstream-reported price
for every recorded route request. Preserve the original currency in each row and show the
summary cost in USD using a fixed CNY-to-USD rate of 7.1.

## Non-Goals

- Do not calculate prices from model pricing tables or token counts.
- Do not invent a price when the upstream response has no price field.
- Do not change provider account pricing configuration.
- Do not store request bodies, response bodies, prompts, completions, or credentials in
  the new usage columns.
- Do not remove or rewrite historical `usage_events` rows.

## Design

### Database

Add nullable columns to `usage_events` in a new additive migration:

- `input_tokens INTEGER`
- `output_tokens INTEGER`
- `cache_tokens INTEGER`
- `price_usd_micros INTEGER`
- `price_cny_micros INTEGER`
- `price_currency TEXT`

Token columns store token counts. Price columns store the original amount in micro-units
of their named currency: one USD or CNY is 1,000,000 units. `price_currency` is `usd` or
`cny` and identifies the authoritative original price for display and aggregation.

The existing request, token, and cost event rows remain readable. New real requests write
one `request` row with the complete usage breakdown instead of adding separate token and
cost rows. Existing metric rows continue to support historical totals and existing manual
route-pool operations.

### Response Parsing

Add a shared response usage extraction result with nullable input, output, cache, and price
values. Only values actually present in the response are populated.

Support the provider usage shapes already represented by this application:

- OpenAI Chat Completions: `prompt_tokens`, `completion_tokens`, and
  `prompt_tokens_details.cached_tokens`.
- OpenAI Responses: `input_tokens`, `output_tokens`, and
  `input_tokens_details.cached_tokens`.
- Anthropic-compatible responses: `input_tokens`, `output_tokens`, and the sum of
  `cache_read_input_tokens` plus `cache_creation_input_tokens`.
- DeepSeek-compatible responses: `prompt_tokens`, `completion_tokens`, and
  `prompt_cache_hit_tokens`.
- Gemini usage metadata: `promptTokenCount`, `candidatesTokenCount`, and
  `cachedContentTokenCount`.

Prices are read only from explicit upstream fields. The parser accepts named USD/CNY
fields such as `price_usd` or `cost_usd`, `price_cny` or `cost_cny`, and a generic price
or cost value when it has an explicit currency/unit. A standard DeepSeek response normally
returns token usage rather than a per-response price; no price is synthesized for that
case. If both currencies are returned, the upstream currency indicator selects the
authoritative value, preventing double counting.

### Request Persistence

For each upstream credential attempt in real proxy traffic, persist one `request` event
after the response has been read. Persist the same shape for model connectivity tests.
Successful and failed attempts are both recorded. Failed attempts have status metadata as
today and nullable usage/price columns when no response usage is available.

The request event remains best-effort so a database write failure does not turn an otherwise
valid upstream response into a proxy failure. The event metadata remains safe routing
metadata only.

### Aggregation

Extend `RoutePoolStats` with input, output, and cache totals while retaining `token_count`
and `cost_micros` for API compatibility. For new request rows:

- input/output/cache totals sum their nullable request columns;
- `token_count` represents input plus output, with cache treated as a subset of input;
- USD price contributes `price_usd_micros` directly;
- CNY price contributes `price_cny_micros / 7.1`, rounded to USD micro-units.

Historical `token` rows and `cost` rows remain included when calculating legacy totals.
New request rows must not also create matching token/cost rows, so a request cannot be
counted twice. The existing `cost_micros` response field remains the total USD micro-unit
amount used by the current summary card.

Request rows returned by the repository include all nullable breakdown and price columns.
Rows from before this migration return null values and render as unavailable rather than
being assigned guessed usage.

### UI

Add input, output, cache, and price values to the request row. Token values are formatted
as counts. A price row uses the original `price_currency`: USD is rendered with `$`, CNY
with `¥`, and missing price with `-`. The expanded request detail repeats the same values.

The summary fee card remains USD-only. It uses the backend's `cost_micros`, which includes
direct USD prices, CNY prices converted at 7.1, and compatible historical USD cost events.

### Error Handling and Privacy

Malformed or missing usage fields are treated as unavailable fields for that request. They
do not fail proxy forwarding. Invalid metadata continues to use the existing per-row UI
fallback. No request or response payload is added to the database schema.

## Testing

Backend tests cover:

- extraction of input/output/cache values from OpenAI, Anthropic, DeepSeek, and Gemini
  usage shapes;
- extraction of explicit USD and CNY prices and omission when no price is returned;
- one request row containing the complete breakdown;
- USD aggregation of direct USD and CNY prices at the 7.1 rate;
- compatibility with historical separate token and cost events.

Frontend tests cover:

- rendering input, output, cache, and original-currency price values in request rows;
- rendering `-` for missing values;
- displaying a USD summary total that includes converted CNY prices;
- showing the same breakdown in request details.

## Expected Files

- Create one additive migration under `src-tauri/migrations`.
- Modify route usage models and repository queries under `src-tauri/src/models` and
  `src-tauri/src/database/repositories`.
- Modify response extraction and persistence in the route proxy/model-test services.
- Modify API types and the account statistics view.
- Extend the existing route pool, route proxy, model-test, and account-screen tests.
