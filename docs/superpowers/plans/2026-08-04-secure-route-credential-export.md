# Secure Route Credential Export Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish the shared portable-transfer foundation and deliver secure export of selected official and CPA API route credentials as AI Switch migration JSON, AI Switch scheme links, and native/Web downloads.

**Architecture:** This plan owns shared transfer DTOs/constants, installation/source-origin persistence, CPA projection and canonical fingerprint primitives, AI Switch link serialization, batched export orchestration, export/save transport commands, frontend export helpers, and the export dialog. The companion import plan consumes these contracts and primitives for parsing and transactional import. Account workspace layout, compact rows, fixed toolbars/status bars, pool health presentation, import commands, and the import dialog are outside this plan.

**Tech Stack:** Rust 2021, Tauri 2, Axum 0.7, SQLite/sqlx 0.8, serde/serde_json, sha2, url, uuid, React 18, TypeScript, Vitest, Testing Library.

## Global Constraints

- The copied and saved migration document is a UTF-8 bare JSON array with two-space indentation and one trailing newline; never add an `accounts` wrapper.
- The shared format constants are `TRANSFER_FORMAT = "ai-switch.route-credential"` and `TRANSFER_SCHEMA_VERSION = 1`.
- Each item always contains the minimum `x-ai-switch` core: `format`, `schema_version`, `platform`, and `kind`; API items also contain `cpa_section`.
- Exporters always emit `source_instance_id` and `source_credential_id` in `x-ai-switch`, even when `include_enhanced_metadata` is `false`; the enhancement toggle controls only optional display, batch, pool, and AI Switch-specific recovery fields. Import remains backward-compatible with files that omit either source ID.
- Supported CPA API sections are `claude-api-key`, `gemini-api-key`, `codex-api-key`, `xai-api-key`, and `openai-compatibility`; shared validation identifies `interactions-api-key`, `vertex-api-key`, and top-level `api-keys` as unsupported.
- API items project CPA-native entry fields and never add a custom top-level `type`; legacy API `type` is an import-only compatibility discriminator owned by the companion plan.
- Export selection is validated against one `platform + pool_scope`; duplicate IDs are removed server-side, and missing or out-of-context IDs are fatal.
- Export accepts at most `2000` IDs and emits at most `8 MiB`; shared import constants also define at most `2000` items and `256 KiB` per serialized item.
- Blocking export errors produce `json_text: null`; the service never silently drops an invalid selected account to return a partial array.
- Full JSON and scheme URLs are returned only in the explicit export result requested by the user. Issues, errors, logs, Toasts, telemetry, persistent browser storage, and test snapshots never include tokens, API keys, complete scheme URLs, query strings, raw items, or fingerprints.
- Current normalized secret/config allowlists override stale trusted CPA raw fields. Empty current values remove both snake_case and camelCase aliases so expired tokens cannot reappear.
- Source identity is `(source_instance_id, source_credential_id, platform, kind)`. The shared origins table stores immutable import-time fingerprints; local edits and token refreshes do not update them.
- `save_route_credential_export` is Tauri-only, accepts no arbitrary path, and atomically writes the exact `json_text` already shown in the dialog.
- This plan does not implement import parsing, preview, commit, batch restoration, pool restoration, workspace layout, account-row redesign, or import UI.
- Work directly on `main`; do not create branches, worktrees, or commits.

---

### Task 1: Define Shared Transfer DTOs, Constants, and Persistence Schema

**Files:**
- Create: `src-tauri/src/models/route_credential_transfer.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Create: `src-tauri/migrations/202608040001_route_credential_transfer.sql`
- Create: `src-tauri/src/database/repositories/route_credential_transfer_repository.rs`
- Modify: `src-tauri/src/database/repositories/mod.rs`
- Modify: `src/lib/api/types.ts`
- Test: `src-tauri/src/models/route_credential_transfer.rs`
- Test: `src-tauri/src/database/repositories/route_credential_transfer_repository.rs`

**Interfaces:**

```rust
pub const TRANSFER_FORMAT: &str = "ai-switch.route-credential";
pub const TRANSFER_SCHEMA_VERSION: u32 = 1;
pub const TRANSFER_MAX_BYTES: usize = 8 * 1024 * 1024;
pub const TRANSFER_MAX_ITEMS: usize = 2_000;
pub const TRANSFER_MAX_ITEM_BYTES: usize = 256 * 1024;
pub const TRANSFER_MAX_EXPORT_IDS: usize = 2_000;

fn default_true() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialSelectionContext {
    pub platform: String,
    pub pool_scope: RouteCredentialPoolScope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExportRouteCredentialsInput {
    pub selection_context: RouteCredentialSelectionContext,
    pub credential_ids: Vec<String>,
    #[serde(default = "default_true")]
    pub include_enhanced_metadata: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransferPlatformChoice {
  pub item_index: usize,
  pub platform: String,
  #[serde(default)]
  pub interface_format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialTransferIssue {
    pub item_index: Option<usize>,
    pub display_name: Option<String>,
    pub code: String,
    pub field: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RouteCredentialExportCounts {
    pub total: usize,
    pub official: usize,
    pub api: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialSchemeLink {
    pub credential_id: String,
    pub display_name: String,
    pub url: Option<String>,
    pub issue_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialExportResult {
    pub json_text: Option<String>,
    pub suggested_file_name: String,
    pub counts: RouteCredentialExportCounts,
    pub scheme_links: Vec<RouteCredentialSchemeLink>,
    pub warnings: Vec<RouteCredentialTransferIssue>,
    pub errors: Vec<RouteCredentialTransferIssue>,
}
```

The same model file also defines the companion import plan's transport DTOs so Rust and TypeScript have one source of naming:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreviewRouteCredentialImportInput {
    pub text: String,
    #[serde(default)]
    pub ambiguous_platform_choices: Vec<TransferPlatformChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialImportPreviewItem {
    pub item_index: usize,
    pub display_name_masked: String,
    pub platform: Option<String>,
    pub kind: Option<String>,
    pub cpa_section: Option<String>,
    pub disposition: String,
    pub issue_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RouteCredentialImportPreviewCounts {
    pub total: usize,
    pub official: usize,
    pub api: usize,
    pub importable: usize,
    pub duplicates: usize,
    pub conflicts: usize,
    pub errors: usize,
    pub restorable_pool_count: usize,
    pub batch_count: usize,
    pub platform_counts: std::collections::BTreeMap<String, usize>,
    pub cpa_section_counts: std::collections::BTreeMap<String, usize>,
    pub legacy_type_counts: std::collections::BTreeMap<String, usize>,
    pub restorable_pool_counts: std::collections::BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialImportPreview {
    pub counts: RouteCredentialImportPreviewCounts,
    pub items: Vec<RouteCredentialImportPreviewItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportRouteCredentialsInput {
    pub text: String,
    #[serde(default)]
    pub ambiguous_platform_choices: Vec<TransferPlatformChoice>,
    #[serde(default)]
    pub restore_pool_membership: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RouteCredentialImportOutcome {
    pub imported: usize,
    pub skipped_duplicates: usize,
    pub conflicts: usize,
    pub failed: usize,
    pub restored_pool_members: usize,
}
```

TypeScript uses the same names and snake-case fields:

```ts
export type TransferPlatformChoice = {
  item_index: number;
  platform: string;
  interface_format?: string | null;
};

export type RouteCredentialSelectionContext = {
  platform: string;
  pool_scope: RouteCredentialPoolScope;
};

export type ExportRouteCredentialsInput = {
  selection_context: RouteCredentialSelectionContext;
  credential_ids: string[];
  include_enhanced_metadata?: boolean;
};

export type RouteCredentialTransferIssue = {
  item_index?: number | null;
  display_name?: string | null;
  code: string;
  field?: string | null;
};

export type RouteCredentialExportCounts = {
  total: number;
  official: number;
  api: number;
};

export type RouteCredentialSchemeLink = {
  credential_id: string;
  display_name: string;
  url?: string | null;
  issue_code?: string | null;
};

export type RouteCredentialExportResult = {
  json_text: string | null;
  suggested_file_name: string;
  counts: RouteCredentialExportCounts;
  scheme_links: RouteCredentialSchemeLink[];
  warnings: RouteCredentialTransferIssue[];
  errors: RouteCredentialTransferIssue[];
};

export type PreviewRouteCredentialImportInput = {
  text: string;
  ambiguous_platform_choices: TransferPlatformChoice[];
};

export type RouteCredentialImportPreviewItem = {
  item_index: number;
  display_name_masked: string;
  platform?: string | null;
  kind?: string | null;
  cpa_section?: string | null;
  disposition: string;
  issue_codes: string[];
};

export type RouteCredentialImportPreviewCounts = {
  total: number;
  official: number;
  api: number;
  importable: number;
  duplicates: number;
  conflicts: number;
  errors: number;
  restorable_pool_count: number;
  batch_count: number;
  platform_counts: Record<string, number>;
  cpa_section_counts: Record<string, number>;
  legacy_type_counts: Record<string, number>;
  restorable_pool_counts: Record<string, number>;
};

export type RouteCredentialImportPreview = {
  counts: RouteCredentialImportPreviewCounts;
  items: RouteCredentialImportPreviewItem[];
};

export type ImportRouteCredentialsInput = {
  text: string;
  ambiguous_platform_choices: TransferPlatformChoice[];
  restore_pool_membership: boolean;
};

export type RouteCredentialImportOutcome = {
  imported: number;
  skipped_duplicates: number;
  conflicts: number;
  failed: number;
  restored_pool_members: number;
};
```

The companion import plan consumes these aliases and adds only its two client functions; it must not redefine them. `include_enhanced_metadata?: boolean` is optional because Rust defaults it to `true`; callers in this plan pass it explicitly.

- [ ] **Step 1: Write failing DTO and redaction tests**

Serialize every DTO and assert exact snake-case names, default `include_enhanced_metadata = true`, structured choices containing both platform and interface format, masked preview naming, mandatory source IDs in exported metadata regardless of the enhancement flag, and absence of `secret_payload_json`, `config_json`, `api_key`, `access_token`, `refresh_token`, and `fingerprint` properties outside explicit export content.

- [ ] **Step 2: Add the transfer migration**

```sql
CREATE TABLE transfer_installation_identity (
  singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
  instance_id TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL
);

CREATE TABLE route_credential_transfer_origins (
  route_credential_id TEXT PRIMARY KEY REFERENCES route_credentials(id) ON DELETE CASCADE,
  source_instance_id TEXT NOT NULL,
  source_credential_id TEXT NOT NULL,
  source_platform TEXT NOT NULL,
  source_kind TEXT NOT NULL,
  source_schema_version INTEGER NOT NULL,
  source_fingerprint TEXT NOT NULL,
  imported_at TEXT NOT NULL,
  UNIQUE(source_instance_id, source_credential_id, source_platform, source_kind)
);

CREATE INDEX idx_transfer_origins_fingerprint
  ON route_credential_transfer_origins(source_fingerprint);
```

Do not add another pool-member uniqueness migration; the current database already enforces `(platform, route_credential_id)` uniqueness.

- [ ] **Step 3: Implement the shared origin repository**

```rust
#[derive(Debug, Clone, sqlx::FromRow, PartialEq, Eq)]
pub struct TransferOrigin {
    pub route_credential_id: String,
    pub source_instance_id: String,
    pub source_credential_id: String,
    pub source_platform: String,
    pub source_kind: String,
    pub source_schema_version: i64,
    pub source_fingerprint: String,
    pub imported_at: String,
}

pub async fn get_or_create_installation_id(pool: &SqlitePool) -> Result<String, AppError>;

pub async fn find_origin_by_identity(
    pool: &SqlitePool,
    source_instance_id: &str,
    source_credential_id: &str,
    platform: &str,
    kind: &str,
) -> Result<Option<TransferOrigin>, AppError>;

pub async fn find_origin_by_identity_tx(
    tx: &mut Transaction<'_, Sqlite>,
    source_instance_id: &str,
    source_credential_id: &str,
    platform: &str,
    kind: &str,
) -> Result<Option<TransferOrigin>, AppError>;

pub async fn insert_origin_tx(
    tx: &mut Transaction<'_, Sqlite>,
    origin: &TransferOrigin,
) -> Result<(), AppError>;
```

Generate the installation UUID with `INSERT ... ON CONFLICT(singleton) DO NOTHING`, then read the stored row. The transaction methods never begin or commit a transaction.

- [ ] **Step 4: Run focused shared-foundation tests**

Run: `cargo test route_credential_transfer --manifest-path src-tauri/Cargo.toml`

Expected: DTO contracts pass, both tables exist, installation identity is stable, duplicate source identity is rejected, and deleting a credential cascades its origin mapping.

---

### Task 2: Implement Pure CPA Projection and Canonical Fingerprint Primitives

**Files:**
- Create: `src-tauri/src/services/cpa_export_service.rs`
- Create: `src-tauri/src/services/route_credential_transfer_codec.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Modify: `src-tauri/src/services/official_agent_identity_service.rs`
- Test: `src-tauri/src/services/cpa_export_service.rs`
- Test: `src-tauri/src/services/route_credential_transfer_codec.rs`

**Interfaces:**

```rust
pub struct ProjectedCredential {
    pub payload: serde_json::Value,
    pub cpa_section: Option<String>,
    pub origin_format: String,
    pub warnings: Vec<RouteCredentialTransferIssue>,
}

pub fn project_credential(
    credential: &RouteCredential,
    instance_id: &str,
    in_pool: bool,
    include_enhanced_metadata: bool,
) -> Result<ProjectedCredential, RouteCredentialTransferIssue>;

pub fn classify_api_section(
    platform: PlatformId,
    dialect: ApiDialect,
    base_url: &str,
) -> Result<&'static str, RouteCredentialTransferIssue>;

pub fn canonical_json(value: &serde_json::Value) -> Result<String, AppError>;

pub fn canonical_fingerprint(
    kind: &str,
    projected_without_metadata: &serde_json::Value,
) -> Result<String, AppError>;

pub fn trusted_cpa_raw_template(
    platform: &str,
    config: &serde_json::Value,
) -> bool;
```

- [ ] **Step 1: Write failing official projection tests**

Use synthetic fixtures to assert current secret/config values replace stale trusted raw tokens, current empty fields delete snake/camel aliases, `client_id` is preserved when present and removed when currently empty, CPA wrapper and Sub2API nested fields flatten to the Auth-File top level, Grok type becomes `xai`, ordinary OAuth requires access or refresh token, and Agent Identity requires private key, runtime, task, and account/workspace identity.

- [ ] **Step 2: Write failing API projection tests**

Assert these exact mappings:

```text
anthropic | anthropic-messages -> claude-api-key
gemini                         -> gemini-api-key
openai-responses               -> codex-api-key
grok + openai + api.x.ai       -> xai-api-key
other openai endpoints         -> openai-compatibility
```

Assert API entries have no custom top-level `type`, OpenAI compatibility contains exactly one `api-key-entries` item, and model fields map `to -> name`, `from -> alias`, `label -> display-name`, and `supports_1m: true -> max-context-length: 1048576`.

- [ ] **Step 3: Implement the trusted-raw predicate and explicit allowlists**

`trusted_cpa_raw_template(platform, config)` returns true only when `config_json` is an object with `raw` as an object, `raw_type` is a recognized CPA/Auth-File type for `platform`, `import_format` is absent or one of `cpa`/`auth-file`, and the stored metadata does not identify the source as `sub2api` or an arbitrary provider. Only a true result permits unknown top-level raw fields to be copied; otherwise export rebuilds from explicit fields and emits a safe warning when unknown raw data is discarded. Official secret allowlist includes `client_id` in addition to `id_token`, `access_token`, `refresh_token`, `account_id`, `workspace_id`, `chatgpt_account_id`, `agent_runtime_id`, `agent_private_key`, `task_id`, `auth_mode`, and `chatgpt_account_is_fedramp`. Config allowlist includes `client_id` alongside `last_refresh`, `expired`, `expires_in`, `disabled`, `base_url`, `token_endpoint`, `auth_kind`, `sub`, `token_type`, `redirect_uri`, and trusted `headers`. API secret allowlist is only `api_key`. Unknown nonempty secret fields are fatal; unknown non-secret API config fields are ignored with a safe issue code. Issue construction receives field names and display metadata, never secret values.

Add tests for `trusted_cpa_raw_template("codex", ...) == true` for a CPA parser payload, false for `import_format = "sub2api"`, false for an unrecognized `raw_type`, and false when `raw` is not an object.

- [ ] **Step 4: Implement kind-aware canonical fingerprints**

Recursively sort object keys and stable-sort normalized headers/model mappings. Normalize endpoints by lowercase scheme/host, remove default ports, and trim trailing slash. API fingerprints include API key, normalized endpoint, dialect, headers, mappings, and compatibility flags. Refresh-token OAuth fingerprints include refresh token, account/workspace identity, authentication endpoint, and mode while excluding access/id token, expiry, and last refresh. Access-token-only fingerprints include the access token. Agent Identity fingerprints include private key, runtime, task, and account/workspace identity. Remove `x-ai-switch`, display/batch/pool metadata, quota, cooldown, failure, and request statistics before hashing with SHA-256.

- [ ] **Step 5: Run projection and codec tests**

Run: `cargo test cpa_export_service --manifest-path src-tauri/Cargo.toml` and `cargo test route_credential_transfer_codec --manifest-path src-tauri/Cargo.toml`.

Expected: all official/API projections and byte-stable kind-aware fingerprints pass, and no error serialization contains a synthetic secret.

---

### Task 3: Add Lossless AI Switch Scheme Serialization

**Files:**
- Modify: `src-tauri/src/services/deeplink_service.rs`
- Test: `src-tauri/src/services/deeplink_service.rs`

**Interfaces:**

```rust
pub struct DeepLinkBuildInput<'a> {
    pub platform: &'a str,
    pub display_name: &'a str,
    pub base_url: &'a str,
    pub api_key: &'a str,
    pub interface_format: &'a str,
    pub model_mappings: &'a [ModelMapping],
    pub headers: &'a serde_json::Value,
    pub api_key_field: Option<&'a str>,
    pub responses_custom_tool_compat: bool,
}

pub fn build_aiswitch_import_url(
    input: &DeepLinkBuildInput<'_>,
) -> Result<String, String>;
```

- [ ] **Step 1: Write failing round-trip tests**

Build one link for each supported existing v1 platform (`codex`, `claude`, `gemini`, `grok`), parse it through `parse_deeplink_url`, and assert platform, name, endpoint, key, and expressible model fields round-trip.

- [ ] **Step 2: Write failing lossy-case tests**

Reject headers, `api_key_field`, custom-tool compatibility, multiple ordinary mappings, `supports_1m`, unsupported platform, nondefault dialect, non-HTTP(S) endpoint, and Claude mappings that cannot map to haiku/sonnet/opus query parameters. Assert returned strings contain only safe reason codes and never the key or complete URL.

- [ ] **Step 3: Implement with `url::Url`**

Construct `aiswitch://v1/import?resource=provider` with `query_pairs_mut`; reuse the existing parser's platform/dialect aliases. Never format or log the completed URL outside the explicit result.

- [ ] **Step 4: Run deeplink tests**

Run: `cargo test deeplink_service --manifest-path src-tauri/Cargo.toml`.

Expected: supported links round-trip and every lossy case returns a safe failure without emitting a partial link.

---

### Task 4: Add Batched Export Repository Reads

**Files:**
- Modify: `src-tauri/src/database/repositories/route_credential_repository.rs`
- Modify: `src-tauri/src/database/repositories/route_pool_repository.rs`
- Test: `src-tauri/src/database/repositories/route_credential_repository.rs`
- Test: `src-tauri/src/database/repositories/route_pool_repository.rs`

**Interfaces:**

```rust
pub async fn list_by_ids(
    pool: &SqlitePool,
    ids: &[String],
    selection: &RouteCredentialSelectionContext,
) -> Result<Vec<RouteCredential>, AppError>;

pub async fn pool_membership_map(
    pool: &SqlitePool,
    platform: &str,
    ids: &[String],
) -> Result<std::collections::HashSet<String>, AppError>;
```

- [ ] **Step 1: Write failing batched-read tests**

Create credentials across two platforms and both pool scopes. Assert duplicate input IDs are bound once, selected-platform rows return in `sort_order ASC, created_at DESC, id ASC`, `in_pool` uses an enabled-member `EXISTS`, `out_of_pool` uses its negation, and the membership map is based on the full selected set rather than the visible page.

- [ ] **Step 2: Implement one query per repository operation**

Use `QueryBuilder<Sqlite>` with bound IDs; do not loop over `RouteCredentialRepository::get`. The repository returns matching rows only. Export orchestration compares requested unique IDs with returned IDs to produce fatal missing/out-of-context issues.

- [ ] **Step 3: Run repository tests**

Run: `cargo test route_credential_repository --manifest-path src-tauri/Cargo.toml` and `cargo test route_pool_repository --manifest-path src-tauri/Cargo.toml`.

Expected: stable, scope-aware batched reads pass with no N+1 credential lookup.

---

### Task 5: Implement All-or-Nothing Export Orchestration

**Files:**
- Create: `src-tauri/src/services/route_credential_transfer_service.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Test: `src-tauri/src/services/route_credential_transfer_service.rs`

**Interfaces:**

```rust
pub async fn export_route_credentials(
    pool: &SqlitePool,
    input: ExportRouteCredentialsInput,
) -> Result<RouteCredentialExportResult, AppError>;

pub fn suggested_export_file_name(
    platform: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> String;
```

- [ ] **Step 1: Write failing orchestration tests**

Cover empty selection, more than `2000` IDs, duplicate IDs, missing ID, wrong platform, wrong pool scope, official/API counts, enhanced metadata on/off, stable ordering, blocking projection errors, warnings, link eligibility, filename format, pretty formatting, trailing newline, and output larger than `8 MiB`.

- [ ] **Step 2: Validate selection before serialization**

Parse `PlatformId`, deduplicate IDs while preserving the first occurrence for validation, batch-load credentials and membership, compare the complete requested set with loaded IDs, and return safe issues for every missing or context-invalid ID. If any error exists, return `json_text: None` and do not project a partial array.

- [ ] **Step 3: Project and serialize once**

Load the stable installation UUID, call `project_credential` for each ordered credential, collect counts and warnings, serialize one `Vec<Value>` with `serde_json::to_string_pretty`, append exactly one newline, and enforce `TRANSFER_MAX_BYTES`. The serializer must write this exact metadata core for every item:

```json
"x-ai-switch": {
  "format": "ai-switch.route-credential",
  "schema_version": 1,
  "source_instance_id": "installation-uuid",
  "source_credential_id": "credential-id",
  "platform": "codex",
  "kind": "official"
}
```

API items add `cpa_section`. `source_instance_id` and `source_credential_id` are always emitted for exports; the enhanced flag controls only display/batch/pool/interface/model metadata and other optional recovery fields.

- [ ] **Step 4: Generate scheme results without weakening JSON export**

Call `build_aiswitch_import_url` for API credentials. Store one `RouteCredentialSchemeLink` per API account with `url: None` and `issue_code` when the link would be lossy. Official accounts receive no scheme-link row. Link failure remains a warning and never removes the complete migration JSON.

- [ ] **Step 5: Run export service tests**

Run: `cargo test route_credential_transfer_service --manifest-path src-tauri/Cargo.toml`.

Expected: valid selections return one deterministic bare array and optional links; invalid selections return no JSON; errors and warnings remain redacted.

---

### Task 6: Expose Export and Desktop-Only Atomic Save Commands

**Files:**
- Create: `src-tauri/src/commands/route_credential_transfer_commands.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/web/handlers/mod.rs`
- Modify: `src-tauri/src/web/router.rs`
- Modify: `src-tauri/src/web/auth.rs`
- Modify: `src-tauri/src/server.rs`
- Modify: `src-tauri/src/services/web_service.rs`
- Modify: `src-tauri/capabilities/default.json`
- Modify: `src/lib/api/types.ts`
- Modify: `src/lib/api/client.ts`
- Modify: `src/lib/api/commandSupport.ts`
- Modify: `src/lib/transport/web-transport.ts`
- Modify: `src/components/settings/web-service-settings.tsx`
- Test: `src-tauri/src/commands/route_credential_transfer_commands.rs`
- Test: `src-tauri/src/web/handlers/mod.rs`
- Test: `src-tauri/src/web/router.rs`
- Test: `tests/transport/command-contract.test.ts`
- Test: `tests/transport/transport.test.ts`
- Test: `tests/SettingsScreen.test.tsx`

**Interfaces:**

```rust
#[tauri::command]
pub async fn export_route_credentials(
    state: State<'_, AppState>,
    input: ExportRouteCredentialsInput,
) -> Result<RouteCredentialExportResult, ApiError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SaveRouteCredentialExportResult {
    pub cancelled: bool,
    pub file_name: Option<String>,
}

#[tauri::command]
pub async fn save_route_credential_export(
    app: tauri::AppHandle,
    suggested_file_name: String,
    json_text: String,
) -> Result<SaveRouteCredentialExportResult, ApiError>;

pub fn is_loopback_host(host: &str) -> bool;
pub fn validate_sensitive_web_transport(host: &str, tls_enabled: bool) -> Result<(), AppError>;
```

TypeScript clients:

```ts
export type SaveRouteCredentialExportResult = {
  cancelled: boolean;
  file_name?: string | null;
};

export function exportRouteCredentials(
  input: ExportRouteCredentialsInput,
): Promise<RouteCredentialExportResult>;

export function saveRouteCredentialExport(input: {
  suggested_file_name: string;
  json_text: string;
}): Promise<SaveRouteCredentialExportResult>;
```

- [ ] **Step 1: Write failing command-contract tests**

Assert `export_route_credentials` exists in Tauri and Web dispatch, `save_route_credential_export` exists only in Tauri and `desktopOnlyCommands`, both use exact snake-case args, save cancellation is a success result, and Web cannot dispatch the save command. The companion import plan may append `preview_route_credential_import` and `import_route_credentials` to this same command module and dispatch table, but must not create a second transfer command module or alter export/save argument names.

- [ ] **Step 2: Enforce transport authentication and TLS ownership**

In `src-tauri/src/web/router.rs`, add a `middleware::from_fn_with_state` authorization layer for `/api/:command` so `Authorization` is checked before the handler's `Json<Value>` extractor; apply a route body limit of at least `12 MiB`, return `Cache-Control: no-store` and `Pragma: no-cache` for export success and error responses, and prohibit request/response body logging. In `src-tauri/src/server.rs`, implement `is_loopback_host` and call `validate_sensitive_web_transport` before binding. Read standalone TLS paths from `AI_SWITCH_TLS_CERT_PATH` and `AI_SWITCH_TLS_KEY_PATH`; both must be present or both absent, and a non-loopback bind without both paths fails before opening the listener. Use `axum_server::bind_rustls` when both are present and report only HTTPS startup details.

In `src-tauri/src/services/web_service.rs`, add serde-default `tls_enabled`, `tls_cert_path`, and `tls_key_path` fields to `WebServiceConfig`, normalize them, require both nonempty paths when TLS is enabled, call the same validator before `WebService::start`, use `axum_server::bind_rustls` for that branch, and report an `https://` base URL only for TLS. Mirror them in TypeScript as `tlsEnabled?: boolean`, `tlsCertPath?: string | null`, and `tlsKeyPath?: string | null`. Update `defaultConfig` and `normalizeConfig` in `web-service-settings.tsx` so loading and saving existing advanced TLS configuration preserves all three fields even though this feature does not add certificate-path controls to the Settings UI. A Tailscale Funnel may expose an HTTPS public URL, but the local sensitive route is still gated by the configured local transport; private tailnet/LAN HTTP without TLS must not register transfer routes. Add tests for loopback HTTP acceptance, non-loopback HTTP rejection, one-path configuration rejection, configured Rustls acceptance, settings round-trip preservation, and public exposure without a secure local listener. Do not weaken existing route-proxy HTTPS controls.

- [ ] **Step 3: Implement native save with the installed dialog API**

Use `app.dialog().file().set_file_name(...).add_filter("JSON", &["json"]).blocking_save_file()`. Treat `None` as `{ cancelled: true, file_name: None }`; normalize the `.json` suffix; revalidate UTF-8, top-level array, `TRANSFER_MAX_BYTES`, and `TRANSFER_MAX_ITEMS`; then call `ConfigWriter::write_atomic(path, &json_text)`. Add `dialog:allow-save` to the Tauri capability and never accept a frontend path.

- [ ] **Step 4: Run command and transport tests**

Run: `cargo test route_credential_transfer_commands --manifest-path src-tauri/Cargo.toml`, `cargo test web::router --manifest-path src-tauri/Cargo.toml`, `cargo test server --manifest-path src-tauri/Cargo.toml`, `cargo test web_service --manifest-path src-tauri/Cargo.toml`, and `pnpm vitest run tests/transport/command-contract.test.ts tests/transport/transport.test.ts`.

Expected: authenticated export works in both transports, sensitive responses are non-cacheable, native save is atomic and Desktop-only, and Web exposes no arbitrary filesystem write.

---

### Task 7: Add Export Helpers and the Sensitive Export Dialog

**Files:**
- Create: `src/lib/routeCredentialTransfer.ts`
- Create: `src/components/accounts/RouteCredentialExportDialog.tsx`
- Modify: `src/lib/api/client.ts`
- Modify: `src/lib/api/types.ts`
- Test: `tests/lib/routeCredentialTransfer.test.ts`
- Test: `tests/RouteCredentialExportDialog.test.tsx`

**Interfaces:**

```ts
export function downloadRouteCredentialJson(
  jsonText: string,
  fileName: string,
): void;

export async function copySensitiveText(text: string): Promise<void>;

export type RouteCredentialExportDialogProps = {
  open: boolean;
  selection_context: RouteCredentialSelectionContext;
  credential_ids: string[];
  onClose: () => void;
};
```

- [ ] **Step 1: Write failing helper tests**

Assert Web download creates `Blob([jsonText], { type: "application/json" })`, uses the suggested filename, clicks one temporary anchor, removes it, and revokes the object URL in `finally`. Assert copy invokes `navigator.clipboard.writeText` exactly once and rejection leaves the source string available to the dialog.

- [ ] **Step 2: Write failing dialog tests**

Assert opening calls `exportRouteCredentials` with all selected IDs and `include_enhanced_metadata: true`; changing the metadata toggle regenerates once with the same IDs/context; tabs display migration JSON and scheme links; blocking errors disable copy/save/download; warnings show safe names/codes; copy and save receive the identical returned `json_text`; closing/unmounting clears JSON and links.

- [ ] **Step 3: Implement ephemeral export state**

Keep `RouteCredentialExportResult` in component-local state rather than React Query. Never write JSON or links to `localStorage`, session storage, URL state, Toast text, analytics, or logs. Show an explicit warning that the content contains credentials and warn again before copying scheme URLs because API keys enter the system clipboard.

- [ ] **Step 4: Preserve the caller-supplied selection contract**

Treat `selection_context` and `credential_ids` as immutable dialog inputs for one export generation. Pass every supplied ID to `exportRouteCredentials`; do not inspect visible account rows, derive pool scope, reorder IDs in React, or mutate selection state. The separate account workspace plan owns creating the icon action and supplying its complete cross-page selection.

- [ ] **Step 5: Implement Desktop save and Web download**

On Desktop call `saveRouteCredentialExport` with `result.suggested_file_name` and the exact non-null `result.json_text`. On Web call `downloadRouteCredentialJson` with the same two values. Do not invoke export a second time for either action.

- [ ] **Step 6: Run focused frontend tests**

Run: `pnpm vitest run tests/lib/routeCredentialTransfer.test.ts tests/RouteCredentialExportDialog.test.tsx`.

Expected: all supplied IDs, metadata regeneration, JSON/link tabs, copy, native save, Web download, blocking errors, and sensitive-state clearing pass without account-screen, layout, or import changes.

---

### Task 8: Validate Shared Contracts and Export Security

**Files:**
- Test: `src-tauri/src/models/route_credential_transfer.rs`
- Test: `src-tauri/src/database/repositories/route_credential_transfer_repository.rs`
- Test: `src-tauri/src/services/cpa_export_service.rs`
- Test: `src-tauri/src/services/route_credential_transfer_codec.rs`
- Test: `src-tauri/src/services/deeplink_service.rs`
- Test: `src-tauri/src/services/route_credential_transfer_service.rs`
- Test: `tests/RouteCredentialExportDialog.test.tsx`
- Test: `tests/transport/command-contract.test.ts`

**Interfaces:**
- No new production API is introduced in this task. The companion import plan consumes the DTOs, constants, codec, origins repository, and command module established here.

- [ ] **Step 1: Run the focused Rust suites**

Run:

```powershell
cargo test route_credential_transfer --manifest-path src-tauri/Cargo.toml
cargo test cpa_export_service --manifest-path src-tauri/Cargo.toml
cargo test route_credential_transfer_codec --manifest-path src-tauri/Cargo.toml
cargo test deeplink_service --manifest-path src-tauri/Cargo.toml
```

Expected: shared contracts, migration, projection, fingerprint, link, export, and save tests pass without compiler warnings.

- [ ] **Step 2: Run frontend and transport suites**

Run:

```powershell
pnpm vitest run tests/lib/routeCredentialTransfer.test.ts
pnpm vitest run tests/RouteCredentialExportDialog.test.tsx
pnpm vitest run tests/transport/command-contract.test.ts tests/transport/transport.test.ts
pnpm typecheck
```

Expected: exact DTO/client contracts pass, save is Desktop-only, and export state is ephemeral; account workflow and layout coverage remains owned by the separate workspace plan.

- [ ] **Step 3: Run the broad build checks**

Run: `pnpm rust:test`, then `pnpm test:run`, then `pnpm build`.

Expected: Rust, React, transport, and production build checks pass; unrelated failures are reported without expanding this plan's scope.

- [ ] **Step 4: Perform a redaction and ownership audit**

Run:

```powershell
rg -n 'println!|eprintln!|tracing|log::|console\.(log|error)|localStorage|sessionStorage|fingerprint|api_key|access_token|refresh_token' src-tauri/src/services/cpa_export_service.rs src-tauri/src/services/route_credential_transfer_codec.rs src-tauri/src/services/route_credential_transfer_service.rs src-tauri/src/commands/route_credential_transfer_commands.rs src-tauri/src/web src/components/accounts/RouteCredentialExportDialog.tsx src/lib/routeCredentialTransfer.ts tests/RouteCredentialExportDialog.test.tsx tests/lib/routeCredentialTransfer.test.ts
```

Expected: no secret logging or persistence; fingerprint occurrences are limited to internal codec/origin fields and synthetic assertions; no import parser, import transaction, import dialog, or workspace-layout implementation is created by this plan. The companion plan may extend the shared DTO/client/command files explicitly named above.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-04-secure-route-credential-export.md`. Execute it before the companion portable-import plan so that import work can consume the shared DTOs, constants, codec, origin repository, and transfer command module without redefining them.
