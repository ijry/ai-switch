# AI Switch Provider Switching B1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build sandbox-first provider switching for the seven default AI tool targets without writing real external tool configuration files.

**Architecture:** The Rust backend owns validation, target/provider loading, deterministic sandbox rendering, atomic writes, snapshot recording, and target state updates. The React frontend calls typed Tauri commands through `src/lib/api`, exposes switching from Providers, and shows switch status from Targets. SQLite remains the business source of truth, and all B1 config writes stay under `~/.ai-switch/targets/<target_key>/provider.json`.

**Tech Stack:** Tauri 2, React 18, TypeScript, Vite, Tailwind CSS, Vitest, Testing Library, Rust, sqlx SQLite, serde, serde_json, chrono, uuid, tokio.

## Global Constraints

- B1 must not write real Claude, Codex, Gemini, OpenCode, OpenClaw, or Hermes configuration files.
- B1 writes sandbox configs only under `~/.ai-switch/targets/<target_key>/provider.json`.
- The request must not accept a filesystem path from the frontend.
- B1 must use `ConfigWriter` for every sandbox config write.
- B1 must record switch attempts in `config_snapshots` and update `target_app_states`.
- B1 must not add a schema migration.
- B1 accepts only `mode = "sandbox"` for provider switching.
- B1 handles provider items only; official account switching remains out of scope.
- B1 must not resolve raw API keys or account tokens; rendered config may include `secret_ref` and redacted markers only.
- Clean-room rule: public behavior, public documentation, and public file formats may be studied, but non-commercial source code from `cockpit-tools` must not be copied or translated.
- No OAuth, token refresh, quota lookup, tray switching, provider preset library, provider import/export, rollback UI, MCP, prompts, skills, proxy, cloud sync, usage tracking, sessions, updater, multi-instance management, or wakeup tasks in B1.

---

## File Structure

Create these backend files:

```text
src-tauri/src/models/config_snapshot.rs
src-tauri/src/models/provider_switch.rs
src-tauri/src/models/target_state.rs
src-tauri/src/database/repositories/config_snapshot_repository.rs
src-tauri/src/database/repositories/target_state_repository.rs
src-tauri/src/adapters/provider_renderers.rs
src-tauri/src/services/provider_switch_service.rs
src-tauri/src/commands/provider_commands.rs
```

Modify these backend files:

```text
src-tauri/src/error.rs
src-tauri/src/models/mod.rs
src-tauri/src/database/repositories/mod.rs
src-tauri/src/database/repositories/provider_repository.rs
src-tauri/src/database/repositories/target_repository.rs
src-tauri/src/adapters/mod.rs
src-tauri/src/services/mod.rs
src-tauri/src/commands/mod.rs
src-tauri/src/commands/target_commands.rs
src-tauri/src/lib.rs
```

Modify these frontend files:

```text
src/lib/api/types.ts
src/lib/api/client.ts
src/test/fixtures.ts
src/screens/ProvidersScreen.tsx
src/screens/TargetsScreen.tsx
```

Create these frontend tests:

```text
tests/apiClient.test.ts
tests/ProvidersScreen.test.tsx
tests/TargetsScreen.test.tsx
```

---

### Task 1: Add Backend Switch State Models And Repositories

**Files:**
- Create: `src-tauri/src/models/config_snapshot.rs`
- Create: `src-tauri/src/models/target_state.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Create: `src-tauri/src/database/repositories/config_snapshot_repository.rs`
- Create: `src-tauri/src/database/repositories/target_state_repository.rs`
- Modify: `src-tauri/src/database/repositories/mod.rs`
- Modify: `src-tauri/src/database/repositories/provider_repository.rs`
- Modify: `src-tauri/src/database/repositories/target_repository.rs`

**Interfaces:**
- Consumes: existing `Provider`, `TargetApp`, `AppError`, SQLite tables `providers`, `target_apps`, `target_app_states`, and `config_snapshots`.
- Produces: `ConfigSnapshot`, `NewConfigSnapshot`, `TargetAppState`, `TargetSwitchStatus`.
- Produces: `ProviderRepository::list(pool: &SqlitePool) -> Result<Vec<Provider>, AppError>`.
- Produces: `TargetRepository::get(pool: &SqlitePool, id: &str) -> Result<TargetApp, AppError>`.
- Produces: `ConfigSnapshotRepository::insert(pool: &SqlitePool, input: NewConfigSnapshot) -> Result<ConfigSnapshot, AppError>`.
- Produces: `ConfigSnapshotRepository::latest_for_target(pool: &SqlitePool, target_app_id: &str) -> Result<Option<ConfigSnapshot>, AppError>`.
- Produces: `TargetStateRepository::upsert_provider_state(pool: &SqlitePool, target_app_id: &str, provider_id: &str, status: &str, error_code: Option<&str>, written_at: &str) -> Result<TargetAppState, AppError>`.
- Produces: `TargetStateRepository::record_failure(pool: &SqlitePool, target_app_id: &str, error_code: &str, written_at: &str) -> Result<TargetAppState, AppError>`.
- Produces: `TargetStateRepository::list_switch_statuses(pool: &SqlitePool) -> Result<Vec<TargetSwitchStatus>, AppError>`.

- [ ] **Step 1: Write failing repository tests**

Append this test to `src-tauri/src/database/repositories/provider_repository.rs` inside its `#[cfg(test)]` module. If the file has no test module yet, add this full module at the end of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};

    #[tokio::test]
    async fn list_returns_providers_ordered_by_sort_and_created_at() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        ProviderRepository::create(
            &pool,
            NewProvider {
                name: "First Provider".to_string(),
                kind: "openai_compatible".to_string(),
                base_url: Some("https://first.example.com/v1".to_string()),
                model_config_json: "{}".to_string(),
                target_options_json: "{}".to_string(),
                secret_ref: None,
            },
        )
        .await
        .expect("first provider");
        ProviderRepository::create(
            &pool,
            NewProvider {
                name: "Second Provider".to_string(),
                kind: "openai_compatible".to_string(),
                base_url: Some("https://second.example.com/v1".to_string()),
                model_config_json: "{}".to_string(),
                target_options_json: "{}".to_string(),
                secret_ref: None,
            },
        )
        .await
        .expect("second provider");

        let providers = ProviderRepository::list(&pool).await.expect("providers");

        assert_eq!(providers.len(), 2);
        assert_eq!(providers[0].name, "Second Provider");
        assert_eq!(providers[1].name, "First Provider");
    }
}
```

Append this test to `src-tauri/src/database/repositories/target_repository.rs` inside its existing `#[cfg(test)]` module. If the file has no test module yet, add this full module at the end of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};

    #[tokio::test]
    async fn get_returns_default_target_by_id() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("targets");

        let loaded = TargetRepository::get(&pool, &targets[0].id)
            .await
            .expect("target");

        assert_eq!(loaded.id, targets[0].id);
        assert_eq!(loaded.key, "claude_code");
    }
}
```

Create `src-tauri/src/database/repositories/target_state_repository.rs` with this failing test scaffold:

```rust
pub struct TargetStateRepository;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::provider_repository::ProviderRepository;
    use crate::database::repositories::target_repository::TargetRepository;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::models::provider::NewProvider;

    #[tokio::test]
    async fn upsert_provider_state_and_list_statuses_return_active_provider() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("targets");
        let provider = ProviderRepository::create(
            &pool,
            NewProvider {
                name: "Acme Provider".to_string(),
                kind: "openai_compatible".to_string(),
                base_url: Some("https://api.example.com/v1".to_string()),
                model_config_json: "{}".to_string(),
                target_options_json: "{}".to_string(),
                secret_ref: Some("secret://provider/acme".to_string()),
            },
        )
        .await
        .expect("provider");

        let state = TargetStateRepository::upsert_provider_state(
            &pool,
            &targets[2].id,
            &provider.id,
            "written",
            None,
            "2026-07-13T00:00:00Z",
        )
        .await
        .expect("state");
        let statuses = TargetStateRepository::list_switch_statuses(&pool)
            .await
            .expect("statuses");
        let codex = statuses
            .iter()
            .find(|status| status.target.key == "codex")
            .expect("codex status");

        assert_eq!(state.active_item_type.as_deref(), Some("provider"));
        assert_eq!(state.active_item_id.as_deref(), Some(provider.id.as_str()));
        assert_eq!(codex.active_provider.as_ref().map(|item| item.name.as_str()), Some("Acme Provider"));
        assert_eq!(codex.last_write_status.as_deref(), Some("written"));
    }
}
```

Create `src-tauri/src/database/repositories/config_snapshot_repository.rs` with this failing test scaffold:

```rust
pub struct ConfigSnapshotRepository;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::target_repository::TargetRepository;
    use crate::database::{create_memory_pool, run_migrations};

    #[tokio::test]
    async fn insert_and_latest_for_target_round_trip_snapshot() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("targets");

        let inserted = ConfigSnapshotRepository::insert(
            &pool,
            NewConfigSnapshot {
                target_app_id: Some(targets[2].id.clone()),
                operation: "switch_provider:sandbox".to_string(),
                path: "C:/Users/example/.ai-switch/targets/codex/provider.json".to_string(),
                before_hash: None,
                after_hash: Some("after".to_string()),
                backup_path: None,
                status: "written".to_string(),
                error_code: None,
            },
        )
        .await
        .expect("insert");

        let latest = ConfigSnapshotRepository::latest_for_target(&pool, &targets[2].id)
            .await
            .expect("latest")
            .expect("snapshot");

        assert_eq!(latest.id, inserted.id);
        assert_eq!(latest.operation, "switch_provider:sandbox");
        assert_eq!(latest.status, "written");
    }
}
```

Update `src-tauri/src/database/repositories/mod.rs` so the new repository modules compile:

```rust
pub mod account_repository;
pub mod batch_repository;
pub mod config_snapshot_repository;
pub mod import_repository;
pub mod provider_repository;
pub mod target_repository;
pub mod target_state_repository;
```

Run:

```powershell
pnpm rust:test provider_repository
pnpm rust:test target_repository
pnpm rust:test target_state_repository
pnpm rust:test config_snapshot_repository
```

Expected: FAIL because `TargetAppState`, `TargetSwitchStatus`, `ConfigSnapshot`, `NewConfigSnapshot`, `ProviderRepository::list`, `TargetRepository::get`, and repository implementations are not complete.

- [ ] **Step 2: Add backend switch state models**

Create `src-tauri/src/models/config_snapshot.rs`:

```rust
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct ConfigSnapshot {
    pub id: String,
    pub target_app_id: Option<String>,
    pub operation: String,
    pub path: String,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub backup_path: Option<String>,
    pub status: String,
    pub error_code: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewConfigSnapshot {
    pub target_app_id: Option<String>,
    pub operation: String,
    pub path: String,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub backup_path: Option<String>,
    pub status: String,
    pub error_code: Option<String>,
}
```

Create `src-tauri/src/models/target_state.rs`:

```rust
use crate::models::provider::Provider;
use crate::models::target_app::TargetApp;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct TargetAppState {
    pub id: String,
    pub target_app_id: String,
    pub active_item_type: Option<String>,
    pub active_item_id: Option<String>,
    pub last_write_status: Option<String>,
    pub last_error_code: Option<String>,
    pub last_written_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TargetSwitchStatus {
    pub target: TargetApp,
    pub active_provider: Option<Provider>,
    pub last_write_status: Option<String>,
    pub last_error_code: Option<String>,
    pub last_written_at: Option<String>,
    pub last_snapshot_path: Option<String>,
    pub last_snapshot_id: Option<String>,
}
```

Update `src-tauri/src/models/mod.rs`:

```rust
pub mod account;
pub mod batch;
pub mod config_snapshot;
pub mod import_job;
pub mod provider;
pub mod settings;
pub mod target_app;
pub mod target_state;
```

- [ ] **Step 3: Implement provider and target repository additions**

Add this method to `impl ProviderRepository` in `src-tauri/src/database/repositories/provider_repository.rs`:

```rust
pub async fn list(pool: &SqlitePool) -> Result<Vec<Provider>, AppError> {
    sqlx::query_as::<_, Provider>(
        "SELECT * FROM providers ORDER BY sort_order ASC, created_at DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(|err| AppError::Database {
        code: "database.provider_list",
        message: "Could not list providers".to_string(),
        details: Some(err.to_string()),
        recoverable: true,
    })
}
```

Add this method to `impl TargetRepository` in `src-tauri/src/database/repositories/target_repository.rs`:

```rust
pub async fn get(pool: &SqlitePool, id: &str) -> Result<TargetApp, AppError> {
    sqlx::query_as::<_, TargetApp>("SELECT * FROM target_apps WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.target_get",
            message: "Could not load target app".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
}
```

- [ ] **Step 4: Implement config snapshot repository**

Replace `src-tauri/src/database/repositories/config_snapshot_repository.rs` with:

```rust
use crate::error::AppError;
use crate::models::config_snapshot::{ConfigSnapshot, NewConfigSnapshot};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct ConfigSnapshotRepository;

impl ConfigSnapshotRepository {
    pub async fn insert(
        pool: &SqlitePool,
        input: NewConfigSnapshot,
    ) -> Result<ConfigSnapshot, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO config_snapshots (id, target_app_id, operation, path, before_hash, after_hash, backup_path, status, error_code, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&input.target_app_id)
        .bind(&input.operation)
        .bind(&input.path)
        .bind(&input.before_hash)
        .bind(&input.after_hash)
        .bind(&input.backup_path)
        .bind(&input.status)
        .bind(&input.error_code)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.config_snapshot_insert",
            message: "Could not record config snapshot".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        sqlx::query_as::<_, ConfigSnapshot>("SELECT * FROM config_snapshots WHERE id = ?")
            .bind(&id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.config_snapshot_get",
                message: "Could not load config snapshot".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn latest_for_target(
        pool: &SqlitePool,
        target_app_id: &str,
    ) -> Result<Option<ConfigSnapshot>, AppError> {
        sqlx::query_as::<_, ConfigSnapshot>(
            "SELECT * FROM config_snapshots WHERE target_app_id = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(target_app_id)
        .fetch_optional(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.config_snapshot_latest",
            message: "Could not load latest config snapshot".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }
}
```

- [ ] **Step 5: Implement target state repository**

Replace `src-tauri/src/database/repositories/target_state_repository.rs` with:

```rust
use crate::error::AppError;
use crate::models::provider::Provider;
use crate::models::target_app::TargetApp;
use crate::models::target_state::{TargetAppState, TargetSwitchStatus};
use chrono::Utc;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct TargetStateRepository;

impl TargetStateRepository {
    pub async fn upsert_provider_state(
        pool: &SqlitePool,
        target_app_id: &str,
        provider_id: &str,
        status: &str,
        error_code: Option<&str>,
        written_at: &str,
    ) -> Result<TargetAppState, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO target_app_states (id, target_app_id, active_item_type, active_item_id, last_write_status, last_error_code, last_written_at, updated_at)
             VALUES (?, ?, 'provider', ?, ?, ?, ?, ?)
             ON CONFLICT(target_app_id) DO UPDATE SET
               active_item_type = 'provider',
               active_item_id = excluded.active_item_id,
               last_write_status = excluded.last_write_status,
               last_error_code = excluded.last_error_code,
               last_written_at = excluded.last_written_at,
               updated_at = excluded.updated_at",
        )
        .bind(&id)
        .bind(target_app_id)
        .bind(provider_id)
        .bind(status)
        .bind(error_code)
        .bind(written_at)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.target_state_upsert",
            message: "Could not update target switch state".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get_for_target(pool, target_app_id).await
    }

    pub async fn record_failure(
        pool: &SqlitePool,
        target_app_id: &str,
        error_code: &str,
        written_at: &str,
    ) -> Result<TargetAppState, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO target_app_states (id, target_app_id, active_item_type, active_item_id, last_write_status, last_error_code, last_written_at, updated_at)
             VALUES (?, ?, NULL, NULL, 'failed', ?, ?, ?)
             ON CONFLICT(target_app_id) DO UPDATE SET
               last_write_status = excluded.last_write_status,
               last_error_code = excluded.last_error_code,
               last_written_at = excluded.last_written_at,
               updated_at = excluded.updated_at",
        )
        .bind(&id)
        .bind(target_app_id)
        .bind(error_code)
        .bind(written_at)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.target_state_upsert",
            message: "Could not update target failure state".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get_for_target(pool, target_app_id).await
    }

    pub async fn get_for_target(
        pool: &SqlitePool,
        target_app_id: &str,
    ) -> Result<TargetAppState, AppError> {
        sqlx::query_as::<_, TargetAppState>("SELECT * FROM target_app_states WHERE target_app_id = ?")
            .bind(target_app_id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.target_state_get",
                message: "Could not load target switch state".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn list_switch_statuses(
        pool: &SqlitePool,
    ) -> Result<Vec<TargetSwitchStatus>, AppError> {
        let rows = sqlx::query(
            "SELECT
                t.id AS target_id,
                t.key AS target_key,
                t.display_name AS target_display_name,
                t.enabled AS target_enabled,
                t.sort_order AS target_sort_order,
                t.created_at AS target_created_at,
                t.updated_at AS target_updated_at,
                s.last_write_status,
                s.last_error_code,
                s.last_written_at,
                p.id AS provider_id,
                p.name AS provider_name,
                p.kind AS provider_kind,
                p.base_url AS provider_base_url,
                p.model_config_json AS provider_model_config_json,
                p.target_options_json AS provider_target_options_json,
                p.secret_ref AS provider_secret_ref,
                p.status AS provider_status,
                p.sort_order AS provider_sort_order,
                p.created_at AS provider_created_at,
                p.updated_at AS provider_updated_at,
                cs.id AS snapshot_id,
                cs.path AS snapshot_path
             FROM target_apps t
             LEFT JOIN target_app_states s ON s.target_app_id = t.id
             LEFT JOIN providers p ON s.active_item_type = 'provider' AND p.id = s.active_item_id
             LEFT JOIN config_snapshots cs ON cs.id = (
                SELECT id FROM config_snapshots
                WHERE target_app_id = t.id
                ORDER BY created_at DESC
                LIMIT 1
             )
             ORDER BY t.sort_order ASC",
        )
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.target_status_list",
            message: "Could not list target switch statuses".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let provider_id: Option<String> = row.get("provider_id");
                let active_provider = provider_id.map(|id| Provider {
                    id,
                    name: row
                        .get::<Option<String>, _>("provider_name")
                        .unwrap_or_default(),
                    kind: row
                        .get::<Option<String>, _>("provider_kind")
                        .unwrap_or_default(),
                    base_url: row.get("provider_base_url"),
                    model_config_json: row
                        .get::<Option<String>, _>("provider_model_config_json")
                        .unwrap_or_else(|| "{}".to_string()),
                    target_options_json: row
                        .get::<Option<String>, _>("provider_target_options_json")
                        .unwrap_or_else(|| "{}".to_string()),
                    secret_ref: row.get("provider_secret_ref"),
                    status: row
                        .get::<Option<String>, _>("provider_status")
                        .unwrap_or_else(|| "ok".to_string()),
                    sort_order: row.get::<Option<i64>, _>("provider_sort_order").unwrap_or(0),
                    created_at: row
                        .get::<Option<String>, _>("provider_created_at")
                        .unwrap_or_default(),
                    updated_at: row
                        .get::<Option<String>, _>("provider_updated_at")
                        .unwrap_or_default(),
                });

                TargetSwitchStatus {
                    target: TargetApp {
                        id: row.get("target_id"),
                        key: row.get("target_key"),
                        display_name: row.get("target_display_name"),
                        enabled: row.get("target_enabled"),
                        sort_order: row.get("target_sort_order"),
                        created_at: row.get("target_created_at"),
                        updated_at: row.get("target_updated_at"),
                    },
                    active_provider,
                    last_write_status: row.get("last_write_status"),
                    last_error_code: row.get("last_error_code"),
                    last_written_at: row.get("last_written_at"),
                    last_snapshot_path: row.get("snapshot_path"),
                    last_snapshot_id: row.get("snapshot_id"),
                }
            })
            .collect())
    }
}
```

- [ ] **Step 6: Run repository tests**

Run:

```powershell
pnpm rust:test provider_repository
pnpm rust:test target_repository
pnpm rust:test target_state_repository
pnpm rust:test config_snapshot_repository
```

Expected: PASS.

- [ ] **Step 7: Commit backend repository layer**

```powershell
git add src-tauri/src/models src-tauri/src/database/repositories
git commit -m "feat: add provider switch state repositories"
```

---

### Task 2: Add Deterministic Sandbox Provider Renderers

**Files:**
- Modify: `src-tauri/src/error.rs`
- Create: `src-tauri/src/adapters/provider_renderers.rs`
- Modify: `src-tauri/src/adapters/mod.rs`

**Interfaces:**
- Consumes: `Provider`, `TargetApp`, and `AppError`.
- Produces: `render_provider_sandbox_config(target: &TargetApp, provider: &Provider) -> Result<String, AppError>`.
- Produces: `AppError::Adapter` and `AppError::code(&self) -> &'static str`.

- [ ] **Step 1: Write failing renderer tests**

Create `src-tauri/src/adapters/provider_renderers.rs` with this test-first scaffold:

```rust
#[cfg(test)]
mod tests {
    use super::render_provider_sandbox_config;
    use crate::models::provider::Provider;
    use crate::models::target_app::TargetApp;
    use serde_json::Value;

    fn target(key: &str) -> TargetApp {
        TargetApp {
            id: format!("{key}-id"),
            key: key.to_string(),
            display_name: key.to_string(),
            enabled: 1,
            sort_order: 0,
            created_at: "2026-07-13T00:00:00Z".to_string(),
            updated_at: "2026-07-13T00:00:00Z".to_string(),
        }
    }

    fn provider() -> Provider {
        Provider {
            id: "provider-1".to_string(),
            name: "Acme Provider".to_string(),
            kind: "openai_compatible".to_string(),
            base_url: Some("https://api.example.com/v1".to_string()),
            model_config_json: "{\"default\":\"gpt-4.1\"}".to_string(),
            target_options_json: "{\"codex\":{\"model\":\"gpt-4.1-mini\"},\"timeout\":30}".to_string(),
            secret_ref: Some("secret://provider/acme".to_string()),
            status: "ok".to_string(),
            sort_order: 0,
            created_at: "2026-07-13T00:00:00Z".to_string(),
            updated_at: "2026-07-13T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn renders_all_default_targets_deterministically_and_redacts_secret() {
        let keys = [
            "claude_code",
            "claude_desktop",
            "codex",
            "gemini_cli",
            "opencode",
            "openclaw",
            "hermes",
        ];

        for key in keys {
            let first = render_provider_sandbox_config(&target(key), &provider()).expect("first render");
            let second = render_provider_sandbox_config(&target(key), &provider()).expect("second render");
            let value: Value = serde_json::from_str(&first).expect("json");

            assert_eq!(first, second);
            assert_eq!(value["schema"], "ai-switch.provider-switch.sandbox.v1");
            assert_eq!(value["target"]["key"], key);
            assert_eq!(value["provider"]["secret_ref"], "secret://provider/acme");
            assert_eq!(value["provider"]["secret_value"], "[redacted]");
            assert_eq!(value["rendered_for"], key);
        }
    }

    #[test]
    fn uses_target_specific_options_when_present() {
        let rendered = render_provider_sandbox_config(&target("codex"), &provider()).expect("render");
        let value: Value = serde_json::from_str(&rendered).expect("json");

        assert_eq!(value["target_options"]["model"], "gpt-4.1-mini");
        assert!(value["target_options"]["timeout"].is_null());
    }

    #[test]
    fn rejects_malformed_model_config_json() {
        let mut provider = provider();
        provider.model_config_json = "{".to_string();

        let error = render_provider_sandbox_config(&target("codex"), &provider).expect_err("error");

        assert_eq!(error.code(), "validation.provider_model_config_json");
    }

    #[test]
    fn rejects_malformed_target_options_json() {
        let mut provider = provider();
        provider.target_options_json = "{".to_string();

        let error = render_provider_sandbox_config(&target("codex"), &provider).expect_err("error");

        assert_eq!(error.code(), "validation.provider_target_options_json");
    }

    #[test]
    fn rejects_unsupported_target_key() {
        let error = render_provider_sandbox_config(&target("unknown"), &provider()).expect_err("error");

        assert_eq!(error.code(), "adapter.target_not_supported");
    }
}
```

Update `src-tauri/src/adapters/mod.rs`:

```rust
pub mod provider_renderers;
```

Keep the existing adapter trait code below that module declaration.

Run:

```powershell
pnpm rust:test provider_renderers
```

Expected: FAIL because `AppError::code` and the renderer implementation do not exist yet.

- [ ] **Step 2: Extend AppError for adapter errors and stable code lookup**

Modify `src-tauri/src/error.rs`.

Add this enum variant after `Secret`:

```rust
#[error("{message}")]
Adapter {
    code: &'static str,
    message: String,
    details: Option<String>,
    recoverable: bool,
},
```

Update `impl From<AppError> for ApiError` so the match includes `AppError::Adapter`:

```rust
AppError::Validation {
    code,
    message,
    details,
    recoverable,
}
| AppError::Filesystem {
    code,
    message,
    details,
    recoverable,
}
| AppError::Database {
    code,
    message,
    details,
    recoverable,
}
| AppError::Secret {
    code,
    message,
    details,
    recoverable,
}
| AppError::Adapter {
    code,
    message,
    details,
    recoverable,
} => Self {
    code: code.to_string(),
    message,
    details,
    recoverable,
    operation_id: None,
},
```

Add this `impl` block below the enum:

```rust
impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Validation { code, .. }
            | AppError::Filesystem { code, .. }
            | AppError::Database { code, .. }
            | AppError::Secret { code, .. }
            | AppError::Adapter { code, .. } => code,
        }
    }
}
```

- [ ] **Step 3: Implement provider sandbox renderer**

Replace `src-tauri/src/adapters/provider_renderers.rs` with:

```rust
use crate::error::AppError;
use crate::models::provider::Provider;
use crate::models::target_app::TargetApp;
use serde::Serialize;
use serde_json::Value;

const SUPPORTED_TARGET_KEYS: [&str; 7] = [
    "claude_code",
    "claude_desktop",
    "codex",
    "gemini_cli",
    "opencode",
    "openclaw",
    "hermes",
];

#[derive(Serialize)]
struct SandboxConfig<'a> {
    schema: &'static str,
    target: SandboxTarget<'a>,
    provider: SandboxProvider<'a>,
    model_config: Value,
    target_options: Value,
    rendered_for: &'a str,
}

#[derive(Serialize)]
struct SandboxTarget<'a> {
    key: &'a str,
    display_name: &'a str,
}

#[derive(Serialize)]
struct SandboxProvider<'a> {
    id: &'a str,
    name: &'a str,
    kind: &'a str,
    base_url: Option<&'a str>,
    secret_ref: Option<&'a str>,
    secret_value: Option<&'static str>,
}

pub fn render_provider_sandbox_config(
    target: &TargetApp,
    provider: &Provider,
) -> Result<String, AppError> {
    if !SUPPORTED_TARGET_KEYS.contains(&target.key.as_str()) {
        return Err(AppError::Adapter {
            code: "adapter.target_not_supported",
            message: "Target app is not supported by the sandbox provider renderer".to_string(),
            details: Some(target.key.clone()),
            recoverable: false,
        });
    }

    let model_config = parse_json_object(
        &provider.model_config_json,
        "validation.provider_model_config_json",
        "Provider model configuration must be a JSON object",
    )?;
    let target_options = parse_json_object(
        &provider.target_options_json,
        "validation.provider_target_options_json",
        "Provider target options must be a JSON object",
    )?;
    let selected_target_options = target_options
        .as_object()
        .and_then(|object| object.get(&target.key))
        .cloned()
        .unwrap_or(target_options);

    let payload = SandboxConfig {
        schema: "ai-switch.provider-switch.sandbox.v1",
        target: SandboxTarget {
            key: &target.key,
            display_name: &target.display_name,
        },
        provider: SandboxProvider {
            id: &provider.id,
            name: &provider.name,
            kind: &provider.kind,
            base_url: provider.base_url.as_deref(),
            secret_ref: provider.secret_ref.as_deref(),
            secret_value: provider.secret_ref.as_ref().map(|_| "[redacted]"),
        },
        model_config,
        target_options: selected_target_options,
        rendered_for: &target.key,
    };

    serde_json::to_string_pretty(&payload).map_err(|err| AppError::Validation {
        code: "validation.provider_render_json",
        message: "Could not render provider sandbox config".to_string(),
        details: Some(err.to_string()),
        recoverable: true,
    })
}

fn parse_json_object(
    raw: &str,
    code: &'static str,
    message: &str,
) -> Result<Value, AppError> {
    let value: Value = serde_json::from_str(raw).map_err(|err| AppError::Validation {
        code,
        message: message.to_string(),
        details: Some(err.to_string()),
        recoverable: true,
    })?;

    if !value.is_object() {
        return Err(AppError::Validation {
            code,
            message: message.to_string(),
            details: Some("Expected a JSON object".to_string()),
            recoverable: true,
        });
    }

    Ok(value)
}
```

Keep the tests from Step 1 at the bottom of the file.

- [ ] **Step 4: Run renderer tests**

Run:

```powershell
pnpm rust:test provider_renderers
pnpm rust:check
```

Expected: PASS.

- [ ] **Step 5: Commit sandbox renderer**

```powershell
git add src-tauri/src/error.rs src-tauri/src/adapters
git commit -m "feat: add sandbox provider renderers"
```

---

### Task 3: Add Provider Switch Service And Tauri Commands

**Files:**
- Create: `src-tauri/src/models/provider_switch.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Create: `src-tauri/src/services/provider_switch_service.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Create: `src-tauri/src/commands/provider_commands.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/commands/target_commands.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: Task 1 repositories, Task 2 renderer, `ConfigWriter`, `AppPaths`, and Tauri `AppState`.
- Produces: `ProviderSwitchRequest`.
- Produces: `ProviderSwitchOutcome`.
- Produces: `ProviderSwitchService::list_providers`.
- Produces: `ProviderSwitchService::list_target_switch_statuses`.
- Produces: `ProviderSwitchService::switch_provider`.
- Produces Tauri commands `list_providers`, `list_target_switch_statuses`, and `switch_target_provider`.

- [ ] **Step 1: Add provider switch models**

Create `src-tauri/src/models/provider_switch.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderSwitchRequest {
    pub target_app_id: String,
    pub provider_id: String,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderSwitchOutcome {
    pub target_app_id: String,
    pub target_key: String,
    pub provider_id: String,
    pub provider_name: String,
    pub mode: String,
    pub path: String,
    pub status: String,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub snapshot_id: String,
    pub state_id: String,
    pub written_at: String,
}
```

Update `src-tauri/src/models/mod.rs` so it includes:

```rust
pub mod provider_switch;
```

- [ ] **Step 2: Write failing provider switch service tests**

Create `src-tauri/src/services/provider_switch_service.rs` with this test-first scaffold:

```rust
pub struct ProviderSwitchService;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::config_snapshot_repository::ConfigSnapshotRepository;
    use crate::database::repositories::provider_repository::ProviderRepository;
    use crate::database::repositories::target_repository::TargetRepository;
    use crate::database::repositories::target_state_repository::TargetStateRepository;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::models::provider::NewProvider;
    use tempfile::tempdir;

    async fn seeded_provider(pool: &SqlitePool) -> Provider {
        ProviderRepository::create(
            pool,
            NewProvider {
                name: "Acme Provider".to_string(),
                kind: "openai_compatible".to_string(),
                base_url: Some("https://api.example.com/v1".to_string()),
                model_config_json: "{\"default\":\"gpt-4.1\"}".to_string(),
                target_options_json: "{\"codex\":{\"model\":\"gpt-4.1-mini\"}}".to_string(),
                secret_ref: Some("secret://provider/acme".to_string()),
            },
        )
        .await
        .expect("provider")
    }

    #[tokio::test]
    async fn switch_provider_writes_sandbox_config_and_records_state() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("targets");
        let codex = targets
            .iter()
            .find(|target| target.key == "codex")
            .expect("codex")
            .clone();
        let provider = seeded_provider(&pool).await;
        let dir = tempdir().expect("tempdir");
        let paths = AppPaths::from_data_dir(dir.path().to_path_buf());
        paths.ensure().await.expect("paths");

        let outcome = ProviderSwitchService::switch_provider(
            &pool,
            &paths,
            ProviderSwitchRequest {
                target_app_id: codex.id.clone(),
                provider_id: provider.id.clone(),
                mode: "sandbox".to_string(),
            },
        )
        .await
        .expect("switch");

        let written = tokio::fs::read_to_string(
            paths.data_dir.join("targets").join("codex").join("provider.json"),
        )
        .await
        .expect("written config");
        let snapshot = ConfigSnapshotRepository::latest_for_target(&pool, &codex.id)
            .await
            .expect("snapshot query")
            .expect("snapshot");
        let state = TargetStateRepository::get_for_target(&pool, &codex.id)
            .await
            .expect("state");

        assert_eq!(outcome.status, "written");
        assert_eq!(outcome.target_key, "codex");
        assert!(written.contains("ai-switch.provider-switch.sandbox.v1"));
        assert!(outcome.path.ends_with("targets\\codex\\provider.json") || outcome.path.ends_with("targets/codex/provider.json"));
        assert_eq!(snapshot.status, "written");
        assert_eq!(state.active_item_type.as_deref(), Some("provider"));
        assert_eq!(state.active_item_id.as_deref(), Some(provider.id.as_str()));
    }

    #[tokio::test]
    async fn switch_provider_rejects_non_sandbox_mode() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("targets");
        let provider = seeded_provider(&pool).await;
        let dir = tempdir().expect("tempdir");
        let paths = AppPaths::from_data_dir(dir.path().to_path_buf());

        let error = ProviderSwitchService::switch_provider(
            &pool,
            &paths,
            ProviderSwitchRequest {
                target_app_id: targets[0].id.clone(),
                provider_id: provider.id,
                mode: "real".to_string(),
            },
        )
        .await
        .expect_err("error");

        assert_eq!(error.code(), "validation.switch_mode");
    }

    #[tokio::test]
    async fn switch_provider_records_failure_state_when_rendering_fails() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("targets");
        let codex = targets
            .iter()
            .find(|target| target.key == "codex")
            .expect("codex")
            .clone();
        let provider = ProviderRepository::create(
            &pool,
            NewProvider {
                name: "Broken Provider".to_string(),
                kind: "openai_compatible".to_string(),
                base_url: None,
                model_config_json: "{".to_string(),
                target_options_json: "{}".to_string(),
                secret_ref: None,
            },
        )
        .await
        .expect("provider");
        let dir = tempdir().expect("tempdir");
        let paths = AppPaths::from_data_dir(dir.path().to_path_buf());
        paths.ensure().await.expect("paths");

        let error = ProviderSwitchService::switch_provider(
            &pool,
            &paths,
            ProviderSwitchRequest {
                target_app_id: codex.id.clone(),
                provider_id: provider.id,
                mode: "sandbox".to_string(),
            },
        )
        .await
        .expect_err("error");
        let snapshot = ConfigSnapshotRepository::latest_for_target(&pool, &codex.id)
            .await
            .expect("snapshot query")
            .expect("snapshot");
        let state = TargetStateRepository::get_for_target(&pool, &codex.id)
            .await
            .expect("state");

        assert_eq!(error.code(), "validation.provider_model_config_json");
        assert_eq!(snapshot.status, "failed");
        assert_eq!(snapshot.error_code.as_deref(), Some("validation.provider_model_config_json"));
        assert_eq!(state.last_write_status.as_deref(), Some("failed"));
        assert_eq!(state.last_error_code.as_deref(), Some("validation.provider_model_config_json"));
    }
}
```

Update `src-tauri/src/services/mod.rs`:

```rust
pub mod batch_service;
pub mod import_service;
pub mod provider_switch_service;
pub mod settings_service;
pub mod target_service;
```

Run:

```powershell
pnpm rust:test provider_switch_service
```

Expected: FAIL because the service methods are scaffolds.

- [ ] **Step 3: Implement provider switch service**

Replace the non-test portion of `src-tauri/src/services/provider_switch_service.rs` with:

```rust
use crate::adapters::provider_renderers::render_provider_sandbox_config;
use crate::config_writer::ConfigWriter;
use crate::database::repositories::config_snapshot_repository::ConfigSnapshotRepository;
use crate::database::repositories::provider_repository::ProviderRepository;
use crate::database::repositories::target_repository::TargetRepository;
use crate::database::repositories::target_state_repository::TargetStateRepository;
use crate::error::AppError;
use crate::models::config_snapshot::NewConfigSnapshot;
use crate::models::provider::Provider;
use crate::models::provider_switch::{ProviderSwitchOutcome, ProviderSwitchRequest};
use crate::models::target_app::TargetApp;
use crate::models::target_state::TargetSwitchStatus;
use crate::paths::AppPaths;
use chrono::Utc;
use sqlx::SqlitePool;
use std::path::PathBuf;

pub struct ProviderSwitchService;

impl ProviderSwitchService {
    pub async fn list_providers(pool: &SqlitePool) -> Result<Vec<Provider>, AppError> {
        ProviderRepository::list(pool).await
    }

    pub async fn list_target_switch_statuses(
        pool: &SqlitePool,
    ) -> Result<Vec<TargetSwitchStatus>, AppError> {
        TargetRepository::ensure_defaults(pool).await?;
        TargetStateRepository::list_switch_statuses(pool).await
    }

    pub async fn switch_provider(
        pool: &SqlitePool,
        paths: &AppPaths,
        request: ProviderSwitchRequest,
    ) -> Result<ProviderSwitchOutcome, AppError> {
        let target = TargetRepository::get(pool, &request.target_app_id).await?;
        let provider = ProviderRepository::get(pool, &request.provider_id).await?;

        if target.enabled == 0 {
            return Err(AppError::Validation {
                code: "validation.target_disabled",
                message: "Target app is disabled".to_string(),
                details: Some(target.key),
                recoverable: true,
            });
        }

        if request.mode != "sandbox" {
            return Err(AppError::Validation {
                code: "validation.switch_mode",
                message: "Provider switching only supports sandbox mode in B1".to_string(),
                details: Some(request.mode),
                recoverable: true,
            });
        }

        let path = sandbox_provider_path(paths, &target)?;
        let written_at = Utc::now().to_rfc3339();
        let rendered = match render_provider_sandbox_config(&target, &provider) {
            Ok(rendered) => rendered,
            Err(error) => {
                record_failed_attempt(pool, &target, &path, error.code(), &written_at).await;
                return Err(error);
            }
        };
        let write_outcome = match ConfigWriter::write_atomic(&path, &rendered).await {
            Ok(outcome) => outcome,
            Err(error) => {
                record_failed_attempt(pool, &target, &path, error.code(), &written_at).await;
                return Err(error);
            }
        };

        let snapshot = ConfigSnapshotRepository::insert(
            pool,
            NewConfigSnapshot {
                target_app_id: Some(target.id.clone()),
                operation: "switch_provider:sandbox".to_string(),
                path: write_outcome.path.clone(),
                before_hash: write_outcome.before_hash.clone(),
                after_hash: write_outcome.after_hash.clone(),
                backup_path: None,
                status: "written".to_string(),
                error_code: None,
            },
        )
        .await?;
        let state = TargetStateRepository::upsert_provider_state(
            pool,
            &target.id,
            &provider.id,
            "written",
            None,
            &written_at,
        )
        .await?;

        Ok(ProviderSwitchOutcome {
            target_app_id: target.id,
            target_key: target.key,
            provider_id: provider.id,
            provider_name: provider.name,
            mode: "sandbox".to_string(),
            path: write_outcome.path,
            status: "written".to_string(),
            before_hash: write_outcome.before_hash,
            after_hash: write_outcome.after_hash,
            snapshot_id: snapshot.id,
            state_id: state.id,
            written_at,
        })
    }
}

fn sandbox_provider_path(paths: &AppPaths, target: &TargetApp) -> Result<PathBuf, AppError> {
    if target.key.is_empty()
        || target.key.contains("..")
        || target.key.contains('/')
        || target.key.contains('\\')
    {
        return Err(AppError::Filesystem {
            code: "filesystem.sandbox_path_invalid",
            message: "Target key cannot be used in a sandbox path".to_string(),
            details: Some(target.key.clone()),
            recoverable: false,
        });
    }

    let targets_dir = paths.data_dir.join("targets");
    let path = targets_dir.join(&target.key).join("provider.json");

    if !path.starts_with(&targets_dir) {
        return Err(AppError::Filesystem {
            code: "filesystem.sandbox_path_invalid",
            message: "Sandbox config path escaped the targets directory".to_string(),
            details: Some(path.display().to_string()),
            recoverable: false,
        });
    }

    Ok(path)
}

async fn record_failed_attempt(
    pool: &SqlitePool,
    target: &TargetApp,
    path: &PathBuf,
    error_code: &str,
    written_at: &str,
) {
    let _ = ConfigSnapshotRepository::insert(
        pool,
        NewConfigSnapshot {
            target_app_id: Some(target.id.clone()),
            operation: "switch_provider:sandbox".to_string(),
            path: path.display().to_string(),
            before_hash: None,
            after_hash: None,
            backup_path: None,
            status: "failed".to_string(),
            error_code: Some(error_code.to_string()),
        },
    )
    .await;
    let _ = TargetStateRepository::record_failure(pool, &target.id, error_code, written_at).await;
}
```

Keep the tests from Step 2 at the bottom of the file.

- [ ] **Step 4: Add Tauri commands**

Create `src-tauri/src/commands/provider_commands.rs`:

```rust
use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::provider::Provider;
use crate::models::provider_switch::{ProviderSwitchOutcome, ProviderSwitchRequest};
use crate::services::provider_switch_service::ProviderSwitchService;
use tauri::State;

#[tauri::command]
pub async fn list_providers(state: State<'_, AppState>) -> Result<Vec<Provider>, ApiError> {
    ProviderSwitchService::list_providers(&state.pool)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn switch_target_provider(
    state: State<'_, AppState>,
    request: ProviderSwitchRequest,
) -> Result<ProviderSwitchOutcome, ApiError> {
    ProviderSwitchService::switch_provider(&state.pool, &state.paths, request)
        .await
        .map_err(ApiError::from)
}
```

Update `src-tauri/src/commands/target_commands.rs`:

```rust
use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::target_app::TargetApp;
use crate::models::target_state::TargetSwitchStatus;
use crate::services::provider_switch_service::ProviderSwitchService;
use crate::services::target_service::TargetService;
use tauri::State;

#[tauri::command]
pub async fn list_target_apps(state: State<'_, AppState>) -> Result<Vec<TargetApp>, ApiError> {
    TargetService::list_targets(&state.pool)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_target_switch_statuses(
    state: State<'_, AppState>,
) -> Result<Vec<TargetSwitchStatus>, ApiError> {
    ProviderSwitchService::list_target_switch_statuses(&state.pool)
        .await
        .map_err(ApiError::from)
}
```

Update `src-tauri/src/commands/mod.rs`:

```rust
pub mod batch_commands;
pub mod import_commands;
pub mod provider_commands;
pub mod settings_commands;
pub mod target_commands;
```

Update command imports in `src-tauri/src/lib.rs`:

```rust
use commands::provider_commands::{list_providers, switch_target_provider};
use commands::target_commands::{list_target_apps, list_target_switch_statuses};
```

Update the `tauri::generate_handler!` list in `src-tauri/src/lib.rs`:

```rust
tauri::generate_handler![
    get_settings,
    save_settings,
    create_batch,
    list_batch_groups,
    create_provider,
    create_official_account,
    import_example_json,
    list_target_apps,
    list_target_switch_statuses,
    list_providers,
    switch_target_provider
]
```

- [ ] **Step 5: Run service and backend checks**

Run:

```powershell
pnpm rust:test provider_switch_service
pnpm rust:check
```

Expected: PASS.

- [ ] **Step 6: Commit provider switch backend API**

```powershell
git add src-tauri/src/models src-tauri/src/services src-tauri/src/commands src-tauri/src/lib.rs
git commit -m "feat: add sandbox provider switch service"
```

---

### Task 4: Add Frontend API Types And Providers Switching Screen

**Files:**
- Modify: `src/lib/api/types.ts`
- Modify: `src/lib/api/client.ts`
- Modify: `src/test/fixtures.ts`
- Create: `tests/apiClient.test.ts`
- Create: `tests/ProvidersScreen.test.tsx`
- Modify: `src/screens/ProvidersScreen.tsx`

**Interfaces:**
- Consumes: Tauri commands `list_providers`, `list_target_switch_statuses`, and `switch_target_provider`.
- Produces: `ProviderSwitchRequest`, `ProviderSwitchOutcome`, `TargetSwitchStatus` TypeScript types.
- Produces: `listProviders`, `listTargetSwitchStatuses`, and `switchTargetProvider` frontend API wrappers.
- Produces: Providers screen with empty state, provider list, target selector, sandbox switch action, and mutation result message.

- [ ] **Step 1: Write failing frontend API wrapper tests**

Create `tests/apiClient.test.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it, vi } from "vitest";
import { listProviders, listTargetSwitchStatuses, switchTargetProvider } from "../src/lib/api/client";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("api client provider switching", () => {
  it("invokes provider and target switching commands", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listProviders();
    expect(invoke).toHaveBeenLastCalledWith("list_providers");

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listTargetSwitchStatuses();
    expect(invoke).toHaveBeenLastCalledWith("list_target_switch_statuses");

    vi.mocked(invoke).mockResolvedValueOnce({ status: "written" });
    await switchTargetProvider({
      target_app_id: "target-1",
      provider_id: "provider-1",
      mode: "sandbox",
    });
    expect(invoke).toHaveBeenLastCalledWith("switch_target_provider", {
      request: {
        target_app_id: "target-1",
        provider_id: "provider-1",
        mode: "sandbox",
      },
    });
  });
});
```

Run:

```powershell
pnpm test:run tests/apiClient.test.ts
```

Expected: FAIL because the new API wrappers do not exist.

- [ ] **Step 2: Add frontend provider switch types and API wrappers**

Append these types to `src/lib/api/types.ts`:

```ts
export type TargetSwitchStatus = {
  target: TargetApp;
  active_provider?: Provider | null;
  last_write_status?: string | null;
  last_error_code?: string | null;
  last_written_at?: string | null;
  last_snapshot_path?: string | null;
  last_snapshot_id?: string | null;
};

export type ProviderSwitchRequest = {
  target_app_id: string;
  provider_id: string;
  mode: "sandbox";
};

export type ProviderSwitchOutcome = {
  target_app_id: string;
  target_key: string;
  provider_id: string;
  provider_name: string;
  mode: "sandbox";
  path: string;
  status: string;
  before_hash?: string | null;
  after_hash?: string | null;
  snapshot_id: string;
  state_id: string;
  written_at: string;
};
```

Update the import line in `src/lib/api/client.ts`:

```ts
import type {
  AppSettings,
  Batch,
  BatchGroup,
  ImportJob,
  Provider,
  ProviderSwitchOutcome,
  ProviderSwitchRequest,
  TargetApp,
  TargetSwitchStatus,
} from "./types";
```

Add these functions to `src/lib/api/client.ts`:

```ts
export function listProviders(): Promise<Provider[]> {
  return invoke("list_providers");
}

export function listTargetSwitchStatuses(): Promise<TargetSwitchStatus[]> {
  return invoke("list_target_switch_statuses");
}

export function switchTargetProvider(request: ProviderSwitchRequest): Promise<ProviderSwitchOutcome> {
  return invoke("switch_target_provider", { request });
}
```

Run:

```powershell
pnpm test:run tests/apiClient.test.ts
pnpm typecheck
```

Expected: PASS.

- [ ] **Step 3: Add frontend fixtures for provider switching**

Update the import in `src/test/fixtures.ts`:

```ts
import type { AppSettings, BatchGroup, Provider, TargetSwitchStatus } from "../lib/api/types";
```

Append these fixtures to `src/test/fixtures.ts`:

```ts
export const providersFixture: Provider[] = [
  {
    id: "provider-1",
    name: "Acme Provider",
    kind: "openai_compatible",
    base_url: "https://api.example.com/v1",
    model_config_json: "{\"default\":\"gpt-4.1\"}",
    target_options_json: "{}",
    secret_ref: "secret://provider/acme",
    status: "ok",
    sort_order: 0,
    created_at: "2026-07-13T00:00:00Z",
    updated_at: "2026-07-13T00:00:00Z",
  },
];

export const targetSwitchStatusesFixture: TargetSwitchStatus[] = [
  {
    target: {
      id: "target-codex",
      key: "codex",
      display_name: "Codex",
      enabled: 1,
      sort_order: 2,
      created_at: "2026-07-13T00:00:00Z",
      updated_at: "2026-07-13T00:00:00Z",
    },
    active_provider: providersFixture[0],
    last_write_status: "written",
    last_error_code: null,
    last_written_at: "2026-07-13T00:00:00Z",
    last_snapshot_path: "C:/Users/example/.ai-switch/targets/codex/provider.json",
    last_snapshot_id: "snapshot-1",
  },
  {
    target: {
      id: "target-claude",
      key: "claude_code",
      display_name: "Claude Code",
      enabled: 1,
      sort_order: 0,
      created_at: "2026-07-13T00:00:00Z",
      updated_at: "2026-07-13T00:00:00Z",
    },
    active_provider: null,
    last_write_status: null,
    last_error_code: null,
    last_written_at: null,
    last_snapshot_path: null,
    last_snapshot_id: null,
  },
];
```

- [ ] **Step 4: Write failing ProvidersScreen tests**

Create `tests/ProvidersScreen.test.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ProvidersScreen } from "../src/screens/ProvidersScreen";
import { providersFixture, targetSwitchStatusesFixture } from "../src/test/fixtures";
import { listProviders, listTargetSwitchStatuses, switchTargetProvider } from "../src/lib/api/client";

vi.mock("../src/lib/api/client", () => ({
  listProviders: vi.fn(),
  listTargetSwitchStatuses: vi.fn(),
  switchTargetProvider: vi.fn(),
}));

function renderWithClient() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });

  return render(
    <QueryClientProvider client={queryClient}>
      <ProvidersScreen />
    </QueryClientProvider>,
  );
}

describe("ProvidersScreen", () => {
  it("shows an empty state when there are no providers", async () => {
    vi.mocked(listProviders).mockResolvedValueOnce([]);
    vi.mocked(listTargetSwitchStatuses).mockResolvedValueOnce(targetSwitchStatusesFixture);

    renderWithClient();

    expect(await screen.findByText("No providers yet. Import example JSON to create one.")).toBeInTheDocument();
  });

  it("switches the selected provider to the selected target in sandbox mode", async () => {
    vi.mocked(listProviders).mockResolvedValueOnce(providersFixture);
    vi.mocked(listTargetSwitchStatuses).mockResolvedValueOnce(targetSwitchStatusesFixture);
    vi.mocked(switchTargetProvider).mockResolvedValueOnce({
      target_app_id: "target-codex",
      target_key: "codex",
      provider_id: "provider-1",
      provider_name: "Acme Provider",
      mode: "sandbox",
      path: "C:/Users/example/.ai-switch/targets/codex/provider.json",
      status: "written",
      before_hash: null,
      after_hash: "after",
      snapshot_id: "snapshot-1",
      state_id: "state-1",
      written_at: "2026-07-13T00:00:00Z",
    });

    renderWithClient();

    expect(await screen.findByText("Acme Provider")).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText("Target for Acme Provider"), "target-codex");
    await userEvent.click(screen.getByRole("button", { name: "Switch Acme Provider in sandbox" }));

    await waitFor(() => {
      expect(switchTargetProvider).toHaveBeenCalledWith({
        target_app_id: "target-codex",
        provider_id: "provider-1",
        mode: "sandbox",
      });
    });
    expect(await screen.findByText("Wrote sandbox config for Acme Provider to Codex.")).toBeInTheDocument();
  });
});
```

Run:

```powershell
pnpm test:run tests/ProvidersScreen.test.tsx
```

Expected: FAIL because `ProvidersScreen` still does not render provider switching controls.

- [ ] **Step 5: Implement ProvidersScreen**

Replace `src/screens/ProvidersScreen.tsx` with:

```tsx
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "../components/ui/Button";
import { listProviders, listTargetSwitchStatuses, switchTargetProvider } from "../lib/api/client";

export function ProvidersScreen() {
  const queryClient = useQueryClient();
  const providersQuery = useQuery({ queryKey: ["providers"], queryFn: listProviders });
  const targetsQuery = useQuery({
    queryKey: ["target-switch-statuses"],
    queryFn: listTargetSwitchStatuses,
  });
  const [selectedTargets, setSelectedTargets] = useState<Record<string, string>>({});
  const switchMutation = useMutation({
    mutationFn: switchTargetProvider,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["target-switch-statuses"] });
      queryClient.invalidateQueries({ queryKey: ["targets"] });
    },
  });

  const providers = providersQuery.data ?? [];
  const statuses = targetsQuery.data ?? [];
  const switchedTargetName = switchMutation.data
    ? statuses.find((status) => status.target.id === switchMutation.data.target_app_id)?.target.display_name ??
      switchMutation.data.target_key
    : null;

  if (providersQuery.isLoading || targetsQuery.isLoading) {
    return <p className="text-steel">Loading providers...</p>;
  }

  if (providersQuery.error || targetsQuery.error) {
    return <p className="text-ember">Could not load provider switching data.</p>;
  }

  if (providers.length === 0) {
    return (
      <section className="rounded-3xl border border-dashed border-ink/20 bg-white/70 p-8 text-center text-steel">
        No providers yet. Import example JSON to create one.
      </section>
    );
  }

  return (
    <section className="space-y-4">
      <div>
        <h1 className="font-display text-3xl font-semibold text-ink">Providers</h1>
        <p className="text-steel">Switch a provider into a sandbox target config without touching real tool files.</p>
      </div>

      <div className="grid gap-4">
        {providers.map((provider) => {
          const selectedTargetId = selectedTargets[provider.id] ?? statuses[0]?.target.id ?? "";
          const selectedStatus = statuses.find((status) => status.target.id === selectedTargetId);

          return (
            <article key={provider.id} className="rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm">
              <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
                <div>
                  <p className="font-display text-xl font-semibold text-ink">{provider.name}</p>
                  <p className="mt-1 text-sm text-steel">{provider.kind}</p>
                  <p className="mt-2 text-sm text-steel">{provider.base_url ?? "No base URL"}</p>
                  <span className="mt-3 inline-flex rounded-full bg-moss/10 px-3 py-1 text-xs font-semibold uppercase tracking-wide text-moss">
                    {provider.status}
                  </span>
                </div>

                <div className="w-full space-y-3 lg:w-80">
                  <label className="block text-sm font-semibold text-ink">
                    Target for {provider.name}
                    <select
                      aria-label={`Target for ${provider.name}`}
                      value={selectedTargetId}
                      onChange={(event) =>
                        setSelectedTargets((current) => ({
                          ...current,
                          [provider.id]: event.target.value,
                        }))
                      }
                      className="mt-2 w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-sm text-ink outline-none focus:border-moss"
                    >
                      {statuses.map((status) => (
                        <option key={status.target.id} value={status.target.id}>
                          {status.target.display_name}
                        </option>
                      ))}
                    </select>
                  </label>
                  <Button
                    type="button"
                    disabled={!selectedTargetId || switchMutation.isPending}
                    aria-label={`Switch ${provider.name} in sandbox`}
                    onClick={() =>
                      switchMutation.mutate({
                        target_app_id: selectedTargetId,
                        provider_id: provider.id,
                        mode: "sandbox",
                      })
                    }
                  >
                    Switch in sandbox
                  </Button>
                  {selectedStatus?.active_provider && (
                    <p className="text-xs text-steel">
                      Current: {selectedStatus.active_provider.name} on {selectedStatus.target.display_name}
                    </p>
                  )}
                </div>
              </div>
            </article>
          );
        })}
      </div>

      {switchMutation.data && (
        <p className="rounded-2xl bg-moss/10 p-4 text-sm font-medium text-moss">
          Wrote sandbox config for {switchMutation.data.provider_name} to {switchedTargetName}.
        </p>
      )}
      {switchMutation.error && (
        <p className="rounded-2xl bg-ember/10 p-4 text-sm font-medium text-ember">
          Sandbox switch failed.
        </p>
      )}
    </section>
  );
}
```

- [ ] **Step 6: Run ProvidersScreen tests**

Run:

```powershell
pnpm test:run tests/apiClient.test.ts tests/ProvidersScreen.test.tsx
pnpm typecheck
```

Expected: PASS.

- [ ] **Step 7: Commit frontend Providers switching flow**

```powershell
git add src/lib/api src/test/fixtures.ts src/screens/ProvidersScreen.tsx tests/apiClient.test.ts tests/ProvidersScreen.test.tsx
git commit -m "feat: add provider sandbox switching ui"
```

---

### Task 5: Show Target Switch Status And Run Full Verification

**Files:**
- Create: `tests/TargetsScreen.test.tsx`
- Modify: `src/screens/TargetsScreen.tsx`
- Modify: `README.md`

**Interfaces:**
- Consumes: `listTargetSwitchStatuses(): Promise<TargetSwitchStatus[]>`.
- Produces: Targets screen cards with target metadata, active provider, last write status, last error code, last write time, and sandbox path.
- Produces: README B1 verification notes.

- [ ] **Step 1: Write failing TargetsScreen test**

Create `tests/TargetsScreen.test.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { TargetsScreen } from "../src/screens/TargetsScreen";
import { targetSwitchStatusesFixture } from "../src/test/fixtures";
import { listTargetSwitchStatuses } from "../src/lib/api/client";

vi.mock("../src/lib/api/client", () => ({
  listTargetSwitchStatuses: vi.fn(),
}));

function renderWithClient() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
    },
  });

  return render(
    <QueryClientProvider client={queryClient}>
      <TargetsScreen />
    </QueryClientProvider>,
  );
}

describe("TargetsScreen", () => {
  it("shows active provider, write status, and sandbox output path", async () => {
    vi.mocked(listTargetSwitchStatuses).mockResolvedValueOnce(targetSwitchStatusesFixture);

    renderWithClient();

    expect(await screen.findByText("Codex")).toBeInTheDocument();
    expect(screen.getByText("Active provider: Acme Provider")).toBeInTheDocument();
    expect(screen.getByText("Last write: written")).toBeInTheDocument();
    expect(screen.getByText("C:/Users/example/.ai-switch/targets/codex/provider.json")).toBeInTheDocument();
    expect(screen.getByText("Active provider: No provider selected")).toBeInTheDocument();
  });
});
```

Run:

```powershell
pnpm test:run tests/TargetsScreen.test.tsx
```

Expected: FAIL because `TargetsScreen` still calls `listTargetApps` and does not render switch status.

- [ ] **Step 2: Implement TargetsScreen switch status display**

Replace `src/screens/TargetsScreen.tsx` with:

```tsx
import { useQuery } from "@tanstack/react-query";
import { listTargetSwitchStatuses } from "../lib/api/client";

export function TargetsScreen() {
  const targetsQuery = useQuery({
    queryKey: ["target-switch-statuses"],
    queryFn: listTargetSwitchStatuses,
  });

  return (
    <section className="space-y-4">
      <div>
        <h1 className="font-display text-3xl font-semibold text-ink">Targets</h1>
        <p className="text-steel">Sandbox switch state for each supported target app.</p>
      </div>
      {targetsQuery.isLoading && <p className="text-steel">Loading targets...</p>}
      {targetsQuery.error && <p className="text-ember">Could not load targets.</p>}
      <div className="grid gap-3 sm:grid-cols-2">
        {targetsQuery.data?.map((status) => (
          <article key={status.target.id} className="rounded-3xl border border-ink/10 bg-white/70 p-4 shadow-sm">
            <div className="flex items-start justify-between gap-4">
              <div>
                <p className="font-semibold text-ink">{status.target.display_name}</p>
                <p className="text-sm text-steel">{status.target.key}</p>
              </div>
              <span className="rounded-full bg-white px-3 py-1 text-xs font-semibold text-steel">
                {status.target.enabled ? "Enabled" : "Disabled"}
              </span>
            </div>

            <div className="mt-4 space-y-2 text-sm text-steel">
              <p>Active provider: {status.active_provider?.name ?? "No provider selected"}</p>
              <p>Last write: {status.last_write_status ?? "Never written"}</p>
              {status.last_error_code && <p className="text-ember">Last error: {status.last_error_code}</p>}
              {status.last_written_at && <p>Last written at: {status.last_written_at}</p>}
              {status.last_snapshot_path && (
                <p className="break-all rounded-2xl bg-paper/70 p-3 font-mono text-xs text-ink">
                  {status.last_snapshot_path}
                </p>
              )}
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}
```

- [ ] **Step 3: Update README with B1 sandbox verification**

Append this section to `README.md`:

````markdown
## Provider Switching B1

Provider switching B1 writes sandbox target configs only. It does not write real Claude, Codex, Gemini, OpenCode, OpenClaw, or Hermes configuration files.

Sandbox output path:

```text
~/.ai-switch/targets/<target_key>/provider.json
```

Verification flow:

1. Import or create a provider.
2. Open `Providers`.
3. Select a target such as `Codex`.
4. Click `Switch in sandbox`.
5. Open `Targets`.
6. Confirm the target shows the active provider, write status, and sandbox output path.
````

- [ ] **Step 4: Run frontend verification**

Run:

```powershell
pnpm test:run tests/apiClient.test.ts tests/ProvidersScreen.test.tsx tests/TargetsScreen.test.tsx
pnpm typecheck
```

Expected: PASS.

- [ ] **Step 5: Run full backend and frontend verification**

Run:

```powershell
pnpm test:run
pnpm typecheck
pnpm rust:check
pnpm rust:test
```

Expected: PASS.

- [ ] **Step 6: Manual smoke check**

Run:

```powershell
pnpm tauri:dev
```

Expected:

- App window opens.
- Import or create an example provider.
- Open `Providers`.
- Select `Codex`.
- Click `Switch in sandbox`.
- Open `Targets`.
- Codex shows the selected provider and status `written`.
- `~/.ai-switch/targets/codex/provider.json` exists.
- No real external tool config file was modified by this workflow.

- [ ] **Step 7: Commit target status UI and documentation**

```powershell
git add src/screens/TargetsScreen.tsx tests/TargetsScreen.test.tsx README.md
git commit -m "feat: show provider switch target status"
```

---

## Final Implementation Verification

After all tasks are complete, run:

```powershell
git status --short
pnpm test:run
pnpm typecheck
pnpm rust:check
pnpm rust:test
```

Expected:

- `git status --short` is clean after the final commit.
- Frontend tests pass.
- TypeScript typecheck passes.
- Rust check passes.
- Rust tests pass.

Manual smoke verification:

```powershell
pnpm tauri:dev
```

Expected:

- The app starts.
- A provider can be imported or created.
- Providers screen can switch that provider to `Codex` in sandbox mode.
- Targets screen shows active provider and status `written`.
- The sandbox file exists at `~/.ai-switch/targets/codex/provider.json`.
- The workflow does not write any real external tool config path.

## Spec Coverage Map

- List providers in frontend: Task 4.
- Show target active provider state and sandbox path: Task 5.
- Backend provider switch command for provider items: Task 3.
- Render target-specific sandbox config for seven target keys: Task 2.
- Write sandbox configs under `~/.ai-switch/targets/<target_key>/`: Task 3.
- Use `ConfigWriter` for writes: Task 3.
- Record switch attempts in `config_snapshots`: Tasks 1 and 3.
- Update `target_app_states`: Tasks 1 and 3.
- Keep secrets redacted: Task 2.
- Rust tests for success and validation/failure paths: Tasks 1, 2, and 3.
- Frontend tests for provider switching and target status: Tasks 4 and 5.
- No schema migration: Global constraints and Task 1 repository design.
- No real external config writes: Global constraints, Task 3 path helper, and final smoke verification.
