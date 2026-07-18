# Route Pool Stats Refresh And Pagination Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add automatic refresh and backend-backed pagination to the route pool statistics request list, while counting historical usage for all existing credentials on the current platform even after they leave the active pool.

**Architecture:** Keep the existing `get_route_pool` command as the single route-pool state API. Extend its payload with request-list pagination, change repository statistics queries from current-pool joins to current-platform credential joins, and add React Query polling only while the statistics panel is open.

**Tech Stack:** React 18, TypeScript, TanStack React Query v5, Vitest, Testing Library, Tauri 2 commands, Rust, SQLx, SQLite.

## Global Constraints

- Work directly on `main` by default. Do not create or switch to feature branches/worktrees unless the user explicitly asks for a separate branch, worktree, or isolation.
- The statistics panel remains inside `AccountsScreen`; no new global statistics page.
- Automatic refresh interval is exactly 5 seconds while the statistics panel is open.
- Closing the statistics panel stops automatic refresh.
- Request list page size defaults to `20`.
- Backend page size is clamped to `1..100`.
- Switching platform or period resets request pagination to page `1`.
- Auto-refresh keeps the current request page selected.
- Statistics totals include all existing `route_credentials` for the current platform, including accounts removed from `route_pool_members`.
- Deleted credentials are not included in the guarantee because the current schema does not preserve platform and display-name metadata for deleted credentials.
- Summary totals are period-filtered and are not affected by request-list pagination.
- Keep existing pool membership, route testing, route proxy, config-writing, and period filter behavior working.

---

## File Structure

- `src-tauri/src/models/route_pool.rs`: owns serialized route-pool data shapes returned to desktop and web clients. Add request-list pagination metadata to `RoutePoolStats`.
- `src-tauri/src/services/route_pool_service.rs`: owns route-pool command semantics and validation. Add pagination normalization and pass normalized values to repository queries.
- `src-tauri/src/database/repositories/route_pool_repository.rs`: owns SQL for pool membership and usage stats. Change usage stats to current-platform credential scope and add `LIMIT/OFFSET` pagination plus total request-row count.
- `src-tauri/src/commands/route_pool_commands.rs`: Tauri command adapter. Accept optional pagination arguments and forward them to the service.
- `src-tauri/src/web/handlers/mod.rs`: web command dispatcher. Parse optional pagination arguments and forward them to the service.
- `src/lib/api/types.ts`: frontend API response types. Add request-list pagination metadata to `RoutePoolStats`.
- `src/lib/api/client.ts`: frontend transport wrapper. Add optional pagination arguments to `getRoutePool`.
- `src/screens/AccountsScreen.tsx`: route credentials UI. Add request-page state, query-key pagination, polling while stats are open, pagination controls, and updated statistics copy.
- `tests/AccountsScreen.test.tsx`: frontend behavior tests for stats copy, pagination calls, period resets, and auto-refresh polling.

---

### Task 1: Backend Historical Stats Scope And Pagination

**Files:**
- Modify: `src-tauri/src/models/route_pool.rs`
- Modify: `src-tauri/src/services/route_pool_service.rs`
- Modify: `src-tauri/src/database/repositories/route_pool_repository.rs`
- Modify: `src-tauri/src/commands/route_pool_commands.rs`
- Modify: `src-tauri/src/web/handlers/mod.rs`
- Test: `src-tauri/src/services/route_pool_service.rs`

**Interfaces:**
- Consumes: existing `RoutePoolService::get(&SqlitePool, String, Option<String>) -> Result<RoutePoolState, AppError>`.
- Produces: `RoutePoolService::get(&SqlitePool, String, Option<String>, Option<i64>, Option<i64>) -> Result<RoutePoolState, AppError>`.
- Produces: `RoutePoolRepository::stats(&SqlitePool, &str, Option<&str>, i64, i64) -> Result<RoutePoolStats, AppError>`.
- Produces: `RoutePoolStats` fields `request_row_count: i64`, `request_page: i64`, and `request_page_size: i64`.
- Produces: `get_route_pool` Tauri/web command arguments `request_page?: i64` and `request_page_size?: i64`.

- [ ] **Step 1: Write failing Rust tests for historical scope and pagination**

Append these tests inside the existing `#[cfg(test)] mod tests` in `src-tauri/src/services/route_pool_service.rs`, near `get_filters_stats_by_since_and_returns_request_rows`:

```rust
    #[tokio::test]
    async fn stats_include_removed_pool_credentials_for_same_platform() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let removed_id = account(&pool, "codex", "RemovedCodex").await;
        let active_id = account(&pool, "codex", "ActiveCodex").await;
        let claude_id = account(&pool, "claude", "ClaudeOne").await;

        RoutePoolService::set_members(
            &pool,
            SetRoutePoolMembersInput {
                platform: "codex".to_string(),
                account_ids: vec![removed_id.clone(), active_id.clone()],
            },
        )
        .await
        .expect("initial members");

        usage_event_at(
            &pool,
            &removed_id,
            "route_proxy",
            "request",
            1,
            "count",
            r#"{"path":"/v1/removed","status":200}"#,
            "2026-07-17T08:00:00Z",
        )
        .await;
        usage_event_at(
            &pool,
            &removed_id,
            "route_proxy",
            "token",
            512,
            "token",
            r#"{"path":"/v1/removed","status":200}"#,
            "2026-07-17T08:00:01Z",
        )
        .await;
        usage_event_at(
            &pool,
            &active_id,
            "route_proxy",
            "request",
            1,
            "count",
            r#"{"path":"/v1/active","status":201}"#,
            "2026-07-17T08:01:00Z",
        )
        .await;
        usage_event_at(
            &pool,
            &claude_id,
            "route_proxy",
            "request",
            1,
            "count",
            r#"{"path":"/v1/claude","status":202}"#,
            "2026-07-17T08:02:00Z",
        )
        .await;

        RoutePoolService::set_members(
            &pool,
            SetRoutePoolMembersInput {
                platform: "codex".to_string(),
                account_ids: vec![active_id.clone()],
            },
        )
        .await
        .expect("removed one member");

        let state = RoutePoolService::get(
            &pool,
            "codex".to_string(),
            None,
            Some(1),
            Some(20),
        )
        .await
        .expect("state");

        assert_eq!(state.stats.member_count, 1);
        assert_eq!(state.stats.request_count, 2);
        assert_eq!(state.stats.token_count, 512);
        assert_eq!(state.stats.request_row_count, 2);
        assert_eq!(state.stats.request_page, 1);
        assert_eq!(state.stats.request_page_size, 20);

        let request_names: Vec<&str> = state
            .stats
            .requests
            .iter()
            .filter_map(|request| request.account_name.as_deref())
            .collect();
        assert!(request_names.contains(&"RemovedCodex"));
        assert!(request_names.contains(&"ActiveCodex"));
        assert!(!request_names.contains(&"ClaudeOne"));
    }

    #[tokio::test]
    async fn stats_paginates_request_rows_and_reports_total() {
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

        usage_event_at(
            &pool,
            &account_id,
            "route_proxy",
            "request",
            1,
            "count",
            r#"{"path":"/v1/oldest","status":200}"#,
            "2026-07-17T08:00:00Z",
        )
        .await;
        usage_event_at(
            &pool,
            &account_id,
            "route_proxy",
            "request",
            1,
            "count",
            r#"{"path":"/v1/middle","status":200}"#,
            "2026-07-17T09:00:00Z",
        )
        .await;
        usage_event_at(
            &pool,
            &account_id,
            "route_proxy",
            "request",
            1,
            "count",
            r#"{"path":"/v1/newest","status":200}"#,
            "2026-07-17T10:00:00Z",
        )
        .await;

        let state = RoutePoolService::get(
            &pool,
            "codex".to_string(),
            None,
            Some(2),
            Some(2),
        )
        .await
        .expect("page two");

        assert_eq!(state.stats.request_count, 3);
        assert_eq!(state.stats.request_row_count, 3);
        assert_eq!(state.stats.request_page, 2);
        assert_eq!(state.stats.request_page_size, 2);
        assert_eq!(state.stats.requests.len(), 1);
        assert!(state.stats.requests[0].metadata_json.contains("/v1/oldest"));
    }

    #[tokio::test]
    async fn stats_normalizes_request_pagination_values() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let state = RoutePoolService::get(
            &pool,
            "codex".to_string(),
            None,
            Some(0),
            Some(500),
        )
        .await
        .expect("normalized pagination");

        assert_eq!(state.stats.request_page, 1);
        assert_eq!(state.stats.request_page_size, 100);
    }
```

- [ ] **Step 2: Run the failing Rust tests**

Run:

```powershell
pnpm rust:test -- route_pool_service
```

Expected: FAIL at compile time because `RoutePoolService::get` does not accept pagination arguments and `RoutePoolStats` does not expose request pagination fields.

- [ ] **Step 3: Extend the Rust response model**

In `src-tauri/src/models/route_pool.rs`, replace `RoutePoolStats` with:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoutePoolStats {
    pub member_count: i64,
    pub request_count: i64,
    pub token_count: i64,
    pub cost_micros: i64,
    pub recent_logs: Vec<RoutePoolUsageLog>,
    pub requests: Vec<RoutePoolUsageLog>,
    pub request_row_count: i64,
    pub request_page: i64,
    pub request_page_size: i64,
}
```

- [ ] **Step 4: Add pagination normalization to the route-pool service**

In `src-tauri/src/services/route_pool_service.rs`, add these constants below `pub struct RoutePoolService;`:

```rust
const DEFAULT_REQUEST_PAGE: i64 = 1;
const DEFAULT_REQUEST_PAGE_SIZE: i64 = 20;
const MAX_REQUEST_PAGE_SIZE: i64 = 100;
```

Replace `RoutePoolService::get` with:

```rust
    pub async fn get(
        pool: &SqlitePool,
        platform: String,
        since: Option<String>,
        request_page: Option<i64>,
        request_page_size: Option<i64>,
    ) -> Result<RoutePoolState, AppError> {
        let platform = normalize_platform(&platform)?;
        let since = normalize_since(since)?;
        let pagination = normalize_request_pagination(request_page, request_page_size);
        Self::state(
            pool,
            &platform,
            since.as_deref(),
            pagination.page,
            pagination.page_size,
        )
        .await
    }
```

Replace `Self::state(pool, &platform, None).await` in `set_members` with:

```rust
        Self::state(
            pool,
            &platform,
            None,
            DEFAULT_REQUEST_PAGE,
            DEFAULT_REQUEST_PAGE_SIZE,
        )
        .await
```

Replace the `stats` call inside `route_once` with:

```rust
            stats: RoutePoolRepository::stats(
                pool,
                &platform,
                None,
                DEFAULT_REQUEST_PAGE,
                DEFAULT_REQUEST_PAGE_SIZE,
            )
            .await?,
```

Replace the private `state` function with:

```rust
    async fn state(
        pool: &SqlitePool,
        platform: &str,
        since: Option<&str>,
        request_page: i64,
        request_page_size: i64,
    ) -> Result<RoutePoolState, AppError> {
        Ok(RoutePoolState {
            platform: platform.to_string(),
            account_ids: RoutePoolRepository::list_member_ids(pool, platform).await?,
            stats: RoutePoolRepository::stats(
                pool,
                platform,
                since,
                request_page,
                request_page_size,
            )
            .await?,
        })
    }
```

Add this helper below `normalize_since`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RequestPagination {
    page: i64,
    page_size: i64,
}

fn normalize_request_pagination(
    page: Option<i64>,
    page_size: Option<i64>,
) -> RequestPagination {
    RequestPagination {
        page: page.unwrap_or(DEFAULT_REQUEST_PAGE).max(1),
        page_size: page_size
            .unwrap_or(DEFAULT_REQUEST_PAGE_SIZE)
            .clamp(1, MAX_REQUEST_PAGE_SIZE),
    }
}
```

Update the existing `get_filters_stats_by_since_and_returns_request_rows` test call from:

```rust
        let state =
            RoutePoolService::get(&pool, "codex".to_string(), Some(since.to_string()))
                .await
                .expect("filtered state");
```

to:

```rust
        let state = RoutePoolService::get(
            &pool,
            "codex".to_string(),
            Some(since.to_string()),
            Some(1),
            Some(20),
        )
        .await
        .expect("filtered state");
```

Add these assertions in that same test after `assert_eq!(state.stats.requests.len(), 1);`:

```rust
        assert_eq!(state.stats.request_row_count, 1);
        assert_eq!(state.stats.request_page, 1);
        assert_eq!(state.stats.request_page_size, 20);
```

Update the existing `get_rejects_invalid_since_timestamp` test call from:

```rust
        let error =
            RoutePoolService::get(&pool, "codex".to_string(), Some("not-a-date".to_string()))
                .await
                .expect_err("invalid since");
```

to:

```rust
        let error = RoutePoolService::get(
            &pool,
            "codex".to_string(),
            Some("not-a-date".to_string()),
            None,
            None,
        )
        .await
        .expect_err("invalid since");
```

- [ ] **Step 5: Replace repository stats SQL with platform-scoped usage queries**

In `src-tauri/src/database/repositories/route_pool_repository.rs`, replace the full `pub async fn stats(...)` function with:

```rust
    pub async fn stats(
        pool: &SqlitePool,
        platform: &str,
        since: Option<&str>,
        request_page: i64,
        request_page_size: i64,
    ) -> Result<RoutePoolStats, AppError> {
        let usage_since_clause = if since.is_some() {
            " AND ue.created_at >= ?"
        } else {
            ""
        };
        let summary_sql = format!(
            "SELECT
               (SELECT COUNT(DISTINCT route_credential_id)
                FROM route_pool_members
                WHERE platform = ? AND enabled = 1) AS member_count,
               COALESCE(SUM(CASE WHEN ue.metric_type = 'request' THEN CASE WHEN ue.amount > 0 THEN ue.amount ELSE 1 END ELSE 0 END), 0) AS request_count,
               COALESCE(SUM(CASE WHEN ue.metric_type = 'token' OR ue.unit = 'token' THEN ue.amount ELSE 0 END), 0) AS token_count,
               COALESCE(SUM(CASE WHEN ue.metric_type = 'cost' AND ue.unit = 'usd_micros' THEN ue.amount ELSE 0 END), 0) AS cost_micros
             FROM usage_events ue
             INNER JOIN route_credentials a ON a.id = ue.route_credential_id
             WHERE a.platform = ?{usage_since_clause}"
        );
        let mut summary_query = sqlx::query(&summary_sql).bind(platform).bind(platform);
        if let Some(since) = since {
            summary_query = summary_query.bind(since);
        }
        let row = summary_query
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_pool_stats",
                message: "Could not load route pool statistics".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        let log_sql = format!(
            "SELECT ue.id, ue.route_credential_id, a.display_name AS account_name,
                    ue.source_label, ue.metric_type, ue.amount, ue.unit, ue.metadata_json, ue.created_at
             FROM usage_events ue
             INNER JOIN route_credentials a ON a.id = ue.route_credential_id
             WHERE a.platform = ?{usage_since_clause}
             ORDER BY ue.created_at DESC, ue.id DESC
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

        let request_count_sql = format!(
            "SELECT COUNT(*) AS request_row_count
             FROM usage_events ue
             INNER JOIN route_credentials a ON a.id = ue.route_credential_id
             WHERE a.platform = ? AND ue.metric_type = 'request'{usage_since_clause}"
        );
        let mut request_count_query = sqlx::query(&request_count_sql).bind(platform);
        if let Some(since) = since {
            request_count_query = request_count_query.bind(since);
        }
        let request_count_row =
            request_count_query
                .fetch_one(pool)
                .await
                .map_err(|err| AppError::Database {
                    code: "database.route_pool_request_count",
                    message: "Could not count route pool requests".to_string(),
                    details: Some(err.to_string()),
                    recoverable: true,
                })?;
        let request_row_count = request_count_row.get("request_row_count");

        let request_sql = format!(
            "SELECT ue.id, ue.route_credential_id, a.display_name AS account_name,
                    ue.source_label, ue.metric_type, ue.amount, ue.unit, ue.metadata_json, ue.created_at
             FROM usage_events ue
             INNER JOIN route_credentials a ON a.id = ue.route_credential_id
             WHERE a.platform = ? AND ue.metric_type = 'request'{usage_since_clause}
             ORDER BY ue.created_at DESC, ue.id DESC
             LIMIT ? OFFSET ?"
        );
        let offset = (request_page - 1) * request_page_size;
        let mut request_query = sqlx::query(&request_sql).bind(platform);
        if let Some(since) = since {
            request_query = request_query.bind(since);
        }
        let request_rows = request_query
            .bind(request_page_size)
            .bind(offset)
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
            request_row_count,
            request_page,
            request_page_size,
        })
    }
```

- [ ] **Step 6: Forward pagination through Tauri and web command adapters**

In `src-tauri/src/commands/route_pool_commands.rs`, replace `get_route_pool` with:

```rust
#[tauri::command]
pub async fn get_route_pool(
    state: State<'_, AppState>,
    platform: String,
    since: Option<String>,
    request_page: Option<i64>,
    request_page_size: Option<i64>,
) -> Result<RoutePoolState, ApiError> {
    RoutePoolService::get(
        &state.pool,
        platform,
        since,
        request_page,
        request_page_size,
    )
    .await
    .map_err(ApiError::from)
}
```

In `src-tauri/src/web/handlers/mod.rs`, replace the `"get_route_pool"` arm with:

```rust
        "get_route_pool" => {
            let platform = required_string_arg(&args, "platform")?;
            let since = optional_string_arg(&args, "since");
            let request_page = optional_i64_arg(&args, "request_page");
            let request_page_size = optional_i64_arg(&args, "request_page_size");
            to_value(
                RoutePoolService::get(
                    &state.pool,
                    platform,
                    since,
                    request_page,
                    request_page_size,
                )
                .await
                .map_err(to_error)?,
            )
        }
```

Add this helper near `optional_string_arg`:

```rust
fn optional_i64_arg(args: &Value, key: &str) -> Option<i64> {
    match args.get(key) {
        Some(Value::Number(number)) => number.as_i64(),
        Some(Value::String(text)) => text.trim().parse::<i64>().ok(),
        _ => None,
    }
}
```

- [ ] **Step 7: Run the backend tests**

Run:

```powershell
pnpm rust:test -- route_pool_service
```

Expected: PASS for all `route_pool_service` tests.

- [ ] **Step 8: Run Rust check**

Run:

```powershell
pnpm rust:check
```

Expected: PASS with no compiler errors.

- [ ] **Step 9: Commit backend changes**

Run:

```powershell
git add src-tauri/src/models/route_pool.rs src-tauri/src/services/route_pool_service.rs src-tauri/src/database/repositories/route_pool_repository.rs src-tauri/src/commands/route_pool_commands.rs src-tauri/src/web/handlers/mod.rs
git commit -m "feat: paginate route pool stats"
```

---

### Task 2: Frontend Request Pagination

**Files:**
- Modify: `src/lib/api/types.ts`
- Modify: `src/lib/api/client.ts`
- Modify: `src/screens/AccountsScreen.tsx`
- Test: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes: `RoutePoolStats.request_row_count`, `RoutePoolStats.request_page`, `RoutePoolStats.request_page_size`.
- Consumes: `get_route_pool` command arguments `request_page` and `request_page_size`.
- Produces: `getRoutePool(platform: string, since?: string | null, requestPage?: number | null, requestPageSize?: number | null): Promise<RoutePoolState>`.
- Produces: `AccountsScreen` request pagination controls with aria labels `上一页请求` and `下一页请求`.

- [ ] **Step 1: Update frontend test fixtures and write failing pagination assertions**

In `tests/AccountsScreen.test.tsx`, replace `statsFixture` with:

```ts
function statsFixture(overrides: Partial<RoutePoolStats> = {}): RoutePoolStats {
  return {
    member_count: 0,
    request_count: 0,
    token_count: 0,
    cost_micros: 0,
    recent_logs: [],
    requests: [],
    request_row_count: 0,
    request_page: 1,
    request_page_size: 20,
    ...overrides,
  };
}
```

Replace the existing `it("renders filtered route request statistics and request rows", ...)` test with:

```ts
  it("renders filtered route request statistics and paginates request rows", async () => {
    const expectedMonthStart = new Date();
    expectedMonthStart.setHours(0, 0, 0, 0);
    expectedMonthStart.setDate(1);

    vi.mocked(getRoutePool).mockImplementation(
      async (platform, since, requestPage = 1, requestPageSize = 20) => ({
        platform,
        account_ids: ["cred-official-1"],
        stats: statsFixture({
          member_count: 1,
          request_count: 99,
          token_count: 2048,
          cost_micros: 1500,
          request_row_count: 42,
          request_page: requestPage ?? 1,
          request_page_size: requestPageSize ?? 20,
          requests: [
            {
              id: `request-${requestPage ?? 1}-${since ?? "all"}`,
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
              account_name: "Broken Metadata Account",
              source_label: "route_proxy",
              metric_type: "request",
              amount: 1,
              unit: "count",
              metadata_json: "{bad json",
              created_at: "2026-07-17T08:01:00Z",
            },
          ],
        }),
      }),
    );

    renderScreen();

    await userEvent.click(await screen.findByLabelText("查看算力池统计"));

    expect(await screen.findByText("请求统计")).toBeInTheDocument();
    expect(screen.getByText("统计当前 Codex 的历史路由请求")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "当日" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "本周" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "本月" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "累计" })).toBeInTheDocument();
    expect(screen.getByText("共 42 条 · 每页 20 条")).toBeInTheDocument();
    expect(screen.getByText("第 1 / 3 页")).toBeInTheDocument();
    expect(screen.getByText("/v1/responses")).toBeInTheDocument();
    expect(screen.getByText("201")).toBeInTheDocument();
    expect(screen.getAllByText("route_proxy")).toHaveLength(2);
    const invalidMetadataRow = screen.getByText("Broken Metadata Account").closest("div");
    expect(invalidMetadataRow).not.toBeNull();
    expect(within(invalidMetadataRow as HTMLElement).getAllByText("-")).toHaveLength(2);

    await userEvent.click(screen.getByLabelText("下一页请求"));

    await waitFor(() =>
      expect(getRoutePool).toHaveBeenLastCalledWith(
        "codex",
        expect.any(String),
        2,
        20,
      ),
    );

    await userEvent.click(screen.getByRole("button", { name: "本月" }));

    await waitFor(() =>
      expect(getRoutePool).toHaveBeenLastCalledWith(
        "codex",
        expectedMonthStart.toISOString(),
        1,
        20,
      ),
    );

    await userEvent.click(screen.getByRole("button", { name: "累计" }));

    await waitFor(() => expect(getRoutePool).toHaveBeenLastCalledWith("codex", null, 1, 20));
  });
```

- [ ] **Step 2: Run the failing frontend test**

Run:

```powershell
pnpm test:run -- AccountsScreen.test.tsx
```

Expected: FAIL because pagination controls and updated historical stats copy do not exist yet.

- [ ] **Step 3: Extend frontend API types**

In `src/lib/api/types.ts`, replace `RoutePoolStats` with:

```ts
export type RoutePoolStats = {
  member_count: number;
  request_count: number;
  token_count: number;
  cost_micros: number;
  recent_logs: RoutePoolUsageLog[];
  requests: RoutePoolUsageLog[];
  request_row_count: number;
  request_page: number;
  request_page_size: number;
};
```

- [ ] **Step 4: Extend the frontend route-pool API wrapper**

In `src/lib/api/client.ts`, replace `getRoutePool` with:

```ts
export function getRoutePool(
  platform: string,
  since?: string | null,
  requestPage?: number | null,
  requestPageSize?: number | null,
): Promise<RoutePoolState> {
  return invoke("get_route_pool", {
    platform,
    since: since ?? null,
    request_page: requestPage ?? null,
    request_page_size: requestPageSize ?? null,
  });
}
```

- [ ] **Step 5: Add pagination state and derived values to `AccountsScreen`**

In `src/screens/AccountsScreen.tsx`, extend the lucide import:

```ts
  ChevronLeft,
  ChevronRight,
```

Add these constants below `routeStatsPeriods`:

```ts
const routeStatsPageSize = 20;
```

Add request-page state below `statsPeriod`:

```ts
  const [requestPage, setRequestPage] = useState(1);
```

Replace the route-pool query with:

```ts
  const routePoolQuery = useQuery({
    queryKey: ["route-pool", activePlatform, statsSince, requestPage, routeStatsPageSize],
    queryFn: () => getRoutePool(activePlatform, statsSince, requestPage, routeStatsPageSize),
  });
```

Add this platform reset effect after the query declarations:

```ts
  useEffect(() => {
    setRequestPage(1);
  }, [activePlatform]);
```

Add these derived values below `costTotal`:

```ts
  const requestRowCount = routeStats?.request_row_count ?? (routeStats?.requests ?? []).length;
  const resolvedRequestPage = routeStats?.request_page ?? requestPage;
  const resolvedRequestPageSize = routeStats?.request_page_size ?? routeStatsPageSize;
  const requestPageCount = Math.max(
    1,
    Math.ceil(requestRowCount / Math.max(1, resolvedRequestPageSize)),
  );
```

Add this page-clamping effect after the existing `configWriteOutcomes` timeout effect:

```ts
  useEffect(() => {
    const stats = routePoolQuery.data?.stats;
    if (!stats) {
      return;
    }
    const nextPageCount = Math.max(
      1,
      Math.ceil(stats.request_row_count / Math.max(1, stats.request_page_size)),
    );
    if (requestPage > nextPageCount) {
      setRequestPage(nextPageCount);
    }
  }, [requestPage, routePoolQuery.data?.stats]);
```

Add this helper next to `testRoute`:

```ts
  const selectStatsPeriod = (period: RouteStatsPeriod) => {
    setStatsPeriod(period);
    setRequestPage(1);
  };
```

- [ ] **Step 6: Add pagination controls and corrected copy**

In the period filter buttons, replace:

```tsx
                    onClick={() => setStatsPeriod(period.key)}
```

with:

```tsx
                    onClick={() => selectStatsPeriod(period.key)}
```

In `src/screens/AccountsScreen.tsx`, replace this copy:

```tsx
                <p className="text-[12px] text-stone-500">仅统计当前 {platformLabels[activePlatform]} 算力池账号</p>
```

with:

```tsx
                <p className="text-[12px] text-stone-500">统计当前 {platformLabels[activePlatform]} 的历史路由请求</p>
```

Replace the request-list header count:

```tsx
                  {(routeStats?.requests ?? []).length} 条
```

with:

```tsx
                  {requestRowCount} 条
```

Add this pagination footer immediately before the closing `</div>` of the request-list container:

```tsx
              <div className="flex flex-col gap-2 border-t border-stone-100 bg-stone-50 px-3 py-2 sm:flex-row sm:items-center sm:justify-between">
                <p className="text-[11px] font-medium text-stone-500">
                  共 {requestRowCount} 条 · 每页 {resolvedRequestPageSize} 条
                </p>
                <div className="flex items-center gap-2">
                  <button
                    aria-label="上一页请求"
                    className="inline-flex items-center gap-1.5 rounded-lg border border-stone-200 bg-white px-2.5 py-1.5 text-[12px] font-semibold text-stone-700 transition-colors hover:bg-stone-50 disabled:opacity-50"
                    disabled={resolvedRequestPage <= 1}
                    onClick={() => setRequestPage((page) => Math.max(1, page - 1))}
                    type="button"
                  >
                    <ChevronLeft className="h-3.5 w-3.5" />
                    上一页
                  </button>
                  <span className="min-w-20 text-center text-[12px] font-semibold text-stone-600">
                    第 {resolvedRequestPage} / {requestPageCount} 页
                  </span>
                  <button
                    aria-label="下一页请求"
                    className="inline-flex items-center gap-1.5 rounded-lg border border-stone-200 bg-white px-2.5 py-1.5 text-[12px] font-semibold text-stone-700 transition-colors hover:bg-stone-50 disabled:opacity-50"
                    disabled={resolvedRequestPage >= requestPageCount}
                    onClick={() => setRequestPage((page) => page + 1)}
                    type="button"
                  >
                    下一页
                    <ChevronRight className="h-3.5 w-3.5" />
                  </button>
                </div>
              </div>
```

- [ ] **Step 7: Run the frontend pagination test**

Run:

```powershell
pnpm test:run -- AccountsScreen.test.tsx
```

Expected: PASS for the updated route request statistics test and all existing `AccountsScreen` tests.

- [ ] **Step 8: Run frontend typecheck**

Run:

```powershell
pnpm typecheck
```

Expected: PASS with no TypeScript errors.

- [ ] **Step 9: Commit frontend pagination changes**

Run:

```powershell
git add src/lib/api/types.ts src/lib/api/client.ts src/screens/AccountsScreen.tsx tests/AccountsScreen.test.tsx
git commit -m "feat: add route stats pagination controls"
```

---

### Task 3: Statistics Auto-Refresh While Open

**Files:**
- Modify: `src/screens/AccountsScreen.tsx`
- Test: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes: existing `routePoolQuery` from Task 2.
- Produces: React Query polling with `refetchInterval: statsOpen ? 5000 : false`.
- Produces: immediate route-pool refetch when the statistics panel is opened.

- [ ] **Step 1: Write failing auto-refresh test**

Append this test in `tests/AccountsScreen.test.tsx` near the stats pagination test:

```ts
  it("auto refreshes route statistics only while the panel is open", async () => {
    vi.mocked(getRoutePool).mockResolvedValue({
      platform: "codex",
      account_ids: [],
      stats: statsFixture({
        request_row_count: 0,
        request_page: 1,
        request_page_size: 20,
      }),
    });

    renderScreen();

    await screen.findByText("Codex 账号");
    expect(getRoutePool).toHaveBeenCalledTimes(1);

    vi.useFakeTimers();

    act(() => {
      fireEvent.click(screen.getByLabelText("查看算力池统计"));
    });

    await act(async () => {
      await Promise.resolve();
    });

    expect(getRoutePool).toHaveBeenCalledTimes(2);

    await act(async () => {
      vi.advanceTimersByTime(5000);
      await Promise.resolve();
    });

    expect(getRoutePool).toHaveBeenCalledTimes(3);

    act(() => {
      fireEvent.click(screen.getByLabelText("查看算力池统计"));
    });

    await act(async () => {
      vi.advanceTimersByTime(5000);
      await Promise.resolve();
    });

    expect(getRoutePool).toHaveBeenCalledTimes(3);
  });
```

- [ ] **Step 2: Run the failing auto-refresh test**

Run:

```powershell
pnpm test:run -- AccountsScreen.test.tsx -t "auto refreshes route statistics"
```

Expected: FAIL because opening the stats panel does not force an immediate refetch and no 5-second polling interval is configured.

- [ ] **Step 3: Add the refresh interval constant**

In `src/screens/AccountsScreen.tsx`, add this constant next to `routeStatsPageSize`:

```ts
const routeStatsRefreshMs = 5000;
```

- [ ] **Step 4: Enable polling only while the statistics panel is open**

In `src/screens/AccountsScreen.tsx`, replace the route-pool query from Task 2 with:

```ts
  const routePoolQuery = useQuery({
    queryKey: ["route-pool", activePlatform, statsSince, requestPage, routeStatsPageSize],
    queryFn: () => getRoutePool(activePlatform, statsSince, requestPage, routeStatsPageSize),
    refetchInterval: statsOpen ? routeStatsRefreshMs : false,
  });
```

- [ ] **Step 5: Refetch immediately when the panel opens**

In `src/screens/AccountsScreen.tsx`, add this helper next to `testRoute`:

```ts
  const toggleStatsPanel = () => {
    if (!statsOpen) {
      void routePoolQuery.refetch();
    }
    setStatsOpen((open) => !open);
  };
```

Then replace the statistics button `onClick`:

```tsx
                onClick={() => setStatsOpen((open) => !open)}
```

with:

```tsx
                onClick={toggleStatsPanel}
```

- [ ] **Step 6: Run the auto-refresh test**

Run:

```powershell
pnpm test:run -- AccountsScreen.test.tsx -t "auto refreshes route statistics"
```

Expected: PASS for the auto-refresh test.

- [ ] **Step 7: Run all `AccountsScreen` tests**

Run:

```powershell
pnpm test:run -- AccountsScreen.test.tsx
```

Expected: PASS for all `AccountsScreen` tests.

- [ ] **Step 8: Commit auto-refresh changes**

Run:

```powershell
git add src/screens/AccountsScreen.tsx tests/AccountsScreen.test.tsx
git commit -m "feat: auto refresh route stats panel"
```

---

### Task 4: Full Verification

**Files:**
- Verify: `src-tauri/src/models/route_pool.rs`
- Verify: `src-tauri/src/services/route_pool_service.rs`
- Verify: `src-tauri/src/database/repositories/route_pool_repository.rs`
- Verify: `src-tauri/src/commands/route_pool_commands.rs`
- Verify: `src-tauri/src/web/handlers/mod.rs`
- Verify: `src/lib/api/types.ts`
- Verify: `src/lib/api/client.ts`
- Verify: `src/screens/AccountsScreen.tsx`
- Verify: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes: completed backend and frontend commits from Tasks 1 through 3.
- Produces: verified working tree with passing focused Rust and frontend checks.

- [ ] **Step 1: Run frontend typecheck**

Run:

```powershell
pnpm typecheck
```

Expected: PASS.

- [ ] **Step 2: Run frontend tests**

Run:

```powershell
pnpm test:run -- AccountsScreen.test.tsx
```

Expected: PASS.

- [ ] **Step 3: Run Rust route-pool tests**

Run:

```powershell
pnpm rust:test -- route_pool_service
```

Expected: PASS.

- [ ] **Step 4: Run Rust check**

Run:

```powershell
pnpm rust:check
```

Expected: PASS.

- [ ] **Step 5: Inspect the final diff**

Run:

```powershell
git status --short
git diff --stat HEAD
```

Expected: `git status --short` shows no unrelated changes staged. `git diff --stat HEAD` only shows files listed in this plan when the last commit is not yet made, or no output after all task commits.

- [ ] **Step 6: Record completion**

If Task 4 found an uncommitted verification-only adjustment, commit it with:

```powershell
git add src-tauri/src/models/route_pool.rs src-tauri/src/services/route_pool_service.rs src-tauri/src/database/repositories/route_pool_repository.rs src-tauri/src/commands/route_pool_commands.rs src-tauri/src/web/handlers/mod.rs src/lib/api/types.ts src/lib/api/client.ts src/screens/AccountsScreen.tsx tests/AccountsScreen.test.tsx
git commit -m "fix: stabilize route stats refresh pagination"
```

Expected: commit succeeds only if Task 4 required a code adjustment. If no adjustment was required, do not create an empty commit.
