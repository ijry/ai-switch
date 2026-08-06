# Route Usage Breakdown Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task with verification checkpoints.

**Goal:** Store input/output/cache usage and upstream-reported USD/CNY prices on each route request, show them in the statistics list, and aggregate the total cost in USD using a 7.1 CNY/USD rate.

**Architecture:** Extend the existing `usage_events` request rows with nullable request-level usage columns. Add one shared response extractor in the route proxy service, write one complete request row for real proxy and model-test attempts, and retain legacy token/cost event aggregation for historical data and manual route-pool operations.

**Tech Stack:** Rust 2021, Tokio, SQLx SQLite migrations, Axum/reqwest route proxy, React 18, TypeScript, Vitest, React Testing Library.

## Global Constraints

- Work directly on `main`; do not create branches or worktrees.
- Read prices only from upstream response fields; never calculate a missing price from token counts or a pricing table.
- Store source currency values in `price_usd_micros` or `price_cny_micros`; use `price_currency` to select the authoritative field.
- Convert CNY to USD for aggregation with exactly `7.1` CNY per USD and round to USD micro-units.
- Keep missing usage and price values nullable and render them as unavailable in the request list.
- Preserve historical `usage_events` rows and their separate token/cost aggregation.
- Do not persist request bodies, response bodies, prompts, completions, API keys, or authorization headers in the new fields.
- Do not commit changes unless the user explicitly requests a commit.

---

### Task 1: Add Request Usage Schema And API Models

**Files:**
- Create: `src-tauri/migrations/202608060002_route_usage_breakdown.sql`
- Modify: `src-tauri/src/models/route_pool.rs:7-30`
- Modify: `src/lib/api/types.ts:366-388`
- Test: `tests/AccountsScreen.test.tsx:126-138`

**Interfaces:**
- `RouteUsageBreakdown` provides `input_tokens`, `output_tokens`, `cache_tokens`, `price_usd_micros`, `price_cny_micros`, and `price_currency` as nullable Rust fields.
- `RoutePoolUsageLog` exposes the same six nullable request-level values in serialized snake_case.
- `RoutePoolStats` exposes `input_token_count`, `output_token_count`, and `cache_token_count` while retaining `token_count` and `cost_micros`.

- [ ] **Step 1: Add the additive SQLite migration**

Create `src-tauri/migrations/202608060002_route_usage_breakdown.sql` with only nullable additions:

```sql
ALTER TABLE usage_events ADD COLUMN input_tokens INTEGER;
ALTER TABLE usage_events ADD COLUMN output_tokens INTEGER;
ALTER TABLE usage_events ADD COLUMN cache_tokens INTEGER;
ALTER TABLE usage_events ADD COLUMN price_usd_micros INTEGER;
ALTER TABLE usage_events ADD COLUMN price_cny_micros INTEGER;
ALTER TABLE usage_events ADD COLUMN price_currency TEXT;
```

Do not rebuild `usage_events`, change existing columns, or add a price default.

- [ ] **Step 2: Define the Rust usage value object**

Add this shape in `src-tauri/src/models/route_pool.rs`:

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RouteUsageBreakdown {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_tokens: Option<i64>,
    pub price_usd_micros: Option<i64>,
    pub price_cny_micros: Option<i64>,
    pub price_currency: Option<String>,
}
```

Add matching nullable fields to `RoutePoolUsageLog`, and add the three token total fields to `RoutePoolStats` without removing the existing fields.

- [ ] **Step 3: Mirror the serialized contract in TypeScript**

Update `src/lib/api/types.ts` so the frontend types match the Rust serialization:

```ts
export type RoutePoolUsageLog = {
  // Keep all existing fields.
  input_tokens?: number | null;
  output_tokens?: number | null;
  cache_tokens?: number | null;
  price_usd_micros?: number | null;
  price_cny_micros?: number | null;
  price_currency?: "usd" | "cny" | null;
};

export type RoutePoolStats = {
  // Keep all existing fields.
  input_token_count: number;
  output_token_count: number;
  cache_token_count: number;
};
```

- [ ] **Step 4: Update the frontend stats fixture**

Add zero values for the three new required `RoutePoolStats` totals in `statsFixture` so existing tests remain type-safe before the backend response is updated.

- [ ] **Step 5: Run the type-only checks for the contract change**

Run: `pnpm typecheck`

Expected: PASS, with no missing required `RoutePoolStats` properties.

---

### Task 2: Implement Shared Response Usage Extraction

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs:2685-2736`
- Modify: `src-tauri/src/services/route_model_test_service.rs:11-15`

**Interfaces:**
- `extract_usage_breakdown(body: &[u8]) -> RouteUsageBreakdown` returns nullable values and never returns a parsing error.
- Existing `extract_token_count` and `extract_cost_micros` callers are either replaced by the new extractor or kept as compatibility wrappers over it.

- [ ] **Step 1: Add failing extractor tests for provider usage shapes**

Add unit tests beside the existing extractor tests in `route_proxy_service.rs`:

```rust
#[test]
fn extract_usage_breakdown_supports_deepseek_usage_and_cny_price() {
    let body = br#"{
        "usage": {
            "prompt_tokens": 120,
            "completion_tokens": 30,
            "prompt_cache_hit_tokens": 80,
            "price_cny": 7.1
        }
    }"#;

    let usage = extract_usage_breakdown(body);

    assert_eq!(usage.input_tokens, Some(120));
    assert_eq!(usage.output_tokens, Some(30));
    assert_eq!(usage.cache_tokens, Some(80));
    assert_eq!(usage.price_cny_micros, Some(7_100_000));
    assert_eq!(usage.price_currency.as_deref(), Some("cny"));
}

#[test]
fn extract_usage_breakdown_supports_openai_responses_and_usd_price() {
    let body = br#"{
        "usage": {
            "input_tokens": 100,
            "output_tokens": 25,
            "input_tokens_details": {"cached_tokens": 60},
            "cost_usd": 0.0042
        }
    }"#;

    let usage = extract_usage_breakdown(body);

    assert_eq!(usage.input_tokens, Some(100));
    assert_eq!(usage.output_tokens, Some(25));
    assert_eq!(usage.cache_tokens, Some(60));
    assert_eq!(usage.price_usd_micros, Some(4_200));
    assert_eq!(usage.price_currency.as_deref(), Some("usd"));
}

#[test]
fn extract_usage_breakdown_leaves_price_empty_when_upstream_omits_price() {
    let usage = extract_usage_breakdown(
        br#"{"usage":{"prompt_tokens":10,"completion_tokens":2}}"#,
    );

    assert_eq!(usage.price_usd_micros, None);
    assert_eq!(usage.price_cny_micros, None);
    assert_eq!(usage.price_currency, None);
}
```

Also cover Anthropic cache read plus cache creation and Gemini `usageMetadata` fields in the same test module.

- [ ] **Step 2: Implement provider-neutral numeric and price helpers**

Implement `extract_usage_breakdown` using `serde_json::Value` and the following precedence:

1. Input: `usage.input_tokens`, then `usage.prompt_tokens`, then `usageMetadata.promptTokenCount`.
2. Output: `usage.output_tokens`, then `usage.completion_tokens`, then `usageMetadata.candidatesTokenCount`.
3. Cache: OpenAI cached-token detail, then DeepSeek `prompt_cache_hit_tokens`, then Anthropic read plus creation, then Gemini `cachedContentTokenCount`.
4. Price: explicit `price_usd`/`cost_usd`, explicit `price_cny`/`cost_cny`, or a generic `price`/`cost` paired with a `currency`/`unit` value.

Accept JSON numbers and numeric strings for prices, reject negative values, multiply source currency values by `1_000_000`, and round to `i64`. Never derive one currency field from the other in the extractor.

- [ ] **Step 3: Preserve safe behavior for malformed responses**

Return `RouteUsageBreakdown::default()` for invalid JSON and leave individual fields `None` when their paths are absent or invalid. Keep token-only compatibility wrappers returning input plus output for existing code that still needs a total.

- [ ] **Step 4: Run the focused Rust extractor tests**

Run: `cd src-tauri; cargo test extract_usage_breakdown`

Expected: PASS for DeepSeek, OpenAI/Responses, Anthropic, Gemini, missing-price, and malformed-JSON cases.

---

### Task 3: Persist Request Rows And Aggregate Prices

**Files:**
- Modify: `src-tauri/src/database/repositories/route_pool_repository.rs:262-430`
- Modify: `src-tauri/src/services/route_pool_service.rs:80-140`
- Modify: `src-tauri/src/services/route_pool_service.rs:300-479`

**Interfaces:**
- `RoutePoolRepository::insert_request_event(pool, account_id, source_label, metadata_json, usage)` inserts one `request` row with `amount = 1` and `unit = "count"`.
- `RoutePoolRepository::stats(...)` returns direct request totals plus compatible historical metric totals.

- [ ] **Step 1: Add repository persistence tests for a complete request row**

Extend the route-pool repository/service test setup with a request containing:

```rust
let usage = RouteUsageBreakdown {
    input_tokens: Some(120),
    output_tokens: Some(30),
    cache_tokens: Some(80),
    price_usd_micros: None,
    price_cny_micros: Some(7_100_000),
    price_currency: Some("cny".to_string()),
};
```

Insert it through the new repository method, load `RoutePoolService::get`, and assert that the request row contains all six values and the stats contain input `120`, output `30`, cache `80`, and `cost_micros = 1_000_000`.

- [ ] **Step 2: Implement `insert_request_event`**

Insert all request fields in one statement:

```sql
INSERT INTO usage_events
  (id, route_credential_id, source_label, metric_type, amount, unit,
   metadata_json, input_tokens, output_tokens, cache_tokens,
   price_usd_micros, price_cny_micros, price_currency, created_at)
VALUES (?, ?, ?, 'request', 1, 'count', ?, ?, ?, ?, ?, ?, ?, ?)
```

Keep the existing `insert_usage_event` method unchanged for legacy/manual metric rows.

- [ ] **Step 3: Extend all repository row projections**

Add the six usage columns to both recent-log and paginated-request `SELECT` lists. Map them as `Option<i64>` and `Option<String>` in `RoutePoolUsageLog` so old rows with NULL values remain valid.

- [ ] **Step 4: Update summary SQL without double counting**

Add these expressions to the existing summary query:

```sql
COALESCE(SUM(CASE WHEN ue.metric_type = 'request' THEN COALESCE(ue.input_tokens, 0) ELSE 0 END), 0) AS input_token_count,
COALESCE(SUM(CASE WHEN ue.metric_type = 'request' THEN COALESCE(ue.output_tokens, 0) ELSE 0 END), 0) AS output_token_count,
COALESCE(SUM(CASE WHEN ue.metric_type = 'request' THEN COALESCE(ue.cache_tokens, 0) ELSE 0 END), 0) AS cache_token_count
```

Keep legacy token totals in `token_count` and add direct request input plus output to it. Add direct price handling to `cost_micros`: use USD micro-units for `price_currency = 'usd'`, use `ROUND(price_cny_micros / 7.1)` for `price_currency = 'cny'`, and continue adding legacy `cost` rows with `unit = 'usd_micros'`. Do not add direct request prices to the legacy cost branch.

- [ ] **Step 5: Keep manual route-pool operations compatible**

Leave `RoutePoolService::route_once` using `insert_usage_event` for its existing total-token and USD-cost inputs because it is a manual selection operation, not an upstream response with an input/output/cache breakdown. Its request, token, and cost rows must still satisfy current tests and historical aggregation.

- [ ] **Step 6: Run the focused repository tests**

Run: `cd src-tauri; cargo test route_pool_service`

Expected: PASS for legacy token/cost rows, filtered request rows, pagination, direct breakdown fields, and CNY-to-USD aggregation.

---

### Task 4: Record Proxy And Model-Test Usage In One Request

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs:389-671,2738-2768`
- Modify: `src-tauri/src/services/route_model_test_service.rs:197-250,935-1050`

**Interfaces:**
- Real proxy and model-test paths call `extract_usage_breakdown` once per complete upstream response.
- Both paths call `RoutePoolRepository::insert_request_event` exactly once per upstream attempt.

- [ ] **Step 1: Add a proxy integration assertion for request-level fields**

Update the existing JSON upstream test in `route_proxy_service.rs` so the fake response includes:

```json
{
  "usage": {
    "prompt_tokens": 120,
    "completion_tokens": 30,
    "prompt_cache_hit_tokens": 80,
    "price_cny": 7.1
  }
}
```

After the proxy response, query the latest `route_proxy` request row and assert the six usage columns, then assert that only one `route_proxy` row exists for that upstream attempt.

- [ ] **Step 2: Replace proxy request/token/cost inserts**

In `forward_request`, construct a default usage breakdown for refresh, request-build, transport, and response-read failures. Insert one request row for each attempted credential. For a complete response, parse the response bytes and pass the extracted breakdown to `insert_request_event`; remove the separate `token` and `cost` inserts.

Ensure the response-read error branch also writes its failed request row before retrying, so every upstream attempt is represented.

- [ ] **Step 3: Update model-test flow and tests**

Replace `token_count` and `cost_micros` parameters in `finish_outcome` with `RouteUsageBreakdown`. Pass `RouteUsageBreakdown::default()` through all failure paths, parse the complete response once, insert one request row, and remove separate token/cost inserts. Keep `RoutePoolModelTestOutcome.stats` behavior unchanged except for the new totals.

Update the existing model-test success assertion to verify input/output/cache and returned price values through the persisted stats.

- [ ] **Step 4: Run focused proxy and model-test tests**

Run: `cd src-tauri; cargo test route_proxy_service`

Run: `cd src-tauri; cargo test route_model_test_service`

Expected: PASS, including retry accounting, trace lookup, model-test statistics, and one request row per upstream attempt.

---

### Task 5: Render Usage Breakdown And Original Currency In Statistics

**Files:**
- Modify: `src/screens/AccountsScreen.tsx:229-330,1803-1808,3296-3350`
- Modify: `tests/AccountsScreen.test.tsx:126-138,1415-1515`

**Interfaces:**
- `formatUsageCount(value: number | null | undefined): string` returns `-` for unavailable values and a locale-formatted integer otherwise.
- `formatUsagePrice(request: RoutePoolUsageLog): string` renders the authoritative USD or CNY source amount in its original currency and returns `-` when unavailable.

- [ ] **Step 1: Extend the screen fixture with usage and price values**

Add the new stats totals to `statsFixture` and add these values to the successful request fixture:

```ts
input_tokens: 120,
output_tokens: 30,
cache_tokens: 80,
price_usd_micros: null,
price_cny_micros: 7_100_000,
price_currency: "cny",
```

Set `cost_micros` to `1_000_000` in the same fixture so the summary visibly renders `$1.00`.

- [ ] **Step 2: Add display helpers**

Implement the two helpers near `parseUsageMetadata`:

```ts
function formatUsageCount(value: number | null | undefined) {
  return value == null ? "-" : value.toLocaleString();
}

function formatUsagePrice(request: RoutePoolUsageLog) {
  if (request.price_currency === "cny" && request.price_cny_micros != null) {
    return `¥${(request.price_cny_micros / 1_000_000).toFixed(6)}`;
  }
  if (request.price_currency === "usd" && request.price_usd_micros != null) {
    return `$${(request.price_usd_micros / 1_000_000).toFixed(6)}`;
  }
  return "-";
}
```

Keep six decimal places so micro-unit prices remain visible and deterministic.

- [ ] **Step 3: Add usage values to request detail and compact row**

Add input, output, cache, and price values to `RouteRequestDetail`. Add four compact columns to the request grid, preserve the existing time/account/status/path/source columns, and keep the detail button accessible. Use the count helper so old rows display `-` rather than `0`.

- [ ] **Step 4: Preserve the USD summary card**

Continue deriving the summary display from `routeStats.cost_micros / 1_000_000`, and keep the `$` prefix. Do not convert row prices in the frontend; conversion is already reflected in the backend aggregate.

- [ ] **Step 5: Update frontend assertions**

Extend the statistics test to assert `120`, `30`, `80`, `¥7.100000`, `$1.00`, and the same values inside the expanded request detail. Keep the invalid metadata row assertions and add a request with missing usage values to verify `-` remains rendered.

- [ ] **Step 6: Run the focused frontend test**

Run: `pnpm exec vitest run tests/AccountsScreen.test.tsx`

Expected: PASS for statistics rendering, details, pagination, period filters, and auto-refresh behavior.

---

### Task 6: Run Full Verification And Review The Patch

**Files:**
- Modify only files already listed in Tasks 1-5.

- [ ] **Step 1: Run Rust formatting and focused tests**

Run: `cd src-tauri; cargo fmt --all -- --check`

Run: `cd src-tauri; cargo test route_pool_service`

Run: `cd src-tauri; cargo test route_proxy_service`

Run: `cd src-tauri; cargo test route_model_test_service`

Expected: formatting check passes and all focused Rust tests pass.

- [ ] **Step 2: Run TypeScript checks and frontend tests**

Run: `pnpm typecheck`

Run: `pnpm exec vitest run tests/AccountsScreen.test.tsx tests/DashboardScreen.test.tsx`

Expected: typecheck and both frontend suites pass.

- [ ] **Step 3: Build the frontend**

Run: `pnpm build`

Expected: TypeScript compilation and Vite production build complete successfully.

- [ ] **Step 4: Inspect the final diff**

Run: `git diff --check`

Run: `git status --short`

Confirm that the patch contains only the approved migration, Rust models/repository/services/tests, API types, account statistics UI/tests, and the two planning documents. Do not revert unrelated pre-existing changes in `package.json`, `src-tauri/Cargo.lock`, `src-tauri/Cargo.toml`, or `src-tauri/tauri.conf.json`.
