# Account Archive Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task with review checkpoints. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add reversible batch archive and restore actions for route credentials and expose an `已归档` account segment while keeping archived credentials out of active routing.

**Architecture:** Store archive state as a nullable `route_credentials.archived_at` timestamp. Extend the existing pool-scope query abstraction with an `archived` scope, preserve `route_pool_members`, and add transactional batch commands that set or clear the timestamp. The React account screen consumes the new scope and exposes selection-dependent archive/restore actions in the existing compact workspace.

**Tech Stack:** Rust, SQLite, SQLx, Tauri commands, the existing web command dispatcher, React, TypeScript, TanStack Query, lucide-react, Vitest, and Testing Library.

## Global Constraints

- Keep archive state separate from the existing `status` field.
- Preserve `route_pool_members` when archiving; archived credentials must not be routed or quota-refreshed.
- Support batch selection actions only; do not add per-row archive text buttons.
- Keep the status-bar segments in the order `算力池`, `未入池`, `已归档`, `统计`.
- Keep archive and restore operations transactional, idempotent, and reversible.
- Do not create a branch, worktree, or commit; work directly on `main` and leave commits to the user.

---

### Task 1: Add Archive Storage And Shared Types

**Files:**
- Create: `src-tauri/migrations/202608050001_route_credential_archive.sql`
- Modify: `src-tauri/src/models/route_credential.rs`
- Modify: `src-tauri/src/models/route_credential_transfer.rs` tests if model literals require updates
- Modify: `src/lib/api/types.ts`
- Test fixtures: `src-tauri/src/database/repositories/route_credential_repository.rs`, `src-tauri/src/services/cpa_export_service.rs`, `src-tauri/src/services/route_credential_transfer_service.rs`, `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Rust `RouteCredential` gains `pub archived_at: Option<String>`.
- Rust `RouteCredentialPoolScope` gains `Archived` serialized as `"archived"`.
- TypeScript `RouteCredential` gains `archived_at?: string | null`.
- TypeScript `RouteCredentialPoolScope` becomes `"in_pool" | "out_of_pool" | "archived"`.

- [ ] **Step 1: Add the additive SQLite migration**

Create `202608050001_route_credential_archive.sql` with:

```sql
ALTER TABLE route_credentials ADD COLUMN archived_at TEXT;

CREATE INDEX IF NOT EXISTS idx_route_credentials_archive
  ON route_credentials(platform, archived_at, sort_order);
```

- [ ] **Step 2: Extend the Rust model and scope enum**

Add `archived_at` to `RouteCredential` near the timestamp fields and add this enum variant:

```rust
#[serde(rename_all = "snake_case")]
pub enum RouteCredentialPoolScope {
    InPool,
    OutOfPool,
    Archived,
}
```

Keep `Default` as `OutOfPool`. Update every Rust `RouteCredential { ... }` test fixture with `archived_at: None`.

- [ ] **Step 3: Update TypeScript API types and fixtures**

Add `archived_at?: string | null` to `RouteCredential` and the `archived` union member to `RouteCredentialPoolScope`. Add `archived_at: null` to `credentialsFixture` and any inline route-credential fixture objects so strict type checking remains enabled.

- [ ] **Step 4: Run model-level checks**

Run `cargo check --manifest-path src-tauri/Cargo.toml` and `pnpm typecheck`.

Expected: the new migration is discovered by the existing migration runner; any failures identify remaining `RouteCredential` literals or scope matches to update in Task 2.

---

### Task 2: Implement Scoped Repository Queries And Runtime Exclusion

**Files:**
- Modify: `src-tauri/src/database/repositories/route_credential_repository.rs`
- Modify: `src-tauri/src/database/repositories/route_pool_repository.rs`
- Modify: `src-tauri/src/services/route_proxy_service.rs`
- Modify: `src-tauri/src/services/route_model_test_service.rs`
- Modify: `src-tauri/src/services/route_quota_service.rs`
- Test: repository and route-pool test modules in the files above

**Interfaces:**
- `RouteCredentialRepository::set_archived(pool: &SqlitePool, ids: &[String], archived: bool) -> Result<(), AppError>` performs one transactional batch update.
- `push_pool_scope_predicate` accepts `Archived` and applies archive-state predicates to page boundaries, pages, and selection queries.
- `RouteCredentialRepository::list_by_platform` returns active credentials only; `get` continues to return an individual credential for editing/restoration.

- [ ] **Step 1: Add failing repository tests for archive scope and batch state**

Extend the existing in-memory repository tests to create two credentials, place one in the pool, call `set_archived` for both, and assert:

```rust
assert_eq!(page(pool, "codex", RouteCredentialPoolScope::InPool).await.unwrap().total, 0);
assert_eq!(page(pool, "codex", RouteCredentialPoolScope::OutOfPool).await.unwrap().total, 0);
assert_eq!(page(pool, "codex", RouteCredentialPoolScope::Archived).await.unwrap().total, 2);
assert_eq!(RoutePoolRepository::list_member_ids(pool, "codex").await.unwrap().len(), 1);
```

Also assert that clearing the archive flag returns the credential to its original in-pool or out-of-pool page, and that an empty ID list and a missing ID return validation errors.

- [ ] **Step 2: Run the focused Rust test and verify it fails**

Run `cargo test --manifest-path src-tauri/Cargo.toml route_credential_repository --lib`.

Expected: FAIL because `Archived` and `set_archived` do not yet exist and current queries do not filter `archived_at`.

- [ ] **Step 3: Add archive columns to every credential projection**

Add `rc.archived_at` to `PAGE_SELECT`, `get`, `list_by_ids`, `list_by_platform`, and transfer-candidate projections. Update the `RouteCredential` field order/aliases so SQLx can decode the nullable timestamp.

- [ ] **Step 4: Centralize the three scope predicates**

Update `push_pool_scope_predicate` so it emits:

```sql
-- InPool / OutOfPool
rc.archived_at IS NULL AND (EXISTS (...) OR NOT EXISTS (...))
-- Archived
rc.archived_at IS NOT NULL
```

Apply the helper to page counts, page items, page boundaries, selected export IDs, and any legacy list fallback. Update `reorder` matching so archived rows can retain sort order without being treated as active pool rows.

- [ ] **Step 5: Add the transactional batch update**

Implement `set_archived` by trimming and de-duplicating IDs, rejecting an empty list, starting a transaction, verifying that the number of requested IDs exists, updating `archived_at` to `Utc::now().to_rfc3339()` or `NULL` plus `updated_at`, and committing. Return `validation.route_credential_not_found` when any ID is missing so the transaction rolls back. Repeating the same state is successful.

- [ ] **Step 6: Exclude archived credentials from active runtime paths**

Make these concrete query changes while preserving pool membership rows:

- `RoutePoolRepository::member_accounts`: add `a.archived_at IS NULL` so route-once cannot select archived accounts.
- `RoutePoolRepository::stats`: count only active members and filter archived accounts out of summary, recent logs, and request pages.
- `route_proxy_service::select_pool_credentials`: add `c.archived_at IS NULL`.
- `route_model_test_service::load_account_credential`: require `archived_at IS NULL` for targeted tests.
- `RouteQuotaService::refresh_one`: reject an archived credential with the same validation style used for unsupported account actions; `list_by_platform` already keeps bulk refresh active-only.

Keep `list_member_ids` and `pool_membership_map` unchanged so the frontend and exports retain the original pool relationship for archived accounts.

- [ ] **Step 7: Run repository and runtime tests**

Run `cargo test --manifest-path src-tauri/Cargo.toml route_credential_repository --lib` and `cargo test --manifest-path src-tauri/Cargo.toml route_pool_repository --lib`.

Expected: archive filtering, restoration, pool preservation, and active runtime exclusion pass without changing existing pool behavior.

---

### Task 3: Expose Batch Archive And Restore Commands

**Files:**
- Modify: `src-tauri/src/services/route_credential_service.rs`
- Modify: `src-tauri/src/commands/route_credential_commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/web/handlers/mod.rs`
- Modify: `src/lib/api/client.ts`

**Interfaces:**
- Rust service methods:

```rust
pub async fn archive(pool: &SqlitePool, ids: Vec<String>) -> Result<(), AppError>;
pub async fn restore(pool: &SqlitePool, ids: Vec<String>) -> Result<(), AppError>;
```

- Tauri commands:

```rust
pub async fn archive_route_credentials(state: State<'_, AppState>, ids: Vec<String>) -> Result<(), ApiError>;
pub async fn restore_route_credentials(state: State<'_, AppState>, ids: Vec<String>) -> Result<(), ApiError>;
```

- TypeScript client functions:

```ts
export function archiveRouteCredentials(ids: string[]): Promise<void>;
export function restoreRouteCredentials(ids: string[]): Promise<void>;
```

- [ ] **Step 1: Add service and command tests or compile-time call sites**

Add service tests that call `archive` and `restore` with multiple IDs and assert the repository page scopes change while `route_pool_members` remains present. Add command registration call sites before implementation so missing imports/functions are compiler errors.

- [ ] **Step 2: Implement service methods as thin batch wrappers**

Validate that `ids` is passed unchanged to `RouteCredentialRepository::set_archived`; keep transaction and validation behavior in the repository so Tauri and web calls share one path.

- [ ] **Step 3: Register desktop and web commands**

Import both commands in `src-tauri/src/lib.rs`, add them to `tauri::generate_handler!`, and add `"archive_route_credentials"` and `"restore_route_credentials"` match arms in `web::handlers::dispatch_command` using `parse_arg(&args, "ids")?`.

- [ ] **Step 4: Add the TypeScript client wrappers**

Add `invoke("archive_route_credentials", { ids })` and `invoke("restore_route_credentials", { ids })` next to the existing credential mutations. No `desktopOnlyCommands` entry is needed because the web dispatcher supports both commands.

- [ ] **Step 5: Run backend and client checks**

Run `cargo test --manifest-path src-tauri/Cargo.toml route_credential_service --lib`, `cargo check --manifest-path src-tauri/Cargo.toml`, and `pnpm typecheck`.

---

### Task 4: Add The Archived Account Segment And Batch Controls

**Files:**
- Modify: `src/screens/AccountsScreen.tsx`
- Modify: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- `AccountView` becomes `"in_pool" | "out_of_pool" | "archived" | "stats"`.
- `accountViewOptions` contains `{ key: "archived", label: "已归档" }` between `未入池` and `统计`.
- `accountScope` maps `accountView === "archived"` to `"archived"`.
- The screen uses `archiveRouteCredentials` and `restoreRouteCredentials` mutations with `selectedAccountIds`.

- [ ] **Step 1: Add failing frontend tests**

Mock `archiveRouteCredentials` and `restoreRouteCredentials`, extend the view helper union to include `已归档`, and add tests that:

1. click `已归档` and assert `listRouteCredentialPage` receives `{ pool_scope: "archived" }`;
2. select an archived fixture and assert `批量恢复账号` is visible while pool add/remove actions are absent;
3. select an active fixture and assert `批量归档账号` is visible;
4. invoke each action and assert the selected IDs are passed, selection is cleared, and account queries are invalidated/refetched;
5. render an empty archived page and assert the existing centered `account-empty-state` is used.

- [ ] **Step 2: Run the focused tests and verify they fail**

Run `pnpm test:run -- tests/AccountsScreen.test.tsx`.

Expected: FAIL because the archived segment, scope mapping, client mocks, and mutations do not exist.

- [ ] **Step 3: Add client imports, view state, and query scope**

Import the two client functions and `Archive`/`ArchiveRestore` icons. Extend `AccountView`, `accountViewOptions`, `accountScope`, and the legacy fallback filter so archived credentials are separated by `credential.archived_at`. Keep `openExport` using the current `accountScope`, which now permits archived exports.

- [ ] **Step 4: Add archive and restore mutations**

Create two `useMutation` instances that call the corresponding client wrapper with `Array.from(selectedAccountIds)`. On success clear selection and call `invalidateAccountData`; on error leave selection intact and render a `role="alert"` message beside the existing batch feedback. Disable both controls while either mutation is pending.

- [ ] **Step 5: Render selection-dependent icon actions**

In the selected-account action group, render `批量归档账号` with `Archive` for `in_pool` and `out_of_pool`, and `批量恢复账号` with `ArchiveRestore` for `archived`. Keep pool membership actions out of the archived view. Use the existing icon-button sizing, `aria-label`, `title`, and screen-reader text conventions.

- [ ] **Step 6: Keep archived rows non-routable in the UI**

Do not render the per-row model-test action or pool add/remove actions for archived rows/view. Leave edit, copy, delete, and export available as existing workflows. Disable drag reorder in the archived view if the reorder handler cannot safely operate on the archived scope; otherwise pass the new scope through unchanged.

- [ ] **Step 7: Run the account tests**

Run `pnpm test:run -- tests/AccountsScreen.test.tsx`.

Expected: all existing account tests plus the archived-segment, batch archive, batch restore, and empty-state tests pass.

---

### Task 5: Full Verification And Diff Review

**Files:**
- Review: all files changed in Tasks 1-4
- Test: `tests/AppLayout.test.tsx`, `tests/AccountsScreen.test.tsx`, Rust library tests

- [ ] **Step 1: Run the focused frontend and backend suites**

Run `pnpm test:run -- tests/AppLayout.test.tsx tests/AccountsScreen.test.tsx` and `cargo test --manifest-path src-tauri/Cargo.toml route_credential --lib`.

- [ ] **Step 2: Run the complete type and build checks**

Run `pnpm typecheck`, `pnpm build`, and `cargo check --manifest-path src-tauri/Cargo.toml`.

Expected: the existing OCRAD `fs/path/eval` build warnings may remain, but no new errors appear.

- [ ] **Step 3: Check formatting and inspect the diff**

Run `git diff --check` and `git diff --stat`, then inspect all archive-related hunks for accidental changes to existing pool, export, or delete behavior. Leave the working tree uncommitted for the user.
