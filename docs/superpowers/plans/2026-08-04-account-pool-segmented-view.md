# 账号池分段视图 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在账号页面加入“已入池 / 未入池 / 统计”分段器，并让分页、筛选、排序和加入/移出操作与当前分段保持一致。

**Architecture:** 后端分页和重排请求携带 `pool_scope`，repository 使用 `route_pool_members` 的启用成员关系做 `EXISTS` / `NOT EXISTS` 过滤，确保总数和跨页边界准确。前端以 `accountView` 管理三段视图，账号查询只在账号段启用，统计段复用现有算力池统计查询；成员操作根据段位只暴露加入或移出。

**Tech Stack:** Rust/Tauri commands, SQLite/sqlx, React, TypeScript, TanStack Query, Vitest, Testing Library。

## Global Constraints

- 分段固定为 `已入池`、`未入池`、`统计`，默认进入 `已入池`。
- 入池条件只统计 `route_pool_members.enabled = 1` 且匹配当前平台的账号。
- 账号分页大小仍只允许 `20`、`50`、`100`。
- 不改变算力池成员存储结构、统计口径和批次筛选语义。
- 直接在 `main` 工作，不创建分支、worktree 或提交。

---

### Task 1: Extend Pool-Scoped Account Contracts

**Files:**
- Modify: `src-tauri/src/models/route_credential.rs`
- Modify: `src/lib/api/types.ts`
- Modify: `src/lib/api/client.ts`
- Modify: `src-tauri/src/commands/route_credential_commands.rs`

**Interfaces:**
- Add Rust enum `RouteCredentialPoolScope` with serde values `in_pool` and `out_of_pool`.
- Add `pool_scope: RouteCredentialPoolScope` to `RouteCredentialPageRequest` and `ReorderRouteCredentialInput`.
- Add TypeScript union `RouteCredentialPoolScope = "in_pool" | "out_of_pool"` and matching request fields.
- Keep command names `list_route_credentials_page` and `reorder_route_credentials` unchanged.

- [ ] **Step 1: Define the shared scope types**

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteCredentialPoolScope {
    InPool,
    OutOfPool,
}
```

Add the enum field to both request structs without changing the existing `filters` or `page_size` fields.

- [ ] **Step 2: Mirror the contract in TypeScript**

```ts
export type RouteCredentialPoolScope = "in_pool" | "out_of_pool";
```

Add `pool_scope: RouteCredentialPoolScope` to `RouteCredentialPageRequest` and `ReorderRouteCredentialInput`; update the client functions only through their existing typed inputs.

- [ ] **Step 3: Run contract/type checks**

Run: `pnpm typecheck`

Expected: TypeScript and Rust-facing invoke payload types compile; failures identify every existing call site that still needs `pool_scope`.

### Task 2: Add Pool Membership Filtering to Repository Pagination and Reorder

**Files:**
- Modify: `src-tauri/src/database/repositories/route_credential_repository.rs`
- Modify: `src-tauri/src/services/route_credential_service.rs`
- Test: `src-tauri/src/database/repositories/route_credential_repository.rs` (existing repository test module)

**Interfaces:**
- Add a helper `push_pool_scope_predicate(builder: &mut QueryBuilder<Sqlite>, scope: RouteCredentialPoolScope)` that emits an enabled-member `EXISTS` clause for `InPool` and `NOT EXISTS` for `OutOfPool`.
- Apply the helper to count, item, and page-boundary queries.
- Reorder receives the same scope, filters only matching IDs, and preserves nonmatching IDs in their original slots.

- [ ] **Step 1: Add failing repository coverage**

Create a fixture with three credentials on one platform, insert two enabled `route_pool_members` rows, then assert:

```rust
let in_page = RouteCredentialRepository::page(pool, request("in_pool")).await.unwrap();
assert_eq!(in_page.total, 2);
assert!(in_page.items.iter().all(|item| member_ids.contains(&item.id)));

let out_page = RouteCredentialRepository::page(pool, request("out_of_pool")).await.unwrap();
assert_eq!(out_page.total, 1);
assert_eq!(out_page.items[0].id, outside_id);
```

Also cover an empty scope (`total == 0`, `page_count == 1`, empty `items`) and a page request beyond `page_count` returning the last valid page.

- [ ] **Step 2: Implement the shared SQL predicate**

Use the existing `QueryBuilder<Sqlite>` pattern and this relationship:

```sql
EXISTS (
  SELECT 1 FROM route_pool_members rpm
  WHERE rpm.platform = rc.platform
    AND rpm.route_credential_id = rc.id
    AND rpm.enabled = 1
)
```

Negate the complete `EXISTS` expression for `OutOfPool`. Apply it after `rc.platform = ?` and before the existing batch predicate in every page query.

- [ ] **Step 3: Make reorder scope-aware**

Load `(id, batch_id)` in the existing sort order, retain rows matching both `pool_scope` and `filters`, validate neighbors within that filtered sequence, and replace only matching slots in the complete platform order. Pass `pool_scope` into the final `Self::page` request so the response stays on the correct segment.

- [ ] **Step 4: Run focused Rust tests**

Run: `cargo test route_credential_repository`

Expected: existing repository tests plus new pool-scope pagination and reorder tests pass.

### Task 3: Replace the Stats Toggle with the Account Segmented Control

**Files:**
- Modify: `src/screens/AccountsScreen.tsx`
- Modify: `src/lib/i18n.tsx` only if shared translation keys are required
- Test: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Add `type AccountView = "in_pool" | "out_of_pool" | "stats"` and `const [accountView, setAccountView] = useState<AccountView>("in_pool")`.
- Include the derived account scope in the page query key and request.
- Keep `routePoolQuery` as the statistics source; its refresh interval is enabled only when `accountView === "stats"`.

- [ ] **Step 1: Add failing UI tests for segment switching**

Extend the existing `AccountsScreen` mocks with `pool_scope` in page requests and assert:

```tsx
expect(screen.getByRole("button", { name: "已入池" })).toHaveAttribute("aria-pressed", "true");
await user.click(screen.getByRole("button", { name: "未入池" }));
expect(screen.getByText("未入池")).toBeInTheDocument();
expect(listRouteCredentialPage).toHaveBeenLastCalledWith(expect.objectContaining({ pool_scope: "out_of_pool" }));

await user.click(screen.getByRole("button", { name: "统计" }));
expect(screen.getByText("请求统计")).toBeInTheDocument();
```

Assert that switching away from an account segment clears selection and resets the account page to `1`.

- [ ] **Step 2: Render the segmented control**

Replace the header's separate statistics toggle with a three-button segmented control. Use `aria-pressed`, stable labels, and the existing `BarChart3` icon for the stats segment. Keep platform navigation and proxy controls unchanged.

- [ ] **Step 3: Scope the account query and stats refresh**

Set `pool_scope` from `accountView`, add it to the query key, reset page/selection on view changes, and set the account query `enabled` flag to `accountView !== "stats"`. Keep `statsOpen` behavior equivalent by deriving it from `accountView === "stats"`.

Render the account-management `<section>` only when `accountView !== "stats"`; the statistics segment must leave only the existing request statistics panel visible.

- [ ] **Step 4: Run the focused UI tests**

Run: `pnpm vitest run tests/AccountsScreen.test.tsx`

Expected: existing account workflows plus the new segment tests pass.

### Task 4: Make Pool Actions Segment-Aware

**Files:**
- Modify: `src/screens/AccountsScreen.tsx`
- Test: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- In `in_pool`, expose only remove actions; in `out_of_pool`, expose only add actions.
- Keep delete, refresh, quota, edit, copy, and model-test actions available in account segments unless already disabled by their existing capability rules.
- Do not render the account-management section, account selection, reorder handles, or pool mutation buttons in `stats`.

- [ ] **Step 1: Add failing action-direction tests**

For each segment, select an account and assert only the corresponding bulk button is present:

```tsx
expect(screen.getByRole("button", { name: "批量移出算力池" })).toBeInTheDocument();
expect(screen.queryByRole("button", { name: "批量加入算力池" })).not.toBeInTheDocument();
```

Repeat with the inverse assertions for `out_of_pool`, and assert both pool action buttons are absent in `stats`.

- [ ] **Step 2: Gate row and bulk actions by `accountView`**

Use the existing `draftPoolIds` membership set only for display badges; choose `addSelectedToPool` or `removeSelectedFromPool` from the active segment and render one button. When a mutation succeeds, invalidate both the scoped account query and the route-pool query so the account moves to the other segment immediately.

- [ ] **Step 3: Pass pool scope into reorder**

When committing an account reorder, include the active account scope in `ReorderRouteCredentialInput`. Keep the current neighbor calculation and cross-page edge behavior intact.

- [ ] **Step 4: Run UI tests and typecheck**

Run: `pnpm typecheck; pnpm vitest run tests/AccountsScreen.test.tsx`

Expected: no type errors and all segment/action tests pass.

### Task 5: Full Verification and Documentation Review

**Files:**
- Verify: `docs/superpowers/specs/2026-08-04-account-pool-segmented-view-design.md`
- Verify: all files modified in Tasks 1-4

- [ ] **Step 1: Run the production and server checks**

Run: `pnpm build; pnpm server:check`

Expected: both commands exit successfully; existing dependency/compiler warnings may remain but no new errors are introduced.

- [ ] **Step 2: Run focused Rust coverage**

Run: `cargo test route_credential_repository; cargo test web::handlers::tests`

Expected: all repository and web handler tests pass.

- [ ] **Step 3: Review the final diff**

Run: `git diff --check; git status --short`

Confirm no generated test logs or unrelated files are added, and that the spec's default segment, operation direction, pagination, sorting, and stats requirements are all represented in code and tests.
