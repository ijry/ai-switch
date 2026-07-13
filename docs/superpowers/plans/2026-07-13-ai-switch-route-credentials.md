# AI Switch Route Credentials Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship unified `route_credentials` so agent tabs can create official CPA accounts and API accounts, put them in the route pool, and have the local proxy route with those credentials.

**Architecture:** Add `route_credentials` as the single source of truth for account rows, pool membership, and outbound proxy auth. Official creates parse CLIProxyAPI-style CPA JSON; API creates store key/base URL/interface format/model mappings plus editable preview drafts in DB. Route pool and proxy stop selecting from `official_accounts`/`providers` for this slice.

**Tech Stack:** Tauri 2, Rust, SQLite/sqlx, Axum proxy, React, TanStack Query, Vitest.

## Global Constraints

- Agent-first IA only: Codex / Claude / Gemini / OpenCode / OpenClaw / Hermes + Settings.
- No competitor product names in user-facing copy.
- Official create = CPA only; no email/password form.
- CPA accepts CLIProxyAPI single-auth JSON object/array + nested tokens + camelCase; rejects `{accounts:[{provider,email,credentials}]}`.
- API advanced: `interface_format` in `openai|openai-responses|anthropic|anthropic-messages|gemini`; mappings `[{from,to,label?}]`.
- Preview files: Codex `auth.json`+`config.toml`; Claude/Gemini `settings.json`; save to DB only.
- Real disk writes remain the explicit `write_route_proxy_configs` action.
- Secrets v1 plaintext in DB.
- No legacy migration from `official_accounts` / old pool rows.
- Do not touch unrelated dirty files.
- Prefer focused files and TDD with frequent commits.

## File Map

**Create**
- `src-tauri/migrations/202607130011_route_credentials.sql`
- `src-tauri/src/models/route_credential.rs`
- `src-tauri/src/database/repositories/route_credential_repository.rs`
- `src-tauri/src/services/cpa_import_service.rs`
- `src-tauri/src/services/route_preview_service.rs`
- `src-tauri/src/services/route_credential_service.rs`
- `src-tauri/src/commands/route_credential_commands.rs`
- `tests/routeCredentials.test.ts` (optional if UI coverage stays in AccountsScreen tests)

**Modify**
- `src-tauri/src/models/mod.rs`
- `src-tauri/src/models/route_pool.rs`
- `src-tauri/src/database/repositories/mod.rs`
- `src-tauri/src/database/repositories/route_pool_repository.rs`
- `src-tauri/src/services/mod.rs`
- `src-tauri/src/services/route_pool_service.rs`
- `src-tauri/src/services/route_proxy_service.rs`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/lib.rs`
- `src/lib/api/types.ts`
- `src/lib/api/client.ts`
- `src/screens/AccountsScreen.tsx`
- `tests/AccountsScreen.test.tsx`

**Leave alone for this slice**
- Old `official_accounts` CRUD can remain compiled but Accounts UI must stop using it for create/list/pool.
- Provider switch experiment code stays, but proxy routing no longer depends on providers.

---

### Task 1: Migration + Rust Models

**Files:**
- Create: `src-tauri/migrations/202607130011_route_credentials.sql`
- Create: `src-tauri/src/models/route_credential.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Modify: `src-tauri/src/models/route_pool.rs`

**Interfaces:**
- Produces table `route_credentials`
- Produces reshaped `route_pool_members.route_credential_id`
- Produces `usage_events.route_credential_id`
- Produces Rust types used by later tasks

- [ ] **Step 1: Write migration**

```sql
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS route_credentials (
  id TEXT PRIMARY KEY,
  platform TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('official', 'api')),
  display_name TEXT NOT NULL,
  email TEXT,
  status TEXT NOT NULL DEFAULT 'ok',
  sort_order INTEGER NOT NULL DEFAULT 0,
  batch_id TEXT,
  secret_payload_json TEXT NOT NULL DEFAULT '{}',
  config_json TEXT NOT NULL DEFAULT '{}',
  preview_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY(batch_id) REFERENCES batches(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_route_credentials_platform
  ON route_credentials(platform, kind, status);
CREATE INDEX IF NOT EXISTS idx_route_credentials_batch
  ON route_credentials(batch_id);

-- Rebuild pool members around credentials. No legacy data migration.
DROP TABLE IF EXISTS route_pool_members;
CREATE TABLE route_pool_members (
  id TEXT PRIMARY KEY,
  platform TEXT NOT NULL,
  route_credential_id TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(platform, route_credential_id),
  FOREIGN KEY(route_credential_id) REFERENCES route_credentials(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_route_pool_members_platform
  ON route_pool_members(platform, enabled);
CREATE INDEX IF NOT EXISTS idx_route_pool_members_credential
  ON route_pool_members(route_credential_id);

-- Additive usage column for credential-scoped stats.
ALTER TABLE usage_events ADD COLUMN route_credential_id TEXT;
CREATE INDEX IF NOT EXISTS idx_usage_events_route_credential
  ON usage_events(route_credential_id);
```

If SQLite complains about re-adding a column on reruns in tests, keep the migration additive and idempotent via the project’s existing migration runner assumptions (one-shot migrations).

- [ ] **Step 2: Add model file**

```rust
// src-tauri/src/models/route_credential.rs
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct RouteCredential {
    pub id: String,
    pub platform: String,
    pub kind: String,
    pub display_name: String,
    pub email: Option<String>,
    pub status: String,
    pub sort_order: i64,
    pub batch_id: Option<String>,
    pub secret_payload_json: String,
    pub config_json: String,
    pub preview_json: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateApiRouteCredentialInput {
    pub platform: String,
    pub display_name: String,
    pub api_key: String,
    pub base_url: String,
    pub interface_format: String,
    pub model_mappings_json: String, // JSON array
    pub preview_json: Option<String>,
    pub batch_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateRouteCredentialInput {
    pub display_name: String,
    pub email: Option<String>,
    pub status: String,
    pub secret_payload_json: String,
    pub config_json: String,
    pub preview_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportOfficialTextInput {
    pub platform: String,
    pub text: String,
    pub batch_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportOfficialFilesInput {
    pub platform: String,
    pub file_paths: Vec<String>,
    pub batch_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialImportFailure {
    pub label: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialImportResult {
    pub imported: Vec<RouteCredential>,
    pub failed: Vec<RouteCredentialImportFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelMapping {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub label: Option<String>,
}
```

- [ ] **Step 3: Export module and update pool model naming**

In `models/mod.rs` add `pub mod route_credential;`.

In `route_pool.rs`, keep public field names stable for UI where possible:
- `RoutePoolState.account_ids` continues to mean selected credential ids.
- `RoutePoolMemberAccount` remains `{ id, display_name }`.
- Add comments that ids are `route_credentials.id`.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/migrations/202607130011_route_credentials.sql src-tauri/src/models/route_credential.rs src-tauri/src/models/mod.rs src-tauri/src/models/route_pool.rs
git commit -m "feat: add route_credentials schema and models"
```

---

### Task 2: Credential Repository

**Files:**
- Create: `src-tauri/src/database/repositories/route_credential_repository.rs`
- Modify: `src-tauri/src/database/repositories/mod.rs`

**Interfaces:**
- Produces:
  - `RouteCredentialRepository::create`
  - `get`
  - `list_by_platform`
  - `update`
  - `delete`
  - `platform_of`

- [ ] **Step 1: Write failing repo unit test** inside the repo file under `#[cfg(test)]` using `create_memory_pool` + `run_migrations`.

```rust
#[tokio::test]
async fn create_and_list_api_credential() {
    let pool = crate::database::create_memory_pool().await.unwrap();
    crate::database::run_migrations(&pool).await.unwrap();
    let created = RouteCredentialRepository::create(
        &pool,
        "codex",
        "api",
        "Demo API",
        None,
        "ok",
        None,
        r#"{"api_key":"sk-test"}"#,
        r#"{"base_url":"https://example.com","interface_format":"openai","model_mappings":[]}"#,
        r#"{"auth_json":"{}","config_toml":""}"#,
    )
    .await
    .unwrap();
    let listed = RouteCredentialRepository::list_by_platform(&pool, "codex")
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, created.id);
    assert_eq!(listed[0].kind, "api");
}
```

- [ ] **Step 2: Run test to verify fail**

```bash
cargo test --manifest-path src-tauri/Cargo.toml create_and_list_api_credential -- --nocapture
```

Expected: compile/link failure because repository methods do not exist.

- [ ] **Step 3: Implement repository**

```rust
impl RouteCredentialRepository {
    pub async fn create(
        pool: &SqlitePool,
        platform: &str,
        kind: &str,
        display_name: &str,
        email: Option<String>,
        status: &str,
        batch_id: Option<String>,
        secret_payload_json: &str,
        config_json: &str,
        preview_json: &str,
    ) -> Result<RouteCredential, AppError> { /* insert + get */ }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<RouteCredential, AppError> { /* ... */ }

    pub async fn list_by_platform(
        pool: &SqlitePool,
        platform: &str,
    ) -> Result<Vec<RouteCredential>, AppError> {
        // ORDER BY sort_order ASC, created_at DESC
    }

    pub async fn update(
        pool: &SqlitePool,
        id: &str,
        input: &UpdateRouteCredentialInput,
    ) -> Result<RouteCredential, AppError> { /* ... */ }

    pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), AppError> { /* ... */ }

    pub async fn platform_of(pool: &SqlitePool, id: &str) -> Result<String, AppError> { /* ... */ }
}
```

Register in `repositories/mod.rs`.

- [ ] **Step 4: Run test to verify pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml create_and_list_api_credential -- --nocapture
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/database/repositories/route_credential_repository.rs src-tauri/src/database/repositories/mod.rs
git commit -m "feat: add route credential repository"
```

---

### Task 3: CPA Import Parser

**Files:**
- Create: `src-tauri/src/services/cpa_import_service.rs`
- Modify: `src-tauri/src/services/mod.rs`

**Interfaces:**
- Produces:
  - `parse_cpa_text(platform, text) -> Result<Vec<ParsedOfficialCredential>, AppError>`
  - `parse_cpa_file(platform, path, content) -> ...` or reuse text parser
  - `ParsedOfficialCredential { display_name, email, secret_payload_json, config_json }`

- [ ] **Step 1: Write failing parser tests**

```rust
#[test]
fn parses_cliproxyapi_codex_object() {
    let text = r#"{
      "type":"codex",
      "email":"a@example.com",
      "id_token":"id",
      "access_token":"at",
      "refresh_token":"rt_1",
      "account_id":"ac_1"
    }"#;
    let parsed = parse_cpa_text("codex", text).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].email.as_deref(), Some("a@example.com"));
    assert!(parsed[0].secret_payload_json.contains("rt_1"));
}

#[test]
fn parses_nested_and_camel_case_array() {
    let text = r#"[{
      "type":"claude",
      "email":"b@example.com",
      "tokens":{"accessToken":"sk-ant-oat01-x","refreshToken":"sk-ant-ort01-y"}
    }]"#;
    let parsed = parse_cpa_text("claude", text).unwrap();
    assert_eq!(parsed.len(), 1);
}

#[test]
fn rejects_accounts_wrapper_export() {
    let text = r#"{"accounts":[{"provider":"anthropic","email":"x@example.com","credentials":{"access_token":"a","refresh_token":"b"}}]}"#;
    let err = parse_cpa_text("claude", text).unwrap_err();
    assert!(format!("{err:?}").contains("cpa_wrapper_unsupported") || format!("{err}").contains("wrapper"));
}

#[test]
fn rejects_platform_mismatch() {
    let text = r#"{"type":"claude","access_token":"a","refresh_token":"b"}"#;
    assert!(parse_cpa_text("codex", text).is_err());
}
```

- [ ] **Step 2: Run tests to verify fail**

```bash
cargo test --manifest-path src-tauri/Cargo.toml parses_cliproxyapi_codex_object -- --nocapture
```

Expected: FAIL missing module/function.

- [ ] **Step 3: Implement parser**

Rules:
1. If top-level object has non-empty `accounts` array with objects containing `credentials`, reject with `validation.cpa_wrapper_unsupported`.
2. Accept object or array.
3. Read `type` or infer from platform only when tokens clearly match; still require current platform match when `type` present.
4. Token extraction order:
   - top-level `access_token`/`accessToken`
   - nested `tokens.access_token`/`tokens.accessToken`
   - same for refresh/id/account_id
5. Require `access_token` or `refresh_token`.
6. `display_name = email.unwrap_or("Official account")`.
7. Build:
   - secret: `{id_token,access_token,refresh_token,account_id}`
   - config: `{type,account_id,last_refresh,expired,raw_type}`

- [ ] **Step 4: Run tests to verify pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml cpa_import -- --nocapture
```

Expected: PASS for the four tests above.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/cpa_import_service.rs src-tauri/src/services/mod.rs
git commit -m "feat: parse CLIProxyAPI CPA credentials"
```

---

### Task 4: Preview Generator + Credential Service

**Files:**
- Create: `src-tauri/src/services/route_preview_service.rs`
- Create: `src-tauri/src/services/route_credential_service.rs`
- Modify: `src-tauri/src/services/mod.rs`

**Interfaces:**
- Produces:
  - `RoutePreviewService::generate(platform, kind, secret_json, config_json) -> String`
  - `RouteCredentialService::create_api`
  - `import_official_text`
  - `import_official_files`
  - `list`
  - `get`
  - `update`
  - `delete`

- [ ] **Step 1: Write failing service tests**

```rust
#[tokio::test]
async fn create_api_persists_preview_and_config() {
    // memory pool + migrations
    let cred = RouteCredentialService::create_api(&pool, CreateApiRouteCredentialInput {
        platform: "codex".into(),
        display_name: "API One".into(),
        api_key: "sk-1".into(),
        base_url: "https://api.example.com/v1".into(),
        interface_format: "openai-responses".into(),
        model_mappings_json: r#"[{"from":"gpt-5","to":"up-gpt"}]"#.into(),
        preview_json: None,
        batch_id: None,
    }).await.unwrap();
    assert_eq!(cred.kind, "api");
    assert!(cred.preview_json.contains("auth_json"));
    assert!(cred.config_json.contains("openai-responses"));
}

#[tokio::test]
async fn import_official_text_supports_partial_failure_via_files_helper() {
    // create temp dir with one good codex json and one bad json
    // call import_official_files
    // assert imported=1 failed=1
}
```

- [ ] **Step 2: Implement preview service**

```rust
pub fn generate(platform: &str, kind: &str, secret_payload_json: &str, config_json: &str) -> String {
    match platform {
        "codex" => {
            // auth_json from official tokens or api key
            // config_toml with model_provider placeholder / base_url note
            serde_json::json!({"auth_json": auth, "config_toml": toml}).to_string()
        }
        "claude" | "gemini" => {
            serde_json::json!({"settings_json": settings}).to_string()
        }
        _ => "{}".to_string(),
    }
}
```

Preview must be valid JSON object string. Content can be conservative but must include the required keys from the spec.

- [ ] **Step 3: Implement credential service validation**

API create validation:
- non-empty `display_name`, `api_key`, `base_url`
- `interface_format` in the 5 allowed values
- parse `model_mappings_json` as array of objects with non-empty `from`/`to`

Official import:
- normalize platform
- parse CPA
- optional batch create when `batch_name` present
- persist each parsed item with `kind=official`
- auto-generate preview when not provided
- files path: read each file, parse independently, collect failures

Update:
- allow secret/config/preview edits
- do not allow kind/platform changes

- [ ] **Step 4: Run tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml route_credential -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml route_preview -- --nocapture
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/route_preview_service.rs src-tauri/src/services/route_credential_service.rs src-tauri/src/services/mod.rs
git commit -m "feat: add route credential and preview services"
```

---

### Task 5: Tauri Commands for Credentials

**Files:**
- Create: `src-tauri/src/commands/route_credential_commands.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces commands:
  - `list_route_credentials(platform: String)`
  - `get_route_credential(id: String)`
  - `create_api_route_credential(input: CreateApiRouteCredentialInput)`
  - `import_official_route_credentials_from_text(input: ImportOfficialTextInput)`
  - `import_official_route_credentials_from_files(input: ImportOfficialFilesInput)`
  - `update_route_credential(id: String, input: UpdateRouteCredentialInput)`
  - `delete_route_credential(id: String)`

- [ ] **Step 1: Implement thin command wrappers** using `AppState.pool` and service methods.

- [ ] **Step 2: Register in `commands/mod.rs` and `lib.rs` invoke handler.**

- [ ] **Step 3: Compile check**

```bash
pnpm rust:check
```

Expected: success, or only pre-existing unrelated warnings.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/route_credential_commands.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat: expose route credential commands"
```

---

### Task 6: Route Pool Uses Credentials

**Files:**
- Modify: `src-tauri/src/database/repositories/route_pool_repository.rs`
- Modify: `src-tauri/src/services/route_pool_service.rs`

**Interfaces:**
- Consumes `route_credentials`
- Keeps command names `get_route_pool`, `set_route_pool_members`, `route_pool_route_once`
- `account_ids` now mean credential ids

- [ ] **Step 1: Update repository SQL**

Replace all `official_account_id` usage with `route_credential_id`.

`member_accounts` joins:

```sql
SELECT c.id, c.display_name
FROM route_pool_members rpm
INNER JOIN route_credentials c ON c.id = rpm.route_credential_id
WHERE rpm.platform = ? AND rpm.enabled = 1 AND c.status = 'ok'
ORDER BY rpm.sort_order ASC, rpm.created_at ASC
```

`account_platform` becomes credential platform lookup via `RouteCredentialRepository::platform_of` or local query.

`insert_usage_event` writes `route_credential_id` column (and may leave `official_account_id` null).

Stats queries filter by pool credential ids / `route_credential_id`.

- [ ] **Step 2: Update service validation**

When setting members:
- ensure each id exists in `route_credentials`
- platform matches
- reject `status != ok` with `validation.route_pool_credential_invalid`

- [ ] **Step 3: Fix/extend pool unit tests** in `route_pool_service.rs` to create credentials instead of official accounts.

- [ ] **Step 4: Run tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml route_pool -- --nocapture
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/database/repositories/route_pool_repository.rs src-tauri/src/services/route_pool_service.rs
git commit -m "feat: bind route pool to route credentials"
```

---

### Task 7: Proxy Routes With Credentials

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs`

**Interfaces:**
- Consumes pool members + credential secrets/config
- Stops selecting providers for outbound auth in this slice

- [ ] **Step 1: Replace provider selection with credential selection helpers**

```rust
struct SelectedCredential {
    id: String,
    platform: String,
    kind: String,
    display_name: String,
    secret_payload_json: String,
    config_json: String,
}

async fn load_pool_credentials(pool: &SqlitePool, platform: &str) -> Result<Vec<SelectedCredential>, AppError>;
fn pick_credential<'a>(items: &'a [SelectedCredential], cursor: i64) -> Option<&'a SelectedCredential>;
fn apply_model_mappings(body: &str, mappings: &[ModelMapping]) -> String;
fn build_upstream_request(...) -> Result<(String /*url*/, HeaderMap, Body), AppError>;
```

- [ ] **Step 2: Official outbound**
- Codex/Claude: use bearer `access_token` when present.
- If only refresh token exists, mark request failure with clear error in v1 (`route_credential.refresh_only_unsupported`) unless refresh helper already exists; do not invent OAuth refresh in this task unless trivial and already available.

- [ ] **Step 3: API outbound**
- Parse `base_url`, `interface_format`, `model_mappings`
- Auth header:
  - openai / openai-responses: `Authorization: Bearer <api_key>`
  - anthropic / anthropic-messages: `x-api-key: <api_key>` (+ anthropic version header if already used)
  - gemini: query `key=` or existing project style if code already has one; prefer header/query consistency with current gemini adapter if present
- Rewrite model fields in JSON body when mapping matches (`model`, and nested common locations if cheap)

- [ ] **Step 4: Empty pool / invalid credential behavior**
- Return JSON 502 body:
```json
{"error":{"code":"route_pool.empty","message":"No enabled route credentials in pool"}}
```

- [ ] **Step 5: Usage event**
- Insert with `route_credential_id`
- metric request always; token/cost best-effort from response JSON

- [ ] **Step 6: Unit tests**
- model mapping rewrite
- credential pick round-robin
- auth header selection by interface_format

```bash
cargo test --manifest-path src-tauri/Cargo.toml route_proxy -- --nocapture
```

Expected: PASS for new unit tests.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/services/route_proxy_service.rs
git commit -m "feat: route proxy uses pool credentials"
```

---

### Task 8: Frontend API Types + Client

**Files:**
- Modify: `src/lib/api/types.ts`
- Modify: `src/lib/api/client.ts`

**Interfaces:**
- Produces TS types and invoke wrappers matching Rust commands

- [ ] **Step 1: Add types**

```ts
export type RouteCredentialKind = "official" | "api";
export type InterfaceFormat =
  | "openai"
  | "openai-responses"
  | "anthropic"
  | "anthropic-messages"
  | "gemini";

export type ModelMapping = {
  from: string;
  to: string;
  label?: string | null;
};

export type RouteCredential = {
  id: string;
  platform: string;
  kind: RouteCredentialKind;
  display_name: string;
  email?: string | null;
  status: string;
  sort_order: number;
  batch_id?: string | null;
  secret_payload_json: string;
  config_json: string;
  preview_json: string;
  created_at: string;
  updated_at: string;
};

export type CreateApiRouteCredentialInput = {
  platform: string;
  display_name: string;
  api_key: string;
  base_url: string;
  interface_format: InterfaceFormat;
  model_mappings_json: string;
  preview_json?: string | null;
  batch_id?: string | null;
};

export type UpdateRouteCredentialInput = {
  display_name: string;
  email?: string | null;
  status: string;
  secret_payload_json: string;
  config_json: string;
  preview_json: string;
};

export type RouteCredentialImportResult = {
  imported: RouteCredential[];
  failed: { label: string; error: string }[];
};
```

Keep `RoutePoolState.account_ids` as credential ids.

- [ ] **Step 2: Add client functions**

```ts
export function listRouteCredentials(platform: string): Promise<RouteCredential[]> {
  return invoke("list_route_credentials", { platform });
}
export function createApiRouteCredential(input: CreateApiRouteCredentialInput) {
  return invoke<RouteCredential>("create_api_route_credential", { input });
}
export function importOfficialRouteCredentialsFromText(input: {
  platform: string;
  text: string;
  batch_name?: string | null;
}) {
  return invoke<RouteCredentialImportResult>("import_official_route_credentials_from_text", { input });
}
export function importOfficialRouteCredentialsFromFiles(input: {
  platform: string;
  file_paths: string[];
  batch_name?: string | null;
}) {
  return invoke<RouteCredentialImportResult>("import_official_route_credentials_from_files", { input });
}
export function updateRouteCredential(id: string, input: UpdateRouteCredentialInput) {
  return invoke<RouteCredential>("update_route_credential", { id, input });
}
export function deleteRouteCredential(id: string) {
  return invoke<void>("delete_route_credential", { id });
}
```

- [ ] **Step 3: Commit**

```bash
git add src/lib/api/types.ts src/lib/api/client.ts
git commit -m "feat: add route credential frontend API"
```

---

### Task 9: AccountsScreen Create/List/Pool UI

**Files:**
- Modify: `src/screens/AccountsScreen.tsx`
- Modify: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes credential list/import/create/update + pool/proxy APIs
- Stops using `createOfficialAccount` metadata form

- [ ] **Step 1: Rewrite data loading**

- Load credentials with `listRouteCredentials(platform)`
- Load pool with `getRoutePool(platform)`
- Group by `batch_id` when present; otherwise flat rows
- Show kind badge: Official / API

- [ ] **Step 2: Replace `+` dialog**

Dialog sections:
1. Kind selector: Official / API (locked after first successful save in edit mode)
2. Official:
   - mode tabs: Single paste / Bulk files
   - paste textarea
   - file input `multiple accept=".json,application/json"`
   - optional batch name
   - import result summary
3. API:
   - display name, api key, base url
   - advanced: interface format select (5 options)
   - model mapping rows editor (`from`, `to`, optional `label`)
   - live preview panels based on platform
     - codex: auth.json + config.toml textareas
     - claude/gemini: settings.json textarea
   - editing preview updates local state and is sent as `preview_json`

- [ ] **Step 3: List actions**
- checkbox toggles membership then calls `setRoutePoolMembers`
- edit opens dialog prefilled from credential
- delete calls `deleteRouteCredential` and refreshes pool/list

- [ ] **Step 4: Keep proxy controls**
- start/stop proxy
- write route configs
- stats icon continues to show pool stats/logs

- [ ] **Step 5: Update frontend tests**

Cover:
- rendering credential rows from mocked list
- official import command called with pasted text
- api create command called with interface format + mappings
- checkbox calls setRoutePoolMembers with credential ids

```bash
pnpm typecheck
pnpm test:run tests/AccountsScreen.test.tsx
```

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/screens/AccountsScreen.tsx tests/AccountsScreen.test.tsx
git commit -m "feat: account UI for CPA and API route credentials"
```

---

### Task 10: End-to-End Verification

**Files:**
- No intentional new files; fix only regressions found

- [ ] **Step 1: Rust format/check/tests**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
pnpm rust:check
pnpm rust:test
```

Expected: all pass for route credential/pool/proxy suites.

- [ ] **Step 2: Frontend checks**

```bash
pnpm typecheck
pnpm test:run
```

Expected: PASS

- [ ] **Step 3: Manual smoke checklist**
1. Open Codex tab, `+` official, paste CPA codex JSON, save, row appears.
2. Bulk import two json files with one bad file; summary shows 1 success 1 fail.
3. Create API credential with mappings and edit preview; save; reopen edit and confirm preview persisted.
4. Checkbox into pool; stats member_count increments.
5. Start proxy; write configs; confirm status base URL shown.
6. Ensure no email/password fields exist for official create.
7. Ensure wrapper accounts JSON is rejected with clear error.

- [ ] **Step 4: Final commit if fixes landed**

```bash
git add -A relevant-fix-files
git commit -m "fix: stabilize route credentials slice"
```

---

## Spec Coverage Check

| Spec requirement | Task |
| --- | --- |
| Unified `route_credentials` | Task 1-2 |
| Official CPA paste + multi-file | Task 3,4,5,9 |
| Reject accounts wrapper | Task 3 |
| API key/baseUrl + 5 formats + mappings array | Task 4,8,9 |
| Preview drafts DB only | Task 4,9 |
| Pool membership on credentials | Task 6,9 |
| Proxy routes official+API credentials | Task 7 |
| Explicit config write unchanged as separate action | Task 7/9 keeps existing command |
| No legacy migration | Task 1 drops/recreates pool members without migrate-from-old |
| Agent-first UI / no competitor copy | Task 9 |

## Placeholder / Consistency Notes

- Public pool field remains `account_ids` but stores credential ids to avoid broad UI churn.
- Gemini official CPA can return clear unsupported/invalid if no matching object; API mode is required path for Gemini in v1.
- Do not implement email/password or wrapper compatibility “just in case”.
