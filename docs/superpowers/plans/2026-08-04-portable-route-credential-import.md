# Portable Route Credential Import Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Consume the shared portable-transfer foundation from `2026-08-04-secure-route-credential-export.md` and add safe mixed-platform preview, conflict-aware batch import, optional append-only pool restoration, and import-only transport/UI wiring.

**Architecture:** The secure export plan is the sole owner of shared DTOs/constants, the transfer migration and origin repository, CPA projection/fingerprint primitives, deep-link serialization, export/save commands, Web authentication/TLS hardening, export helpers, and the export dialog. This plan owns only import parsing and reverse mapping, asynchronous redacted preview, import-specific transaction primitives, atomic commit, two import command additions, API client calls, and `RouteCredentialImportDialog`. The desktop workspace plan owns `AccountsScreen` layout, toolbar callbacks, query invalidation, and dialog placement.

**Tech Stack:** Rust 2021, SQLite/SQLx 0.8, serde_json, Axum 0.7, Tauri 2, React 18, TypeScript, Vitest, Testing Library.

## Global Constraints

- Execute the secure export plan first. Do not recreate or rename `route_credential_transfer.rs`, `202608040001_route_credential_transfer.sql`, `route_credential_transfer_repository.rs`, `route_credential_transfer_codec.rs`, `route_credential_transfer_commands.rs`, export services, export helpers, or export dialogs.
- Import accepts only a UTF-8 bare JSON array, at most `8 MiB`, `2000` items, and `256 KiB` per serialized item. A top-level wrapper such as `{ "accounts": [...] }` is invalid.
- `ambiguous_platform_choices` is `Vec<TransferPlatformChoice>` keyed only by the original `item_index`; duplicate choice indices, invalid platforms, and incompatible interface formats are validation errors.
- Preview is asynchronous because it queries source origins and local fingerprint candidates. It returns only masked names, counts, classifications, dispositions, and safe issue codes; never raw items, tokens, API keys, complete URLs, query strings, or fingerprints.
- Complete source identity is exactly `(source_instance_id, source_credential_id, platform, kind)`. Missing or partial source IDs remain backward-compatible metadata but never create an origin row or global idempotency key.
- Import never overwrites or merges by email, display name, account ID, or batch name. Equal complete-source identity plus equal fingerprint is a duplicate; unequal fingerprint is a conflict.
- Default `restore_pool_membership` is `false`. Opt-in restoration appends only credentials created by the current transaction and marked `in_pool: true`; it never deletes, disables, replaces, or reorders existing members.
- Commit reparses the exact source text, begins one SQLite transaction, rechecks duplicate/conflict state inside that transaction, and rolls back credentials, batches, origins, and pool members on any SQL error.
- Tauri always registers preview/import commands. Web adds them only to the sensitive-command set established by the export plan, so authorization still runs before body extraction, responses remain `no-store`, and non-loopback HTTP remains rejected by the shared TLS gate.
- Work directly on `main`; do not create branches, worktrees, or commits.

## Consumed Foundation

Use the exact shared contracts from the secure export plan without redefining them:

- DTOs: `TransferPlatformChoice`, `RouteCredentialTransferIssue`, `PreviewRouteCredentialImportInput`, `RouteCredentialImportPreviewItem`, `RouteCredentialImportPreviewCounts`, `RouteCredentialImportPreview`, `ImportRouteCredentialsInput`, and `RouteCredentialImportOutcome`.
- Preview count fields: `total`, `official`, `api`, `importable`, `duplicates`, `conflicts`, `errors`, `restorable_pool_count`, `batch_count`, `platform_counts`, `cpa_section_counts`, `legacy_type_counts`, and `restorable_pool_counts`.
- Projection/codec functions: `project_credential`, `canonical_fingerprint(kind: &str, projected_without_metadata: &serde_json::Value)`, and `trusted_cpa_raw_template(platform: &str, config: &serde_json::Value)`.
- Origin repository functions: `get_or_create_installation_id`, `find_origin_by_identity`, `find_origin_by_identity_tx`, and `insert_origin_tx`.
- Shared transport policy: pre-body authorization middleware, `12 MiB` Axum body limit, sensitive response cache headers, `is_loopback_host`, and `validate_sensitive_web_transport`.

---

### Task 1: Add Import Transaction and Preview Repository Primitives

**Files:**
- Modify: `src-tauri/src/database/repositories/batch_repository.rs`
- Modify: `src-tauri/src/database/repositories/route_credential_repository.rs`
- Modify: `src-tauri/src/database/repositories/route_pool_repository.rs`
- Test: the three repository modules

**Interfaces:**

```rust
impl BatchRepository {
    pub async fn create_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        input: NewBatch,
    ) -> Result<Batch, AppError>;
}

impl RouteCredentialRepository {
    pub async fn create_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        platform: &str,
        kind: &str,
        display_name: &str,
        email: Option<String>,
        status: &str,
        batch_id: Option<String>,
        secret_payload_json: &str,
        config_json: &str,
        preview_json: &str,
    ) -> Result<RouteCredential, AppError>;

    pub async fn list_transfer_fingerprint_candidates(
        pool: &sqlx::SqlitePool,
        platforms: &[String],
    ) -> Result<Vec<RouteCredential>, AppError>;
}

impl RoutePoolRepository {
    pub async fn append_members_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        platform: &str,
        credential_ids: &[String],
    ) -> Result<usize, AppError>;
}
```

- [ ] **Step 1: Write failing repository tests** for `BatchRepository::create_tx`, `RouteCredentialRepository::create_tx`, one-query fingerprint candidate loading, append-only `MAX(sort_order)` allocation, `ON CONFLICT(platform, route_credential_id) DO NOTHING`, and rollback through a caller-owned transaction.
- [ ] **Step 2: Run focused tests to verify failure**

Run:

```powershell
cargo test batch_repository --manifest-path src-tauri/Cargo.toml
cargo test route_credential_repository --manifest-path src-tauri/Cargo.toml
cargo test route_pool_repository --manifest-path src-tauri/Cargo.toml
```

Expected: FAIL because the import-specific transaction/read methods do not exist.

- [ ] **Step 3: Implement shared private SQL helpers** so existing pool-based create methods and new `*_tx` methods use the same insert logic. `create_tx` methods never begin or commit. Preserve route-credential quota columns and allocate route sort order inside the supplied transaction.
- [ ] **Step 4: Implement candidate loading and pool append** with bound `QueryBuilder<Sqlite>` values. Candidate loading returns only affected platforms in deterministic `platform, kind, id` order. Pool append reads one current maximum per platform, increments only for successful inserts, and never calls `replace_members`.
- [ ] **Step 5: Re-run the focused repository tests**

Expected: PASS; caller rollback removes all new rows and leaves existing pool order unchanged.

---

### Task 2: Implement Strict Parsing and Reverse CPA Mapping

**Files:**
- Create: `src-tauri/src/services/route_credential_transfer_import_service.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Test: `src-tauri/src/services/route_credential_transfer_import_service.rs`

**Internal interfaces:**

```rust
struct CompleteSourceIdentity {
    source_instance_id: String,
    source_credential_id: String,
    source_platform: String,
    source_kind: String,
    source_schema_version: i64,
}

struct ImportBatchKey {
    source_instance_id: Option<String>,
    source_batch_id: Option<String>,
    batch_name: String,
}

struct NormalizedImportItem {
    item_index: usize,
    platform: String,
    kind: String,
    cpa_section: Option<String>,
    legacy_type: Option<String>,
    display_name: String,
    display_name_masked: String,
    email: Option<String>,
    secret_payload_json: String,
    config_json: String,
    preview_json: String,
    source_identity: Option<CompleteSourceIdentity>,
    batch_key: Option<ImportBatchKey>,
    in_pool: bool,
    fingerprint: String,
    issue_codes: Vec<String>,
}

fn validate_transfer_text(
    text: &str,
) -> Result<Vec<serde_json::Map<String, serde_json::Value>>, AppError>;

fn classify_transfer_item(
    item_index: usize,
    item: &serde_json::Map<String, serde_json::Value>,
    choices: &[TransferPlatformChoice],
) -> Result<NormalizedImportItem, RouteCredentialTransferIssue>;
```

The secret-bearing internal structs above do not derive `Serialize` or `Debug` and never leave the service module.

Classification rules:

1. Validate one unique choice per `item_index`; reject choices that target a missing/non-ambiguous item or provide an interface format incompatible with the selected platform/CPA section.
2. Prefer validated `x-ai-switch.kind`, `platform`, and `cpa_section`. Require `format = ai-switch.route-credential`, supported major `schema_version = 1`, API-only `cpa_section`, and no metadata/payload semantic conflict.
3. Accept metadata-free official CPA Auth-File objects only when `type` plus OAuth/Agent-Identity fields identify one platform unambiguously. Ordinary OAuth requires access or refresh token; Agent Identity requires private key, runtime, task, and account/workspace identity. Preserve `client_id` and the other shared official allowlist fields.
4. Treat legacy API `type` only as an import discriminator and remove it before local config creation. A shape-only API entry requires a structured platform/interface choice; `openai-compatibility` without `x-ai-switch.platform` always requires one.
5. Reject `interactions-api-key`, `vertex-api-key`, top-level `api-keys`, unsupported sections, unknown major versions, missing API key/Base URL, multiple `openai-compatibility.api-key-entries`, and contradictory dialect/endpoint/platform combinations.
6. Reverse-map CPA models with `name -> to`, `alias -> from`, `display-name -> label`, and `max-context-length >= 1048576 -> supports_1m: true`. Restore `interface_format`, `responses_custom_tool_compat`, `api_key_field`, and original `model_mappings` only from compatible `x-ai-switch` metadata.
7. For official items, remove `x-ai-switch`, store the remaining validated Auth-File object as `config.raw`, set normalized `raw_type`, and set backend-owned `import_format = "auth-file"`. Keep unknown raw fields only when that constructed config passes `trusted_cpa_raw_template`; never trust an incoming `raw`/`import_format` claim by itself. API unknown non-secret fields become warnings; unknown secret-bearing fields are fatal.
8. Create `CompleteSourceIdentity` only when both source IDs and validated platform/kind are present. Partial source metadata produces a safe warning and no global identity.
9. Compute the internal fingerprint with the shared kind-aware `canonical_fingerprint`; never include it in issues, preview DTOs, logs, or snapshots.

Generate `display_name_masked` deterministically: trim the name; use `Item <item_index + 1>` when empty; for one or two Unicode scalar values keep the first plus `*`; otherwise keep the first and last values with `***` between them. Generate `preview_json` through the existing `RoutePreviewService::generate(platform, kind, secret_payload_json, config_json)` after normalization rather than accepting preview content from the migration file.

- [ ] **Step 1: Write failing parser/classifier tests** for non-array roots, non-object entries, byte/item limits, duplicate/unused choices, mixed official/API arrays, metadata-free API choices, unsupported sections, schema conflicts, Agent Identity requirements, `client_id`, trusted raw handling, reverse model mappings, and warning-only same-major optional fields.
- [ ] **Step 2: Run the focused service tests**

Run: `cargo test route_credential_transfer_import_service --manifest-path src-tauri/Cargo.toml`

Expected: FAIL because the import service does not exist.

- [ ] **Step 3: Implement parsing and normalization** with safe issue constructors that receive only `item_index`, masked display name, field name, and code. Measure each parsed object's compact serialized byte length for the `256 KiB` limit.
- [ ] **Step 4: Re-run the focused tests**

Expected: PASS; parser errors never contain source JSON or secret values.

---

### Task 3: Build Asynchronous Redacted Preview

**Files:**
- Modify: `src-tauri/src/services/route_credential_transfer_import_service.rs`
- Test: `src-tauri/src/services/route_credential_transfer_import_service.rs`

**Interface:**

```rust
pub async fn preview_route_credential_import(
    pool: &sqlx::SqlitePool,
    input: PreviewRouteCredentialImportInput,
) -> Result<RouteCredentialImportPreview, AppError>;
```

Use these stable `disposition` values: `import`, `input_duplicate`, `source_duplicate`, `possible_duplicate`, `conflict`, and `error`. Choice-required items use `error` plus `transfer.choice_required`. `duplicates` counts input and source duplicates; `possible_duplicate` remains importable; `errors` counts non-importable parse/classification items.

- [ ] **Step 1: Write failing preview tests** for same-input fingerprints, complete-source duplicates, same-source/different-fingerprint conflicts, possible duplicates without trusted source identity, masked names, stable item indices, batch grouping, all rich count maps, and redaction.
- [ ] **Step 2: Run the focused preview tests**

Run: `cargo test preview_route_credential_import --manifest-path src-tauri/Cargo.toml`

Expected: FAIL until asynchronous origin/candidate analysis exists.

- [ ] **Step 3: Implement duplicate and conflict analysis**. Retain the first same-input fingerprint. Query `find_origin_by_identity` only for complete identities. Load the stable installation ID once, batch-load current credentials through `list_transfer_fingerprint_candidates`, call `project_credential(candidate, &instance_id, false, false)`, feed `projected.payload` to `canonical_fingerprint`, and mark identity-less matches as `possible_duplicate` without skipping them. A local candidate that cannot be safely projected is omitted from possible-duplicate matching with no secret-bearing diagnostic.
- [ ] **Step 4: Implement batch/count prediction**. Complete identities group by `(source_instance_id, source_batch_id, batch_name)`; legacy items without `source_instance_id` group only within this file by source batch fields; name-only items group locally by `batch_name`; missing batch metadata creates no batch. Count only groups with at least one importable item. Never merge with an existing same-name local batch.
- [ ] **Step 5: Build only the shared redacted DTOs**. Populate `platform_counts`, `cpa_section_counts`, `legacy_type_counts`, `restorable_pool_counts`, and aggregate counts without serializing any `NormalizedImportItem` secret field.
- [ ] **Step 6: Re-run preview tests**

Expected: PASS; serialized preview contains no fingerprint, raw item, key, token, or complete URL.

---

### Task 4: Commit Imports in One Transaction

**Files:**
- Modify: `src-tauri/src/services/route_credential_transfer_import_service.rs`
- Test: `src-tauri/src/services/route_credential_transfer_import_service.rs`

**Interface:**

```rust
pub async fn import_route_credentials(
    pool: &sqlx::SqlitePool,
    input: ImportRouteCredentialsInput,
) -> Result<RouteCredentialImportOutcome, AppError>;
```

- [ ] **Step 1: Write failing atomicity tests** for exact-text reparse, transaction-time duplicate/conflict recheck, input duplicates, possible duplicates, lazy batch creation, same-name batch isolation, no-batch items, complete-identity-only origins, default pool ignore, opt-in append-only restore, and injected SQL rollback after each write category.
- [ ] **Step 2: Run the focused commit tests**

Run: `cargo test import_route_credentials --manifest-path src-tauri/Cargo.toml`

Expected: FAIL because the commit path does not exist.

- [ ] **Step 3: Implement one transaction** with exactly one `let mut tx = pool.begin().await?`. Reparse the original text and choices, recompute fingerprints, and call `find_origin_by_identity_tx` before each complete-identity insert. Use only `BatchRepository::create_tx`, `RouteCredentialRepository::create_tx`, shared `insert_origin_tx`, and `RoutePoolRepository::append_members_tx` for writes.
- [ ] **Step 4: Create batches lazily** after the first eligible credential in a predicted group, using `NewBatch { name: batch_name, source: "route_credential_transfer", notes: None }`. Create route credentials with status `"ok"`, the normalized payloads, generated preview JSON, and the new local batch ID. Insert origins only for complete validated source identities. Input/source duplicates increment `skipped_duplicates`; conflicts increment `conflicts`; preview errors increment `failed`; `possible_duplicate` items remain eligible.
- [ ] **Step 5: Restore pool membership only when opted in**. Group newly created `in_pool: true` IDs by platform and append them after credential/origin creation. Existing duplicates, conflicts, errors, and existing pool members never change state or order.
- [ ] **Step 6: Commit and return aggregate counts only after every write succeeds**. Any SQL error returns one structured error and rolls back all new credentials, batches, origins, and members.
- [ ] **Step 7: Re-run atomicity tests**

Expected: PASS; forced failures leave zero partial import rows.

---

### Task 5: Add Import Commands and Client Calls

**Files:**
- Modify: `src-tauri/src/commands/route_credential_transfer_commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/web/handlers/mod.rs`
- Modify: `src-tauri/src/web/router.rs`
- Modify: `src/lib/api/client.ts`
- Test: `src-tauri/src/commands/route_credential_transfer_commands.rs`
- Test: `src-tauri/src/web/handlers/mod.rs`
- Test: `src-tauri/src/web/router.rs`
- Test: `tests/transport/command-contract.test.ts`
- Test: `tests/transport/transport.test.ts`

**Interfaces:**

```rust
#[tauri::command]
pub async fn preview_route_credential_import(
    state: tauri::State<'_, AppState>,
    input: PreviewRouteCredentialImportInput,
) -> Result<RouteCredentialImportPreview, ApiError>;

#[tauri::command]
pub async fn import_route_credentials(
    state: tauri::State<'_, AppState>,
    input: ImportRouteCredentialsInput,
) -> Result<RouteCredentialImportOutcome, ApiError>;
```

```ts
export function previewRouteCredentialImport(
  input: PreviewRouteCredentialImportInput,
): Promise<RouteCredentialImportPreview>;

export function importRouteCredentials(
  input: ImportRouteCredentialsInput,
): Promise<RouteCredentialImportOutcome>;
```

The secure export plan creates the command module, pre-body authorization middleware, `12 MiB` body limit, cache-control helper, and loopback/TLS registration gate. This task only appends the two wrappers, Tauri registrations, Web dispatch arms, client calls, and sensitive-command names. It does not modify `src-tauri/src/server.rs`, `src-tauri/src/services/web_service.rs`, `src-tauri/src/web/auth.rs`, shared DTO types, `desktopOnlyCommands`, or native save behavior.

- [x] **Step 1: Write failing command/transport tests** asserting exact command names and `{ input }` arguments, Tauri/Web availability, Tauri-only save preservation, redacted `ApiError`, pre-body rejection of unauthorized oversized import requests, `no-store` import responses, and non-loopback HTTP refusal through the existing shared gate.
- [x] **Step 2: Implement wrappers, dispatch arms, sensitive-name additions, and the two typed client functions** without logging request bodies or response content.
- [x] **Step 3: Run focused transport tests**

Run:

```powershell
cargo test route_credential_transfer_commands --manifest-path src-tauri/Cargo.toml
cargo test web::handlers --manifest-path src-tauri/Cargo.toml
cargo test web::router --manifest-path src-tauri/Cargo.toml
pnpm vitest run tests/transport/command-contract.test.ts tests/transport/transport.test.ts
```

Expected: PASS; import inherits authentication/body-limit/TLS hardening and Web still cannot invoke native save.

---

### Task 6: Build the Portable Import Dialog

**Files:**
- Create: `src/components/accounts/RouteCredentialImportDialog.tsx`
- Consume: `src/lib/api/client.ts`
- Consume: `src/lib/api/types.ts`
- Test: `tests/RouteCredentialImportDialog.test.tsx`

**Interface (consumed unchanged by the workspace layout plan):**

```ts
export type RouteCredentialImportDialogProps = {
  open: boolean;
  onClose: () => void;
  onImported: (outcome: RouteCredentialImportOutcome) => void;
};
```

- [x] **Step 1: Write failing dialog tests** for paste input, `.json` browser file input, strict UTF-8/`8 MiB` checks, asynchronous preview, stale-preview suppression, mixed-platform redacted rows, structured item-index choices, rich duplicate/conflict/error counts, default-off pool restoration, one confirmation page, a local completion page, and sensitive-state clearing.
- [x] **Step 2: Implement ephemeral input state** with `File.arrayBuffer()` and `new TextDecoder("utf-8", { fatal: true })`. Keep source text, choices, preview, and pending state only in component memory; never use React Query caching, persistent storage, URL state, logs, Toast payloads, or snapshots.
- [x] **Step 3: Implement asynchronous preview safely**. Increment a request sequence for every text/choice change and apply only the latest response. Disable commit until the displayed preview matches the current exact text and structured choices. Render only shared redacted preview fields and require every `transfer.choice_required` item to receive a valid platform/interface choice.
- [x] **Step 4: Implement one confirmation and commit**. Ordinary import uses the preview confirmation page. When pool restoration is enabled, the same page adds the credential-file warning and `restorable_pool_counts` platform summary; do not open a second confirmation modal. Submit the identical original text and choices to `importRouteCredentials`.
- [x] **Step 5: Keep the completion page inside the dialog**. After success, store only `RouteCredentialImportOutcome`, clear the original source text/choices/preview, and call `onImported` so the workspace can show short feedback and invalidate account, pool, and batch queries. The workspace must not close the dialog from `onImported`; the user closes the completion page through `onClose`, which clears all remaining local state. Unmount performs the same cleanup.
- [x] **Step 6: Run dialog tests**

Run: `pnpm vitest run tests/RouteCredentialImportDialog.test.tsx`

Expected: PASS; preview races cannot display stale classifications and pool recovery remains explicit opt-in.

---

### Task 7: Validate Import and Cross-Plan Ownership

**Files:**
- Test: `src-tauri/src/services/route_credential_transfer_import_service.rs`
- Test: `src-tauri/src/database/repositories/batch_repository.rs`
- Test: `src-tauri/src/database/repositories/route_credential_repository.rs`
- Test: `src-tauri/src/database/repositories/route_pool_repository.rs`
- Test: `tests/RouteCredentialImportDialog.test.tsx`
- Test: `tests/transport/command-contract.test.ts`

- [ ] **Step 1: Run focused Rust suites**

```powershell
cargo test route_credential_transfer_import_service --manifest-path src-tauri/Cargo.toml
cargo test batch_repository --manifest-path src-tauri/Cargo.toml
cargo test route_credential_repository --manifest-path src-tauri/Cargo.toml
cargo test route_pool_repository --manifest-path src-tauri/Cargo.toml
cargo test route_credential_transfer --manifest-path src-tauri/Cargo.toml
```

Expected: parser, preview, source identity, fingerprint, transaction, rollback, and shared-contract tests pass.

- [ ] **Step 2: Run focused frontend/transport suites**

```powershell
pnpm vitest run tests/RouteCredentialImportDialog.test.tsx
pnpm vitest run tests/transport/command-contract.test.ts tests/transport/transport.test.ts
pnpm typecheck
```

Expected: exact shared DTO/client contracts, import commands, sensitive transport behavior, layout-owned dialog props, and redacted UI pass.

- [ ] **Step 3: Run broad validation**

Run: `pnpm rust:test`; `pnpm test:run`; `pnpm build`.

Expected: existing account, CPA, Sub2API, pool, export, and layout tests remain green; unrelated failures are reported without scope expansion.

- [ ] **Step 4: Audit placeholders, ownership, and redaction**

```powershell
rg -n 'T[B]D|T[O]DO|FIX[M]E|implement l[a]ter|fill in d[e]tails|similar to T[a]sk' docs/superpowers/plans/2026-08-04-portable-route-credential-import.md
rg -n 'println!|eprintln!|tracing|log::|console\.(log|error)|localStorage|sessionStorage|fingerprint|api_key|access_token|refresh_token' src-tauri/src/services/route_credential_transfer_import_service.rs src-tauri/src/commands/route_credential_transfer_commands.rs src-tauri/src/web src/components/accounts/RouteCredentialImportDialog.tsx tests/RouteCredentialImportDialog.test.tsx
```

Expected: no plan placeholders; no secret logging/persistence; no duplicate shared model, migration, origin repository, codec, export/save implementation, export helper/dialog, TLS configuration, or `AccountsScreen` layout work.

---

## Execution Handoff

Execute `docs/superpowers/plans/2026-08-04-secure-route-credential-export.md` first, then this import plan, then `docs/superpowers/plans/2026-08-04-account-desktop-workspace-layout.md`. Each later plan consumes the earlier artifacts without redefining them.
