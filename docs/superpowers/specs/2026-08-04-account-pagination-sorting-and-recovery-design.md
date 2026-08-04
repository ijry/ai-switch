# Account Pagination, Sorting, and Recovery Design

## Status

Approved in conversation on 2026-08-04. This specification covers four related changes to account management and routing behavior:

1. Disable `ccswitch://` compatibility by default and expose an explicit setting.
2. Let an explicit account test bypass cooldown and recover a healthy account.
3. Add server-backed account pagination with persistent drag-and-drop ordering.
4. Treat semantic `response.failed` responses as account errors even when HTTP status is successful.

The repository currently contains unrelated uncommitted work. Implementation must preserve those changes and work directly on `main`.

## Problem Statement

AI Switch statically declares both `aiswitch://` and `ccswitch://` as desktop URL schemes. Installing AI Switch can therefore claim the `ccswitch://` protocol from cc-switch even when the compatibility path is not needed.

Route credentials already store retry and cooldown state. An explicit account model test, however, currently rejects a cooling account before sending the request. This prevents the user from verifying that an upstream has recovered. Successful model tests already clear transient failure state for non-revoked accounts, but recovery behavior is incomplete and does not restore a revoked account after its credentials have been corrected.

Accounts have a persisted `sort_order`, but the Accounts screen loads every account, groups accounts into batch cards, and provides no ordering UI or account pagination. Adding pagination requires the order to remain global, stable, and transactionally persisted while still allowing drag-and-drop across page boundaries.

Some Responses-compatible upstreams return HTTP success while the response body reports failure:

```json
{
  "type": "response.failed",
  "response": {
    "status": "failed",
    "error": {
      "message": "【服务维护公告】当前模型 [gpt-5.5] 正在维护中，暂时无法提供服务。",
      "type": "api_error",
      "code": null
    }
  }
}
```

The current HTTP-status-only success check can treat this as a successful request, clear cooldown, and keep the account eligible for automatic routing.

## Goals

1. Keep `aiswitch://` registered as the native AI Switch import protocol.
2. Do not claim `ccswitch://` in a default installation.
3. Let Windows and Linux users explicitly enable or disable `ccswitch://` compatibility from Settings.
4. Make explicit single-account tests ignore cooldown and account status gates.
5. Clear all transient failure state and restore `ok` after a successful explicit account test.
6. Detect `response.failed` semantics in JSON and SSE responses for both model tests and proxy routing.
7. Mark semantic response failures as `error` without treating them as revoked credentials or adding cooldown.
8. Provide server-backed account pagination with page sizes `20`, `50`, and `100`.
9. Replace batch-group cards with one flat paginated account list while preserving batch labels.
10. Persist drag-and-drop order globally and support cross-page dragging through delayed automatic page changes.
11. Keep filtering, pagination, selection, pool membership, copying, testing, editing, and deletion coherent after reorder operations.

## Non-Goals

- Do not change route-pool member ordering in this feature. Account-list order and route-pool rotation order remain separate concerns.
- Do not add infinite scrolling or virtualized rendering.
- Do not add user-defined page sizes outside `20`, `50`, and `100`.
- Do not automatically retry semantic maintenance failures.
- Do not classify every application-level error shape from every provider. This feature recognizes the Responses failure envelope described below.
- Do not remove `ccswitch://` parsing support; only registration and handling are gated.
- Do not provide dynamic `ccswitch://` registration on macOS because the current Tauri deep-link plugin does not support runtime registration or unregistration there.

## Architecture

### Settings and Protocol Registration

Add `ccswitch_deeplink_compat_enabled: bool` to backend and frontend `AppSettings`. Use a serde default function returning `false` so settings files created by older releases load without migration or parse failure.

Remove `ccswitch` from the static deep-link scheme list in `src-tauri/tauri.conf.json`. Keep only `aiswitch` in the install-time configuration. This is required because a statically declared scheme may be claimed during installation before application settings are loaded.

On Windows and Linux, saving settings applies the requested protocol association before the settings file is committed:

- `false -> true`: call the Tauri deep-link runtime registration API for `ccswitch`;
- `true -> false`: call the runtime unregistration API for `ccswitch`;
- no value change: do not modify protocol association.

If registration or unregistration fails, return a structured settings error and do not persist the new value. The UI keeps the previous setting and shows the failure. This prevents the persisted setting from claiming compatibility that the operating system did not apply.

At startup, Windows and Linux reconcile the configured setting:

- always ensure the statically configured `aiswitch` scheme is registered where the existing application already performs registration;
- register `ccswitch` when compatibility is enabled;
- unregister AI Switch's `ccswitch` association when compatibility is disabled.

Before unregistration, verify through the plugin API that AI Switch is the current handler. Unregistration must only remove AI Switch's association and must not delete, register, or restore cc-switch on behalf of another application.

On macOS and in the standalone Web server, the setting is visible but disabled with an explanation that runtime desktop protocol compatibility is unavailable. Its value remains `false`.

If protocol registration succeeds but writing the settings file fails, the settings coordinator immediately applies the inverse protocol operation to restore the previous association. If that compensation also fails, the returned error reports both failures and startup reconciliation remains the final repair path.

Deep-link input handling must check both scheme and setting:

- `aiswitch://` is always accepted;
- `ccswitch://` is accepted only when compatibility is enabled;
- disabled `ccswitch://` command-line arguments are ignored and do not display an import dialog or error banner.

The parser remains capable of parsing both schemes so the compatibility behavior stays isolated from provider import parsing.

### Account Page API

Replace the Accounts screen's unbounded credential query with a paginated route credential query. The backend request contains:

```text
platform: PlatformId
page: integer >= 1
page_size: one of 20, 50, 100
filters: zero or more batch IDs plus the single-account sentinel
```

The response contains:

```text
items: RouteCredential[]
total: integer
page: integer
page_size: integer
page_count: integer
previous_page_account_id: string | null
next_page_account_id: string | null
filter_options: all platform batch IDs/labels plus the single-account option
official_account_count: integer for the complete platform account set
```

The repository applies filters to the complete platform account set, orders by `sort_order ASC, created_at DESC`, counts the filtered rows, clamps the requested page to the last valid page, and then applies `LIMIT/OFFSET`. The two boundary IDs identify the filtered accounts immediately outside the returned page so a drop at the first or last visible position still describes an exact global insertion point.

Batch filter options require a small metadata query that is independent of the current page. The Accounts screen must not infer available filters only from the visible page. The metadata response supplies all batch IDs/names present on the platform and whether any single accounts exist.

Mutations that need a complete platform view, such as quota refresh and route-pool membership, continue to use dedicated backend operations. The frontend must not reconstruct global state from the current page.

### Global Reorder API

Add a transactional reorder operation with this logical input:

```text
platform: PlatformId
moved_account_id: string
previous_account_id: string | null
next_account_id: string | null
active_filters: the same filter set used by pagination
page_size: one of 20, 50, 100
```

The client describes the moved account's final position relative to neighboring accounts in the filtered order instead of sending every account ID. The backend loads the complete platform order and filtered order in one transaction, validates that all referenced accounts belong to the platform and filter result, moves the account, merges the filtered order back into the full order, and rewrites dense `sort_order` values starting at zero.

Accounts excluded by the active filter retain their relative order. The moved account changes global position only as required to appear between its selected filtered neighbors. Dense rewriting avoids duplicate and increasingly sparse sort values after repeated operations.

The operation returns the normalized page containing the moved account plus the updated total. The frontend invalidates account page and filter metadata queries after success.

### Flat Paginated List

The Accounts screen replaces batch-group cards with one flat list. Each row retains:

- account name;
- account kind;
- batch label when present;
- status and cooldown labels;
- quota and request statistics;
- selection checkbox;
- quota, copy, test, and edit actions.

Pagination controls appear below the list. The default page size is `20`; users may choose `20`, `50`, or `100`. Page size and current page are screen state, not global application settings.

Changing platform, filters, or page size resets the page to `1`. Deleting the last row on a page causes the backend-clamped page to become the new current page. Selection remains ID-based across page changes so batch operations may include accounts selected on different pages. The selected count remains visible even when selected rows are off-page.

### Drag-and-Drop Behavior

Each account row has a dedicated drag handle. Interactive row controls do not initiate dragging.

During a drag, the frontend keeps a local projection of the visible filtered order and the dragged account ID. Dragging over a row displays the insertion position. Dropping sends one relative reorder request.

When the pointer stays in the top or bottom edge zone for `600ms`:

- the screen changes to the previous or next page when one exists;
- the dragged account remains active;
- the target page loads and accepts an insertion position;
- repeatedly holding at the edge can continue across multiple pages.

The current drag is cancelled if the platform, filters, or page size changes manually, or if the account disappears because of a concurrent mutation.

On reorder failure, the frontend clears the optimistic projection, refetches the current page, and shows an error message. Selection and route-pool draft membership are not changed.

Keyboard accessibility uses the same relative reorder operation. A focused drag handle can enter move mode, move the account before or after adjacent filtered accounts, cross page boundaries, and commit or cancel without a pointer.

## Model Test Recovery

### Explicit Account Selection

When `RoutePoolModelTestRequest.account_id` is present, loading the account validates only that the account exists on the requested platform. It does not reject the account for:

- `next_retry_at`;
- `cooldown_until`;
- `warning` status;
- `error` status;
- `revoked` status.

Capability and credential-kind validation still apply. Pool-wide tests without an explicit account continue to use normal automatic routing eligibility and cooldown behavior.

### Successful Test Recovery

A successful explicit account test performs one recovery update that:

- sets `status = 'ok'`;
- sets `transient_failure_count = 0`;
- clears `next_retry_at` and `cooldown_until`;
- clears `last_failure_kind` and `last_failure_message`;
- updates the normal modification timestamp.

This recovery applies to `warning`, `error`, and `revoked`. Restoring `revoked` is intentional because the explicit test proves the currently stored credential can successfully generate a response after the user may have edited it.

Pool-wide tests and successful proxy traffic continue clearing transient failure state under their existing rules, but they must not silently restore a revoked account that was not explicitly selected and proven healthy.

## Semantic Response Failure Detection

### Recognized Envelopes

Introduce one shared parser used by route model testing and route proxy handling. It recognizes a semantic Responses failure when a JSON object satisfies either condition:

- top-level `type` equals `response.failed`; or
- top-level `response.status` equals `failed`.

The preferred error message is `response.error.message`. Fallbacks are top-level `error.message`, then a stable generic message such as `Upstream response reported failure`.

For `text/event-stream` bodies, inspect each `data:` payload that contains JSON. Ignore blank lines, comments, non-JSON events, and `[DONE]`. The first recognized failure produces the semantic failure result.

The parser must not expose secrets and must operate on the bounded response bytes already retained by the relevant request path.

### Status and Cooldown Effects

A recognized semantic failure overrides HTTP success for model-test outcome and routing decisions:

- outcome `success` is `false`;
- credential status becomes `error`;
- `last_failure_kind` becomes `semantic_response_failed`;
- `last_failure_message` stores the sanitized upstream message;
- existing transient cooldown fields are cleared rather than incremented;
- the credential is excluded from automatic routing because automatic selection requires `status = 'ok'`;
- the credential is not marked `revoked`.

Maintenance and model availability errors are therefore visible and fail closed without pretending that authentication has been revoked.

Existing failure classification remains in place for other cases:

- connection errors, timeouts, `429`, `502`, `503`, and `504` remain transient and create cooldown;
- permanent authentication or refresh failures remain revoked;
- other non-retryable HTTP errors retain existing pass-through or classification behavior unless separately specified.

### Proxy Retry Behavior

When a route-proxy attempt receives a semantic failure, record the account error and try the next eligible account in the pool. Do not return the failed HTTP-success body to the client while another account is available.

If every eligible account fails, return the existing aggregated route-proxy error response. The aggregated details include sanitized semantic failure messages for diagnosis.

## Error Handling

- Invalid page or page-size input returns a validation error; the UI only emits allowed values.
- Reorder references to missing, cross-platform, or filtered-out accounts fail without changing any `sort_order`.
- A database failure during dense reorder rolls back the complete transaction.
- Protocol registration failures leave both the saved setting and UI toggle at the previous value.
- A semantic failure parser error never converts a clear HTTP failure into success. Unrecognized malformed bodies fall back to HTTP classification.
- Account test response details remain sanitized through the existing credential-aware storage path.

## Testing

### Rust Settings and Deep Links

- Loading an old settings file without `ccswitch_deeplink_compat_enabled` yields `false`.
- New settings default the compatibility field to `false`.
- Saving a toggle calls the protocol registrar before persisting.
- Registrar failure leaves the settings file unchanged.
- Startup reconciliation registers or unregisters `ccswitch` according to the saved value on Windows/Linux.
- Disabled compatibility ignores `ccswitch://` input while `aiswitch://` remains accepted.
- Static Tauri configuration contains `aiswitch` but not `ccswitch`.

Protocol registration logic must be behind a small injectable interface so unit tests do not change the developer machine's real protocol association.

### Rust Pagination and Reordering

- Default and allowed page sizes return correct rows and totals.
- The backend clamps a page after deletion or an oversized page request.
- Batch and single-account filters are applied before count and pagination.
- Filter metadata includes batches and single accounts not visible on the current page.
- Page ordering is stable for equal or legacy duplicate sort values.
- Page-local, next-page, previous-page, and multi-page moves produce dense global `sort_order` values.
- Filtered reorder preserves the relative order of excluded accounts.
- Invalid reorder references leave every row unchanged.
- A forced write error rolls back the reorder transaction.

### Rust Model Tests and Proxy Routing

- An explicitly selected cooling account sends a model test request.
- An explicitly selected `error` or `revoked` account can be tested.
- Explicit success restores `ok` and clears every transient failure field.
- Pool-wide selection still honors status and cooldown.
- JSON `response.failed` with HTTP `200` yields failure and status `error` without cooldown.
- JSON with `response.status = failed` yields the same result.
- SSE `data:` containing `response.failed` yields the same result.
- A normal Responses JSON or SSE success remains successful.
- Proxy routing retries another account after semantic failure.
- Authentication failures remain revoked and transient transport/status failures retain cooldown behavior.

### React Accounts and Settings

- Settings displays the compatibility toggle as off by default and rolls it back on save failure.
- The account list is flat and preserves batch labels.
- Pagination shows the correct total, page, and allowed page sizes.
- Platform, filter, and page-size changes reset to page `1`.
- Selection persists across pages and batch actions use all selected IDs.
- Page-local drag sends correct neighbor IDs.
- Holding in an edge zone for `600ms` changes page and retains the dragged item.
- Reorder failure refetches and displays an error without clearing selection.
- Keyboard move mode can reorder and cross page boundaries.
- A successful manual test refreshes the row so cooldown and error labels disappear.

## Validation Commands

Run focused tests first, followed by broader checks appropriate to the repository:

```powershell
pnpm vitest run tests/AccountsScreen.test.tsx
pnpm vitest run tests/SettingsScreen.test.tsx
Set-Location src-tauri
cargo test settings_service
cargo test route_credential
cargo test route_model_test_service
cargo test route_proxy_service
cargo check
Set-Location ..
pnpm build
```

If the repository uses different exact Vitest scripts or no dedicated Settings screen test yet, use the existing package scripts and add the focused test file alongside the current screen tests.

## Acceptance Criteria

1. A default installation no longer claims `ccswitch://`.
2. Enabling compatibility on Windows/Linux registers `ccswitch://`; disabling it releases AI Switch's association immediately.
3. `aiswitch://` imports remain unchanged.
4. Accounts are server-paginated in a flat list with page sizes `20`, `50`, and `100`.
5. Dragging can reorder within and across pages, and the order persists after restart.
6. Filtering and reordering preserve the relative order of accounts outside the active filter.
7. A cooling or abnormal account can always be explicitly tested.
8. A successful explicit test restores the account to `ok` and clears cooldown and failure details.
9. JSON and SSE `response.failed` envelopes mark the account `error`, do not add cooldown, and cause proxy retry.
10. Existing transient and revoked failure classifications continue to behave as before.
