# Account Archive Design

## Goal

Add a reversible archive workflow for route credentials and expose archived accounts as a first-class account view without changing account status semantics or losing route-pool membership.

## User Experience

- The account status bar segments are `算力池`, `未入池`, `已归档`, and `统计`.
- The archived segment reuses the existing account list, filters, pagination, and selection controls.
- Account selection is the only archive interaction: selected active accounts show a compact archive action, and selected archived accounts show a restore action.
- Archiving an account removes it from active pool and out-of-pool lists and prevents it from being routed or quota-refreshed.
- The account's existing route-pool membership is preserved while archived. Restoring it returns the account to the same pool view it had before archiving.
- Switching views clears selection and resets pagination. Empty views use the existing centered empty-state treatment.

## Data Model

- Add nullable `archived_at TEXT` to `route_credentials` with an index supporting platform and archive-state filtering.
- Add `archived_at` to the Rust and TypeScript `RouteCredential` models.
- Extend `RouteCredentialPoolScope` with `Archived` and the serialized value `archived`.
- Active scopes require `archived_at IS NULL` before applying their in-pool or out-of-pool predicate. The archived scope requires `archived_at IS NOT NULL`.
- Keep `route_pool_members` unchanged when archiving or restoring. Runtime pool selection and account refresh queries must exclude archived credentials.

## API and Data Flow

- Add batch commands `archive_route_credentials(ids)` and `restore_route_credentials(ids)`.
- The service and repository update all requested IDs in one transaction, set or clear `archived_at`, update `updated_at`, and treat repeated archive/restore requests as idempotent.
- Empty ID lists and missing credentials return validation errors. A failed batch rolls back completely.
- Page, boundary, selection, transfer/export, and legacy list paths accept the archived scope where they already accept pool scope. Active routing, pool statistics, and quota refresh paths exclude archived credentials.
- The frontend mutation invalidates account-page, route-pool, and related account caches after success; failed mutations keep the current selection so the user can retry.

## Testing

- Rust repository tests cover archived filtering, restoration to the prior pool scope, exclusion from runtime pool queries, idempotent batches, and transaction rollback.
- TypeScript tests cover the archived segment, selection-dependent archive/restore controls, view reset behavior, and the empty archived state.
- Run focused Vitest tests, `pnpm typecheck`, `pnpm build`, and `git diff --check` before handoff.

## Scope Boundaries

- Archive is separate from `status`; it does not create a new account status.
- No per-row archive button is added; archive and restore are batch selection actions.
- No automatic deletion, expiration policy, or archive metadata beyond the timestamp is introduced.
