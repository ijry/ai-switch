---
title: Usage and Request Stats
description: How AI Switch records per-request input/output/cache tokens and dual-currency pricing, how it estimates cost from a local price table when the upstream sends none, the stats panel's time ranges and metric definitions, and how local Claude Code / Codex session usage is computed.
---

# Usage and Request Stats

After running a multi-account pool for a while you want three numbers: **how many requests, how many tokens, how much money**. AI Switch records every forward as one usage event, and what the numbers mean is decided entirely by the aggregation queries in the database. This page unpacks each of those definitions so the figures you read have unambiguous meaning.

## One request, one event

All usage lands in the `usage_events` table. The table itself dates from an early migration; the route-credential link and the breakdown columns came later:

| Migration | What it added |
| --- | --- |
| `202607130004_routing_usage.sql` | Created the table: `source_label`, `metric_type`, `amount`, `unit`, `metadata_json`, `created_at` |
| `202607130011_route_credentials.sql` | Added the `route_credential_id` column and its index |
| `202608060002_route_usage_breakdown.sql` | Added six breakdown columns |
| `202608200001_route_usage_price_source.sql` | Added `price_source`, separating a real upstream price from a local estimate |

Those breakdown columns are the entire source of token and cost data:

```sql
ALTER TABLE usage_events ADD COLUMN input_tokens INTEGER;
ALTER TABLE usage_events ADD COLUMN output_tokens INTEGER;
ALTER TABLE usage_events ADD COLUMN cache_tokens INTEGER;
ALTER TABLE usage_events ADD COLUMN price_usd_micros INTEGER;
ALTER TABLE usage_events ADD COLUMN price_cny_micros INTEGER;
ALTER TABLE usage_events ADD COLUMN price_currency TEXT;
ALTER TABLE usage_events ADD COLUMN price_source TEXT;
```

Prices are stored as integers in **micros**: 1 USD = 1,000,000 micros. That avoids floating-point accumulation error while preserving six decimal places. USD and CNY get a column each, and `price_currency` records which currency the upstream actually quoted.

### Who writes to this table

`source_label` identifies the event's origin, and it is the only way to tell "real traffic" from "a test I clicked":

| `source_label` | Origin | When it is written |
| --- | --- | --- |
| `route_proxy` | Local proxy forwarding | At the end of every forward (success and failure alike) |
| `route_pool_model_test` | Model connectivity test | Every time you click test |
| `route_pool` | A single in-pool routing call | When the routing API is invoked |

The written shape is fixed: `metric_type = 'request'`, `amount = 1`, `unit = 'count'` — one row per request. Tokens and prices no longer get their own rows; they fill the breakdown columns on that same row.

::: warning The stats include the tests you clicked
Model connectivity test events go into the same table with the same `metric_type` as real forwards, and **the stats panel does not separate them**. Testing frequently inflates the request count. The only way to distinguish them is the "source" column in the request list.
:::

## How upstream usage becomes tokens

Third-party gateways name their usage fields differently, so the proxy falls back through aliases and takes the first usable value:

| Metric | Fields tried, in order |
| --- | --- |
| Input tokens | `usage.input_tokens` → `usage.prompt_tokens` → `usageMetadata.promptTokenCount` |
| Output tokens | `usage.output_tokens` → `usage.completion_tokens` → `usageMetadata.candidatesTokenCount` |
| Cache tokens | `usage.input_tokens_details.cached_tokens` → `usage.prompt_tokens_details.cached_tokens` → `usage.prompt_cache_hit_tokens` → the sum of `cache_read_input_tokens + cache_creation_input_tokens` → `usageMetadata.cachedContentTokenCount` |

Between them these three chains cover the Responses, Chat Completions, Gemini, Anthropic, and DeepSeek-family conventions. Values may be integers or strings; negatives are discarded as invalid.

**No usage means empty — no token is estimated.** When the upstream returns no usage, the corresponding columns stay `NULL` and the UI shows `-`. AI Switch will not back-derive a token count from character length just to display a nicer-looking number. (**Price** is a different matter — when the upstream sends none, the amount is estimated from tokens, but always labelled as such; see [Estimating cost when the upstream sends no price](#estimating-cost-when-the-upstream-sends-no-price).)

### Streaming responses are accumulated frame by frame

A streaming response body is SSE text (`event: ...` / `data: {...}`), not a single JSON document, so parsing it whole always fails. In that case the parser falls back to reading each frame and merging their usage. **Claude Code and Codex both stream by default**, so this path covers the vast majority of real requests.

Providers put usage in different frames, and the merge has to handle all three:

| Upstream | Where usage appears |
| --- | --- |
| Anthropic | Split across two frames — `message_start` carries input and cache tokens (nested under `message`), `message_delta` carries output tokens |
| OpenAI | Only in the final chunk; earlier chunks have `usage: null` |
| Gemini | `usageMetadata` in the last frame |

When merging, each field takes the **last non-empty value** rather than a sum: Anthropic and Gemini re-report cumulative totals, so summing would double them.

If the stream is cut off, a partial frame is left at the end. The parser skips frames that fail to parse instead of discarding the whole body — the tokens already received are real spend and should not be lost to one broken tail.

## How upstream prices are parsed

Price parsing is another alias chain, and it distinguishes "already in micros" from "needs ×1,000,000":

| Target column | Micros fields | Plain-unit fields (auto ×1,000,000) |
| --- | --- | --- |
| `price_usd_micros` | `price_usd_micros`, `cost_usd_micros`, `cost_micros` | `price_usd`, `cost_usd` |
| `price_cny_micros` | `price_cny_micros`, `cost_cny_micros` | `price_cny`, `cost_cny` |

There is a generic layer too: if the upstream only supplies `price` or `cost` (possibly an object carrying its own `currency` / `unit`), the currency is identified first and the value routed to the matching column. Currency matching is loose — containing `usd` or `dollar`, or equal to `$`, means USD; containing `cny`, `rmb`, or `yuan`, or equal to `¥`, means CNY.

`price_currency` is determined in this order: an explicit currency field → the currency inside a generic price object → inference from whichever single column has a value. If none of the three settles it, the field stays empty.

## Estimating cost when the upstream sends no price

Anthropic, OpenAI, and Gemini return token counts but **no price**. If only upstream prices counted, the amount for those requests would always be zero — so when the upstream sends no price, the cost is estimated from tokens using a local price table.

The `price_source` column records where an amount came from, so an estimate never masquerades as a real charge:

| `price_source` | Meaning | In the UI |
| --- | --- | --- |
| `upstream` | The response carried an explicit price | Shown normally |
| `estimated` | Computed locally from tokens and the price table | Amount suffixed with 「估」 |
| `NULL` | No price at all | Shown as `-` |

The precedence is unambiguous: **an upstream price always wins and is never overwritten.** Estimation only happens when both price columns are empty.

### The price table and custom rates

The built-in table matches by model family (`claude-opus`, `claude-sonnet`, `gpt-5`, `gemini-2.5-flash`, …) in US dollars per million tokens. Cache tokens follow Anthropic's published rules: **a cache write costs the input rate × 1.25, a cache read × 0.1**.

Model IDs are normalized before matching: vendor prefixes are stripped (`anthropic/claude-opus-5-aws` → `claude-opus-5-aws`), as are context suffixes like `[1m]`. The longest matching pattern wins, so `claude-haiku-4-5` is not captured by the broader `claude`.

Discounted relay rates, or a new model not yet in the built-in table, can be overridden in `~/.ai-switch/model-prices.json`:

```json
{
  "claude-opus-5": { "input_per_mtok": 4.0, "output_per_mtok": 20.0 },
  "claude-sonnet": { "input_per_mtok": 1.5, "output_per_mtok": 7.5 }
}
```

A key can be a full model ID or a family (the `claude-sonnet` above matches every Sonnet). A missing file is normal and not an error; a malformed one falls back silently to the built-in table, and negative or non-finite rates are discarded rather than corrupting the totals.

::: warning An unknown model stays empty, not zero
A model absent from the price table is **not** treated as free — `price_source` stays `NULL`, the amount is excluded from the total, and the stats panel tells you separately how many requests went unpriced for this reason. That keeps "missing price data" distinguishable from "genuinely free". An estimate is still an estimate: upstreams usually report cache tokens as a single total without splitting reads from writes, and the estimator charges those at the cheaper read rate, so the result errs low.
:::

## What the stats panel actually measures

The stats panel lives in the "Stats" view on the accounts page and is scoped per platform. It has **four time ranges** and **six metric cards**.

### Four time ranges

| Option | `since` value |
| --- | --- |
| Today | Local time today at 00:00:00 |
| This week | Local time 00:00:00 on Monday of this week (Sunday counts as the previous week) |
| This month | Local time 00:00:00 on the 1st of this month |
| All time | No `since`, no lower bound |

`since` is passed to the backend as an RFC 3339 string; an invalid format returns `validation.route_pool_since`. The SQL clause it produces is a single `AND ue.created_at >= ?` — **a start point with no end point**. So these four options are really "cumulative totals since some moment", not bucketed statistics.

### Six metric cards

| Card | Backend field | Exact semantics |
| --- | --- | --- |
| Requests | `request_count` | Sum over `metric_type = 'request'` events; uses `amount` when `amount > 0`, otherwise counts 1 |
| Input tokens | `input_token_count` | Sum of `input_tokens` on request rows (`NULL` as 0) |
| Output tokens | `output_token_count` | Sum of `output_tokens` on request rows |
| Cache tokens | `cache_token_count` | Sum of `cache_tokens` on request rows |
| Total tokens | `token_count` | Sum of `input_tokens + output_tokens` on request rows, **plus** the `amount` of legacy `metric_type = 'token'` or `unit = 'token'` events |
| Total cost (USD) | `cost_micros` | See the conversion rule below; displayed divided by 1,000,000 to two decimals |

Two things are easy to misread:

- **Total tokens excludes cache tokens.** It is input plus output; cache tokens have their own card.
- **Total tokens carries a legacy definition.** Early versions recorded tokens as standalone event rows, and that historical data still counts toward the total while never appearing in the input/output/cache cards.

Beyond these six, the backend also returns `member_count` — the number of enabled, non-archived members in that platform's pool — used elsewhere in the UI.

### How cost is converted

`cost_micros` is normalized to USD micros:

```sql
COALESCE(SUM(CASE
    WHEN ue.metric_type = 'request' AND ue.price_currency = 'usd' THEN COALESCE(ue.price_usd_micros, 0)
    WHEN ue.metric_type = 'request' AND ue.price_currency = 'cny' THEN CAST(ROUND(COALESCE(ue.price_cny_micros, 0) / 7.1) AS INTEGER)
    WHEN ue.metric_type = 'cost' AND ue.unit = 'usd_micros' THEN ue.amount
    ELSE 0
END), 0) AS cost_micros
```

::: warning The exchange rate is hardcoded at 7.1
CNY-priced events are converted to USD by a fixed divisor of **7.1**. There is no rate API and no configuration option in the code. So when you mix USD- and CNY-priced gateways, total cost is a reference figure rather than a billing figure. Each row in the request list shows **the original amount in its original currency** — that is the number that is exact.
:::

### Which accounts are counted

Every stats query uses `INNER JOIN route_credentials` with `a.platform = ? AND a.archived_at IS NULL`. Therefore:

- **Isolated per platform.** Each platform's stats are independent; there is no cross-platform rollup view.
- **Archived accounts are excluded.** Archive an account and its historical events vanish from the stats immediately; un-archive it and they come back.
- **A deleted account's events no longer appear.** The event rows are not cascade-deleted with the account, but since the join no longer matches, they are absent from the aggregate.

## The request list

Below the metric cards is a per-request list, returned paginated and ordered by `created_at DESC, id DESC`.

- It contains only `metric_type = 'request'` rows.
- The UI fixes the page size at **20**; the backend accepts page sizes of 1–100 and clamps out-of-range values into that interval, and a page number below 1 is raised to 1.
- While the panel is open it auto-refreshes every **5 seconds**, and stops polling when closed.

Each row shows: time, account name, status code, path, model, token total, price, source. Of which:

- **The model column** renders as `requested->upstream` when the two differ, so an active model mapping is visible at a glance.
- **The token total** is input plus output; hovering reveals input, output, and cache separately.
- **Price** picks its symbol from `price_currency` — `¥` for CNY, `$` for USD — both to six decimal places; locally estimated amounts are suffixed with 「估」; no price shows `-`.

### Expanding a row

Clicking "details" expands a row to show the account name, account ID, source, metric (`amount` + `unit`), input/output/cache tokens, price, and timestamp, plus two raw blocks:

- **The raw upstream response**, from `metadata_json.response_body`. Successful requests keep only the first **2 KiB**; failed requests keep the first **16 KiB** — diagnosis needs the whole error body, whereas a short slice of a successful response is enough to identify it.
- **The full `metadata_json`**, pretty-printed. If it fails to parse it is shown verbatim with a notice.

The fields the proxy writes into `metadata_json`:

| Field | Contents |
| --- | --- |
| `platform` | Platform |
| `route_credential_id` / `route_credential_name` | The account that was used |
| `entry_path` / `path` | Entry path |
| `target_url` | The final upstream URL requested |
| `status` | HTTP status code |
| `success` | Whether it succeeded |
| `duration_ms` | Duration |
| `trace_id` | Trace ID (used to look up the selected account when a model test goes through the proxy) |
| `error_message` | Error message |
| `requested_model` / `upstream_model` | The model the client asked for and the model actually sent upstream |
| `response_body` | The truncated upstream response body |

Model tests write more fields into `metadata_json` — see [Model Connectivity Tests](/en/guide/model-test).

## The success rate on the account list

The request count and success rate shown on each account list row are a different aggregation with different semantics:

```sql
LEFT JOIN usage_events ue
  ON ue.route_credential_id = rc.id
 AND ue.source_label IN ('route_proxy', 'route_pool_model_test')
 AND ue.metric_type = 'request'
```

- **Only the `route_proxy` and `route_pool_model_test` sources count**; single in-pool routing calls do not.
- Success versus failure is decided by `json_extract(ue.metadata_json, '$.success') = 1`.
- Success rate = successes × 100 ÷ total; with no requests it is `NULL` (the UI shows `-`).
- **There is no time range.** This is the account's full history, and it does not follow the stats panel's range selector.

So for the same account, the request count on the stats panel (with "Today" selected) and the one on the account list are very likely to differ. That is not a bug; they are two different definitions.

## Local session usage

Everything above only sees **traffic that went through this app's proxy**. But Claude Code and Codex also record every request they make directly, in their own local session files — and the proxy never sees that spend. The "local session usage" block in the lower half of the stats panel is computed from those files, shown alongside the route stats for comparison.

Directories scanned:

| Client | Path | Environment override |
| --- | --- | --- |
| Claude Code | `~/.claude/projects/**/*.jsonl` | `CLAUDE_CONFIG_DIR` |
| Codex CLI | `~/.codex/sessions/**/*.jsonl` | `CODEX_HOME` |

This is **read-only**: AI Switch never modifies or deletes a session file.

### Two formats, two counting rules

The two clients account for usage completely differently, and each has one rule that — if you get it wrong — throws the numbers off badly.

**Claude Code — you must deduplicate by `message.id`.** One JSON object per line; assistant messages carry `message.usage` (input, output, cache write, cache read). The catch is that resume and context compaction **re-serialize the same message into several files**, so summing lines directly double-counts.

Measured against a real corpus on one machine (1186 files, 3.3 GB):

| Deduplication | Rows counted | Estimated cost |
| --- | --- | --- |
| None | 4020 | $3,675 |
| By `(message.id, requestId)` | 2911 | $3,160 |
| **By `message.id`** | **2008** | **$1,908** |

No deduplication overstates the total by **93%**. The composite key does not work either: about 1158 high-token rows have a `message.id` but **no** `requestId`, so a composite key misses many duplicates.

Two more details: `message.model` carries vendor prefixes (`anthropic/claude-opus-5-ps-aws-dst` was observed in practice) and must be normalized before a price lookup; `<synthetic>` marks locally generated messages that were never billed and are skipped. Subagent (`isSidechain`) tokens **are** counted — that is real spend, even though the session *list* hides them.

**Codex CLI — `total_token_usage` is cumulative, so only the last one counts.** The `info.total_token_usage` inside a `payload.type == "token_count"` event is the **running total for the whole session**, not the delta for that turn. One real file contained 690 such events; summing them gave 28.1 billion tokens against an actual final value of 80.5 million — an overstatement of **350×**.

Summing `info.last_token_usage` (the per-turn delta) is also inaccurate — it suffers replays too; the final total is the reliable value. The model ID lives in separate `turn_context.model` records, on different lines from the usage, so it has to be tracked while scanning.

Codex's `input_tokens` already includes `cached_input_tokens`, so the estimator subtracts the cached portion and charges it at the cache-read rate instead of billing it at full price; `reasoning_output_tokens` is already part of `output_tokens` and is not added again.

### What the metrics measure

Six cards: request count, input tokens, output tokens, cache writes, cache reads, estimated cost. Two differences from the route stats:

- **Cache writes and reads are separate cards.** The route stats have a single combined "cache tokens" figure because most upstreams only report one total; session files keep the two apart, and since they are priced 12.5× apart (write ×1.25, read ×0.1), combining them would misprice the result.
- **Cost is always an estimate.** Session files contain token counts and no amounts, so there is no `upstream` case here.

Below the cards is a per-model breakdown (by cost, descending, up to 12 rows); models with no price data show "无价格".

The four time ranges above are reused, filtering on each record's `timestamp`. **Records with no timestamp are only counted under "All time"** — otherwise picking a period would pull in undated rows and inflate the figure for no visible reason.

### Performance and refresh rate

Session files add up (3.3 GB on the machine measured), and a first scan takes **well over a hundred seconds**. Two optimizations address that:

- **Each file's parse is cached by (mtime, size).** Session files are essentially append-only, so unchanged files are not re-read. A cold scan takes 115 s; a warm one **0.9 s**.
- **A string pre-filter runs before parsing.** Only about 23% of lines carry usage, so a substring check gates the JSON parser.

Even so it is far heavier than the route stats, hence: **it is only queried while the stats panel is open, and refreshes every 60 seconds** (the route stats refresh every 5). The first open shows a "reading" state; subsequent refreshes hit the cache.

::: tip Do not add the two figures together
The route stats and session usage **overlap**: a Claude Code request that went through this app's proxy is recorded by both. They are two viewpoints — the route stats show "traffic through the proxy", session usage shows "all CLI spend on this machine" — not two halves to be summed.
:::

::: warning Scan limit
Beyond 50,000 files the scan truncates, and the panel then states explicitly that the figures are incomplete rather than presenting a truncated total as complete. Normal use is nowhere near that (1186 files on the machine measured).
:::

## What if you want live traffic

Usage events are **outcome records**: one row written after a request finishes, with a truncated body. If what you want to see is how a request is being rewritten right now, use the proxy's live request log — it captures four stages of every forwarded request but keeps only 100 entries in memory and never writes to disk. The two are complementary: one is for accounting, the other for debugging. Details in [Protocol Routing and Bridging](/en/guide/protocol-routing).

## Next

- [Accounts and the Pool](/en/guide/accounts) — the account list, archiving, and batch operations
- [Model Connectivity Tests](/en/guide/model-test) — how test events get into the stats
- [Reliability and Auto Recovery](/en/guide/reliability) — how a failed request affects account state
- [FAQ](/en/faq)
