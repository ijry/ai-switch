# Account Desktop Workspace Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Agent account page with a compact desktop workspace whose toolbar, pool strip, and status bar stay fixed while only the account/statistics content scrolls.

**Architecture:** `AccountsScreen` remains the query, mutation, and page-state coordinator. Focused presentational components under `src/components/accounts/` receive explicit data and callbacks; the only backend change in this layout plan is an optional full-pool health summary on `RoutePoolState`, because paginated credentials cannot produce truthful pool counts. The separate portable-transfer plans own the import/export dialogs and commands; this plan only defines the toolbar callback boundary that consumes them.

**Tech Stack:** React 18, TypeScript, TanStack Query, UnoCSS, Lucide React, Vitest/Testing Library, Rust, SQLx, Tauri.

## Global Constraints

- Work directly on `main`; do not create a branch or worktree.
- Do not commit unless the user explicitly requests it.
- Scope is only Agent account workspaces; other screens keep their current scrolling and layout.
- Fixed vertical rows are exactly `44px` toolbar, `30px` pool strip, `minmax(0, 1fr)` content, and `32px` status bar.
- Account rows target `48–54px`; common actions stay visible as icon-only Lucide buttons.
- The new account workspace uses `1px` neutral borders, `6–8px` normal radii, light shadows, stable hover/focus states, and no bevels, thick outlines, gradients, or scale-on-hover effects. Do not restyle the shared `AppLayout` background for non-Agent screens.
- Preserve existing paging, filtering, cross-page ordering, account tests, editing, deletion, quota refresh, and pool membership semantics.
- Selection clears on platform or segment changes and persists across paging, filtering, and page-size changes.
- Migration import/export behavior is implemented by separate plans. This plan defines optional `importAction`/`exportAction` component props for isolated layout tests, omits an action when its callback is absent, and supplies real dialog callbacks in Task 7.
- Cross-plan shared files are additive: preserve the secure-export plan's transfer DTOs and `RoutePoolRepository::pool_membership_map`, preserve the portable-import plan's `append_members_tx`, and add only the health-summary/layout contracts owned here.

---

### Task 1: Add Full-Pool Health Summary Contract

**Files:**
- Modify: `src-tauri/src/models/route_pool.rs:20`
- Modify: `src-tauri/src/database/repositories/route_pool_repository.rs:9`
- Modify: `src-tauri/src/services/route_pool_service.rs:158`
- Modify: `src/lib/api/types.ts:276`

**Interfaces:**
- Produces Rust `RoutePoolHealthSummary { total, available, cooldown, error }`.
- Adds `RoutePoolState.health_summary: Option<RoutePoolHealthSummary>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`.
- Adds TypeScript `RoutePoolHealthSummary` and optional `RoutePoolState.health_summary?: RoutePoolHealthSummary | null`.
- Classification order is mutually exclusive: every enabled member whose status is not `ok` (including disabled/auth-invalid/manual-action states) is `error`; an `ok` member with a malformed non-empty `next_retry_at` or `cooldown_until` is also `error`; remaining `ok` members with at least one valid future retry/cooldown timestamp are `cooldown`; all remaining enabled `ok` members are `available`. Past valid timestamps do not count as cooldown.

- [ ] **Step 1: Write the failing Rust service test**

Add `route_pool_health_summary_is_mutually_exclusive_and_uses_all_members` to the existing `#[cfg(test)]` module in `src-tauri/src/services/route_pool_service.rs`:

```rust
#[tokio::test]
async fn route_pool_health_summary_is_mutually_exclusive_and_uses_all_members() {
    let pool = create_memory_pool().await.expect("pool");
    run_migrations(&pool).await.expect("migrations");
    let available_id = credential(&pool, "codex", "Available", "ok").await;
    let cooldown_id = credential(&pool, "codex", "Cooldown", "ok").await;
    let error_id = credential(&pool, "codex", "Error", "error").await;
    let revoked_id = credential(&pool, "codex", "Revoked", "revoked").await;
    let malformed_id = credential(&pool, "codex", "Malformed", "ok").await;
    let future = (chrono::Utc::now() + chrono::Duration::minutes(10)).to_rfc3339();
    sqlx::query("UPDATE route_credentials SET next_retry_at = ? WHERE id = ?")
        .bind(future)
        .bind(&cooldown_id)
        .execute(&pool)
        .await
        .expect("cooldown");
    sqlx::query("UPDATE route_credentials SET cooldown_until = 'not-a-date' WHERE id = ?")
        .bind(&malformed_id)
        .execute(&pool)
        .await
        .expect("malformed cooldown");

    RoutePoolService::set_members(
        &pool,
        SetRoutePoolMembersInput {
            platform: "codex".to_string(),
            account_ids: vec![available_id, cooldown_id, error_id, revoked_id, malformed_id],
        },
    )
    .await
    .expect("members");

    let state = RoutePoolService::get(&pool, "codex".to_string(), None, None, None)
        .await
        .expect("state");
    let summary = state.health_summary.expect("health summary");
    assert_eq!(summary.total, 5);
    assert_eq!(summary.available, 1);
    assert_eq!(summary.cooldown, 1);
    assert_eq!(summary.error, 3);
    assert_eq!(summary.total, summary.available + summary.cooldown + summary.error);
}
```

- [ ] **Step 2: Run the focused test and verify it fails**

Run: `cargo test route_pool_health_summary_is_mutually_exclusive_and_uses_all_members --manifest-path src-tauri/Cargo.toml`

Expected: FAIL because `RoutePoolState` has no `health_summary` field.

- [ ] **Step 3: Add the DTO and repository classifier**

Add to `src-tauri/src/models/route_pool.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutePoolHealthSummary {
    pub total: i64,
    pub available: i64,
    pub cooldown: i64,
    pub error: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoutePoolState {
    pub platform: String,
    pub account_ids: Vec<String>,
    pub stats: RoutePoolStats,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_summary: Option<RoutePoolHealthSummary>,
}
```

Add `RoutePoolRepository::health_summary(pool, platform)` without changing the existing `pool_membership_map` or future `append_members_tx` signatures owned by the transfer plans. Select distinct `route_credential_id` values from enabled `route_pool_members`, join those IDs to `route_credentials`, parse both retry timestamps with `DateTime::parse_from_rfc3339`, apply the classification order above, and return counts whose sum equals `total`. The existing schema already enforces `UNIQUE(platform, route_credential_id)`; do not add or replace that migration in this plan. Keep the defensive `DISTINCT` so the summary remains truthful if a recovered legacy database contains duplicate rows. Call it from `RoutePoolService::state` and return `Some(summary)`.

- [ ] **Step 4: Mirror the optional contract in TypeScript**

Add to `src/lib/api/types.ts`:

```ts
export type RoutePoolHealthSummary = {
  total: number;
  available: number;
  cooldown: number;
  error: number;
};

export type RoutePoolState = {
  platform: string;
  account_ids: string[];
  stats: RoutePoolStats;
  health_summary?: RoutePoolHealthSummary | null;
};
```

- [ ] **Step 5: Run contract validation**

Run: `cargo test route_pool_health_summary_is_mutually_exclusive_and_uses_all_members --manifest-path src-tauri/Cargo.toml`

Expected: PASS with `total = available + cooldown + error`.

Run: `pnpm typecheck`

Expected: PASS; existing mocks remain valid because the new field is optional.

Update the existing route-model-test `queryClient.setQueryData(["route-pool", ...])` cache write in `AccountsScreen` to preserve `health_summary: routePoolQuery.data?.health_summary ?? null`; a model test may update `stats`, but it must not erase the full-pool health summary. Add a frontend assertion that the health counts remain visible after a successful model test.

### Task 2: Make Account Pages Own Their Scroll Container

**Files:**
- Modify: `src/components/layout/AppLayout.tsx:139`
- Modify: `tests/AppLayout.test.tsx:7`
- Create: `src/components/accounts/AccountIconButton.tsx`
- Create: `tests/AccountIconButton.test.tsx`

**Interfaces:**
- `AppLayout` derives `agentWorkspaceActive = Boolean(platformByAgentScreen[activeScreen])`.
- The right content shell exposes `data-testid="app-content-shell"` and uses `overflow-hidden` only for Agent screens; all other screens retain `overflow-y-auto`.
- `AccountIconButton` consumes `{ label, icon, tone?, pressed?, tooltipSide?, ...buttonProps }` and produces a stable `28px` icon button plus a hover/focus tooltip.

- [ ] **Step 1: Write failing layout and icon-button tests**

Add these assertions to `tests/AppLayout.test.tsx`:

```tsx
const { rerender } = render(
  <I18nProvider initialLanguage="zh-CN">
    <AppLayout activeScreen="Codex" onNavigate={vi.fn()}><div>content</div></AppLayout>
  </I18nProvider>,
);
expect(screen.getByTestId("app-content-shell")).toHaveClass("overflow-hidden");

rerender(
  <I18nProvider initialLanguage="zh-CN">
    <AppLayout activeScreen="OCR" onNavigate={vi.fn()}><div>content</div></AppLayout>
  </I18nProvider>,
);
expect(screen.getByTestId("app-content-shell")).toHaveClass("overflow-y-auto");
```

Create `tests/AccountIconButton.test.tsx` to render a `Copy` icon and assert the button is exactly `h-7 w-7`, has `aria-label="复制账号"`, and references a `role="tooltip"` node through `aria-describedby`. Tab to the button, assert the tooltip is available to the focused control, press Enter, and assert the callback fires. Add an `aria-disabled="true"` case that remains focusable, exposes the supplied reason, and ignores click/Enter; do not assert implementation-specific tooltip utility-class strings.

- [ ] **Step 2: Run focused tests and verify they fail**

Run: `pnpm test:run -- tests/AppLayout.test.tsx tests/AccountIconButton.test.tsx`

Expected: FAIL because the shell test id and `AccountIconButton` do not exist.

- [ ] **Step 3: Implement conditional overflow**

Keep existing padding/background classes and change only the overflow owner:

```tsx
const agentWorkspaceActive = Boolean(platformByAgentScreen[activeScreen]);

<section
  className={`h-full min-h-0 min-w-0 bg-gradient-to-br from-white via-stone-50 to-slate-100 p-3 sm:p-4 ${
    agentWorkspaceActive ? "overflow-hidden" : "overflow-y-auto"
  }`}
  data-testid="app-content-shell"
>
  {children}
</section>
```

- [ ] **Step 4: Implement the shared icon button**

Use this public shape in `src/components/accounts/AccountIconButton.tsx`:

```tsx
export type AccountIconButtonProps = Omit<
  ButtonHTMLAttributes<HTMLButtonElement>,
  "children" | "aria-label"
> & {
  label: string;
  icon: ReactNode;
  tone?: "neutral" | "primary" | "success" | "danger";
  pressed?: boolean;
  tooltipSide?: "top" | "bottom";
};
```

Render a `relative inline-flex` wrapper, a `grid h-7 w-7 place-items-center rounded-md border` button, and an absolutely positioned `role="tooltip"` element. Translate the incoming disabled state to `aria-disabled="true"` plus a guarded click/keyboard handler so the control remains keyboard-focusable and can expose its disabled reason; do not invoke the callback while disabled or pending. Use opacity/visibility transitions only, `focus-visible:ring-2`, `aria-disabled:opacity-40`, and no scaling.

- [ ] **Step 5: Run focused tests**

Run: `pnpm test:run -- tests/AppLayout.test.tsx tests/AccountIconButton.test.tsx`

Expected: PASS; Codex is non-scrolling, OCR still scrolls, and the icon button is keyboard operable.

Add a table-driven assertion for every key in `platformByAgentScreen` so all Agent account screens use `overflow-hidden`, and retain one non-Agent assertion for `OCR` using `overflow-y-auto`.

### Task 3: Build the Fixed Toolbar and Selection Mode

**Files:**
- Create: `src/components/accounts/AccountWorkspaceToolbar.tsx`
- Modify: `src/screens/AccountsScreen.tsx:1216`
- Modify: `tests/AccountsScreen.test.tsx:417`

**Interfaces:**
- Produces `AccountWorkspaceToolbar` with normal and selection action groups.
- The component owns only filter-Popover open/close state; filters, selection, mutations, and paging remain in `AccountsScreen`.
- Migration callbacks are optional at the component boundary so layout work can compile independently, but the completed product receives real callbacks from the portable-transfer integration. When a callback is absent, omit that action rather than rendering a knowingly non-functional control.

Use this prop contract:

```ts
export type WorkspaceToolbarAction = {
  disabled?: boolean;
  pending?: boolean;
  reason?: string;
  onInvoke: () => void;
};

export type AccountWorkspaceToolbarProps = {
  platformLabel: string;
  support: { displayName: string; supportLevel: PlatformSupportLevel } | null;
  view: "in_pool" | "out_of_pool" | "stats";
  selectedCount: number;
  filters: string[];
  filterOptions: Array<{ key: string; label: string }>;
  onToggleFilter: (key: string) => void;
  onClearFilters: () => void;
  createAction: WorkspaceToolbarAction;
  importAction?: WorkspaceToolbarAction;
  sessionsAction: WorkspaceToolbarAction;
  refreshAction: WorkspaceToolbarAction;
  refreshQuotaAction: WorkspaceToolbarAction;
  poolSelectionAction: WorkspaceToolbarAction;
  exportAction?: WorkspaceToolbarAction;
  deleteAction: WorkspaceToolbarAction;
  clearSelectionAction: WorkspaceToolbarAction;
};
```

For final integration, `AccountsScreen` snapshots `{ selection_context: { platform, pool_scope }, credential_ids: Array.from(selectedAccountIds) }` when `exportAction.onInvoke` runs, maps `importAction.onInvoke` to the import dialog opener, and passes the immutable snapshot to the export dialog. `exportAction.disabled` is true only when `selectedCount === 0`; export generation/pending state belongs to `RouteCredentialExportDialog`, not the toolbar or `AccountsScreen`.

- [ ] **Step 1: Write failing toolbar-mode tests**

In `tests/AccountsScreen.test.tsx`, add a test that initially finds icon buttons by `aria-label` for `新增账号`, `会话管理`, `打开账号筛选`, `刷新账号列表`, and `刷新官方账号额度`; when the portable-transfer callbacks are supplied, also find `批量迁移导入`. Select `Team Account` and assert the normal actions are replaced by `批量加入算力池`, `批量删除账号`, and `取消账号选择`, plus `导出所选账号` when its callback is supplied. Assert every rendered action has an accessible name, a tooltip containing its label or disabled reason, and a stable `h-7 w-7` hit box while pending.

- [ ] **Step 2: Run the focused test and verify it fails**

Run: `pnpm test:run -- tests/AccountsScreen.test.tsx -t "switches the fixed toolbar between normal and selection modes"`

Expected: FAIL because the existing page uses text buttons and a separate selection banner.

- [ ] **Step 3: Implement the toolbar**

Render a `h-11` header with a single-line left title/support area and `h-7 w-7` `AccountIconButton`s on the right. Use `Plus`, `FileInput` (or `Upload` if that is the icon exported by the installed `lucide-react` version), `MessageSquareText`, `ListFilter`, and `RefreshCw` in normal mode; use pool add/remove, `Download`, `Trash2`, and `X` in selection mode. Put the active filter count in a small absolute badge on the filter button.

The filter Popover must reuse the existing batch/single-account option labels, close on outside pointer down, call `onToggleFilter(key)`, and never add height to the workspace.

- [ ] **Step 4: Wire existing page state without changing selection semantics**

Remove `accountFilterMenuOpen`, `accountFilterMenuRef`, and their outside-click effect from `AccountsScreen`. Keep `toggleAccountFilter` and `removeAccountFilter` resetting only `accountPage`; do not clear `selectedAccountIds`. Keep the existing platform and `accountView` effects that clear selection.

Keep Task 3 presentational: toolbar unit tests pass explicit action callbacks, and `AccountsScreen` may omit the optional import/export action objects until Task 7. Do not import dialogs or duplicate transfer state in this task; final dialog placement, immutable export snapshots, and query invalidation have one owner in Task 7. No test treats a disabled migration action as the finished product.

- [ ] **Step 5: Run toolbar and existing account-action tests**

Run: `pnpm test:run -- tests/AccountsScreen.test.tsx -t "toolbar|filters accounts|supports batch"`

Expected: PASS; filter behavior and batch membership mutations remain unchanged.

### Task 4: Build the Compact Pool Status Strip and Details Popover

**Files:**
- Create: `src/components/accounts/PoolStatusStrip.tsx`
- Modify: `src/screens/AccountsScreen.tsx:1481`
- Modify: `tests/AccountsScreen.test.tsx:541`
- Modify: `tests/AccountsScreen.test.tsx:1412`
- Modify: `tests/AccountsScreen.test.tsx:1641`

**Interfaces:**
- `PoolStatusStrip` receives already-fetched pool/proxy data and callbacks; it starts no queries.
- It shows truthful full-pool counts from an explicit health-state union and never derives health counts from the current page. When `health.status === "ready"`, render `health.summary.total` as the member total so `available + cooldown + error` visibly reconciles with it; use `memberCount` only for loading/legacy responses.
- Detailed config-write and model-test output moves into a Popover anchored below the fixed strip.

Use this prop contract:

```ts
export type PoolHealthState =
  | { status: "loading" }
  | { status: "ready"; summary: RoutePoolHealthSummary }
  | { status: "unavailable" };

export type PoolStatusStripProps = {
  memberCount: number;
  health: PoolHealthState;
  proxyStatus?: RouteProxyStatus;
  lastRouteAccount: string | null;
  configWriteOutcomes: ConfigWriteOutcome[];
  configWriteError: string | null;
  modelTestOutcome: RoutePoolModelTestOutcome | null;
  modelTestError: string | null;
  proxyAction: WorkspaceToolbarAction;
  writeConfigAction: WorkspaceToolbarAction;
  testAction: WorkspaceToolbarAction;
  onClearConfigWriteResults: () => void;
  onCloseModelTestOutcome: () => void;
};
```

- [ ] **Step 1: Write failing strip tests**

Extend `statsFixture`/`getRoutePool` mocks with `health_summary: { total: 4, available: 2, cooldown: 1, error: 1 }`. Map query state to `{ status: "loading" }` while the first pool request is pending, `{ status: "ready", summary }` when the field exists, and `{ status: "unavailable" }` only after a successful legacy response omits it. Assert the strip exposes separate accessible elements for the pool label and total (`算力池` and `4 个账号`), plus `可用 2`, `冷却 1`, and `异常 1`; assert diagnostics are absent until `展开算力池详情` is activated. Assert loading shows `统计同步中`, while a completed legacy response shows `健康统计不可用`, never ambiguous em dashes.

Update the model-connectivity test to open the details Popover before asserting route-chain content. Replace `clears route config write results after a short delay` with a test that advances the timer and confirms the result remains available until the user activates `清除配置写入结果` or the proxy stops. Assert merely closing and reopening the Popover does not discard the stored result.

- [ ] **Step 2: Run focused tests and verify they fail**

Run: `pnpm test:run -- tests/AccountsScreen.test.tsx -t "pool status strip|credential pool route|keeps route config write results"`

Expected: FAIL because pool diagnostics currently consume page height and health counts are not rendered.

- [ ] **Step 3: Implement the fixed strip**

Render a `h-[30px]` row with a status dot, member total, three compact health labels, and icon-only proxy/write/test/details actions. Use `Power`/`PowerOff`, `FileCode2`, `Play`, and `ChevronDown`; preserve current disabled reasons as `aria-describedby` tooltips, and keep all action buttons `h-7 w-7` even while pending.

The Popover contains proxy URL, last routed account, config-write outcomes with an explicit `清除配置写入结果` control, model-test summary/route chain with its existing close control, and nested `<details>` for request/response payloads. It must be `absolute`, layered above the middle pane, and must not change strip height.

- [ ] **Step 4: Retain diagnostics and feed short status messages downward**

Remove the three-second `configWriteOutcomes` clearing effect from `AccountsScreen`. Keep clearing config-write results only when the proxy stops or `onClearConfigWriteResults` runs; closing the Popover changes only its open state. Preserve detailed errors in the Popover, while deriving one short feedback string for the status bar in Task 6; the status line uses `aria-live="polite"`.

- [ ] **Step 5: Run pool tests**

Run: `pnpm test:run -- tests/AccountsScreen.test.tsx -t "pool status strip|credential pool route|route config write"`

Expected: PASS; fixed actions stay usable with the details Popover closed.

### Task 5: Build Compact Account Rows and On-Demand Details

**Files:**
- Create: `src/components/accounts/accountWorkspacePresentation.ts`
- Create: `src/components/accounts/AccountListPane.tsx`
- Modify: `src/screens/AccountsScreen.tsx:1507`
- Modify: `tests/AccountsScreen.test.tsx:417`
- Modify: `tests/AccountsScreen.test.tsx:466`
- Modify: `tests/AccountsScreen.test.tsx:1478`

**Interfaces:**
- `buildAccountRowPresentation(credential)` returns non-secret row and detail data.
- `AccountListPane` owns no query or mutation state and permits only one expanded credential ID supplied by its parent.
- Current-page select-all adds/removes only visible IDs and leaves off-page selected IDs untouched.

Use these public types:

```ts
export type AccountDetailItem = { label: string; value: string; mono?: boolean };
export type AccountRowPresentation = {
  kindLabel: string;
  statusLabel: string;
  statusTone: "ok" | "warning" | "error";
  batchLabel: string | null;
  summary: string;
  details: AccountDetailItem[];
};

export type AccountListPaneProps = {
  scope: RouteCredentialPoolScope;
  credentials: RouteCredential[];
  total: number;
  filterSummary: string;
  selectedIds: ReadonlySet<string>;
  expandedCredentialId: string | null;
  draggedCredentialId: string | null;
  dragTargetIndex: number | null;
  loading: boolean;
  fetching: boolean;
  errorMessage: string | null;
  quotaActionAvailable: boolean;
  quotaUnavailableReason?: string;
  refreshingQuotaId: string | null;
  copiedCredentialId: string | null;
  copyPendingId: string | null;
  testPendingId: string | null;
  testCredentialAllowed: (credential: RouteCredential) => boolean;
  testUnavailableReason?: string;
  onToggleSelection: (credentialId: string) => void;
  onToggleCurrentPage: (selected: boolean) => void;
  onToggleExpanded: (credentialId: string) => void;
  onAddToPool: (credentialId: string) => void;
  onRemoveFromPool: (credentialId: string) => void;
  onRefreshQuota: (credentialId: string) => void;
  onCopy: (credential: RouteCredential) => void;
  onTest: (credential: RouteCredential) => void;
  onEdit: (credential: RouteCredential) => void;
  onDragStart: (credentialId: string, index: number) => void;
  onDragCancel: () => void;
  onDragTarget: (index: number) => void;
  onCommitReorder: (credentialId: string, index: number) => void;
  onRequestEdgePage: (direction: -1 | 1) => void;
  canRequestPreviousPage: boolean;
  canRequestNextPage: boolean;
  onRetryLoad: () => void;
};
```

- [ ] **Step 1: Write failing compact-row tests**

Assert each `[data-account-row]` contains an `h-13` (`52px`, within the approved `48–54px` target) default row. Its `复制 Team Account`, `测试 Team Account`, and `编辑 Team Account` buttons contain only SVGs and no visible action text, remain `h-7 w-7` while pending, and expose both `aria-label` and a focusable tooltip/disabled reason. Secondary values such as `team@example.com`, the full batch name, retry/failure data, and API Base URL are absent initially, while the compact truncated batch label remains visible in the default row.

Click `展开 Team Account 详情`; assert those secondary values appear. Expand `API Account`; assert Team Account details close and API Base URL/interface/model-mapping summary appear. Keep the existing copy and test callbacks asserted through their unchanged `aria-label`s. Rerender with a different `platform` and assert `expandedCredentialId` resets to `null`, so stale details cannot remain scoped to the previous platform.

- [ ] **Step 2: Run focused tests and verify they fail**

Run: `pnpm test:run -- tests/AccountsScreen.test.tsx -t "compact account rows|duplicates an account|single credential"`

Expected: FAIL because rows currently expose many badges and textual action labels and have no account-detail disclosure.

- [ ] **Step 3: Implement safe presentation helpers**

Move row-only formatting from `AccountsScreen` into `accountWorkspacePresentation.ts`. Parse `config_json` only for non-secret API details (`base_url`, `interface_format`, and model mapping labels/count); never expose `secret_payload_json` or API keys. Include email, kind, full batch, status, retry/cooldown, last failure, quota/subscription/reset, and request totals in `details` only.

- [ ] **Step 4: Implement the list pane**

Render a sticky `h-8` list header with an indeterminate current-page checkbox, column hints, filter summary, and total. Render skeleton rows at `h-13`, a centered empty state, or a compact retry banner as appropriate.

Each default row contains drag handle, checkbox, truncated name, short kind/batch/status indicators, one summary line, pool add/remove, applicable quota refresh, copy, test, edit, and disclosure icon buttons. Preserve existing drag-and-drop and keyboard behavior: Space/Enter toggles move mode, arrows commit adjacent moves, and Escape cancels. Keep `data-testid="account-list-edge-top"` and `data-testid="account-list-edge-bottom"`; dragging over either eligible edge for `600ms` calls `onRequestEdgePage(-1 | 1)` once, and disabled edge states do not page.

- [ ] **Step 5: Add parent handlers without clearing cross-page selection**

Add `expandedCredentialId` state to `AccountsScreen` and clear it in the same platform-change effect that clears selection; close it when the expanded credential leaves the active scoped result. Implement `onToggleCurrentPage(selected)` by cloning `selectedAccountIds` and adding/removing only `credentials.map(({ id }) => id)`. Implement row pool actions only after verifying the target ID exists in the current `credentials` array and matches the active `platform` plus `pool_scope`; then clone `draftPoolIds`, change one ID, and call existing `applyPoolMembership`. Ignore stale/out-of-scope IDs without mutating membership.

- [ ] **Step 6: Run list tests**

Run: `pnpm test:run -- tests/AccountsScreen.test.tsx -t "compact account rows|filters accounts|duplicates an account|single credential|allows an error-status|pages while dragging at list edges"`

Expected: PASS with unchanged mutation payloads and `aria-label`s.

### Task 6: Move Segments and Pagination into the Fixed Status Bar

**Files:**
- Create: `src/components/accounts/AccountWorkspaceStatusBar.tsx`
- Create: `src/components/accounts/AccountStatsPane.tsx`
- Modify: `src/screens/AccountsScreen.tsx:1227`
- Modify: `tests/AccountsScreen.test.tsx:516`
- Modify: `tests/AccountsScreen.test.tsx:1234`

**Interfaces:**
- `AccountWorkspaceStatusBar` switches between account and request pagination without initiating queries.
- `AccountStatsPane` renders the period selector, metric summary, request rows, and request details; pagination is removed from this pane.
- Add `requestPageSize` state initialized to `20`; replace the fixed `routeStatsPageSize` in the route-pool query key and call. It is page-local state: changing platform resets it to `20` and request page to `1`; changing statistics period resets only request page to `1`; switching account segments away from and back to statistics preserves both values for the current platform.
- Move the currently local `RouteStatsPeriod` alias out of `AccountsScreen`; `AccountStatsPane.tsx` exports `type RouteStatsPeriod = "today" | "week" | "month" | "all"`, and `AccountsScreen` imports that exact type.

Use this status-bar contract:

```ts
export type WorkspaceFeedback = {
  type: "success" | "error" | "info";
  message: string;
  details?: string;
};

export type WorkspacePagination = {
  page: number;
  pageCount: number;
  pageSize: number;
  pageSizeOptions: number[];
  onPageSizeChange: (size: number) => void;
  onPrevious: () => void;
  onNext: () => void;
};

export type AccountWorkspaceStatusBarProps = {
  view: "in_pool" | "out_of_pool" | "stats";
  selectedCount: number;
  total: number;
  feedback: WorkspaceFeedback | null;
  loading: boolean;
  errorMessage: string | null;
  accountPagination: WorkspacePagination;
  requestPagination: WorkspacePagination;
  onViewChange: (view: "in_pool" | "out_of_pool" | "stats") => void;
};

export type RouteStatsPeriod = "today" | "week" | "month" | "all";

export type AccountStatsPaneProps = {
  period: RouteStatsPeriod;
  since: string | null;
  stats: RoutePoolStats | null;
  loading: boolean;
  errorMessage: string | null;
  expandedRequestId: string | null;
  onPeriodChange: (period: RouteStatsPeriod) => void;
  onToggleRequest: (requestId: string) => void;
  onRetry: () => void;
};
```

- [ ] **Step 1: Write failing status-bar tests**

Update the segment test to locate `已入池`, `未入池`, and `统计` inside `data-testid="account-workspace-status-bar"`. Assert account view has a compact page-size select with `aria-label="账号每页数量"` plus icon-only `上一页账号` and `下一页账号` buttons; statistics view instead has `aria-label="请求每页数量"` plus icon-only `上一页请求` and `下一页请求` buttons. Assert previous/next controls contain only SVGs, retain `aria-label`/tooltips, and keep stable `h-7 w-7` dimensions.

In the statistics test, change request page size to `50` and assert `getRoutePool("codex", expectedSince, 1, 50)`. Add an explicit two-page `listRouteCredentialPage` mock: for `page: 1` return only `cred-official-1` with `page_count: 2`, `next_page_account_id: "cred-api-1"`; for `page: 2` return only `cred-api-1` with `page_count: 2`, `previous_page_account_id: "cred-official-1"`. Select page-1 and page-2 IDs and assert `已选 2 个账号`; switching segment clears it, while filter and page-size changes do not.

- [ ] **Step 2: Run focused tests and verify they fail**

Run: `pnpm test:run -- tests/AccountsScreen.test.tsx -t "status bar|segments with scoped actions|route request statistics|preserves cross-page selection"`

Expected: FAIL because segments and both pagers currently live in scrollable content and request page size is fixed.

- [ ] **Step 3: Implement the statistics pane**

Move existing statistics markup and `RouteRequestDetail` into `AccountStatsPane.tsx`. Give the period control a sticky `top-0` compact row, keep request details on demand, and remove its internal footer pager. The outer statistics list scrolls in `account-workspace-scroll-region`; bounded diagnostic `<pre>` payloads may keep their own nested `overflow-auto` so long lines do not widen the workspace.

- [ ] **Step 4: Implement the status bar**

Render a `h-8` footer with a flat bordered three-way segmented control on the left, one truncated `aria-live="polite"` status line in the center, and compact pagination on the right. Page-size selects retain their accessible compact labels; previous/next controls are icon-only `ChevronLeft`/`ChevronRight` buttons with stable labels, tooltips, dimensions, and disabled reasons. Page-size options are `20`, `50`, and `100`.

The status line resolves in the approved order: selected count, current segment total, latest operation feedback, then loading/error summary. `AccountWorkspaceStatusBar` owns only the local open/close state of a small upward details Popover; when `feedback.details` or `errorMessage` is longer than the summary, it exposes an icon-only `查看错误详情` control. Updates remain announced by `aria-live="polite"`; no undefined page callback is required.

- [ ] **Step 5: Wire account and request pagination**

Replace the local `RoutePoolFeedback` alias in `AccountsScreen` with the exported `WorkspaceFeedback` type so import outcomes can provide optional long details without widening the fixed status row. Changing either page size resets only its corresponding page to `1`. Changing statistics period resets request page to `1`; changing platform resets request page and request page size to their defaults. Paging, filtering, and page-size changes must not call `clearAccountSelection`; `selectAccountView` and platform effects continue to clear it.

- [ ] **Step 6: Run status-bar tests**

Run: `pnpm test:run -- tests/AccountsScreen.test.tsx -t "status bar|segments with scoped actions|route request statistics|preserves cross-page selection"`

Expected: PASS; only the active segment's pager is rendered.

### Task 7: Assemble the Four-Row Desktop Workspace

**Files:**
- Modify: `src/screens/AccountsScreen.tsx:2426`
- Consume: `src/components/accounts/RouteCredentialExportDialog.tsx`
- Consume: `src/components/accounts/RouteCredentialImportDialog.tsx`
- Consume: `src/lib/api/types.ts` (`RouteCredentialSelectionContext`, `RouteCredentialImportOutcome`)
- Modify: `tests/AccountsScreen.test.tsx:417`
- Modify: `tests/AppLayout.test.tsx:7`

**Interfaces:**
- `AccountsScreen` produces one `data-testid="account-workspace"` panel with four grid rows.
- The only page-level vertical scroll owner is `data-testid="account-workspace-scroll-region"`; nested bounded diagnostic payloads (`pre`/request details) may scroll internally and are not page scroll owners.
- Existing create, model-test, and edit overlays remain fixed overlays outside the four-row grid.
- `AccountsScreen` owns `exportRequest: { selection_context: RouteCredentialSelectionContext; credential_ids: string[] } | null` and `importDialogOpen: boolean`; transfer dialogs remain pure child components.
- The import dialog owns source text, preview, confirmation, commit, and completion-page state. `AccountsScreen.onImported` owns only short feedback plus invalidation of the global `route-credential-page`, `route-pool`, and `batch-groups` query prefixes, because one import may affect multiple platforms.
- Import success never calls `setRoutePoolMembers`/`replace_members` from the UI. Optional pool restoration remains the portable-import service's append-only transaction using the existing `UNIQUE(platform, route_credential_id)` constraint and `ON CONFLICT DO NOTHING`.
- Import success does not clear `selectedAccountIds`: the transfer array may contain mixed platforms, but existing selections still belong to the unchanged current platform/segment and are refreshed by the invalidated page query. Normal platform/segment changes remain the only selection reset boundary.

- [ ] **Step 1: Write the failing workspace-structure test**

Render `AccountsScreen`, then assert:

```tsx
const workspace = screen.getByTestId("account-workspace");
expect(workspace).toHaveClass(
  "grid",
  "h-full",
  "min-h-0",
  "overflow-hidden",
  "grid-rows-[44px_30px_minmax(0,1fr)_32px]",
);
expect(screen.getByTestId("account-workspace-toolbar")).not.toHaveClass("overflow-y-auto");
expect(screen.getByTestId("pool-status-strip")).not.toHaveClass("overflow-y-auto");
expect(screen.getByTestId("account-workspace-scroll-region")).toHaveClass("min-h-0", "overflow-y-auto");
expect(screen.getByTestId("account-workspace-status-bar")).not.toHaveClass("overflow-y-auto");
```

Switch to statistics and assert the same middle test id remains the sole page-level scroll owner; bounded request/response payloads may still have their own nested scroll containers.

- [ ] **Step 2: Run the focused test and verify it fails**

Run: `pnpm test:run -- tests/AccountsScreen.test.tsx -t "uses a four-row desktop workspace|wires portable transfer dialogs"`

Expected: FAIL because the current screen is multiple vertically stacked cards and does not own the final transfer-dialog callbacks/query invalidation.

- [ ] **Step 3: Replace the old render tree with the workspace shell**

Use this exact outer structure:

```tsx
<section className="flex h-full min-h-0 flex-col overflow-hidden">
  <div
    className="grid h-full min-h-0 grid-rows-[44px_30px_minmax(0,1fr)_32px] overflow-hidden rounded-lg border border-stone-200 bg-white shadow-sm"
    data-testid="account-workspace"
  >
    <AccountWorkspaceToolbar {...toolbarProps} />
    <PoolStatusStrip {...poolProps} />
    <div className="min-h-0 overflow-y-auto" data-testid="account-workspace-scroll-region">
      {statsOpen ? <AccountStatsPane {...statsProps} /> : <AccountListPane {...listProps} />}
    </div>
    <AccountWorkspaceStatusBar {...statusBarProps} />
  </div>
  {modelTestDialog}
  {routeCredentialImportDialog}
  {routeCredentialExportDialog}
  {createDialog}
  {editDrawer}
</section>
```

At the final integration checkpoint, implement these exact callbacks:

```tsx
const openExport = () => {
  if (accountView === "stats" || selectedAccountIds.size === 0) {
    return;
  }
  setExportRequest({
    selection_context: { platform: activePlatform, pool_scope: accountScope },
    credential_ids: Array.from(selectedAccountIds),
  });
};
const openImport = () => setImportDialogOpen(true);
const handleImported = (outcome: RouteCredentialImportOutcome) => {
  setRoutePoolFeedback({
    type: "success",
    message: `已导入 ${outcome.imported} 个账号`,
    details: `跳过重复 ${outcome.skipped_duplicates}，冲突 ${outcome.conflicts}，失败 ${outcome.failed}，恢复入池 ${outcome.restored_pool_members}`,
  });
  void Promise.all([
    queryClient.invalidateQueries({
      queryKey: ["route-credential-page"],
      refetchType: "active",
    }),
    queryClient.invalidateQueries({
      queryKey: ["route-pool"],
      refetchType: "active",
    }),
    queryClient.invalidateQueries({
      queryKey: ["batch-groups"],
      refetchType: "active",
    }),
  ]);
};

<AccountWorkspaceToolbar
  {...toolbarProps}
  importAction={{ onInvoke: openImport }}
  exportAction={{ disabled: selectedAccountIds.size === 0, onInvoke: openExport }}
/>
{exportRequest ? (
  <RouteCredentialExportDialog
    open
    selection_context={exportRequest.selection_context}
    credential_ids={exportRequest.credential_ids}
    onClose={() => setExportRequest(null)}
  />
) : null}
<RouteCredentialImportDialog
  open={importDialogOpen}
  onClose={() => setImportDialogOpen(false)}
  onImported={handleImported}
/>
```

Before opening export, assert `accountView !== "stats"`; the selection-mode toolbar is not rendered for statistics, so `accountScope` always represents the exact selected `in_pool`/`out_of_pool` segment. Freeze the selection snapshot for the dialog lifetime: later paging, filtering, or checkbox changes must not mutate `exportRequest`, and closing the dialog must not clear the underlying selection.

Dialogs remain overlays and never become a fifth workspace row. `handleImported` deliberately leaves the import dialog open so its completion page remains visible until the user closes it. Add integration tests that (1) open export, change current-page selection, and confirm dialog props keep the original IDs/context; (2) invoke `onImported`, assert all three query prefixes are invalidated exactly once with active refetch, assert no pool replacement client is called, preserve current selection, and keep the dialog open until `onClose`. The layout unit tests use explicit callback props; this assembly test runs after the two transfer plans have supplied the exact dialog components and props above.

Delete the old page header, gradient pool card, inline feedback blocks, selection banner, account-list card footer, statistics pager, and page-level `space-y-*` wrapper after their behavior is represented by the new components. Enumerate and relocate every existing branch: `quotaRefreshMessage`, `routePoolFeedback`, `configWriteError`, model-test pending/error/result, and their close/retry actions; no feedback state may be silently dropped. Add `motion-reduce:transition-none motion-reduce:animate-none` to the shared icon button, disclosure, Popover, and pending-state classes so reduced-motion users receive only necessary state changes.

- [ ] **Step 4: Preserve compact behavior at narrow desktop widths**

Use `min-w-0`, truncation, and fixed icon widths throughout. Allow toolbar/status text to truncate; do not wrap fixed rows or introduce horizontal page scrolling. Apply only short `transition-colors`/opacity transitions and retain visible focus rings.

- [ ] **Step 5: Run the complete frontend suite**

Run: `pnpm test:run -- tests/AppLayout.test.tsx tests/AccountIconButton.test.tsx tests/AccountsScreen.test.tsx tests/RouteCredentialExportDialog.test.tsx tests/RouteCredentialImportDialog.test.tsx tests/VibeScreen.test.tsx`

Expected: PASS with updated compact-layout assertions and all existing CRUD, pool, quota, model-test, filter, ordering, and statistics behaviors preserved.

Run: `pnpm typecheck`

Expected: PASS.

Run: `pnpm build`

Expected: PASS with no UnoCSS or TypeScript errors.

### Task 8: Final Regression and Accessibility Review

**Files:**
- Review: `src/components/accounts/AccountIconButton.tsx`
- Review: `src/components/accounts/AccountWorkspaceToolbar.tsx`
- Review: `src/components/accounts/PoolStatusStrip.tsx`
- Review: `src/components/accounts/AccountListPane.tsx`
- Review: `src/components/accounts/AccountStatsPane.tsx`
- Review: `src/components/accounts/AccountWorkspaceStatusBar.tsx`
- Review: `src/screens/AccountsScreen.tsx`

**Interfaces:**
- Confirms the approved layout and accessibility contract without adding new behavior.

- [ ] **Step 1: Run all Rust tests affected by the contract**

Run: `pnpm rust:test`

Expected: PASS, including the full-pool health summary test.

- [ ] **Step 2: Run the complete frontend test suite**

Run: `pnpm test:run`

Expected: PASS; unrelated screens retain their existing scroll behavior.

- [ ] **Step 3: Verify keyboard and disclosure behavior manually**

Run: `pnpm dev` (manual, long-running checkpoint; stop it after inspection)

Verify at common desktop window sizes that Tab reaches every icon action, each focused/hovered icon exposes a tooltip with an accessible name/reason, Enter/Space activates buttons, only one account detail is expanded, drag keyboard mode supports arrows and Escape, edge paging retains the `600ms` guard, and changing platform/segment clears selection while paging/filter/page-size changes do not. Check every Agent key in `platformByAgentScreen` and one non-Agent screen for the overflow exception.

- [ ] **Step 4: Verify fixed geometry manually**

Confirm the toolbar is `44px`, pool strip `30px`, status bar `32px`, default account rows are `52px`, and scrolling a long account list never moves the three fixed rows off screen. Confirm expanded request JSON can scroll inside its bounded diagnostic block without creating page-level scrolling.

- [ ] **Step 5: Verify visual constraints**

Confirm all common actions use Lucide icons without visible labels, borders are thin and neutral, radii are `6–8px`, shadows are light and limited to workspace/Popover/overlays, hover states do not move layout, and reduced-motion mode does not run decorative animation.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-04-account-desktop-workspace-layout.md`. Execute the secure-export and portable-import plans before this plan's final assembly step so the dialog props are available.

1. **Subagent-Driven (recommended)** — dispatch a fresh subagent per task with review checkpoints.
2. **Inline Execution** — execute tasks in this session using `superpowers:executing-plans` with batch checkpoints.
