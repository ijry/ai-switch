# Account Pagination, Sorting, and Recovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan task-by-task with review checkpoints.

**Goal:** Add opt-in cc-switch deep-link compatibility, paginated drag-sortable account management, cooldown-bypassing manual tests, and semantic `response.failed` account recovery behavior.

**Architecture:** Keep `AppSettings` as the persisted user payload and return a flattened settings view with runtime deep-link capability. Use an injectable protocol runtime so Tauri can register/unregister `ccswitch` safely while the standalone Web server reports the capability as unavailable. Add paginated credential/reorder contracts backed by transactional SQL, keep route-pool ordering independent, and share one bounded JSON/SSE semantic-failure parser between model tests and the route proxy.

**Tech Stack:** Rust, Tauri 2, Axum Web command dispatcher, SQLite/SQLx, React 18, TanStack Query, Vitest, Testing Library, native HTML drag events plus keyboard handlers.

## Global Constraints

- Work directly on `main`; do not create or switch branches or worktrees.
- Preserve the existing uncommitted workspace changes and touch only files required by this feature.
- Do not run `git commit` unless the user explicitly requests it.
- `ccswitch://` is disabled by default, removed from static bundle registration, and dynamically registered only on Windows/Linux when enabled.
- Before unregistering `ccswitch`, verify that AI Switch is the current handler; never remove another application's association.
- `aiswitch://` behavior remains unchanged.
- Account page sizes are exactly `20`, `50`, and `100`; filtering precedes counting and pagination.
- Account order is a dense platform-wide `sort_order`; route-pool membership order is not changed by account-list reorder.
- The account list is flat with batch labels; selection is ID-based and survives page changes.
- Explicit account tests ignore cooldown and status gates; successful explicit tests restore `ok` and clear all failure fields.
- `response.failed` semantic failures set `error`, clear cooldown, never set `revoked`, and cause proxy retry.
- Existing transient transport/status cooldown and permanent authentication classification remain intact.

---

### Task 1: Add Settings and Deep-Link Runtime Control

**Files:**
- Create: `src-tauri/src/services/deeplink_protocol_service.rs`
- Modify: `src-tauri/src/models/settings.rs`
- Modify: `src-tauri/src/services/settings_service.rs`
- Modify: `src-tauri/src/core/settings.rs`
- Modify: `src-tauri/src/app_state.rs`
- Modify: `src-tauri/src/commands/settings_commands.rs`
- Modify: `src-tauri/src/web/handlers/mod.rs`
- Modify: `src-tauri/src/server.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/tauri.conf.json`

**Interfaces:**
- `AppSettings` gains `ccswitch_deeplink_compat_enabled: bool` with a serde default of `false`.
- Add a flattened API-only `AppSettingsView` containing all `AppSettings` fields plus `ccswitch_deeplink_compat_supported: bool`.
- Add `DeepLinkProtocolStatus { supported: bool, ccswitch_registered: bool, reason: Option<String> }` internally and expose `ccswitch_deeplink_compat_supported` through the settings view.
- Add `DeepLinkProtocolRuntime` to `AppState`; the desktop runtime is attached to the Tauri `AppHandle` during setup, while the standalone server uses an unavailable runtime.
- Change `get_settings_core` and `save_settings_core` to accept the runtime and return `AppSettingsView`; `save_settings_core` applies protocol changes before writing and compensates with the inverse operation if file persistence fails.

- [ ] **Step 1: Write failing settings and ownership tests**

Add tests in `src-tauri/src/services/deeplink_protocol_service.rs` and `src-tauri/src/services/settings_service.rs`. Define the fake registrar in the test module with an `AtomicBool` ownership flag and a `Mutex<Vec<String>>` call log:

```rust
struct FakeRegistrar {
    owns_scheme: AtomicBool,
    calls: Mutex<Vec<String>>,
}

impl FakeRegistrar {
    fn registered(owns_scheme: bool) -> Self {
        Self { owns_scheme: AtomicBool::new(owns_scheme), calls: Mutex::new(Vec::new()) }
    }
}

#[tokio::test]
async fn missing_compatibility_field_defaults_to_off() {
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(temp.path().to_path_buf());
    tokio::fs::write(
        &paths.settings_file,
        r#"{"language":"zh-CN","theme":"system","copy_import_sources":false,"logging_enabled":true,"secret_storage":"keyring","data_dir":"x"}"#,
    ).await.unwrap();

    let settings = SettingsService::load(&paths).await.unwrap();
    assert!(!settings.ccswitch_deeplink_compat_enabled);
}

#[tokio::test]
async fn disabling_only_unregisters_when_ai_switch_owns_the_scheme() {
    let registrar = FakeRegistrar::registered(false);
    let runtime = DeepLinkProtocolRuntime::with_registrar(Arc::new(registrar));
    let result = runtime.set_ccswitch_enabled(false);
    assert!(result.is_ok());
    assert!(runtime.status().ccswitch_registered == false);
}

#[tokio::test]
async fn settings_write_failure_compensates_protocol_change() {
    let registrar = Arc::new(FakeRegistrar::registered(false));
    let runtime = DeepLinkProtocolRuntime::with_registrar(registrar.clone());
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(temp.path().join("settings"));
    let initial = AppSettings::defaults_for_data_dir(paths.data_dir.display().to_string());
    SettingsService::save(&paths, &initial).await.unwrap();
    let mut permissions = std::fs::metadata(&paths.settings_file).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&paths.settings_file, permissions).unwrap();
    let mut next = AppSettings::defaults_for_data_dir(paths.data_dir.display().to_string());
    next.ccswitch_deeplink_compat_enabled = true;
    let result = save_settings_core(&paths, &runtime, next).await;
    assert!(result.is_err());
    assert_eq!(registrar.calls(), vec!["register:ccswitch", "unregister:ccswitch"]);
}
```

Implement the fake registrar's trait methods by recording `register:ccswitch` and `unregister:ccswitch`, returning `owns_scheme` from `is_registered`, and exposing a cloned call vector through `calls()`. The read-only settings file in the compensation test makes the production save call fail on both Windows and Unix.

- [ ] **Step 2: Run the focused Rust tests and verify failure**

Run: `cd src-tauri; cargo test deeplink_protocol_service settings_service`

Expected: compile failures for the new field, runtime, fake registrar helpers, and settings view.

- [ ] **Step 3: Implement the injectable protocol runtime**

Implement `DeepLinkProtocolRegistrar` with `is_registered`, `register`, and `unregister` operations. The Tauri adapter calls `app.deep_link().is_registered("ccswitch")` before `unregister`; the unavailable adapter returns `supported = false` and a stable reason code. Store the enabled flag in an `AtomicBool` so `handle_deeplink_url` can synchronously reject disabled `ccswitch://` command-line arguments.

Use the following transition contract:

```rust
pub trait DeepLinkProtocolRegistrar: Send + Sync {
    fn status(&self) -> DeepLinkProtocolStatus;
    fn set_ccswitch_enabled(&self, enabled: bool) -> Result<(), AppError>;
}

pub async fn save_settings_core(
    paths: &AppPaths,
    runtime: &DeepLinkProtocolRuntime,
    next: AppSettings,
) -> Result<AppSettingsView, AppError> {
    let previous = SettingsService::load(paths).await?;
    let changed = previous.ccswitch_deeplink_compat_enabled
        != next.ccswitch_deeplink_compat_enabled;
    if changed {
        runtime.set_ccswitch_enabled(next.ccswitch_deeplink_compat_enabled)?;
    }
    if let Err(error) = SettingsService::save(paths, &next).await {
        if changed {
            let _ = runtime.set_ccswitch_enabled(previous.ccswitch_deeplink_compat_enabled);
        }
        return Err(error);
    }
    Ok(runtime.view(next))
}
```

`SettingsService` serializes a private persisted struct so the runtime support flag never enters `settings.json`. `AppSettings::defaults_for_data_dir` sets compatibility to `false`. `get_settings_core` loads the persisted payload and combines it with runtime capability.

- [ ] **Step 4: Wire Tauri, Web, and static scheme registration**

Add `deeplink_protocols: DeepLinkProtocolRuntime` to every `AppState` constructor. Attach the Tauri registrar in `setup`, synchronously load settings before processing initial argv, reconcile the saved `ccswitch` value, and keep the existing `aiswitch` registration path. Update `handle_deeplink_url` to accept `aiswitch://` always and `ccswitch://` only when `runtime.ccswitch_enabled()` is true. The standalone server keeps the unavailable runtime, so attempting to enable compatibility returns `capability.deeplink_compat_unavailable` without changing the file.

Remove `ccswitch` from `src-tauri/tauri.conf.json`; leave only `aiswitch` under the desktop scheme list. Update the Web dispatcher and Tauri settings command to return the flattened settings view and to pass the same runtime into the core functions.

- [ ] **Step 5: Run focused tests and check the static bundle config**

Run: `cd src-tauri; cargo test deeplink_protocol_service settings_service web::handlers::tests; cargo check`

Expected: PASS, and `Select-String 'ccswitch' src-tauri/tauri.conf.json` returns no static scheme entry.

- [ ] **Step 6: Review the diff without committing**

Run: `git diff --check -- src-tauri; git status --short`

Confirm the runtime never calls `unregister` when `is_registered` is false and the settings file is unchanged after protocol/file failure.

---

### Task 2: Expose and Test the Compatibility Setting

**Files:**
- Modify: `src/lib/api/types.ts`
- Modify: `src/lib/api/client.ts`
- Modify: `src/screens/SettingsScreen.tsx`
- Modify: `src/lib/i18n.tsx`
- Modify: `src/test/fixtures.ts`
- Modify: `tests/SettingsScreen.test.tsx`

**Interfaces:**
- `AppSettings` gains `ccswitch_deeplink_compat_enabled: boolean`.
- Add `AppSettingsView = AppSettings & { ccswitch_deeplink_compat_supported: boolean }`.
- `getSettings(): Promise<AppSettingsView>` and `saveSettings(settings: AppSettings): Promise<AppSettingsView>` continue using `get_settings` and `save_settings` commands.

- [ ] **Step 1: Add failing screen tests**

Extend the fixture and add tests with the existing QueryClient/I18n harness:

```tsx
it("shows cc-switch compatibility off by default and saves it when enabled", async () => {
  vi.mocked(getSettings).mockResolvedValue({
    ...settingsFixture,
    ccswitch_deeplink_compat_enabled: false,
    ccswitch_deeplink_compat_supported: true,
  });
  vi.mocked(saveSettings).mockImplementation(async (settings) => ({
    ...settings,
    ccswitch_deeplink_compat_supported: true,
  }));
  render(
    <QueryClientProvider client={createQueryClient()}>
      <I18nProvider initialLanguage="zh-CN"><SettingsScreen /></I18nProvider>
    </QueryClientProvider>,
  );

  const toggle = await screen.findByRole("checkbox", { name: /cc-switch/i });
  expect(toggle).not.toBeChecked();
  await userEvent.click(toggle);
  await waitFor(() => expect(saveSettings).toHaveBeenCalledWith(expect.objectContaining({
    ccswitch_deeplink_compat_enabled: true,
  })));
});

it("disables the compatibility toggle when the runtime cannot register protocols", async () => {
  vi.mocked(getSettings).mockResolvedValue({
    ...settingsFixture,
    ccswitch_deeplink_compat_enabled: false,
    ccswitch_deeplink_compat_supported: false,
  });
  render(
    <QueryClientProvider client={createQueryClient()}>
      <I18nProvider initialLanguage="zh-CN"><SettingsScreen /></I18nProvider>
    </QueryClientProvider>,
  );
  expect(await screen.findByRole("checkbox", { name: /cc-switch/i })).toBeDisabled();
});
```

- [ ] **Step 2: Run the screen test to verify failure**

Run: `pnpm vitest run tests/SettingsScreen.test.tsx`

Expected: FAIL because the type, query response, and toggle do not exist yet.

- [ ] **Step 3: Implement API types and the Settings UI**

Add English and Simplified Chinese keys for the toggle label, conflict warning, unsupported-runtime message, and save error. Render the toggle in the existing app preferences section as a labeled checkbox. Submit only persisted settings fields, keep the checkbox controlled by the query result, disable it when unsupported or saving, and leave the previous value visible when the mutation fails.

- [ ] **Step 4: Run the screen and type checks**

Run: `pnpm vitest run tests/SettingsScreen.test.tsx; pnpm typecheck`

Expected: PASS with no TypeScript errors.

- [ ] **Step 5: Review the diff without committing**

Verify that a Web/macOS settings view can display the saved value but cannot issue an enable request, while desktop Windows/Linux can toggle it.

---

### Task 3: Add Paginated Credential Contracts and Queries

**Files:**
- Modify: `src-tauri/src/models/route_credential.rs`
- Modify: `src-tauri/src/database/repositories/route_credential_repository.rs`
- Modify: `src-tauri/src/services/route_credential_service.rs`
- Modify: `src-tauri/src/commands/route_credential_commands.rs`
- Modify: `src-tauri/src/web/handlers/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/api/types.ts`
- Modify: `src/lib/api/client.ts`
- Modify: `tests/transport/transport.test.ts`

**Interfaces:**
- Add `RouteCredentialPageRequest { platform, page, page_size, filters }` where `filters` contains batch IDs and `"__single__"`.
- Add `RouteCredentialPage { items, total, page, page_count, page_size, previous_page_account_id, next_page_account_id, filter_options, official_account_count }`.
- Add `RouteCredentialFilterOption { key, label }`.
- Add command `list_route_credentials_page(input)` and client function `listRouteCredentialPage(input)`; keep the legacy unbounded command for compatibility with existing callers.

- [ ] **Step 1: Write failing repository/API tests**

Add repository tests that create at least 25 credentials, assign two batch IDs, and set deterministic sort orders:

Define one local `insert_test_credential(pool, platform, batch_id, name) -> String` helper in the repository test module. It calls `RouteCredentialRepository::create` with `kind = "api"`, `status = "ok"`, `secret_payload_json = {"api_key":"test"}`, `config_json = {"base_url":"https://example.com","interface_format":"openai","model_mappings":[]}`, and `preview_json = {}`; the helper returns the created row ID. Insert the required batch rows with `INSERT INTO batches (id, name, source, sort_order, created_at, updated_at) VALUES (?, ?, 'test', 0, ?, ?)` before creating credentials.

Define `insert_test_batch(pool, id, name)` beside it; bind the two IDs plus one RFC3339 `now` value for both timestamps. These two helpers are test-only and are reused by the reorder tests in Task 4.

```rust
#[tokio::test]
async fn page_query_filters_counts_and_returns_boundary_ids() {
    let pool = crate::database::create_memory_pool().await.unwrap();
    crate::database::run_migrations(&pool).await.unwrap();
    let ids = (0..25)
        .map(|index| format!("account-{index}"))
        .collect::<Vec<_>>();
    for (index, name) in ids.iter().enumerate() {
        insert_test_credential(&pool, "codex", None, name).await;
        sqlx::query("UPDATE route_credentials SET sort_order = ? WHERE display_name = ?")
            .bind(index as i64).bind(name).execute(&pool).await.unwrap();
    }
    let page = RouteCredentialRepository::page(
        &pool,
        RouteCredentialPageRequest {
            platform: "codex".into(),
            page: 2,
            page_size: 20,
            filters: vec![],
        },
    ).await.unwrap();

    assert_eq!(page.total, 25);
    assert_eq!(page.page, 2);
    assert_eq!(page.page_count, 2);
    assert_eq!(page.items.len(), 5);
    assert_eq!(page.previous_page_account_id.as_deref(), Some(ids[19].as_str()));
    assert!(page.next_page_account_id.is_none());
}

#[tokio::test]
async fn page_query_uses_batch_ids_not_batch_names() {
    let pool = crate::database::create_memory_pool().await.unwrap();
    crate::database::run_migrations(&pool).await.unwrap();
    insert_test_batch(&pool, "batch-a", "Same name").await;
    insert_test_batch(&pool, "batch-b", "Same name").await;
    insert_test_credential(&pool, "codex", Some("batch-a"), "a").await;
    insert_test_credential(&pool, "codex", Some("batch-b"), "b").await;
    let page = RouteCredentialRepository::page(&pool, RouteCredentialPageRequest {
        platform: "codex".into(), page: 1, page_size: 20, filters: vec!["batch-a".into()],
    }).await.unwrap();
    assert!(page.items.iter().all(|item| item.batch_id.as_deref() == Some("batch-a")));
}
```

Add a Web dispatcher test asserting `dispatch_command("list_route_credentials_page", {"input": ...})` returns `items`, `total`, and `filter_options`. Update the transport test to assert the new command arguments.

- [ ] **Step 2: Run focused Rust and transport tests to verify failure**

Run: `cd src-tauri; cargo test route_credential_repository web::handlers::tests; cd ..; pnpm vitest run tests/transport/transport.test.ts`

Expected: FAIL because the page types, repository method, command, and client function are absent.

- [ ] **Step 3: Implement validated page types and SQL queries**

Validate page size with `20`, `50`, and `100`, clamp page to at least `1`, and return page `1` for an empty result. Use `sqlx::QueryBuilder<Sqlite>` for the filter predicate so selected batch IDs are bound parameters. Apply the same predicate to the count query, item query, and boundary lookup. Keep the existing aggregate request statistics projection and `ORDER BY rc.sort_order ASC, rc.created_at DESC`.

Return all filter options from platform-wide metadata queries, not from the page rows. Include `official_account_count` from a separate platform-wide count so automatic quota refresh does not depend on the visible page.

- [ ] **Step 4: Wire Tauri/Web commands and TypeScript client types**

Add the command to `route_credential_commands.rs`, register it in `lib.rs`, dispatch it in `src-tauri/src/web/handlers/mod.rs`, and implement `listRouteCredentialPage` in `src/lib/api/client.ts`. Keep the legacy `listRouteCredentials(platform)` untouched until the Accounts screen migration is complete.

- [ ] **Step 5: Fix new-account ordering and run tests**

Change `RouteCredentialRepository::create` to allocate `MAX(sort_order) + 1` inside the same SQLite write transaction instead of always inserting `0`. Add a regression test asserting two newly created accounts have distinct increasing order values.

Run: `cd src-tauri; cargo test route_credential_repository web::handlers::tests; cd ..; pnpm vitest run tests/transport/transport.test.ts`

Expected: PASS.

- [ ] **Step 6: Review the diff without committing**

Check that no route-pool SQL or `route_pool_members.sort_order` query changed in this task.

---

### Task 4: Add Transactional Global Reordering

**Files:**
- Modify: `src-tauri/src/models/route_credential.rs`
- Modify: `src-tauri/src/database/repositories/route_credential_repository.rs`
- Modify: `src-tauri/src/services/route_credential_service.rs`
- Modify: `src-tauri/src/commands/route_credential_commands.rs`
- Modify: `src-tauri/src/web/handlers/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/api/types.ts`
- Modify: `src/lib/api/client.ts`

**Interfaces:**
- Add `ReorderRouteCredentialInput { platform, moved_account_id, previous_account_id, next_account_id, filters, page_size }`.
- Add command `reorder_route_credentials(input)` returning the normalized `RouteCredentialPage` containing the moved account.

- [ ] **Step 1: Write failing reorder tests**

Add repository tests for unfiltered, filtered, page-boundary, and rollback behavior:

Define `type TestFixture = (SqlitePool, Vec<String>)`, `async fn interleaved_fixture() -> TestFixture`, and `async fn four_account_fixture() -> TestFixture` in the repository test module. `interleaved_fixture` inserts `a-1` and `a-3` with `batch-a`, `b-1` and `b-2` with `batch-b`, then updates sort orders to `0..3`; `four_account_fixture` inserts four unbatched accounts and assigns `0..3`. Both use the `insert_test_batch` and `insert_test_credential` helpers defined in Task 3.

Also define `async fn all_sort_orders(pool) -> Vec<(String, i64)>` with `SELECT id, sort_order FROM route_credentials ORDER BY sort_order, id`, and construct `invalid_reorder()` inline with a `next_account_id` that is not part of the active filter.

```rust
#[tokio::test]
async fn reorder_replaces_only_filtered_slots_and_rewrites_dense_order() {
    let (pool, _ids) = interleaved_fixture().await;
    let result = RouteCredentialRepository::reorder(
        &pool,
        ReorderRouteCredentialInput {
            platform: "codex".into(),
            moved_account_id: "a-3".into(),
            previous_account_id: None,
            next_account_id: Some("a-1".into()),
            filters: vec!["batch-a".into()],
            page_size: 20,
        },
    ).await.unwrap();

    assert_eq!(result.items.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
        vec!["a-3", "b-1", "a-1", "b-2"]);
    assert_eq!(result.items.iter().map(|item| item.sort_order).collect::<Vec<_>>(),
        vec![0, 1, 2, 3]);
}

#[tokio::test]
async fn invalid_neighbors_leave_every_sort_order_unchanged() {
    let (pool, _ids) = four_account_fixture().await;
    let before = all_sort_orders(&pool).await;
    let error = RouteCredentialRepository::reorder(&pool, invalid_reorder()).await.unwrap_err();
    assert!(error.to_string().contains("route credential"));
    assert_eq!(before, all_sort_orders(&pool).await);
}
```

Use a SQLite trigger that raises `ABORT` for one account in a separate test to prove a mid-transaction update rolls back every sort order.

- [ ] **Step 2: Run reorder tests to verify failure**

Run: `cd src-tauri; cargo test reorder`

Expected: FAIL because the input type, transaction, and command do not exist.

- [ ] **Step 3: Implement the reorder transaction**

Inside one transaction:

1. Load all platform IDs ordered by `sort_order, created_at, id`.
2. Derive the filtered IDs using the same batch/single predicate as pagination.
3. Remove `moved_account_id`; validate it and both neighbor IDs are in the filtered set.
4. Require both supplied neighbors to be adjacent after removal; insert the moved ID before `next_account_id` or after `previous_account_id`.
5. Replace only filtered slots in the full order with the reordered filtered IDs, preserving excluded accounts' relative order.
6. Update every platform credential with dense zero-based `sort_order` values, then commit.
7. Compute the moved ID's filtered page and return `RouteCredentialPage` with fresh boundary IDs.

Reject a cross-platform ID, a filtered-out ID, non-adjacent neighbors, or an invalid page size before any update.

- [ ] **Step 4: Wire command, Web dispatcher, and client**

Register `reorder_route_credentials` in both transports and invalidate page queries from the frontend caller after the returned normalized page is received.

- [ ] **Step 5: Run Rust tests and review transaction boundaries**

Run: `cd src-tauri; cargo test route_credential_repository route_credential_service web::handlers::tests`

Expected: PASS; no update occurs before all validation completes, and all sort updates share one transaction.

- [ ] **Step 6: Review the diff without committing**

Verify no route-pool membership order is rewritten and no secret/config columns are selected solely for reorder.

---

### Task 5: Migrate Accounts Screen to Flat Pagination

**Files:**
- Modify: `src/screens/AccountsScreen.tsx`
- Modify: `src/lib/api/types.ts`
- Modify: `src/lib/api/client.ts`
- Modify: `src/test/fixtures.ts`
- Modify: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Query key: `["route-credential-page", activePlatform, accountPage, accountPageSize, accountFilters]`.
- Filter metadata comes from the page response and uses batch IDs as keys; `"__single__"` remains the unbatched sentinel.
- Existing account actions receive a `RouteCredential` row and continue using their current mutations.

- [ ] **Step 1: Add failing pagination and selection tests**

Replace the unbounded `credentialsFixture` mock with page responses. Add a `routeCredentialPageFixture(overrides)` helper beside the current fixtures that returns every `RouteCredentialPage` field (`items`, `total`, `page`, `page_count`, `page_size`, both boundary IDs, `filter_options`, and `official_account_count`) with deterministic defaults. Export a `const accountPageFixtures = Array.from({ length: 21 }, (_, index) => ({ ...credentialsFixture[index % credentialsFixture.length], id: `page-${index}`, display_name: `Page Account ${index}` }))` in the test file. Add tests:

```tsx
it("renders a flat paginated account list and changes page", async () => {
  const firstTwenty = accountPageFixtures.slice(0, 20);
  const lastAccount = accountPageFixtures[20];
  vi.mocked(listRouteCredentialPage)
    .mockResolvedValueOnce(routeCredentialPageFixture({ page: 1, total: 21, items: firstTwenty }))
    .mockResolvedValueOnce(routeCredentialPageFixture({ page: 2, total: 21, items: [lastAccount] }));
  renderScreen("codex");

  expect(await screen.findByText("第 1 / 2 页")).toBeInTheDocument();
  expect(screen.getByText("Team Account")).toBeInTheDocument();
  expect(screen.getByText("批量 July imports")).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "下一页账号" }));
  expect(await screen.findByText("第 2 / 2 页")).toBeInTheDocument();
  expect(screen.getByText("Last Account")).toBeInTheDocument();
});

it("keeps selected IDs when moving between pages", async () => {
  // Select one account on page one, move to page two, then submit pool membership.
  expect(setRoutePoolMembers).toHaveBeenCalledWith(expect.objectContaining({
    account_ids: expect.arrayContaining(["cred-page-one"]),
  }));
});
```

Add tests that changing platform, filter, or page size resets to page `1`, and that a delete response refetches a backend-clamped page.

- [ ] **Step 2: Run the Accounts tests to verify failure**

Run: `pnpm vitest run tests/AccountsScreen.test.tsx`

Expected: FAIL because the screen still expects an array and renders grouped cards.

- [ ] **Step 3: Replace the credential query and filter derivation**

Add `accountPage`, `accountPageSize` state, normalize page size to `20/50/100`, call `listRouteCredentialPage`, and use `placeholderData: keepPreviousData` while a new page loads. Remove `filteredCredentials` and `groupedCredentials`. Render one flat list from `page.items`, showing the returned batch label as a row badge.

Use page metadata for filter options so duplicate batch names remain distinct:

```tsx
const accountFilterOptions = page?.filter_options ?? [];
const toggleAccountFilter = (key: string) => {
  setAccountPage(1);
  setAccountFilters((current) =>
    current.includes(key) ? current.filter((value) => value !== key) : [...current, key],
  );
};
```

Add page-size and page controls with accessible labels `上一页账号`, `下一页账号`, and `账号每页数量`. Disable controls at page bounds.

- [ ] **Step 4: Update mutations and derived state for paginated data**

Invalidate the page query prefix after create/import/copy/edit/delete/test/quota operations. Remove cache merging that assumes an array. Trigger platform quota refresh from the page's `official_account_count`, not the visible rows. Keep `selectedAccountIds` as a `Set<string>` across page changes; clear it only on platform change or after a completed batch action. Enable pool-wide model testing when the pool has members and the backend model-test capability allows the platform, rather than checking only the visible page.

- [ ] **Step 5: Run screen tests and type checks**

Run: `pnpm vitest run tests/AccountsScreen.test.tsx; pnpm typecheck`

Expected: PASS with existing account actions still available on every row.

- [ ] **Step 6: Review the diff without committing**

Check that request-statistics pagination remains independent from account pagination and that no account action accidentally uses only the current page to identify an off-page selected ID.

---

### Task 6: Add Cross-Page Drag and Keyboard Reordering

**Files:**
- Create: `src/lib/accountReorder.ts`
- Create: `src/components/accounts/AccountSortableList.tsx`
- Modify: `src/screens/AccountsScreen.tsx`
- Modify: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- `AccountSortNeighbors = { previousAccountId: string | null; nextAccountId: string | null }`.
- `AccountSortableList` accepts current page rows, returned boundary IDs, current page/page count, page-change callback, and `onCommit(movedId, neighbors)`.
- The list uses native drag events, so no new package dependency is required.

- [ ] **Step 1: Write pure reorder-helper and interaction tests**

Add `src/lib/accountReorder.test.ts` with deterministic neighbor calculations:

```ts
it("uses page boundary IDs when dropping at the first row", () => {
  expect(neighborsForDrop({
    items: [{ id: "b" }, { id: "c" }],
    movedId: "x",
    targetIndex: 0,
    previousPageAccountId: "a",
    nextPageAccountId: "d",
  })).toEqual({ previousAccountId: "a", nextAccountId: "b" });
});
```

Extend `AccountsScreen.test.tsx` with tests for page-local drop, edge hover for `600ms`, continued dragging after page change, keyboard Space/Arrow movement, and reorder-error rollback/refetch:

```tsx
vi.useFakeTimers();
fireEvent.dragStart(screen.getByLabelText("拖动 Team Account"));
fireEvent.dragOver(screen.getByTestId("account-list-edge-bottom"), { clientY: 999 });
vi.advanceTimersByTime(600);
expect(await screen.findByText("第 2 / 2 页")).toBeInTheDocument();
fireEvent.drop(screen.getByLabelText("放置在 Last Account 前"));
expect(reorderRouteCredentials).toHaveBeenCalledWith(expect.objectContaining({
  previous_account_id: expect.any(String),
  next_account_id: "last-account",
}));
```

- [ ] **Step 2: Run the focused tests to verify failure**

Run: `pnpm vitest run src/lib/accountReorder.test.ts tests/AccountsScreen.test.tsx`

Expected: FAIL because no sortable list, drag handle, keyboard mode, or reorder mutation exists.

- [ ] **Step 3: Implement the sortable list without adding a dependency**

Render a dedicated drag handle per row with `draggable`, `aria-label`, and `aria-grabbed`. Keep `draggedAccountId` in the parent list so changing pages does not discard the active drag. On edge `dragover`, start one `window.setTimeout` for `600ms`; advance exactly one page, clear the timer, and keep the dragged ID. Use the page response's `previous_page_account_id` and `next_page_account_id` when calculating drop neighbors. Cancel drag state on platform/filter/page-size changes and after mutation settlement.

Implement keyboard move mode on the same handle: Space/Enter toggles grabbed state, ArrowUp/ArrowDown moves one filtered neighbor, and a move at the first/last visible row uses the page boundary ID and changes page before committing. Escape cancels. The list must expose visible insertion markers and stable row dimensions while a drag is active.

- [ ] **Step 4: Wire the reorder mutation and rollback**

Call `reorderRouteCredentials` with the exact platform, moved ID, previous/next IDs, active filter keys, and page size. On success, set the returned page in cache and invalidate the page query. On error, clear the optimistic marker, refetch the current query, preserve `selectedAccountIds`, and display the existing account feedback banner.

- [ ] **Step 5: Run UI tests and type checks**

Run: `pnpm vitest run src/lib/accountReorder.test.ts tests/AccountsScreen.test.tsx; pnpm typecheck`

Expected: PASS, including page-edge auto-advance and keyboard movement.

- [ ] **Step 6: Review the diff without committing**

Confirm action buttons do not start a drag, the drag timer is cleaned up on unmount, and a dropped account never sends display names where the backend expects IDs.

---

### Task 7: Add Shared Semantic Failure Detection and Recovery State

**Files:**
- Create: `src-tauri/src/services/response_failure_service.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Modify: `src-tauri/src/database/repositories/route_credential_repository.rs`
- Modify: `src-tauri/src/services/route_model_test_service.rs`
- Modify: `src-tauri/src/models/route_credential.rs`

**Interfaces:**
- `SemanticResponseFailure { message: String }`.
- `detect_response_failed(body: &[u8]) -> Option<SemanticResponseFailure>` parses a complete JSON object first, then each SSE `data:` JSON payload, ignoring comments, blank data, and `[DONE]`.
- `RouteCredentialRepository::record_semantic_failure(pool, id, message)` sets `status = 'error'`, `last_failure_kind = 'semantic_response_failed'`, stores the bounded message, and clears transient count/retry/cooldown fields.
- `RouteCredentialRepository::recover_after_explicit_test(pool, id)` sets `status = 'ok'` and clears all failure fields atomically.

- [ ] **Step 1: Write parser and repository tests**

Add parser tests for the sample JSON, nested `response.status`, SSE data, `[DONE]`, and normal success. Add repository tests:

In the repository test module, define `seeded_credential()` by calling `create_memory_pool`, `run_migrations`, and `RouteCredentialRepository::create` with the same API payload used in Task 3; return `(pool, id)`.

```rust
#[tokio::test]
async fn semantic_failure_marks_error_without_cooldown() {
    let (pool, id) = seeded_credential().await;
    RouteCredentialRepository::record_transient_failure(
        &pool, &id, "transport", "old failure",
    ).await.unwrap();
    RouteCredentialRepository::record_semantic_failure(
        &pool, &id, "model is under maintenance",
    ).await.unwrap();

    let row = RouteCredentialRepository::get(&pool, &id).await.unwrap();
    assert_eq!(row.status, "error");
    assert_eq!(row.transient_failure_count, 0);
    assert!(row.cooldown_until.is_none());
    assert_eq!(row.last_failure_kind.as_deref(), Some("semantic_response_failed"));
}
```

- [ ] **Step 2: Run focused tests to verify failure**

Run: `cd src-tauri; cargo test response_failure_service route_credential_repository`

Expected: FAIL because the parser and repository recovery methods do not exist.

- [ ] **Step 3: Implement bounded JSON/SSE parsing**

Parse a body as `serde_json::Value` and recognize `value["type"] == "response.failed"` or `value["response"]["status"] == "failed"`. Prefer `response.error.message`, then `error.message`, then `Upstream response reported failure`. For SSE, trim `data:`, skip `[DONE]`, and pass each JSON payload through the same value recognizer. Truncate only through the existing credential-safe failure-message helper before persistence.

- [ ] **Step 4: Implement atomic semantic failure and explicit recovery updates**

Add repository SQL updates that set all related columns in one statement. Reuse the existing timestamp/error mapping conventions and return a validation error when the credential ID is missing.

- [ ] **Step 5: Run parser/repository tests**

Run: `cd src-tauri; cargo test response_failure_service route_credential_repository`

Expected: PASS.

- [ ] **Step 6: Review the diff without committing**

Ensure the parser never stores API keys or full authorization headers and that normal HTTP error classification still uses the existing `classify_proxy_failure` path.

---

### Task 8: Integrate Manual-Test Recovery and Proxy Retry

**Files:**
- Modify: `src-tauri/src/services/route_model_test_service.rs`
- Modify: `src-tauri/src/services/route_proxy_service.rs`
- Modify: `src-tauri/src/services/route_pool_service.rs` (only if shared outcome metadata types require it)
- Modify: `src-tauri/src/services/route_credential_repository.rs`
- Modify: `src-tauri/src/services/route_model_test_service.rs` tests
- Modify: `src-tauri/src/services/route_proxy_service.rs` tests
- Modify: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Explicit `RoutePoolModelTestRequest.account_id` bypasses `next_retry_at`, `cooldown_until`, and account status checks; platform/capability/kind validation remains.
- Pool-wide tests continue using `select_pool_credentials` and therefore keep cooldown/status filtering.
- `finish_outcome` receives `explicit_account_test: bool` and `semantic_failure_message: Option<String>` so semantic failures never enter transient backoff classification.

- [ ] **Step 1: Add failing service tests**

Add tests in `route_model_test_service.rs`:

Reuse the existing `create_api_credential(&pool, &base_url)` helper in that module and add a local `request_for(account_id)` constructor returning `RoutePoolModelTestRequest { platform: "codex".into(), account_id: Some(account_id.into()), model: None, interface_format: None }` with the remaining fields matching the existing explicit-account tests.

```rust
#[tokio::test]
async fn explicit_test_can_use_cooling_error_or_revoked_account() {
    let pool = crate::database::create_memory_pool().await.unwrap();
    crate::database::run_migrations(&pool).await.unwrap();
    let id = create_api_credential(&pool, "http://127.0.0.1:1/v1").await;
    RouteCredentialRepository::record_transient_failure(
        &pool, &id, "transport", "cooling",
    ).await.unwrap();
    RouteCredentialRepository::update_status(&pool, &id, "error")
        .await.unwrap();

    let outcome = RouteModelTestService::test_model(&pool, request_for(&id))
        .await.unwrap();
    assert!(outcome.success);
    let recovered = RouteCredentialRepository::get(&pool, &id).await.unwrap();
    assert_eq!(recovered.status, "ok");
    assert!(recovered.cooldown_until.is_none());
}
```

Add proxy tests with two pool members: the first returns HTTP `200` plus the sample `response.failed` JSON, and the second returns a normal success. Assert the first account becomes `error`, the second is returned, and no cooldown is recorded for the first.

- [ ] **Step 2: Run the service tests to verify failure**

Run: `cd src-tauri; cargo test route_model_test_service route_proxy_service`

Expected: FAIL because explicit loading rejects cooldown/status and proxy treats HTTP `200` as success.

- [ ] **Step 3: Remove cooldown/status gating only for explicit account loading**

Change `load_account_credential` to select by `id` and `platform` without querying retry timestamps for rejection. Keep `select_pool_credentials` unchanged. Track whether selection was explicit and pass that flag through every `finish_outcome` path.

On successful explicit tests call `recover_after_explicit_test`; on non-explicit success keep the existing transient-clear behavior and do not restore `revoked`.

- [ ] **Step 4: Integrate semantic failure into model tests**

After `send_model_test_request` reads the bounded body, call `detect_response_failed`. Set `success = status.is_success() && semantic_failure.is_none()`, expose the semantic message as `error_message`, and pass it to `finish_outcome`. In `finish_outcome`, call `record_semantic_failure` before normal `classify_proxy_failure` whenever the override is present.

- [ ] **Step 5: Integrate semantic failure into route proxy retry**

After the proxy reads each upstream body, detect semantic failure before deciding `proxy_success` or clearing transient state. Record the account error, add a sanitized message to `retry_errors`, mark usage metadata `success = false`, and continue to the next eligible account. Only clear transient state and return the body when no quota exhaustion, semantic failure, or retryable HTTP failure exists.

- [ ] **Step 6: Run Rust and UI regression tests**

Run: `cd src-tauri; cargo test route_model_test_service route_proxy_service; cd ..; pnpm vitest run tests/AccountsScreen.test.tsx`

Expected: PASS. The Accounts screen test should verify that a successful manual test refetches the row and removes the error/cooldown badges.

- [ ] **Step 7: Review the diff without committing**

Verify semantic failures do not call `record_transient_failure`, do not call `update_status(..., "revoked")`, and do not return the failed HTTP-success body when another pool account succeeds.

---

### Task 9: Run Full Validation and Reconcile Documentation

**Files:**
- Modify only documentation or test fixtures if validation exposes an actual contract mismatch.

- [ ] **Step 1: Run focused frontend validation**

Run: `pnpm vitest run tests/SettingsScreen.test.tsx tests/AccountsScreen.test.tsx tests/transport/transport.test.ts src/lib/accountReorder.test.ts`

Expected: PASS.

- [ ] **Step 2: Run focused and broad Rust validation**

Run:

```powershell
Set-Location src-tauri
cargo test settings_service deeplink_protocol_service route_credential_repository route_model_test_service route_proxy_service web::handlers::tests
cargo check
Set-Location ..
```

Expected: PASS.

- [ ] **Step 3: Run production typecheck/build**

Run: `pnpm typecheck; pnpm build; pnpm server:check`

Expected: PASS with no generated or unrelated metadata changes.

- [ ] **Step 4: Inspect final behavior and diff**

Run: `git diff --check; git status --short`

Manually verify on Windows that a default install registers `aiswitch` only, enabling/disabling `ccswitch` changes the association immediately, account pagination and cross-page drag persist after refresh, and the maintenance response marks the account abnormal while a successful manual test recovers it.

- [ ] **Step 5: Hand off without committing**

Report focused test results, any pre-existing failures, and the exact files changed. Do not commit unless the user explicitly requests it.
