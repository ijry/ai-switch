---
title: Usage and Request Stats
description: How AI Switch records per-request input/output/cache tokens and dual-currency pricing, the four time ranges and six metrics on the stats panel, and what the request list can expand to show.
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

Those breakdown columns are the entire source of token and cost data:

```sql
ALTER TABLE usage_events ADD COLUMN input_tokens INTEGER;
ALTER TABLE usage_events ADD COLUMN output_tokens INTEGER;
ALTER TABLE usage_events ADD COLUMN cache_tokens INTEGER;
ALTER TABLE usage_events ADD COLUMN price_usd_micros INTEGER;
ALTER TABLE usage_events ADD COLUMN price_cny_micros INTEGER;
ALTER TABLE usage_events ADD COLUMN price_currency TEXT;
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

**No usage means empty — nothing is estimated.** When the upstream returns no usage, the corresponding columns stay `NULL` and the UI shows `-`. AI Switch will not back-derive a token count from character length just to display a nicer-looking number.

## How upstream prices are parsed

Price parsing is another alias chain, and it distinguishes "already in micros" from "needs ×1,000,000":

| Target column | Micros fields | Plain-unit fields (auto ×1,000,000) |
| --- | --- | --- |
| `price_usd_micros` | `price_usd_micros`, `cost_usd_micros`, `cost_micros` | `price_usd`, `cost_usd` |
| `price_cny_micros` | `price_cny_micros`, `cost_cny_micros` | `price_cny`, `cost_cny` |

There is a generic layer too: if the upstream only supplies `price` or `cost` (possibly an object carrying its own `currency` / `unit`), the currency is identified first and the value routed to the matching column. Currency matching is loose — containing `usd` or `dollar`, or equal to `$`, means USD; containing `cny`, `rmb`, or `yuan`, or equal to `¥`, means CNY.

`price_currency` is determined in this order: an explicit currency field → the currency inside a generic price object → inference from whichever single column has a value. If none of the three settles it, the field stays empty.

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
- **Price** picks its symbol from `price_currency` — `¥` for CNY, `$` for USD — both to six decimal places; no price shows `-`.

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

## What if you want live traffic

Usage events are **outcome records**: one row written after a request finishes, with a truncated body. If what you want to see is how a request is being rewritten right now, use the proxy's live request log — it captures four stages of every forwarded request but keeps only 100 entries in memory and never writes to disk. The two are complementary: one is for accounting, the other for debugging. Details in [Protocol Routing and Bridging](/en/guide/protocol-routing).

## Next

- [Accounts and the Pool](/en/guide/accounts) — the account list, archiving, and batch operations
- [Model Connectivity Tests](/en/guide/model-test) — how test events get into the stats
- [Reliability and Auto Recovery](/en/guide/reliability) — how a failed request affects account state
- [FAQ](/en/faq)
