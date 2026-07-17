# Route Pool Request Stats Filter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add per-agent route pool request statistics with `当日`, `本周`, `本月`, and `累计` filters plus a request list.

**Architecture:** Reuse the existing `get_route_pool` command and `usage_events` table. The frontend maps the selected period to an optional RFC3339 `since` timestamp, and the backend filters aggregate stats and request rows for the active platform's enabled route pool members.

**Tech Stack:** Rust, Tauri commands, Axum web command dispatch, SQLx SQLite, React, TanStack Query, TypeScript, Vitest.

## Global Constraints

- Work directly on `main`; do not create or switch branches/worktrees.
- Preserve unrelated worktree changes, including the existing `README.md` change.
- Use path-limited commits with `git commit --only ...` so unrelated staged files are not included.
- No new dependencies.
- No global cross-platform statistics page.
- No custom date range picker.
- No export, pagination, or deletion workflow.
- No schema rewrite for existing `usage_events`.
- Metadata parsing is best-effort for display and must not crash the page.

---

## File Structure

- `src-tauri/src/models/route_pool.rs`: Extend route pool stats DTOs with `source_label` and request rows.
- `src-tauri/src/database/repositories/route_pool_repository.rs`: Filter aggregate stats and logs by optional `since`; return request-only rows.
- `src-tauri/src/services/route_pool_service.rs`: Validate optional RFC3339 `since` and pass it into repository queries.
- `src-tauri/src/commands/route_pool_commands.rs`: Accept optional `since` for desktop Tauri IPC.
- `src-tauri/src/web/handlers/mod.rs`: Accept optional `since` for Web transport command dispatch.
- `src/lib/api/types.ts`: Mirror Rust DTO changes in frontend types.
- `src/lib/api/client.ts`: Pass optional `since` to `get_route_pool`.
- `src/screens/AccountsScreen.tsx`: Add period state, period-to-`since` mapping, filter buttons, and request list rendering.
- `tests/AccountsScreen.test.tsx`: Update mocks and add coverage for filtering and request-list rendering.

---

### Task 1: Backend Stats Filtering and Request Rows

**Files:**
- Modify: `src-tauri/src/models/route_pool.rs`
- Modify: `src-tauri/src/database/repositories/route_pool_repository.rs`
- Modify: `src-tauri/src/services/route_pool_service.rs`
- Modify: `src-tauri/src/commands/route_pool_commands.rs`
- Modify: `src-tauri/src/web/handlers/mod.rs`
- Test: `src-tauri/src/services/route_pool_service.rs`

**Interfaces:**
- Consumes: Existing `usage_events` rows with `route_credential_id`, `source_label`, `metric_type`, `amount`, `unit`, `metadata_json`, and `created_at`.
- Produces: `RoutePoolService::get(pool: &SqlitePool, platform: String, since: Option<String>) -> Result<RoutePoolState, AppError>`.
- Produces: `RoutePoolRepository::stats(pool: &SqlitePool, platform: &str, since: Option<&str>) -> Result<RoutePoolStats, AppError>`.
- Produces: `RoutePoolStats.requests: Vec<RoutePoolUsageLog>` containing only `metric_type = 'request'` rows.
- Produces: `RoutePoolUsageLog.source_label: String`.
- Produces: Tauri command `get_route_pool(platform: String, since: Option<String>)`.
- Produces: Web dispatch support for optional `since`.

- [ ] **Step 1: Write the failing backend tests**

Append these tests and helper inside the existing `#[cfg(test)] mod tests` in `src-tauri/src/services/route_pool_service.rs`:

```rust
    async fn usage_event_at(
        pool: &SqlitePool,
        account_id: &str,
        source_label: &str,
        metric_type: &str,
        amount: i64,
        unit: &str,
        metadata_json: &str,
        created_at: &str,
    ) {
        sqlx::query(
            "INSERT INTO usage_events
             (id, route_credential_id, source_label, metric_type, amount, unit, metadata_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(account_id)
        .bind(source_label)
        .bind(metric_type)
        .bind(amount)
        .bind(unit)
        .bind(metadata_json)
        .bind(created_at)
        .execute(pool)
        .await
        .expect("usage event");
    }

    #[tokio::test]
    async fn get_filters_stats_by_since_and_returns_request_rows() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let account_id = account(&pool, "codex", "CodexOne").await;

        RoutePoolService::set_members(
            &pool,
            SetRoutePoolMembersInput {
                platform: "codex".to_string(),
                account_ids: vec![account_id.clone()],
            },
        )
        .await
        .expect("members");

        let old_time = "2026-07-01T00:00:00Z";
        let since = "2026-07-17T00:00:00Z";
        let new_time = "2026-07-17T08:00:00Z";

        usage_event_at(
            &pool,
            &account_id,
            "route_proxy",
            "request",
            1,
            "count",
            r#"{"path":"/v1/old","status":200}"#,
            old_time,
        )
        .await;
        usage_event_at(
            &pool,
            &account_id,
            "route_proxy",
            "token",
            100,
            "token",
            r#"{"path":"/v1/old","status":200}"#,
            old_time,
        )
        .await;
        usage_event_at(
            &pool,
            &account_id,
            "route_proxy",
            "request",
            1,
            "count",
            r#"{"path":"/v1/responses","status":201}"#,
            new_time,
        )
        .await;
        usage_event_at(
            &pool,
            &account_id,
            "route_proxy",
            "token",
            200,
            "token",
            r#"{"path":"/v1/responses","status":201}"#,
            new_time,
        )
        .await;
        usage_event_at(
            &pool,
            &account_id,
            "route_proxy",
            "cost",
            300,
            "usd_micros",
            r#"{"path":"/v1/responses","status":201}"#,
            new_time,
        )
        .await;

        let state = RoutePoolService::get(
            &pool,
            "codex".to_string(),
            Some(since.to_string()),
        )
        .await
        .expect("filtered state");

        assert_eq!(state.stats.member_count, 1);
        assert_eq!(state.stats.request_count, 1);
        assert_eq!(state.stats.token_count, 200);
        assert_eq!(state.stats.cost_micros, 300);
        assert_eq!(state.stats.recent_logs.len(), 3);
        assert_eq!(state.stats.requests.len(), 1);
        assert_eq!(state.stats.requests[0].metric_type, "request");
        assert_eq!(state.stats.requests[0].source_label, "route_proxy");
        assert_eq!(state.stats.requests[0].account_name.as_deref(), Some("CodexOne"));
        assert!(state.stats.requests[0].metadata_json.contains("/v1/responses"));
    }

    #[tokio::test]
    async fn get_rejects_invalid_since_timestamp() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = RoutePoolService::get(
            &pool,
            "codex".to_string(),
            Some("not-a-date".to_string()),
        )
        .await
        .expect_err("invalid since");

        match error {
            AppError::Validation { code, .. } => {
                assert_eq!(code, "validation.route_pool_since");
            }
            _ => panic!("expected validation error"),
        }
    }
```

- [ ] **Step 2: Run backend tests to verify they fail**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml route_pool -- --nocapture
```

Expected: FAIL because `RoutePoolService::get` still takes two arguments and `RoutePoolStats` has no `requests` field.

- [ ] **Step 3: Extend the Rust route pool models**

Update `src-tauri/src/models/route_pool.rs` so the two structs become:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutePoolUsageLog {
    pub id: String,
    pub account_id: Option<String>,
    pub account_name: Option<String>,
    pub source_label: String,
    pub metric_type: String,
    pub amount: i64,
    pub unit: String,
    pub metadata_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoutePoolStats {
    pub member_count: i64,
    pub request_count: i64,
    pub token_count: i64,
    pub cost_micros: i64,
    pub recent_logs: Vec<RoutePoolUsageLog>,
    pub requests: Vec<RoutePoolUsageLog>,
}
```

- [ ] **Step 4: Replace repository stats with filtered stats and request rows**

In `src-tauri/src/database/repositories/route_pool_repository.rs`, replace `pub async fn stats(...)` with:

```rust
    pub async fn stats(
        pool: &SqlitePool,
        platform: &str,
        since: Option<&str>,
    ) -> Result<RoutePoolStats, AppError> {
        let join_since_clause = if since.is_some() {
            " AND ue.created_at >= ?"
        } else {
            ""
        };
        let summary_sql = format!(
            "SELECT
               COUNT(DISTINCT rpm.route_credential_id) AS member_count,
               COALESCE(SUM(CASE WHEN ue.metric_type = 'request' THEN CASE WHEN ue.amount > 0 THEN ue.amount ELSE 1 END ELSE 0 END), 0) AS request_count,
               COALESCE(SUM(CASE WHEN ue.metric_type = 'token' OR ue.unit = 'token' THEN ue.amount ELSE 0 END), 0) AS token_count,
               COALESCE(SUM(CASE WHEN ue.metric_type = 'cost' AND ue.unit = 'usd_micros' THEN ue.amount ELSE 0 END), 0) AS cost_micros
             FROM route_pool_members rpm
             LEFT JOIN usage_events ue ON ue.route_credential_id = rpm.route_credential_id{join_since_clause}
             WHERE rpm.platform = ? AND rpm.enabled = 1"
        );
        let mut summary_query = sqlx::query(&summary_sql);
        if let Some(since) = since {
            summary_query = summary_query.bind(since);
        }
        let row = summary_query
            .bind(platform)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_pool_stats",
                message: "Could not load route pool statistics".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        let log_since_clause = if since.is_some() {
            " AND ue.created_at >= ?"
        } else {
            ""
        };
        let log_sql = format!(
            "SELECT ue.id, ue.route_credential_id, a.display_name AS account_name,
                    ue.source_label, ue.metric_type, ue.amount, ue.unit, ue.metadata_json, ue.created_at
             FROM usage_events ue
             INNER JOIN route_pool_members rpm
               ON rpm.route_credential_id = ue.route_credential_id
              AND rpm.platform = ?
              AND rpm.enabled = 1
             LEFT JOIN route_credentials a ON a.id = ue.route_credential_id
             WHERE 1 = 1{log_since_clause}
             ORDER BY ue.created_at DESC
             LIMIT 10"
        );
        let mut log_query = sqlx::query(&log_sql).bind(platform);
        if let Some(since) = since {
            log_query = log_query.bind(since);
        }
        let log_rows = log_query
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_pool_logs",
                message: "Could not load route pool logs".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        let request_sql = format!(
            "SELECT ue.id, ue.route_credential_id, a.display_name AS account_name,
                    ue.source_label, ue.metric_type, ue.amount, ue.unit, ue.metadata_json, ue.created_at
             FROM usage_events ue
             INNER JOIN route_pool_members rpm
               ON rpm.route_credential_id = ue.route_credential_id
              AND rpm.platform = ?
              AND rpm.enabled = 1
             LEFT JOIN route_credentials a ON a.id = ue.route_credential_id
             WHERE ue.metric_type = 'request'{log_since_clause}
             ORDER BY ue.created_at DESC
             LIMIT 50"
        );
        let mut request_query = sqlx::query(&request_sql).bind(platform);
        if let Some(since) = since {
            request_query = request_query.bind(since);
        }
        let request_rows = request_query
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_pool_requests",
                message: "Could not load route pool requests".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        let map_usage_log = |row: sqlx::sqlite::SqliteRow| RoutePoolUsageLog {
            id: row.get("id"),
            account_id: row.get("route_credential_id"),
            account_name: row.get("account_name"),
            source_label: row.get("source_label"),
            metric_type: row.get("metric_type"),
            amount: row.get("amount"),
            unit: row.get("unit"),
            metadata_json: row.get("metadata_json"),
            created_at: row.get("created_at"),
        };

        Ok(RoutePoolStats {
            member_count: row.get("member_count"),
            request_count: row.get("request_count"),
            token_count: row.get("token_count"),
            cost_micros: row.get("cost_micros"),
            recent_logs: log_rows.into_iter().map(map_usage_log).collect(),
            requests: request_rows.into_iter().map(map_usage_log).collect(),
        })
    }
```

- [ ] **Step 5: Update route pool service to validate and pass `since`**

In `src-tauri/src/services/route_pool_service.rs`, add this import near the top:

```rust
use chrono::DateTime;
```

Replace `RoutePoolService::get`, the `route_once` stats call, and `state` with:

```rust
    pub async fn get(
        pool: &SqlitePool,
        platform: String,
        since: Option<String>,
    ) -> Result<RoutePoolState, AppError> {
        let platform = normalize_platform(&platform)?;
        let since = normalize_since(since)?;
        Self::state(pool, &platform, since.as_deref()).await
    }
```

```rust
            stats: RoutePoolRepository::stats(pool, &platform, None).await?,
```

```rust
    async fn state(
        pool: &SqlitePool,
        platform: &str,
        since: Option<&str>,
    ) -> Result<RoutePoolState, AppError> {
        Ok(RoutePoolState {
            platform: platform.to_string(),
            account_ids: RoutePoolRepository::list_member_ids(pool, platform).await?,
            stats: RoutePoolRepository::stats(pool, platform, since).await?,
        })
    }
```

Add this helper below `normalize_metadata_json`:

```rust
fn normalize_since(since: Option<String>) -> Result<Option<String>, AppError> {
    let Some(value) = since.map(|value| value.trim().to_string()).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    DateTime::parse_from_rfc3339(&value).map_err(|err| AppError::Validation {
        code: "validation.route_pool_since",
        message: "Route pool stats start time is invalid".to_string(),
        details: Some(err.to_string()),
        recoverable: true,
    })?;

    Ok(Some(value))
}
```

Update existing test calls in the same file from:

```rust
RoutePoolService::get(&pool, "codex".to_string())
```

to:

```rust
RoutePoolService::get(&pool, "codex".to_string(), None)
```

Also update `set_members` to call the new `state` signature:

```rust
        Self::state(pool, &platform, None).await
```

- [ ] **Step 6: Update Rust command call sites**

In `src-tauri/src/commands/route_pool_commands.rs`, replace `get_route_pool` with:

```rust
#[tauri::command]
pub async fn get_route_pool(
    state: State<'_, AppState>,
    platform: String,
    since: Option<String>,
) -> Result<RoutePoolState, ApiError> {
    RoutePoolService::get(&state.pool, platform, since)
        .await
        .map_err(ApiError::from)
}
```

In `src-tauri/src/web/handlers/mod.rs`, replace the `get_route_pool` match arm with:

```rust
        "get_route_pool" => {
            let platform = required_string_arg(&args, "platform")?;
            let since = optional_string_arg(&args, "since");
            to_value(
                RoutePoolService::get(&state.pool, platform, since)
                    .await
                    .map_err(to_error)?,
            )
        }
```

- [ ] **Step 7: Run backend tests to verify they pass**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml route_pool -- --nocapture
```

Expected: PASS for all route pool tests.

- [ ] **Step 8: Commit backend filtering**

Run:

```powershell
git status --short
git add src-tauri\src\models\route_pool.rs src-tauri\src\database\repositories\route_pool_repository.rs src-tauri\src\services\route_pool_service.rs src-tauri\src\commands\route_pool_commands.rs src-tauri\src\web\handlers\mod.rs
git commit --only src-tauri\src\models\route_pool.rs src-tauri\src\database\repositories\route_pool_repository.rs src-tauri\src\services\route_pool_service.rs src-tauri\src\commands\route_pool_commands.rs src-tauri\src\web\handlers\mod.rs -m "feat: filter route pool stats by time"
```

Expected: Commit includes only the backend model, repository, service, and Rust command dispatch files.

---

### Task 2: Frontend API Contract

**Files:**
- Modify: `src/lib/api/types.ts`
- Modify: `src/lib/api/client.ts`
- Modify: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes: Rust DTO shape from Task 1.
- Produces: `getRoutePool(platform: string, since?: string | null): Promise<RoutePoolState>`.
- Produces: Test mocks that satisfy `RoutePoolStats.requests` and `RoutePoolUsageLog.source_label`.

- [ ] **Step 1: Update TypeScript DTOs**

In `src/lib/api/types.ts`, replace `RoutePoolUsageLog` and `RoutePoolStats` with:

```ts
export type RoutePoolUsageLog = {
  id: string;
  account_id?: string | null;
  account_name?: string | null;
  source_label: string;
  metric_type: string;
  amount: number;
  unit: string;
  metadata_json: string;
  created_at: string;
};

export type RoutePoolStats = {
  member_count: number;
  request_count: number;
  token_count: number;
  cost_micros: number;
  recent_logs: RoutePoolUsageLog[];
  requests: RoutePoolUsageLog[];
};
```

- [ ] **Step 2: Update frontend API client**

In `src/lib/api/client.ts`, replace `getRoutePool` with:

```ts
export function getRoutePool(platform: string, since?: string | null): Promise<RoutePoolState> {
  return invoke("get_route_pool", { platform, since: since ?? null });
}
```

- [ ] **Step 3: Add a stats fixture helper for the new DTO shape**

In `tests/AccountsScreen.test.tsx`, update the type import:

```ts
import type { RouteCredential, RoutePoolStats } from "../src/lib/api/types";
```

Add this helper after `credentialsFixture`:

```ts
function statsFixture(overrides: Partial<RoutePoolStats> = {}): RoutePoolStats {
  return {
    member_count: 0,
    request_count: 0,
    token_count: 0,
    cost_micros: 0,
    recent_logs: [],
    requests: [],
    ...overrides,
  };
}
```

- [ ] **Step 4: Replace inline stats mocks with the helper**

Replace each inline `stats: { ... }` object in the test setup with `stats: statsFixture(...)`.

Use these exact replacements for the default mocks:

```ts
      stats: statsFixture(),
```

```ts
      stats: statsFixture({
        member_count: input.account_ids.length,
        request_count: 1,
        token_count: 4096,
        cost_micros: 2500,
      }),
```

```ts
      stats: statsFixture({
        member_count: 1,
        request_count: 2,
        token_count: 5120,
        cost_micros: 3700,
      }),
```

- [ ] **Step 5: Run contract checks**

Run:

```powershell
pnpm typecheck
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: PASS for both commands.

- [ ] **Step 6: Commit frontend API contract**

Run:

```powershell
git status --short
git add src\lib\api\types.ts src\lib\api\client.ts tests\AccountsScreen.test.tsx
git commit --only src\lib\api\types.ts src\lib\api\client.ts tests\AccountsScreen.test.tsx -m "feat: pass route pool stats start time"
```

Expected: Commit includes only frontend API types, API client, and mock-shape updates.

---

### Task 3: Account Screen Period Filters and Request List

**Files:**
- Modify: `src/screens/AccountsScreen.tsx`

**Interfaces:**
- Consumes: `getRoutePool(platform, since)` from Task 2.
- Consumes: `RoutePoolStats.requests`.
- Produces: Period buttons labeled `当日`, `本周`, `本月`, `累计`.
- Produces: Request list rows showing time, account, status, path, and source.

- [ ] **Step 1: Add period types and display helpers**

In `src/screens/AccountsScreen.tsx`, add these constants and helpers after `type CreateMode = "api" | "official";`:

```ts
const routeStatsPeriods = [
  { key: "today", label: "当日" },
  { key: "week", label: "本周" },
  { key: "month", label: "本月" },
  { key: "all", label: "累计" },
] as const;

type RouteStatsPeriod = (typeof routeStatsPeriods)[number]["key"];

function routeStatsSince(period: RouteStatsPeriod, now = new Date()) {
  if (period === "all") {
    return null;
  }

  const start = new Date(now);
  start.setHours(0, 0, 0, 0);

  if (period === "week") {
    const day = start.getDay();
    const daysSinceMonday = day === 0 ? 6 : day - 1;
    start.setDate(start.getDate() - daysSinceMonday);
  }

  if (period === "month") {
    start.setDate(1);
  }

  return start.toISOString();
}

function formatUsageTime(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString();
}

function parseUsageMetadata(metadataJson: string) {
  try {
    const value = JSON.parse(metadataJson) as unknown;
    if (!value || typeof value !== "object") {
      return { path: "-", status: "-" };
    }
    const record = value as Record<string, unknown>;
    return {
      path: typeof record.path === "string" && record.path.trim() ? record.path : "-",
      status:
        typeof record.status === "number" || typeof record.status === "string"
          ? String(record.status)
          : "-",
    };
  } catch {
    return { path: "-", status: "-" };
  }
}
```

- [ ] **Step 2: Add period state and filtered query key**

Inside `AccountsScreen`, add state near the existing `statsOpen` state:

```ts
  const [statsPeriod, setStatsPeriod] = useState<RouteStatsPeriod>("today");
```

Add the computed `since` near `const activePlatform = platform ?? "codex";`:

```ts
  const statsSince = useMemo(() => routeStatsSince(statsPeriod), [statsPeriod]);
```

Replace the route pool query with:

```ts
  const routePoolQuery = useQuery({
    queryKey: ["route-pool", activePlatform, statsSince],
    queryFn: () => getRoutePool(activePlatform, statsSince),
  });
```

- [ ] **Step 3: Refresh filtered stats after pool mutations**

In `routePoolMutation`, replace the `onSuccess` body with:

```ts
    onSuccess: (state) => {
      setDraftPoolIds(new Set(state.account_ids));
      void queryClient.invalidateQueries({ queryKey: ["route-pool", activePlatform] });
    },
```

In `routeOnceMutation`, replace the `onSuccess` body with:

```ts
    onSuccess: (outcome) => {
      void queryClient.invalidateQueries({ queryKey: ["route-pool", activePlatform] });
      setStatsOpen(true);
      setLastRouteAccount(outcome.selected_account_name);
    },
```

- [ ] **Step 4: Replace the expanded stats panel with filters and request list**

Replace the current `statsOpen && (...)` block with:

```tsx
        {statsOpen && (
          <div className="space-y-3 border-t border-stone-200 px-4 py-3">
            <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
              <div>
                <p className="text-[13px] font-semibold text-stone-950">请求统计</p>
                <p className="text-[12px] text-stone-500">仅统计当前 {platformLabels[activePlatform]} 算力池账号</p>
              </div>
              <div className="grid grid-cols-4 gap-1 rounded-xl bg-stone-100 p-1">
                {routeStatsPeriods.map((period) => (
                  <button
                    className={`rounded-lg px-2.5 py-1.5 text-[12px] font-semibold transition-colors ${
                      statsPeriod === period.key
                        ? "bg-white text-stone-950 shadow-sm"
                        : "text-stone-500 hover:text-stone-900"
                    }`}
                    key={period.key}
                    onClick={() => setStatsPeriod(period.key)}
                    type="button"
                  >
                    {period.label}
                  </button>
                ))}
              </div>
            </div>

            <div className="grid gap-2 sm:grid-cols-3">
              <div className="rounded-xl border border-stone-200 bg-stone-50 p-3">
                <p className="text-[11px] font-medium text-stone-500">请求</p>
                <p className="mt-1 text-lg font-semibold text-stone-950">{routeStats?.request_count ?? 0}</p>
              </div>
              <div className="rounded-xl border border-stone-200 bg-stone-50 p-3">
                <p className="text-[11px] font-medium text-stone-500">Token</p>
                <p className="mt-1 text-lg font-semibold text-stone-950">
                  {(routeStats?.token_count ?? 0).toLocaleString()}
                </p>
              </div>
              <div className="rounded-xl border border-stone-200 bg-stone-50 p-3">
                <p className="text-[11px] font-medium text-stone-500">费用</p>
                <p className="mt-1 text-lg font-semibold text-stone-950">${costTotal.toFixed(2)}</p>
              </div>
            </div>

            <div className="overflow-hidden rounded-xl border border-stone-200 bg-white">
              <div className="flex items-center justify-between border-b border-stone-100 bg-stone-50 px-3 py-2">
                <p className="text-[12px] font-semibold text-stone-700">请求列表</p>
                <p className="text-[11px] font-medium text-stone-500">
                  {(routeStats?.requests ?? []).length} 条
                </p>
              </div>
              {(routeStats?.requests ?? []).length === 0 ? (
                <p className="px-3 py-4 text-[12px] text-stone-500">当前筛选范围内暂无请求。</p>
              ) : (
                <div className="divide-y divide-stone-100">
                  {(routeStats?.requests ?? []).map((request) => {
                    const metadata = parseUsageMetadata(request.metadata_json);
                    return (
                      <div
                        className="grid gap-2 px-3 py-2.5 text-[12px] text-stone-600 lg:grid-cols-[1.2fr_1fr_0.5fr_1.4fr_0.8fr] lg:items-center"
                        key={request.id}
                      >
                        <span className="font-medium text-stone-800">{formatUsageTime(request.created_at)}</span>
                        <span className="truncate">{request.account_name ?? request.account_id ?? "-"}</span>
                        <span className="rounded-lg bg-stone-100 px-2 py-1 text-center font-semibold text-stone-700">
                          {metadata.status}
                        </span>
                        <span className="truncate font-mono text-[11px]">{metadata.path}</span>
                        <span className="truncate">{request.source_label}</span>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          </div>
        )}
```

- [ ] **Step 5: Run TypeScript check to verify UI compiles**

Run:

```powershell
pnpm typecheck
```

Expected: PASS.

- [ ] **Step 6: Commit UI implementation**

Run:

```powershell
git status --short
git add src\screens\AccountsScreen.tsx
git commit --only src\screens\AccountsScreen.tsx -m "feat: show route pool request stats filters"
```

Expected: Commit includes only `src/screens/AccountsScreen.tsx`.

---

### Task 4: Frontend Tests and Mock Updates

**Files:**
- Modify: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes: `RoutePoolStats.requests` and `RoutePoolUsageLog.source_label` from Task 2.
- Produces: Test coverage for period filters and request list rendering.

- [ ] **Step 1: Add request list and period filter test**

Add this test near the existing route proxy/statistics tests:

```ts
  it("renders filtered route request statistics and request rows", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-17T10:00:00+08:00"));
    vi.mocked(getRoutePool).mockImplementation(async (platform, since) => ({
      platform,
      account_ids: ["cred-official-1"],
      stats: statsFixture({
        member_count: 1,
        request_count: 1,
        token_count: 2048,
        cost_micros: 1500,
        requests: [
          {
            id: `request-${since ?? "all"}`,
            account_id: "cred-official-1",
            account_name: "Team Account",
            source_label: "route_proxy",
            metric_type: "request",
            amount: 1,
            unit: "count",
            metadata_json: "{\"path\":\"/v1/responses\",\"status\":201}",
            created_at: "2026-07-17T08:00:00Z",
          },
          {
            id: "request-invalid-metadata",
            account_id: "cred-api-1",
            account_name: "API Account",
            source_label: "route_proxy",
            metric_type: "request",
            amount: 1,
            unit: "count",
            metadata_json: "{bad json",
            created_at: "2026-07-17T08:01:00Z",
          },
        ],
      }),
    }));

    renderScreen();

    await userEvent.click(await screen.findByLabelText("查看算力池统计"));

    expect(await screen.findByText("请求统计")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "当日" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "本周" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "本月" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "累计" })).toBeInTheDocument();
    expect(screen.getByText("/v1/responses")).toBeInTheDocument();
    expect(screen.getByText("201")).toBeInTheDocument();
    expect(screen.getAllByText("route_proxy")).toHaveLength(2);
    expect(screen.getByText("API Account")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "本月" }));

    await waitFor(() =>
      expect(getRoutePool).toHaveBeenLastCalledWith(
        "codex",
        new Date("2026-07-01T00:00:00+08:00").toISOString(),
      ),
    );

    await userEvent.click(screen.getByRole("button", { name: "累计" }));

    await waitFor(() => expect(getRoutePool).toHaveBeenLastCalledWith("codex", null));
  });
```

- [ ] **Step 2: Run frontend test file**

Run:

```powershell
pnpm test:run tests/AccountsScreen.test.tsx
```

Expected: PASS for all `AccountsScreen` tests.

- [ ] **Step 3: Commit frontend tests**

Run:

```powershell
git status --short
git add tests\AccountsScreen.test.tsx
git commit --only tests\AccountsScreen.test.tsx -m "test: cover route pool request stats filters"
```

Expected: Commit includes only `tests/AccountsScreen.test.tsx`.

---

### Task 5: Full Verification

**Files:**
- No source edits expected.

**Interfaces:**
- Consumes: All commits from Tasks 1 through 4.
- Produces: Verified implementation ready for handoff.

- [ ] **Step 1: Run Rust tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml route_pool -- --nocapture
```

Expected: PASS.

- [ ] **Step 2: Run frontend tests**

Run:

```powershell
pnpm test:run tests/AccountsScreen.test.tsx
```

Expected: PASS.

- [ ] **Step 3: Run full type and Rust checks**

Run:

```powershell
pnpm typecheck
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: PASS for both commands.

- [ ] **Step 4: Inspect final worktree**

Run:

```powershell
git status --short
```

Expected: The task files are clean after commits. The pre-existing `README.md` change may still be present and must not be reverted or committed unless the user requests it.

- [ ] **Step 5: Summarize implementation**

Report:

```text
Implemented per-agent route pool request statistics with today/week/month/all filters.
Verified with route pool Rust tests, AccountsScreen Vitest tests, TypeScript check, and cargo check.
Unrelated README.md change remains untouched.
```
