# Route Credential Retry Cooldown Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make temporary route-credential failures recoverable with persisted backoff, three-failure cooldown, and automatic success recovery while preserving permanent revocation handling.

**Architecture:** Add retry metadata columns to `route_credentials`; expose repository methods that update failure state transactionally; make proxy and model-test pool selection exclude credentials until their retry/cooldown timestamps pass. Proxy and model-test outcomes classify transient versus permanent failures through shared helpers, and successful requests clear retry metadata.

**Tech Stack:** Rust 2021, Tauri backend, SQLite/sqlx migrations, chrono, Tokio, existing route proxy and model-test services.

## Global Constraints

- Temporary failures never write `status = 'error'`.
- `invalid_grant`, revoked refresh tokens, and explicit permanent credential failures still write `status = 'revoked'`.
- Failure backoff is 30 seconds, 2 minutes, then a 10-minute cooldown from the third failure onward.
- Persist retry metadata in SQLite and update it transactionally.
- Keep existing pool membership identifiers and `route_pool.empty` behavior compatible.

---

### Task 1: Add Persistent Retry Metadata

**Files:**
- Create: `src-tauri/migrations/202607300001_route_credential_retry.sql`
- Modify: `src-tauri/src/models/route_credential.rs`
- Modify: `src-tauri/src/database/repositories/route_credential_repository.rs`
- Test: `src-tauri/src/database/repositories/route_credential_repository.rs` (existing tests module)

**Interfaces:**
- Produces `RouteCredential` fields `transient_failure_count`, `next_retry_at`, `cooldown_until`, `last_failure_kind`, and `last_failure_message`.
- Produces `RouteCredentialRepository::record_transient_failure(pool, id, kind, message) -> Result<RetryState, AppError>`.
- Produces `RouteCredentialRepository::clear_transient_failure(pool, id) -> Result<(), AppError>`.
- Produces `RetryState { failure_count: i64, next_retry_at: Option<String>, cooldown_until: Option<String> }`.

- [ ] **Step 1: Add the additive SQLite migration**

Create the five nullable/defaulted columns and an index supporting platform/status/time filtering:

```sql
ALTER TABLE route_credentials ADD COLUMN transient_failure_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE route_credentials ADD COLUMN next_retry_at TEXT;
ALTER TABLE route_credentials ADD COLUMN cooldown_until TEXT;
ALTER TABLE route_credentials ADD COLUMN last_failure_kind TEXT;
ALTER TABLE route_credentials ADD COLUMN last_failure_message TEXT;
CREATE INDEX IF NOT EXISTS idx_route_credentials_retry
  ON route_credentials(platform, status, next_retry_at, cooldown_until);
```

- [ ] **Step 2: Extend the model and repository SELECT statements**

Add the fields to `RouteCredential` with `#[sqlx(default)]` only where needed for old test fixtures, and include all five columns in `get` and `list_by_platform` queries. Keep create/update SQL defaults intact so existing callers remain source-compatible.

- [ ] **Step 3: Implement transactional failure recording**

Inside a SQLite transaction, read the current count, increment it, select the base delay (`30s`, `120s`, or `600s` for count >= 3), calculate a deterministic 0.8-1.2 jitter from credential ID and failure count, and update all retry fields with one `UPDATE`. Store a truncated message (maximum 512 bytes) and return the resulting timestamps. Return a validation error when the ID does not exist.

- [ ] **Step 4: Implement atomic success clearing**

Update the credential by ID, setting count to `0`, all retry timestamps and failure details to `NULL`, and `updated_at` to current UTC. Treat a missing ID as the existing validation error.

- [ ] **Step 5: Add repository tests**

Use the existing migrated memory-pool helpers to verify first/second/third failures produce increasing future timestamps, the third sets `cooldown_until`, messages are truncated, `clear_transient_failure` resets every field, and two sequential updates preserve the count.

- [ ] **Step 6: Run focused repository tests**

Run `cargo test --manifest-path src-tauri/Cargo.toml route_credential_repository -- --nocapture` and expect PASS.

- [ ] **Step 7: Commit the persistence slice**

```bash
git add src-tauri/migrations/202607300001_route_credential_retry.sql src-tauri/src/models/route_credential.rs src-tauri/src/database/repositories/route_credential_repository.rs
git commit -m "feat: persist route credential retry state"
```

### Task 2: Centralize Failure Classification and Pool Eligibility

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs`
- Modify: `src-tauri/src/services/route_model_test_service.rs`
- Test: `src-tauri/src/services/route_proxy_service.rs`
- Test: `src-tauri/src/services/route_model_test_service.rs`

**Interfaces:**
- Produces `ProxyFailureKind` and `classify_proxy_failure(status, message) -> ProxyFailureKind`.
- Produces `credential_is_retryable_now(next_retry_at, cooldown_until, now) -> bool`.
- Reuses the existing `SelectedCredential` shape; no frontend API change.

- [ ] **Step 1: Write classification and eligibility tests**

Cover network/transport errors, `502/503/504`, ambiguous `401/403`, permanent `invalid_grant` and revoked-token text, successful responses, and timestamp eligibility before/after expiry.

- [ ] **Step 2: Implement shared classification helpers**

Define transient versus permanent outcomes. Treat `401/403` as transient unless the response/error body contains explicit permanent markers (`invalid_grant`, `revoked`, or equivalent existing OAuth helper result). Keep quota exhaustion separate from credential failure.

- [ ] **Step 3: Extend pool queries with retry timestamps**

Select `next_retry_at` and `cooldown_until` from both proxy and model-test pool queries, then filter rows in Rust using `Utc::now()` and RFC3339 parsing. Invalid timestamps should be treated as eligible so a malformed legacy value cannot strand an account.

- [ ] **Step 4: Add model-test selection guards**

When selecting from the pool, exclude credentials in backoff/cooldown. When a caller explicitly requests an account ID, retain the explicit behavior but apply the same retry guard before sending the request and return a recoverable validation error if it is cooling down.

- [ ] **Step 5: Run helper and model-test unit tests**

Run `cargo test --manifest-path src-tauri/Cargo.toml route_model_test_service route_proxy_service -- --nocapture` and expect PASS for the new classification and eligibility tests.

- [ ] **Step 6: Commit eligibility and classification**

```bash
git add src-tauri/src/services/route_proxy_service.rs src-tauri/src/services/route_model_test_service.rs
git commit -m "feat: classify route credential failures"
```

### Task 3: Apply Retry State in Proxy and Model-Test Flows

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs`
- Modify: `src-tauri/src/services/route_model_test_service.rs`
- Test: `src-tauri/src/services/route_proxy_service.rs`
- Test: `src-tauri/src/services/route_model_test_service.rs`

**Interfaces:**
- Consumes `RouteCredentialRepository::record_transient_failure` and `clear_transient_failure` from Task 1.
- Consumes `classify_proxy_failure` and pool eligibility helpers from Task 2.

- [ ] **Step 1: Replace direct unavailable status writes**

For request-build errors, transport errors, and retryable upstream statuses, call `record_transient_failure` with a sanitized kind/message. Remove `mark_route_credential_unavailable` from those paths. Keep `mark_route_credential_revoked` for permanent OAuth refresh failures.

- [ ] **Step 2: Clear retry state on successful upstream responses**

After a non-quota, non-retryable response is received, call `clear_transient_failure` before returning the response. Do the same for successful model-test outcomes. A failed best-effort state update must not fail an otherwise successful request.

- [ ] **Step 3: Record permanent failures distinctly**

When refresh or response classification is permanent, update `status = 'revoked'` and do not increment transient counters. Continue to try the next pool credential within the current request only when the existing flow treats the result as retryable.

- [ ] **Step 4: Add end-to-end proxy tests**

Update the existing unauthorized retry test to assert the failed credential remains `ok`, has count `1`, and has a future `next_retry_at`. Add a test that directly records three failures, confirms `cooldown_until`, excludes the credential from `load_pool_credentials`, then clears state and confirms eligibility. Add a transport-failure test proving status is not `error`.

- [ ] **Step 5: Add model-test recovery tests**

Verify a failed model connectivity test records transient retry metadata instead of `error`, a later successful test clears it, and a permanent revoked-token response still results in `revoked`.

- [ ] **Step 6: Run focused integration tests**

Run `cargo test --manifest-path src-tauri/Cargo.toml route_proxy_service route_model_test_service -- --nocapture` and expect PASS.

- [ ] **Step 7: Commit request-flow changes**

```bash
git add src-tauri/src/services/route_proxy_service.rs src-tauri/src/services/route_model_test_service.rs
git commit -m "fix: cool down transient route credential failures"
```

### Task 4: Full Verification and Compatibility Cleanup

**Files:**
- Modify only files required by failing checks.

- [ ] **Step 1: Format and compile Rust**

Run `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` and `pnpm rust:check`; fix only formatting or compile errors from this feature.

- [ ] **Step 2: Run the complete Rust test suite**

Run `pnpm rust:test` and verify existing route pool, quota, import, and proxy tests remain green.

- [ ] **Step 3: Inspect migration and working tree**

Run `git diff --check`, verify the new migration is additive and no unrelated log files are staged, then run `git status --short`.

- [ ] **Step 4: Commit only required compatibility fixes**

```bash
git add src-tauri/migrations/202607300001_route_credential_retry.sql src-tauri/src/models/route_credential.rs src-tauri/src/database/repositories/route_credential_repository.rs src-tauri/src/services/route_proxy_service.rs src-tauri/src/services/route_model_test_service.rs
git commit -m "test: verify route credential cooldown behavior"
```
