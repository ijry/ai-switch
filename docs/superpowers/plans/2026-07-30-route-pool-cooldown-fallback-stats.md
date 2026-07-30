# Route Pool Cooldown Fallback And Stats Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (recommended) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Probe the earliest cooling credential when the pool has no immediately eligible account and show per-account request outcomes in the account list.

**Architecture:** Centralize pool credential selection in the proxy service so normal eligibility and all-cooling fallback use one implementation shared by model tests. Extend the existing route credential list query with one aggregate usage-events subquery, then render the returned counters in the existing account row.

**Tech Stack:** Rust 2021, SQLite/sqlx, chrono, Tauri IPC, React/TypeScript, Vitest.

## Global Constraints

- When every account is cooling, immediately probe only the credential with the earliest retry time.
- A request still attempts each credential at most once.
- Request statistics use existing `route_proxy` and `route_pool_model_test` request events.
- Remove email and platform/agent placeholder text from the account row.
- Empty pool membership continues to return `route_pool.empty`.

---

### Task 1: Centralize Cooldown Fallback Selection

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs`
- Modify: `src-tauri/src/services/route_model_test_service.rs`
- Test: `src-tauri/src/services/route_proxy_service.rs`
- Test: `src-tauri/src/services/route_model_test_service.rs`

**Interfaces:**
- Produces `pub async fn select_pool_credentials(pool: &SqlitePool, platform: &str) -> Result<Vec<SelectedCredential>, AppError>`.
- `select_pool_credentials` returns all immediately eligible credentials, or a one-element vector containing the earliest cooling credential when none are eligible.

- [ ] **Step 1: Add failing fallback tests**

Create two credentials with future `cooldown_until` values and assert the helper returns the earlier one. Add a tie test that asserts route-pool order wins. Add a test proving an eligible credential suppresses all cooling credentials.

- [ ] **Step 2: Implement one shared selector**

Move the proxy pool query into `select_pool_credentials`, load all pool rows with retry timestamps, filter quota-exhausted records, then split rows with `credential_is_retryable_now`. Sort cooling candidates by parsed retry time and original pool order; return the first candidate only when the eligible list is empty. Treat missing or invalid timestamps as eligible.

- [ ] **Step 3: Reuse the selector from proxy and model tests**

Replace proxy `load_pool_credentials` calls and remove the duplicate model-test loader. Import `select_pool_credentials` in `route_model_test_service.rs`; keep explicit account selection checks unchanged.

- [ ] **Step 4: Run focused selector tests**

Run `cargo test --manifest-path src-tauri/Cargo.toml route_proxy_service route_model_test_service -- --nocapture` as two separate cargo commands because Cargo accepts one filter at a time. Expect all focused tests to pass.

- [ ] **Step 5: Commit selector changes**

```bash
git add src-tauri/src/services/route_proxy_service.rs src-tauri/src/services/route_model_test_service.rs
git commit -m "fix: probe earliest cooling route credential"
```

### Task 2: Add Per-Credential Request Statistics

**Files:**
- Modify: `src-tauri/src/models/route_credential.rs`
- Modify: `src-tauri/src/database/repositories/route_credential_repository.rs`
- Test: `src-tauri/src/database/repositories/route_credential_repository.rs`

**Interfaces:**
- Adds `RouteCredential.request_count: i64`, `success_count: i64`, `failure_count: i64`, and `success_rate: Option<f64>`.
- `RouteCredentialRepository::list_by_platform` returns aggregate counters from `usage_events`.

- [ ] **Step 1: Add model fields with safe defaults**

Add the four serialized fields. Use `#[sqlx(default)]` for fields omitted by `get` and create/update queries, so single-credential commands remain compatible while list queries provide aggregates.

- [ ] **Step 2: Extend the list query with one aggregate join**

Join a grouped `usage_events` subquery filtered to `source_label IN ('route_proxy', 'route_pool_model_test')` and `metric_type = 'request'`. Count successes with `json_extract(metadata_json, '$.success') = 1`; compute failures as total minus successes and success rate as a nullable `success * 100.0 / total`.

- [ ] **Step 3: Add repository aggregation tests**

Insert synthetic request events for two credentials with mixed success values and assert totals, failures, percentages, and `None` success rate for an account without events. Confirm non-request token events do not affect counters.

- [ ] **Step 4: Run repository and service regression tests**

Run `cargo test --manifest-path src-tauri/Cargo.toml route_credential_repository -- --nocapture` and verify existing route credential tests remain green.

- [ ] **Step 5: Commit statistics changes**

```bash
git add src-tauri/src/models/route_credential.rs src-tauri/src/database/repositories/route_credential_repository.rs
git commit -m "feat: expose route credential request statistics"
```

### Task 3: Render Statistics In Account Rows

**Files:**
- Modify: `src/lib/api/types.ts`
- Modify: `src/screens/AccountsScreen.tsx`
- Modify: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Extends `RouteCredential` TypeScript type with `request_count`, `success_count`, `failure_count`, and nullable `success_rate`.

- [ ] **Step 1: Add frontend type fields and format helper**

Add numeric fields to `RouteCredential` and implement a helper that renders `请求 N · 成功 N · 失败 N · 成功率 N%`, or `暂无请求` when total is zero. Format rates with at most one decimal place.

- [ ] **Step 2: Replace the placeholder subtitle**

Remove `{credential.email ?? credential.platform} · {shortId(credential.id)}` from the account row and render the statistics line below the account name. Keep the existing kind, pool, status, quota, and retry badges.

- [ ] **Step 3: Update account screen tests**

Add a credential fixture with mixed counters and assert the statistics line renders. Assert the email and platform placeholder text are absent. Add a zero-request fixture and assert `暂无请求`.

- [ ] **Step 4: Run frontend checks**

Run `pnpm typecheck` and `pnpm test:run tests/AccountsScreen.test.tsx`; expect PASS.

- [ ] **Step 5: Commit frontend changes**

```bash
git add src/lib/api/types.ts src/screens/AccountsScreen.tsx tests/AccountsScreen.test.tsx
git commit -m "feat: show route credential request stats"
```

### Task 4: Full Verification

**Files:**
- Modify only feature files if a check exposes a direct regression.

- [ ] **Step 1: Format and compile Rust**

Run `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` and `pnpm rust:check`.

- [ ] **Step 2: Run full Rust tests**

Run `pnpm rust:test` and expect all non-ignored tests to pass.

- [ ] **Step 3: Run frontend typecheck and focused tests**

Run `pnpm typecheck` and `pnpm test:run tests/AccountsScreen.test.tsx`.

- [ ] **Step 4: Inspect the final worktree**

Run `git -c core.whitespace=cr-at-eol diff --check` and `git status --short`; leave pre-existing untracked `tauri-dev.err` and `tauri-dev.log.err` untouched.

