# 按模型维度冷却实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让冷却与异常判定落到 `(账号, 模型)` 粒度，使同账号下某个模型失败时其他模型继续可用，并支持手动暂停单个模型。

**Architecture:** 新增 `route_credential_models` 表承载模型级状态（三态 + 冷却时间戳 + 语义连击）。`route_credentials` 上的 7 个失败字段一列不动、语义不变，只是写入时机变少。选号链路从「装载即分桶」拆成「装载 → 规则过滤 → 模型过滤（顺带算模型键）→ 查模型状态 → 按冷却分桶」，使冷却判定能同时看到账号、模型与请求三者。失败按 `kind` + HTTP 状态码分级：凭证与网络类失败仍冷却整账号，其余只冷却命中的模型；一个账号的全部已知模型都不可用时自动升级为账号级冷却。

**Tech Stack:** Rust（Tauri 2、sqlx 0.8 + SQLite、axum 0.7、tokio 1、chrono）、TypeScript + React 18（Vite 5、@tanstack/react-query 5、Vitest 2 + jsdom + @testing-library/react、UnoCSS）。包管理 pnpm 10.12.4。

**Spec:** `docs/superpowers/specs/2026-09-02-per-model-cooldown-design.md`

## Global Constraints

- **构建目录**：AI 执行任何 Rust 命令必须带 `CARGO_TARGET_DIR=target-codex`（`AGENTS.md:6-10` 硬性要求）。只允许 `src-tauri/target/`（人工 dev）与 `src-tauri/target-codex/`（AI），禁止创建第三个。
- **工作分支**：当前分支 `task/8`。可自由提交，但**不得** merge 到、rebase 到或 push 基线分支 `main`。
- **迁移文件不可改**：已提交的 migration 改动会导致 checksum 冲突，`open_migrated_pool()` 会把用户旧库隔离进 `backups/` 并重建——那是丢数据的兜底，不是开发手段。新迁移文件前缀必须大于 `202608200001`。
- **迁移写法**：首行 `PRAGMA foreign_keys = ON;`（现有 24 个文件都如此）。`NOT NULL` 列必须带 `DEFAULT`（SQLite 限制）。
- **Rust 格式**：`src-tauri/rustfmt.toml` 指定 `edition/style_edition = 2021`、`newline_style = Unix`。提交前跑 `cargo fmt`。
- **命令契约三处同步**：新增 Tauri command 必须同时改 `src/lib/api/client.ts`、`src-tauri/src/lib.rs` 的 `generate_handler![]`、`src-tauri/src/web/handlers/mod.rs`，否则 `tests/transport/command-contract.test.ts` 失败。
- **前端无 i18n**：`src/screens/AccountsScreen.tsx` 全中文硬编码，本次沿用，不新建 i18n key。
- **注释语言**：Rust 与 TS 代码注释一律英文（与现有代码一致）。文档与规格用中文。
- **提交信息**：中文、`type: 摘要` 格式（参考 `git log`：`feat: 账号临时失败显示错误次数并支持配置冷却秒数`）。
- **数据库连接**：`foreign_keys(true)` 已在 `src-tauri/src/database/mod.rs:28` 与 `:51` 开启，`ON DELETE CASCADE` 在测试内存库里同样生效。
- **测试库构造**：Rust 测试统一 `crate::database::create_memory_pool()` + `crate::database::run_migrations(&pool)`。

## 术语

- **模型键（model_key）**：模型级状态的主键之一。api 账号取 `resolve_mapping_target` 解析出的上游 `to`；official 与空映射账号取请求原名；两者都再剥掉 `[1m]` 后缀。
- **账号级失败 / 模型级失败**：见 Task 4 的分级表。
- **升级（escalate）**：一个账号的全部非 `paused` 已知模型都不可用时，顺带写账号级冷却。

---

## File Structure

**新建：**

| 文件 | 职责 |
|---|---|
| `src-tauri/migrations/202609020002_route_credential_models.sql` | 建 `route_credential_models` 表与查询索引 |
| `src-tauri/src/models/route_credential_model.rs` | `RouteCredentialModelState` 行结构、`ModelStatus` 常量、`FailureScope` 枚举 |
| `src-tauri/src/database/repositories/route_credential_model_repository.rs` | 模型级状态的全部 SQL：批量读、写冷却、写语义连击、清除、设状态、按账号列出 |
| `src-tauri/src/services/route_failure_scope.rs` | `is_account_scoped_failure(kind, status)` 分级判定，纯函数 + 单测 |

**修改：**

| 文件 | 改动 |
|---|---|
| `src-tauri/src/models/mod.rs:7` 附近 | 挂 `pub mod route_credential_model;` |
| `src-tauri/src/database/repositories/mod.rs:6` 附近 | 挂 `pub mod route_credential_model_repository;` |
| `src-tauri/src/services/mod.rs` | 挂 `pub mod route_failure_scope;` |
| `src-tauri/src/models/route_credential.rs:136` | `RouteCredential` 加 `#[sqlx(skip)] model_states` |
| `src-tauri/src/services/route_model_capability.rs` | 加 `model_state_key`、`known_upstream_models` |
| `src-tauri/src/services/route_proxy_service.rs:2520-2650` | 拆 `select_pool_credentials`，`PoolCandidate` 携带 `model_key` |
| `src-tauri/src/services/route_proxy_service.rs:539-1460` | 转发链路改用候选形态；9 处记账点传 scope |
| `src-tauri/src/services/route_model_test_service.rs:1278-1360` | `finish_outcome` 按模型记账/清账 |
| `src-tauri/src/services/route_recovery_service.rs:99-171` | `needs_recovery` 加条件；Healthcheck 显式选探测模型 |
| `src-tauri/src/services/route_credential_service.rs:49-90` | `list`/`get`/`page` 后批量填充 `model_states` |
| `src-tauri/src/commands/route_credential_commands.rs` | 两个新 command |
| `src-tauri/src/lib.rs:439-458` | 注册两个新 command |
| `src-tauri/src/web/handlers/mod.rs:495` 附近 | 两个新 command 的 HTTP 分发 |
| `src/lib/api/types.ts:172-210` | `RouteCredentialModelState` 类型、`RouteCredential.model_states` |
| `src/lib/api/client.ts:345` 附近 | 两个新 command 的封装 |
| `src/screens/AccountsScreen.tsx` | 行内徽章、悬停明细、抽屉区块、倒计时扩展 |
| `tests/AccountsScreen.test.tsx` | 前端回归 |
| `docs-site/docs/guide/reliability.md`、`accounts.md` 及 `en/` 镜像 | 文档同步 |

**为什么模型级状态单独开仓储文件**：`route_credential_repository.rs` 已 3000+ 行。模型级状态自成一个查询边界（一张表、一组 CRUD），放进现有文件只会让它更难改。`route_failure_scope.rs` 独立是因为它被代理、模型测试两个服务共用，放任一边都会造成反向依赖。

---

### Task 1: 建表与行结构

**Files:**
- Create: `src-tauri/migrations/202609020002_route_credential_models.sql`
- Create: `src-tauri/src/models/route_credential_model.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Test: `src-tauri/src/database/test_support.rs`（追加一个 `#[tokio::test]`）

**Interfaces:**
- Consumes: 无（首个任务）
- Produces:
  - `crate::models::route_credential_model::RouteCredentialModelState`，字段：`route_credential_id: String`、`model_key: String`、`status: String`、`transient_failure_count: i64`、`cooldown_until: Option<String>`、`semantic_failure_streak_count: i64`、`semantic_failure_streak_fingerprint: Option<String>`、`last_failure_kind: Option<String>`、`last_failure_message: Option<String>`、`last_failure_response_json: Option<String>`、`created_at: String`、`updated_at: String`。派生 `Debug, Clone, Serialize, Deserialize, FromRow, PartialEq`。
  - 常量 `MODEL_STATUS_OK: &str = "ok"`、`MODEL_STATUS_ERROR: &str = "error"`、`MODEL_STATUS_PAUSED: &str = "paused"`
  - 表 `route_credential_models`，主键 `(route_credential_id, model_key)`

- [ ] **Step 1: 写失败的测试**

在 `src-tauri/src/database/test_support.rs` 末尾追加：

```rust
#[tokio::test]
async fn migrations_create_route_credential_models_table() {
    let pool = create_memory_pool().await.expect("pool");
    run_migrations(&pool).await.expect("migrations");

    let columns = sqlx::query("PRAGMA table_info(route_credential_models)")
        .fetch_all(&pool)
        .await
        .expect("model cooldown columns");
    let names: Vec<String> = columns
        .iter()
        .map(|column| column.get::<String, _>("name"))
        .collect();
    for expected in [
        "route_credential_id",
        "model_key",
        "status",
        "transient_failure_count",
        "cooldown_until",
        "semantic_failure_streak_count",
        "semantic_failure_streak_fingerprint",
        "last_failure_kind",
        "last_failure_message",
        "last_failure_response_json",
        "created_at",
        "updated_at",
    ] {
        assert!(names.iter().any(|name| name == expected), "missing {expected}");
    }

    // status is constrained to the three model-level states.
    sqlx::query(
        "INSERT INTO route_credentials
         (id, platform, kind, display_name, status, sort_order, secret_payload_json,
          config_json, preview_json, created_at, updated_at)
         VALUES ('cred-1', 'codex', 'api', 'Fixture', 'ok', 0, '{}', '{}', '{}', '2026-09-02T00:00:00Z', '2026-09-02T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect("seed credential");
    let rejected = sqlx::query(
        "INSERT INTO route_credential_models
         (route_credential_id, model_key, status, created_at, updated_at)
         VALUES ('cred-1', 'gpt-5.6-sol', 'revoked', '2026-09-02T00:00:00Z', '2026-09-02T00:00:00Z')",
    )
    .execute(&pool)
    .await;
    assert!(rejected.is_err(), "status must reject values outside the three model states");

    sqlx::query(
        "INSERT INTO route_credential_models
         (route_credential_id, model_key, status, created_at, updated_at)
         VALUES ('cred-1', 'gpt-5.6-sol', 'paused', '2026-09-02T00:00:00Z', '2026-09-02T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect("insert paused model");
    sqlx::query("DELETE FROM route_credentials WHERE id = 'cred-1'")
        .execute(&pool)
        .await
        .expect("delete credential");
    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM route_credential_models")
            .fetch_one(&pool)
            .await
            .expect("count after cascade");
    assert_eq!(remaining, 0, "deleting an account must cascade its model rows");
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test migrations_create_route_credential_models_table`
Expected: FAIL —— `PRAGMA table_info` 返回空表，`missing route_credential_id` 断言失败。

- [ ] **Step 3: 写迁移**

创建 `src-tauri/migrations/202609020002_route_credential_models.sql`：

```sql
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS route_credential_models (
  route_credential_id TEXT NOT NULL,
  model_key TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'ok' CHECK (status IN ('ok', 'error', 'paused')),
  transient_failure_count INTEGER NOT NULL DEFAULT 0,
  cooldown_until TEXT,
  semantic_failure_streak_count INTEGER NOT NULL DEFAULT 0,
  semantic_failure_streak_fingerprint TEXT,
  last_failure_kind TEXT,
  last_failure_message TEXT,
  last_failure_response_json TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (route_credential_id, model_key),
  FOREIGN KEY (route_credential_id) REFERENCES route_credentials(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_route_credential_models_lookup
  ON route_credential_models(route_credential_id, status, cooldown_until);
```

- [ ] **Step 4: 写行结构**

创建 `src-tauri/src/models/route_credential_model.rs`：

```rust
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Healthy: selectable, no backoff. Rows in this state exist only transiently
/// (a success deletes them), so it is mostly the value synthesised for models
/// that have no row at all.
pub const MODEL_STATUS_OK: &str = "ok";
/// Set automatically when a semantic failure streak reaches the account's
/// `semantic_error_threshold`. Hard-excluded from selection, never probed.
pub const MODEL_STATUS_ERROR: &str = "error";
/// Set only by the user. Survives success, scheduled recovery and account-level
/// reactivation — automation must not override an explicit human decision.
pub const MODEL_STATUS_PAUSED: &str = "paused";

/// Per-(account, model) failure state. Mirrors the account-level columns on
/// `route_credentials` minus the redundant second timestamp: the account level
/// writes `next_retry_at` and `cooldown_until` the same value, so one suffices.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq)]
pub struct RouteCredentialModelState {
    pub route_credential_id: String,
    pub model_key: String,
    pub status: String,
    pub transient_failure_count: i64,
    pub cooldown_until: Option<String>,
    pub semantic_failure_streak_count: i64,
    pub semantic_failure_streak_fingerprint: Option<String>,
    pub last_failure_kind: Option<String>,
    pub last_failure_message: Option<String>,
    pub last_failure_response_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
```

在 `src-tauri/src/models/mod.rs` 的 `pub mod route_credential;`（第 7 行）之后插入一行，保持字母序：

```rust
pub mod route_credential_model;
```

- [ ] **Step 5: 跑测试确认通过**

Run: `cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test migrations_create_route_credential_models_table`
Expected: PASS

- [ ] **Step 6: 全量迁移测试与格式化**

Run: `cd src-tauri && CARGO_TARGET_DIR=target-codex cargo fmt && CARGO_TARGET_DIR=target-codex cargo test database::`
Expected: PASS，无 fmt 差异。

- [ ] **Step 7: 提交**

```bash
git add src-tauri/migrations/202609020002_route_credential_models.sql src-tauri/src/models/route_credential_model.rs src-tauri/src/models/mod.rs src-tauri/src/database/test_support.rs
git commit -m "feat: 新增模型级失败状态表"
```

---

### Task 2: 模型键与已知模型集合

**Files:**
- Modify: `src-tauri/src/services/route_model_capability.rs`（在 `resolve_mapping_target` 之后、`advertised_model_ids` 之前插入两个函数；测试加在文件末尾的 `mod tests` 里）
- Test: `src-tauri/src/services/route_model_capability.rs` 的 `#[cfg(test)] mod tests`（起于 `:344`）

**Interfaces:**
- Consumes: Task 1 无关。本任务只依赖现有的 `ModelCapability`、`resolve_mapping_target`（`:186`）、`default_client_models`（`:255`）、`is_fallback_mapping`、`strip_one_m_suffix_for_route_lookup`（`:326`）。
- Produces:
  - `pub(crate) fn model_state_key(platform: &str, capability: &ModelCapability, kind: &str, requested_model: &str) -> String`
  - `pub(crate) fn known_upstream_models(platform: &str, capability: &ModelCapability, kind: &str) -> Vec<String>`

`kind` 是账号的 `route_credentials.kind`，取值 `"official"` 或 `"api"`。

- [ ] **Step 1: 写失败的测试**

在 `src-tauri/src/services/route_model_capability.rs` 的 `mod tests` 内追加。注意 `use super::{...}` 那一行（`:346-350`）要补上两个新函数名：

```rust
    #[test]
    fn model_state_key_uses_the_mapped_upstream_model_for_api_accounts() {
        let capability = parse_model_capability(
            r#"{"model_mappings":[{"from":"gpt-5.6-sol","to":"upstream-sol"}]}"#,
        );
        assert_eq!(
            model_state_key("codex", &capability, "api", "gpt-5.6-sol"),
            "upstream-sol"
        );
    }

    #[test]
    fn model_state_key_collapses_catch_all_aliases_onto_one_key() {
        let capability = parse_model_capability(&format!(
            r#"{{"model_mappings":[{{"from":"{FALLBACK_MODEL_ALIAS}","to":"upstream-any"}}]}}"#
        ));
        // Every client-side name funnels into the same upstream model, so one
        // failure parks it for all of them instead of once per alias.
        assert_eq!(
            model_state_key("claude", &capability, "api", "claude-sonnet-alias"),
            "upstream-any"
        );
        assert_eq!(
            model_state_key("claude", &capability, "api", "whatever-else"),
            "upstream-any"
        );
    }

    #[test]
    fn model_state_key_keeps_the_requested_name_for_official_and_empty_mappings() {
        let empty = parse_model_capability(r#"{"model_mappings":[]}"#);
        assert_eq!(
            model_state_key("codex", &empty, "api", "gpt-5.6-sol"),
            "gpt-5.6-sol"
        );

        // build_official_upstream_request never rewrites the model, so an
        // official account's key must be the name the client sent.
        let official = parse_model_capability(
            r#"{"model_mappings":[{"from":"gpt-5.6-sol","to":"upstream-sol"}]}"#,
        );
        assert_eq!(
            model_state_key("codex", &official, "official", "gpt-5.6-sol"),
            "gpt-5.6-sol"
        );
    }

    #[test]
    fn model_state_key_strips_the_one_m_suffix() {
        let empty = parse_model_capability(r#"{"model_mappings":[]}"#);
        // Same upstream model, only a different beta header — one cooldown.
        assert_eq!(
            model_state_key("claude", &empty, "api", "claude-sonnet-alias[1m]"),
            model_state_key("claude", &empty, "api", "claude-sonnet-alias")
        );
    }

    #[test]
    fn known_upstream_models_returns_the_platform_baseline_without_mappings() {
        let empty = parse_model_capability(r#"{"model_mappings":[]}"#);
        let models = known_upstream_models("codex", &empty, "api");
        assert!(models.contains(&"gpt-5.6-sol".to_string()));
        assert!(models.contains(&"gpt-5.5".to_string()));
    }

    #[test]
    fn known_upstream_models_dedupes_targets_and_includes_the_catch_all_target() {
        let capability = parse_model_capability(&format!(
            r#"{{"model_mappings":[
                {{"from":"gpt-5.6-sol","to":"upstream-a"}},
                {{"from":"glm-5.3","to":"upstream-a"}},
                {{"from":"gpt-5.5","to":"upstream-b"}},
                {{"from":"{FALLBACK_MODEL_ALIAS}","to":"upstream-any"}}
            ]}}"#
        ));
        let mut models = known_upstream_models("codex", &capability, "api");
        models.sort();
        assert_eq!(models, vec!["upstream-a", "upstream-any", "upstream-b"]);
    }

    #[test]
    fn known_upstream_models_ignores_mappings_for_official_accounts() {
        let capability = parse_model_capability(
            r#"{"model_mappings":[{"from":"gpt-5.6-sol","to":"upstream-sol"}]}"#,
        );
        let models = known_upstream_models("codex", &capability, "official");
        assert!(models.contains(&"gpt-5.6-sol".to_string()));
        assert!(!models.contains(&"upstream-sol".to_string()));
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test route_model_capability`
Expected: FAIL —— 编译错误 `cannot find function model_state_key in this scope`。

- [ ] **Step 3: 实现两个函数**

在 `src-tauri/src/services/route_model_capability.rs` 的 `resolve_mapping_target`（结束于 `:197`）之后插入：

```rust
/// The key a `(account, model)` failure state is recorded under.
///
/// For `api` accounts this is the upstream model the request is rewritten to, so
/// a relay that rate-limits one upstream model parks exactly that one — and a
/// catch-all mapping funnels every client alias onto a single key instead of
/// letting each alias hit the wall separately.
///
/// `official` accounts never get their model rewritten
/// (`build_official_upstream_request`), so their key is the requested name.
pub(crate) fn model_state_key(
    platform: &str,
    capability: &ModelCapability,
    kind: &str,
    requested_model: &str,
) -> String {
    let requested = strip_one_m_suffix_for_route_lookup(requested_model);
    if kind == "official" {
        return requested.to_string();
    }
    let _ = platform;
    resolve_mapping_target(&capability.mappings, requested)
        .map(|target| strip_one_m_suffix_for_route_lookup(target).to_string())
        .unwrap_or_else(|| requested.to_string())
}

/// Every model key this account could ever produce. Used as the denominator when
/// deciding whether an account-level escalation is due, and to list models the
/// user may pause before any of them has failed.
pub(crate) fn known_upstream_models(
    platform: &str,
    capability: &ModelCapability,
    kind: &str,
) -> Vec<String> {
    if kind == "official" || capability.mappings.is_empty() {
        return default_client_models(platform)
            .iter()
            .map(|model| (*model).to_string())
            .collect();
    }

    let mut models = Vec::new();
    for mapping in &capability.mappings {
        let target = strip_one_m_suffix_for_route_lookup(&mapping.to);
        if target.is_empty() || models.iter().any(|model| model == target) {
            continue;
        }
        models.push(target.to_string());
    }
    models
}
```

`let _ = platform;` 保留参数是为了让签名与 `known_upstream_models` 对称、调用点无需记两套参数顺序。若 clippy 报未使用参数，改为 `_platform: &str`。

- [ ] **Step 4: 跑测试确认通过**

Run: `cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test route_model_capability`
Expected: PASS，包含 6 个新测试。

- [ ] **Step 5: 格式化并提交**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo fmt
cd .. && git add src-tauri/src/services/route_model_capability.rs
git commit -m "feat: 解析模型级冷却键与账号已知模型集合"
```

---

### Task 3: 模型级状态仓储

**Files:**
- Create: `src-tauri/src/database/repositories/route_credential_model_repository.rs`
- Modify: `src-tauri/src/database/repositories/mod.rs`
- Modify: `src-tauri/src/database/repositories/route_credential_repository.rs`（把 `truncate_failure_message`、`semantic_failure_fingerprint`、`truncate_failure_response`、`database_error` 从私有改为 `pub(crate)`，供新仓储复用）
- Test: `src-tauri/src/database/repositories/route_credential_model_repository.rs` 内的 `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: Task 1 的 `RouteCredentialModelState`、`MODEL_STATUS_*` 常量与表结构。
- Produces: `RouteCredentialModelRepository`，方法：
  - `load_states(pool, keys: &[(String, String)]) -> Result<HashMap<(String, String), RouteCredentialModelState>, AppError>`
  - `list_for_credentials(pool, credential_ids: &[String]) -> Result<Vec<RouteCredentialModelState>, AppError>`
  - `record_transient_failure(tx: &mut SqliteConnection, credential_id, model_key, kind, message, response_body: Option<&[u8]>, cooldown_seconds: Option<u32>, response_status: Option<u16>, semantic_error_threshold: i64, error_status_enabled: bool) -> Result<(), AppError>`
  - `clear(pool, credential_id, model_key) -> Result<(), AppError>`
  - `clear_all_unpaused(tx_or_pool, credential_id) -> Result<(), AppError>`（接 `&mut SqliteConnection`）
  - `set_status(pool, credential_id, model_key, status: &str) -> Result<(), AppError>`
  - `unavailable_keys(tx: &mut SqliteConnection, credential_id, now_rfc3339: &str) -> Result<Vec<String>, AppError>`
  - `oldest_recoverable_key(pool, credential_id, now_rfc3339: &str) -> Result<Option<String>, AppError>`
  - `has_unpaused_rows(pool, credential_id) -> Result<bool, AppError>`

`record_transient_failure` 收 `&mut SqliteConnection` 而非 `&SqlitePool`，因为 Task 4 需要它与账号级升级判定在同一事务内执行。`cooldown_seconds` 为 `None` 表示该账号关闭了冷却（只累计次数、不写时间戳）。

- [ ] **Step 1: 写失败的测试**

创建文件时把测试一起写进去（下一步的实现放同一文件的上半部分）。测试内容：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};

    async fn seed(pool: &SqlitePool) -> String {
        sqlx::query(
            "INSERT INTO route_credentials
             (id, platform, kind, display_name, status, sort_order, secret_payload_json,
              config_json, preview_json, created_at, updated_at)
             VALUES ('cred-1', 'codex', 'api', 'Fixture', 'ok', 0, '{}', '{}', '{}',
                     '2026-09-02T00:00:00Z', '2026-09-02T00:00:00Z')",
        )
        .execute(pool)
        .await
        .expect("seed credential");
        "cred-1".to_string()
    }

    #[tokio::test]
    async fn transient_failure_writes_a_cooldown_and_success_deletes_the_row() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        let mut conn = pool.acquire().await.expect("conn");
        RouteCredentialModelRepository::record_transient_failure(
            &mut conn, &id, "upstream-sol", "upstream_status",
            "upstream returned 429", None, Some(30), Some(429), 10, true,
        )
        .await
        .expect("record");
        drop(conn);

        let states = RouteCredentialModelRepository::list_for_credentials(&pool, &[id.clone()])
            .await
            .expect("list");
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].model_key, "upstream-sol");
        assert_eq!(states[0].status, MODEL_STATUS_OK);
        assert_eq!(states[0].transient_failure_count, 1);
        assert!(states[0].cooldown_until.is_some());
        assert_eq!(states[0].last_failure_kind.as_deref(), Some("upstream_status"));

        RouteCredentialModelRepository::clear(&pool, &id, "upstream-sol")
            .await
            .expect("clear");
        assert!(RouteCredentialModelRepository::list_for_credentials(&pool, &[id])
            .await
            .expect("list after clear")
            .is_empty());
    }

    #[tokio::test]
    async fn disabled_cooldown_counts_without_parking_the_model() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        let mut conn = pool.acquire().await.expect("conn");
        RouteCredentialModelRepository::record_transient_failure(
            &mut conn, &id, "upstream-sol", "upstream_status", "boom", None, None, Some(500), 10, true,
        )
        .await
        .expect("record");
        drop(conn);

        let states = RouteCredentialModelRepository::list_for_credentials(&pool, &[id])
            .await
            .expect("list");
        assert_eq!(states[0].transient_failure_count, 1);
        assert!(states[0].cooldown_until.is_none());
    }

    #[tokio::test]
    async fn clear_keeps_a_paused_row_but_resets_its_failure_fields() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        RouteCredentialModelRepository::set_status(&pool, &id, "upstream-sol", MODEL_STATUS_PAUSED)
            .await
            .expect("pause");
        let mut conn = pool.acquire().await.expect("conn");
        RouteCredentialModelRepository::record_transient_failure(
            &mut conn, &id, "upstream-sol", "upstream_status", "boom", None, Some(30), Some(429), 10, true,
        )
        .await
        .expect("record");
        drop(conn);

        RouteCredentialModelRepository::clear(&pool, &id, "upstream-sol")
            .await
            .expect("clear");
        let states = RouteCredentialModelRepository::list_for_credentials(&pool, &[id])
            .await
            .expect("list");
        // A success must not silently un-pause what the user paused.
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].status, MODEL_STATUS_PAUSED);
        assert_eq!(states[0].transient_failure_count, 0);
        assert!(states[0].cooldown_until.is_none());
    }

    #[tokio::test]
    async fn repeated_semantic_failures_flip_the_model_to_error_at_the_threshold() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        for expected in 1..=3 {
            let mut conn = pool.acquire().await.expect("conn");
            RouteCredentialModelRepository::record_transient_failure(
                &mut conn, &id, "upstream-sol", "semantic_response_transient",
                "content blocked", None, Some(30), Some(200), 3, true,
            )
            .await
            .expect("record");
            drop(conn);
            let states = RouteCredentialModelRepository::list_for_credentials(&pool, &[id.clone()])
                .await
                .expect("list");
            assert_eq!(states[0].semantic_failure_streak_count, expected);
            // Cooldown and streak accumulate together: unlike the account-level
            // pair of functions these are not mutually exclusive, otherwise a
            // cooling model could never reach the threshold.
            assert!(states[0].cooldown_until.is_some());
            let expected_status = if expected >= 3 { MODEL_STATUS_ERROR } else { MODEL_STATUS_OK };
            assert_eq!(states[0].status, expected_status);
        }
    }

    #[tokio::test]
    async fn a_different_failure_fingerprint_restarts_the_streak() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        for message in ["first reason", "first reason", "second reason"] {
            let mut conn = pool.acquire().await.expect("conn");
            RouteCredentialModelRepository::record_transient_failure(
                &mut conn, &id, "upstream-sol", "semantic_response_transient",
                message, None, Some(30), Some(200), 3, true,
            )
            .await
            .expect("record");
            drop(conn);
        }
        let states = RouteCredentialModelRepository::list_for_credentials(&pool, &[id])
            .await
            .expect("list");
        assert_eq!(states[0].semantic_failure_streak_count, 1);
        assert_eq!(states[0].status, MODEL_STATUS_OK);
    }

    #[tokio::test]
    async fn error_status_toggle_off_keeps_counting_without_flipping_status() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        for _ in 0..5 {
            let mut conn = pool.acquire().await.expect("conn");
            RouteCredentialModelRepository::record_transient_failure(
                &mut conn, &id, "upstream-sol", "semantic_response_transient",
                "blocked", None, Some(30), Some(200), 2, false,
            )
            .await
            .expect("record");
            drop(conn);
        }
        let states = RouteCredentialModelRepository::list_for_credentials(&pool, &[id])
            .await
            .expect("list");
        assert_eq!(states[0].status, MODEL_STATUS_OK);
        assert!(states[0].semantic_failure_streak_count >= 2);
    }

    #[tokio::test]
    async fn unavailable_keys_reports_cooling_error_and_paused_models() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        let mut conn = pool.acquire().await.expect("conn");
        RouteCredentialModelRepository::record_transient_failure(
            &mut conn, &id, "cooling", "upstream_status", "boom", None, Some(600), Some(429), 10, true,
        )
        .await
        .expect("cooling");
        RouteCredentialModelRepository::record_transient_failure(
            &mut conn, &id, "expired", "upstream_status", "boom", None, None, Some(429), 10, true,
        )
        .await
        .expect("expired");
        drop(conn);
        RouteCredentialModelRepository::set_status(&pool, &id, "paused", MODEL_STATUS_PAUSED)
            .await
            .expect("pause");

        let mut conn = pool.acquire().await.expect("conn");
        let mut keys = RouteCredentialModelRepository::unavailable_keys(
            &mut conn, &id, "2026-09-02T00:00:00Z",
        )
        .await
        .expect("unavailable");
        drop(conn);
        keys.sort();
        // "expired" has no cooldown timestamp at all, so it stays selectable.
        assert_eq!(keys, vec!["cooling", "paused"]);
    }

    #[tokio::test]
    async fn clear_all_unpaused_keeps_paused_rows() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        let mut conn = pool.acquire().await.expect("conn");
        RouteCredentialModelRepository::record_transient_failure(
            &mut conn, &id, "cooling", "upstream_status", "boom", None, Some(600), Some(429), 10, true,
        )
        .await
        .expect("cooling");
        drop(conn);
        RouteCredentialModelRepository::set_status(&pool, &id, "held", MODEL_STATUS_PAUSED)
            .await
            .expect("pause");

        let mut conn = pool.acquire().await.expect("conn");
        RouteCredentialModelRepository::clear_all_unpaused(&mut conn, &id)
            .await
            .expect("clear all");
        drop(conn);

        let states = RouteCredentialModelRepository::list_for_credentials(&pool, &[id])
            .await
            .expect("list");
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].model_key, "held");
    }

    #[tokio::test]
    async fn load_states_only_returns_the_requested_pairs() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        let mut conn = pool.acquire().await.expect("conn");
        for key in ["a", "b"] {
            RouteCredentialModelRepository::record_transient_failure(
                &mut conn, &id, key, "upstream_status", "boom", None, Some(30), Some(429), 10, true,
            )
            .await
            .expect("record");
        }
        drop(conn);

        let states = RouteCredentialModelRepository::load_states(
            &pool,
            &[(id.clone(), "a".to_string()), (id.clone(), "missing".to_string())],
        )
        .await
        .expect("load");
        assert_eq!(states.len(), 1);
        assert!(states.contains_key(&(id, "a".to_string())));
    }

    #[tokio::test]
    async fn oldest_recoverable_key_prefers_the_stalest_expired_model() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        for (key, updated_at) in [("stale", "2026-09-01T00:00:00Z"), ("fresh", "2026-09-02T00:00:00Z")] {
            sqlx::query(
                "INSERT INTO route_credential_models
                 (route_credential_id, model_key, status, transient_failure_count,
                  cooldown_until, created_at, updated_at)
                 VALUES (?, ?, 'ok', 1, NULL, ?, ?)",
            )
            .bind(&id)
            .bind(key)
            .bind(updated_at)
            .bind(updated_at)
            .execute(&pool)
            .await
            .expect("seed model row");
        }
        sqlx::query(
            "INSERT INTO route_credential_models
             (route_credential_id, model_key, status, created_at, updated_at)
             VALUES (?, 'held', 'paused', '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
        )
        .bind(&id)
        .execute(&pool)
        .await
        .expect("seed paused");

        let key = RouteCredentialModelRepository::oldest_recoverable_key(
            &pool, &id, "2026-09-03T00:00:00Z",
        )
        .await
        .expect("oldest");
        // A paused row is older still, but probing it would fight the user.
        assert_eq!(key.as_deref(), Some("stale"));
    }

    #[tokio::test]
    async fn has_unpaused_rows_ignores_paused_only_accounts() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = seed(&pool).await;

        RouteCredentialModelRepository::set_status(&pool, &id, "held", MODEL_STATUS_PAUSED)
            .await
            .expect("pause");
        assert!(!RouteCredentialModelRepository::has_unpaused_rows(&pool, &id)
            .await
            .expect("paused only"));

        let mut conn = pool.acquire().await.expect("conn");
        RouteCredentialModelRepository::record_transient_failure(
            &mut conn, &id, "cooling", "upstream_status", "boom", None, Some(600), Some(429), 10, true,
        )
        .await
        .expect("cooling");
        drop(conn);
        assert!(RouteCredentialModelRepository::has_unpaused_rows(&pool, &id)
            .await
            .expect("with cooling"));
    }
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test route_credential_model_repository`
Expected: FAIL —— `route_credential_model_repository` 模块未挂载 / `RouteCredentialModelRepository` 未定义。

- [ ] **Step 3: 放开三个辅助函数的可见性**

在 `src-tauri/src/database/repositories/route_credential_repository.rs` 把四处签名改为 `pub(crate)`：

- `:107` `fn database_error(` → `pub(crate) fn database_error(`
- `:1748` `fn truncate_failure_message(` → `pub(crate) fn truncate_failure_message(`
- `:1758` `fn semantic_failure_fingerprint(` → `pub(crate) fn semantic_failure_fingerprint(`
- `:1778` `fn truncate_failure_response(` → `pub(crate) fn truncate_failure_response(`

复用而非复制：指纹算法必须与账号级完全一致，否则同一个失败在两级会被算成不同指纹。

- [ ] **Step 4: 写仓储实现**

创建 `src-tauri/src/database/repositories/route_credential_model_repository.rs`，实现放在 Step 1 的 `mod tests` 之前：

```rust
use crate::database::repositories::route_credential_repository::{
    database_error, semantic_failure_fingerprint, truncate_failure_message,
    truncate_failure_response,
};
use crate::error::AppError;
use crate::models::route_credential_model::{
    RouteCredentialModelState, MODEL_STATUS_ERROR, MODEL_STATUS_OK, MODEL_STATUS_PAUSED,
};
use chrono::Utc;
use sqlx::{QueryBuilder, Sqlite, SqliteConnection, SqlitePool};
use std::collections::HashMap;

const STATE_SELECT: &str = "SELECT route_credential_id, model_key, status,
    transient_failure_count, cooldown_until, semantic_failure_streak_count,
    semantic_failure_streak_fingerprint, last_failure_kind, last_failure_message,
    last_failure_response_json, created_at, updated_at
 FROM route_credential_models";

pub struct RouteCredentialModelRepository;

impl RouteCredentialModelRepository {
    /// Batch-load exactly the `(account, model)` pairs a request needs. Two
    /// accounts can map the same requested model to different upstream names, so
    /// the key is the pair — never the model alone.
    pub async fn load_states(
        pool: &SqlitePool,
        keys: &[(String, String)],
    ) -> Result<HashMap<(String, String), RouteCredentialModelState>, AppError> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }
        let mut builder = QueryBuilder::<Sqlite>::new(STATE_SELECT);
        builder.push(" WHERE (route_credential_id, model_key) IN (");
        let mut separated = builder.separated(", ");
        for (credential_id, model_key) in keys {
            separated.push("(");
            separated.push_bind_unseparated(credential_id);
            separated.push_unseparated(", ");
            separated.push_bind_unseparated(model_key);
            separated.push_unseparated(")");
        }
        builder.push(")");
        let states = builder
            .build_query_as::<RouteCredentialModelState>()
            .fetch_all(pool)
            .await
            .map_err(|err| {
                database_error(
                    "database.route_credential_model_states",
                    "Could not load per-model failure state",
                    err,
                )
            })?;
        Ok(states
            .into_iter()
            .map(|state| {
                (
                    (state.route_credential_id.clone(), state.model_key.clone()),
                    state,
                )
            })
            .collect())
    }

    pub async fn list_for_credentials(
        pool: &SqlitePool,
        credential_ids: &[String],
    ) -> Result<Vec<RouteCredentialModelState>, AppError> {
        if credential_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut builder = QueryBuilder::<Sqlite>::new(STATE_SELECT);
        builder.push(" WHERE route_credential_id IN (");
        let mut separated = builder.separated(", ");
        for credential_id in credential_ids {
            separated.push_bind(credential_id);
        }
        builder.push(") ORDER BY route_credential_id, model_key");
        builder
            .build_query_as::<RouteCredentialModelState>()
            .fetch_all(pool)
            .await
            .map_err(|err| {
                database_error(
                    "database.route_credential_model_list",
                    "Could not list per-model failure state",
                    err,
                )
            })
    }
}
```

- [ ] **Step 5: 写记账方法**

在 `impl RouteCredentialModelRepository` 内继续追加：

```rust
    /// Record one model-scoped failure. Unlike the account-level pair of
    /// functions, the cooldown window and the semantic streak accumulate
    /// together — keeping them mutually exclusive would mean a cooling model
    /// could never reach `semantic_error_threshold`.
    ///
    /// `cooldown_seconds` is `None` when the account has cooldown switched off:
    /// the failure is still counted, the model just stays selectable.
    #[allow(clippy::too_many_arguments)]
    pub async fn record_transient_failure(
        conn: &mut SqliteConnection,
        credential_id: &str,
        model_key: &str,
        kind: &str,
        message: &str,
        response_body: Option<&[u8]>,
        cooldown_seconds: Option<u32>,
        response_status: Option<u16>,
        semantic_error_threshold: i64,
        error_status_enabled: bool,
    ) -> Result<(), AppError> {
        let now = Utc::now();
        let now_text = now.to_rfc3339();
        let cooldown_until = cooldown_seconds.map(|seconds| {
            (now + chrono::Duration::seconds(i64::from(seconds))).to_rfc3339()
        });
        let fingerprint = semantic_failure_fingerprint(response_status, message);
        let threshold = semantic_error_threshold.max(1);
        let message = truncate_failure_message(message);
        let response = truncate_failure_response(response_body);

        sqlx::query(
            "INSERT INTO route_credential_models
                 (route_credential_id, model_key, status, transient_failure_count,
                  cooldown_until, semantic_failure_streak_count,
                  semantic_failure_streak_fingerprint, last_failure_kind,
                  last_failure_message, last_failure_response_json, created_at, updated_at)
             VALUES (?, ?, ?, 1, ?, 1, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(route_credential_id, model_key) DO UPDATE SET
                 transient_failure_count = transient_failure_count + 1,
                 cooldown_until = excluded.cooldown_until,
                 semantic_failure_streak_count = CASE
                     WHEN semantic_failure_streak_fingerprint = excluded.semantic_failure_streak_fingerprint
                         THEN MIN(semantic_failure_streak_count + 1, ?)
                     ELSE 1
                 END,
                 semantic_failure_streak_fingerprint = excluded.semantic_failure_streak_fingerprint,
                 status = CASE
                     WHEN status = ? THEN status
                     WHEN NOT ? THEN status
                     WHEN CASE
                         WHEN semantic_failure_streak_fingerprint = excluded.semantic_failure_streak_fingerprint
                             THEN MIN(semantic_failure_streak_count + 1, ?)
                         ELSE 1
                     END >= ? THEN ?
                     ELSE status
                 END,
                 last_failure_kind = excluded.last_failure_kind,
                 last_failure_message = excluded.last_failure_message,
                 last_failure_response_json = excluded.last_failure_response_json,
                 updated_at = excluded.updated_at",
        )
        .bind(credential_id)
        .bind(model_key)
        .bind(if error_status_enabled && threshold <= 1 { MODEL_STATUS_ERROR } else { MODEL_STATUS_OK })
        .bind(cooldown_until.as_deref())
        .bind(&fingerprint)
        .bind(kind)
        .bind(&message)
        .bind(&response)
        .bind(&now_text)
        .bind(&now_text)
        .bind(threshold)
        .bind(MODEL_STATUS_PAUSED)
        .bind(error_status_enabled)
        .bind(threshold)
        .bind(threshold)
        .bind(MODEL_STATUS_ERROR)
        .execute(conn)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_model_failure",
                "Could not record per-model failure",
                err,
            )
        })?;
        Ok(())
    }
```

`status = CASE WHEN status = 'paused' THEN status` 是刻意的：暂停中的模型照常记失败账，但用户的暂停意图不被自动改写。`WHEN NOT ?` 对应 `error_status_enabled` 关闭——streak 继续攒，只是不置 `error`，这样把开关打开后是按真实历史判断而不是从零开始（与账号级 `:1472-1473` 的注释同一个理由）。

- [ ] **Step 6: 写清除、设状态与查询方法**

继续追加：

```rust
    /// A success proves this model works. Delete the row — unless the user
    /// paused it, in which case keep the status and only reset the failure
    /// bookkeeping.
    pub async fn clear(
        pool: &SqlitePool,
        credential_id: &str,
        model_key: &str,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE route_credential_models
             SET transient_failure_count = 0, cooldown_until = NULL,
                 semantic_failure_streak_count = 0,
                 semantic_failure_streak_fingerprint = NULL,
                 last_failure_kind = NULL, last_failure_message = NULL,
                 last_failure_response_json = NULL, updated_at = ?
             WHERE route_credential_id = ? AND model_key = ? AND status = ?",
        )
        .bind(&now)
        .bind(credential_id)
        .bind(model_key)
        .bind(MODEL_STATUS_PAUSED)
        .execute(&mut *pool.acquire().await.map_err(|err| {
            database_error(
                "database.route_credential_model_clear",
                "Could not acquire connection to clear per-model state",
                err,
            )
        })?)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_model_clear",
                "Could not reset paused per-model state",
                err,
            )
        })?;

        sqlx::query(
            "DELETE FROM route_credential_models
             WHERE route_credential_id = ? AND model_key = ? AND status != ?",
        )
        .bind(credential_id)
        .bind(model_key)
        .bind(MODEL_STATUS_PAUSED)
        .execute(pool)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_model_clear",
                "Could not clear per-model state",
                err,
            )
        })?;
        Ok(())
    }

    /// Account-level reactivation (scheduled recovery, explicit account test)
    /// wipes automatic model state but leaves paused models paused.
    pub async fn clear_all_unpaused(
        conn: &mut SqliteConnection,
        credential_id: &str,
    ) -> Result<(), AppError> {
        sqlx::query(
            "DELETE FROM route_credential_models
             WHERE route_credential_id = ? AND status != ?",
        )
        .bind(credential_id)
        .bind(MODEL_STATUS_PAUSED)
        .execute(conn)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_model_clear_all",
                "Could not clear per-model state for account",
                err,
            )
        })?;
        Ok(())
    }

    /// Only `ok` and `paused` are valid inputs — `error` is reached exclusively
    /// through a semantic failure streak. Creates the row when the model has
    /// never failed, since a healthy model has no row to update.
    pub async fn set_status(
        pool: &SqlitePool,
        credential_id: &str,
        model_key: &str,
        status: &str,
    ) -> Result<(), AppError> {
        if status == MODEL_STATUS_OK {
            return Self::clear_status(pool, credential_id, model_key).await;
        }
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO route_credential_models
                 (route_credential_id, model_key, status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(route_credential_id, model_key) DO UPDATE SET
                 status = excluded.status, updated_at = excluded.updated_at",
        )
        .bind(credential_id)
        .bind(model_key)
        .bind(status)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_model_status",
                "Could not set per-model status",
                err,
            )
        })?;
        Ok(())
    }

    async fn clear_status(
        pool: &SqlitePool,
        credential_id: &str,
        model_key: &str,
    ) -> Result<(), AppError> {
        sqlx::query(
            "DELETE FROM route_credential_models
             WHERE route_credential_id = ? AND model_key = ?",
        )
        .bind(credential_id)
        .bind(model_key)
        .execute(pool)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_model_status",
                "Could not clear per-model status",
                err,
            )
        })?;
        Ok(())
    }

    /// Models this account cannot serve right now: still cooling, flipped to
    /// `error`, or paused by the user.
    pub async fn unavailable_keys(
        conn: &mut SqliteConnection,
        credential_id: &str,
        now_rfc3339: &str,
    ) -> Result<Vec<String>, AppError> {
        sqlx::query_scalar::<_, String>(
            "SELECT model_key FROM route_credential_models
             WHERE route_credential_id = ?
               AND (status != ? OR (cooldown_until IS NOT NULL AND cooldown_until > ?))",
        )
        .bind(credential_id)
        .bind(MODEL_STATUS_OK)
        .bind(now_rfc3339)
        .fetch_all(conn)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_model_unavailable",
                "Could not read unavailable models",
                err,
            )
        })
    }

    /// The model a healthcheck probe should target: the stalest row that is not
    /// paused and whose cooldown has already expired. Probing a paused model
    /// would fight the user's decision.
    pub async fn oldest_recoverable_key(
        pool: &SqlitePool,
        credential_id: &str,
        now_rfc3339: &str,
    ) -> Result<Option<String>, AppError> {
        sqlx::query_scalar::<_, String>(
            "SELECT model_key FROM route_credential_models
             WHERE route_credential_id = ?
               AND status != ?
               AND (cooldown_until IS NULL OR cooldown_until <= ?)
             ORDER BY updated_at ASC, model_key ASC
             LIMIT 1",
        )
        .bind(credential_id)
        .bind(MODEL_STATUS_PAUSED)
        .bind(now_rfc3339)
        .fetch_optional(pool)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_model_probe",
                "Could not pick a probe model",
                err,
            )
        })
    }

    /// Whether the recovery scheduler should consider this account even though
    /// its account-level columns look healthy.
    pub async fn has_unpaused_rows(
        pool: &SqlitePool,
        credential_id: &str,
    ) -> Result<bool, AppError> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM route_credential_models
             WHERE route_credential_id = ? AND status != ?",
        )
        .bind(credential_id)
        .bind(MODEL_STATUS_PAUSED)
        .fetch_one(pool)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_model_has_rows",
                "Could not count per-model rows",
                err,
            )
        })?;
        Ok(count > 0)
    }
```

在 `src-tauri/src/database/repositories/mod.rs` 的 `pub mod route_credential_repository;`（第 6 行）之后插入，保持字母序：

```rust
pub mod route_credential_model_repository;
```

- [ ] **Step 7: 跑测试确认通过**

Run: `cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test route_credential_model_repository`
Expected: PASS，12 个测试全绿。

若 `load_states` 的 `(a, b) IN ((?, ?), ...)` 行值语法在 SQLite 版本上报错，退化为 `OR` 串联的等价条件：`(route_credential_id = ? AND model_key = ?) OR (...)`，测试断言不变。

- [ ] **Step 8: 格式化、检查、提交**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo fmt && CARGO_TARGET_DIR=target-codex cargo check
cd .. && git add src-tauri/src/database/repositories/route_credential_model_repository.rs src-tauri/src/database/repositories/mod.rs src-tauri/src/database/repositories/route_credential_repository.rs
git commit -m "feat: 模型级失败状态仓储"
```

---

### Task 4: 失败分级判定

**Files:**
- Create: `src-tauri/src/services/route_failure_scope.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Test: 同文件内的 `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: 无（纯函数，不依赖前面任务）
- Produces: `pub(crate) fn is_account_scoped_failure(kind: &str, status: Option<u16>) -> bool`

分级表（规格第 2 节）：

| `kind` | 归属 |
|---|---|
| `refresh` | 账号 |
| `request_build` | 账号 |
| `transport` | 账号 |
| `model_test` | 账号（传输层失败） |
| `upstream_status` / `model_test_status` + 401/403 | 账号 |
| `upstream_status` / `model_test_status` + 其他状态码 | 模型 |
| `response_transform` | 模型 |
| `semantic_response_transient` | 模型 |
| 未知 kind | 账号（保守） |

- [ ] **Step 1: 写失败的测试**

创建文件时把测试一起写进去：

```rust
#[cfg(test)]
mod tests {
    use super::is_account_scoped_failure;

    #[test]
    fn credential_and_network_failures_park_the_whole_account() {
        for kind in ["refresh", "request_build", "transport", "model_test"] {
            assert!(
                is_account_scoped_failure(kind, None),
                "{kind} must be account scoped"
            );
        }
    }

    #[test]
    fn auth_rejections_park_the_whole_account() {
        // A dead key rejects every model, so charging each one separately would
        // just make the account fail N times before it settles.
        for kind in ["upstream_status", "model_test_status"] {
            assert!(is_account_scoped_failure(kind, Some(401)));
            assert!(is_account_scoped_failure(kind, Some(403)));
        }
    }

    #[test]
    fn other_upstream_statuses_park_only_the_requested_model() {
        for status in [400, 404, 408, 429, 500, 502, 503] {
            assert!(
                !is_account_scoped_failure("upstream_status", Some(status)),
                "status {status} must stay model scoped"
            );
            assert!(!is_account_scoped_failure("model_test_status", Some(status)));
        }
    }

    #[test]
    fn content_level_failures_park_only_the_requested_model() {
        for kind in ["semantic_response_transient", "response_transform"] {
            assert!(!is_account_scoped_failure(kind, Some(200)));
            assert!(!is_account_scoped_failure(kind, None));
        }
    }

    #[test]
    fn an_unknown_kind_falls_back_to_account_scope() {
        // Erring account-wide is the safe default: it can only over-park, never
        // let a broken credential keep serving.
        assert!(is_account_scoped_failure("something_new", Some(500)));
    }
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test route_failure_scope`
Expected: FAIL —— 模块未挂载。

- [ ] **Step 3: 写实现**

在 `src-tauri/src/services/route_failure_scope.rs` 的 `mod tests` 之前写：

```rust
/// Whether a failure means "this credential or its network is broken" (park the
/// whole account) rather than "the upstream refused this one model" (park just
/// that model).
///
/// The split matters because a relay that rate-limits `gpt-5.6-sol` usually
/// still serves `glm-5.3` on the same key — parking the account would take out a
/// model that works.
pub(crate) fn is_account_scoped_failure(kind: &str, status: Option<u16>) -> bool {
    match kind {
        // The credential itself, or the path to the upstream, is at fault.
        "refresh" | "request_build" | "transport" | "model_test" => true,
        // A rejected key rejects every model, so settle it once at the account
        // level; every other status is the upstream's verdict on one model.
        "upstream_status" | "model_test_status" => matches!(status, Some(401) | Some(403)),
        // The upstream answered about this specific model.
        "semantic_response_transient" | "response_transform" => false,
        // Unknown kinds park the account: over-parking is recoverable, letting a
        // broken credential keep serving is not.
        _ => true,
    }
}
```

在 `src-tauri/src/services/mod.rs` 的 `pub mod route_credential_transfer_service;`（第 23 行）之后插入，保持字母序：

```rust
pub(crate) mod route_failure_scope;
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test route_failure_scope`
Expected: PASS，5 个测试。

- [ ] **Step 5: 格式化并提交**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo fmt
cd .. && git add src-tauri/src/services/route_failure_scope.rs src-tauri/src/services/mod.rs
git commit -m "feat: 区分账号级与模型级失败"
```

---

### Task 5: 拆分选号并按模型分桶

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs:2519-2650`（`SelectedCredential` 之后新增 `PoolCandidate`，重写 `select_pool_credentials`，改写两个 filter）
- Test: `src-tauri/src/services/route_proxy_service.rs` 的 `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: Task 1 的 `RouteCredentialModelState` 与 `MODEL_STATUS_*`；Task 2 的 `model_state_key`；Task 3 的 `RouteCredentialModelRepository::load_states`。
- Produces:
  - `pub struct PoolCandidate { pub credential: SelectedCredential, pub cooldown_until: Option<String>, pub model_key: Option<String> }`
  - `pub async fn load_pool_candidates(pool: &SqlitePool, platform: &str) -> Result<Vec<PoolCandidate>, AppError>`
  - `fn filter_candidates_for_rule(Vec<PoolCandidate>, &CapabilityRule) -> Vec<PoolCandidate>`
  - `fn filter_candidates_for_model(platform: &str, Vec<PoolCandidate>, Option<&str>) -> Vec<PoolCandidate>`
  - `async fn load_candidate_model_states(pool, &[PoolCandidate]) -> Result<HashMap<(String, String), RouteCredentialModelState>, AppError>`
  - `pub fn partition_by_cooldown(Vec<PoolCandidate>, &HashMap<(String, String), RouteCredentialModelState>, DateTime<Utc>) -> Vec<SelectedCredential>`
  - `pub async fn select_pool_credentials(pool, platform) -> Result<Vec<SelectedCredential>, AppError>`（签名不变，实现变为 load + partition(空表)）

`filter_credentials_for_rule`（`:2612`）与 `filter_credentials_for_model`（`:2626`）被候选版本取代并删除，规则/模型过滤逻辑仍只有一份。

- [ ] **Step 1: 写失败的测试**

在 `route_proxy_service.rs` 的 `mod tests` 内追加。测试直接构造 `PoolCandidate`，不碰数据库，因为分桶是纯函数：

```rust
    fn candidate(id: &str, cooldown_until: Option<&str>, model_key: Option<&str>) -> PoolCandidate {
        PoolCandidate {
            credential: SelectedCredential {
                id: id.to_string(),
                platform: "codex".to_string(),
                kind: "api".to_string(),
                display_name: id.to_string(),
                status: "ok".to_string(),
                route_priority: 3,
                max_concurrency: 5,
                secret_payload_json: r#"{"api_key":"sk"}"#.to_string(),
                config_json: r#"{"base_url":"https://example.com","model_mappings":[]}"#
                    .to_string(),
            },
            cooldown_until: cooldown_until.map(str::to_string),
            model_key: model_key.map(str::to_string),
        }
    }

    fn model_state(
        credential_id: &str,
        model_key: &str,
        status: &str,
        cooldown_until: Option<&str>,
    ) -> ((String, String), RouteCredentialModelState) {
        (
            (credential_id.to_string(), model_key.to_string()),
            RouteCredentialModelState {
                route_credential_id: credential_id.to_string(),
                model_key: model_key.to_string(),
                status: status.to_string(),
                transient_failure_count: 1,
                cooldown_until: cooldown_until.map(str::to_string),
                semantic_failure_streak_count: 0,
                semantic_failure_streak_fingerprint: None,
                last_failure_kind: None,
                last_failure_message: None,
                last_failure_response_json: None,
                created_at: "2026-09-02T00:00:00Z".to_string(),
                updated_at: "2026-09-02T00:00:00Z".to_string(),
            },
        )
    }

    fn now_for_partition() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-02T12:00:00Z")
            .expect("fixed now")
            .with_timezone(&Utc)
    }

    #[test]
    fn a_cooling_model_does_not_park_its_siblings_on_the_same_account() {
        let now = now_for_partition();
        let states = HashMap::from([model_state(
            "cred-1",
            "upstream-sol",
            MODEL_STATUS_OK,
            Some("2026-09-02T12:00:30Z"),
        )]);

        // The request asks for the healthy sibling, so the account is eligible.
        let healthy = partition_by_cooldown(
            vec![candidate("cred-1", None, Some("upstream-glm"))],
            &states,
            now,
        );
        assert_eq!(
            healthy.into_iter().map(|item| item.id).collect::<Vec<_>>(),
            vec!["cred-1"]
        );

        // The same account for the cooling model: only reachable as the
        // all-cooling probe, never as a normal pick.
        let cooling = partition_by_cooldown(
            vec![
                candidate("cred-1", None, Some("upstream-sol")),
                candidate("cred-2", None, Some("upstream-sol")),
            ],
            &states,
            now,
        );
        assert_eq!(
            cooling.into_iter().map(|item| item.id).collect::<Vec<_>>(),
            vec!["cred-2"]
        );
    }

    #[test]
    fn account_level_cooldown_still_parks_every_model() {
        let now = now_for_partition();
        let selected = partition_by_cooldown(
            vec![
                candidate("cooling", Some("2026-09-02T12:05:00Z"), Some("upstream-glm")),
                candidate("ready", None, Some("upstream-glm")),
            ],
            &HashMap::new(),
            now,
        );
        assert_eq!(
            selected.into_iter().map(|item| item.id).collect::<Vec<_>>(),
            vec!["ready"]
        );
    }

    #[test]
    fn paused_and_error_models_are_hard_excluded_even_when_nothing_else_is_left() {
        let now = now_for_partition();
        let states = HashMap::from([
            model_state("paused-acc", "upstream-sol", MODEL_STATUS_PAUSED, None),
            model_state("error-acc", "upstream-sol", MODEL_STATUS_ERROR, None),
        ]);
        let selected = partition_by_cooldown(
            vec![
                candidate("paused-acc", None, Some("upstream-sol")),
                candidate("error-acc", None, Some("upstream-sol")),
            ],
            &states,
            now,
        );
        // No probe fallback: unlike a cooldown these are verdicts, not waits.
        assert!(selected.is_empty());
    }

    #[test]
    fn all_cooling_falls_back_to_the_earliest_recovering_candidate() {
        let now = now_for_partition();
        let states = HashMap::from([
            model_state("late", "upstream-sol", MODEL_STATUS_OK, Some("2026-09-02T12:10:00Z")),
            model_state("soon", "upstream-sol", MODEL_STATUS_OK, Some("2026-09-02T12:01:00Z")),
        ]);
        let selected = partition_by_cooldown(
            vec![
                candidate("late", None, Some("upstream-sol")),
                candidate("soon", None, Some("upstream-sol")),
            ],
            &states,
            now,
        );
        assert_eq!(
            selected.into_iter().map(|item| item.id).collect::<Vec<_>>(),
            vec!["soon"]
        );
    }

    #[test]
    fn a_candidate_without_a_model_key_only_consults_account_level_cooldown() {
        let now = now_for_partition();
        // A Gemini-style request carries its model in the path, so there is no
        // key to look up; the model table must not park it.
        let states = HashMap::from([model_state(
            "cred-1",
            "upstream-sol",
            MODEL_STATUS_PAUSED,
            None,
        )]);
        let selected = partition_by_cooldown(vec![candidate("cred-1", None, None)], &states, now);
        assert_eq!(
            selected.into_iter().map(|item| item.id).collect::<Vec<_>>(),
            vec!["cred-1"]
        );
    }

    #[test]
    fn filter_candidates_for_model_records_the_upstream_key() {
        let mut item = candidate("cred-1", None, None);
        item.credential.config_json =
            r#"{"model_mappings":[{"from":"gpt-5.6-sol","to":"upstream-sol"}]}"#.to_string();
        let filtered = filter_candidates_for_model("codex", vec![item], Some("gpt-5.6-sol"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].model_key.as_deref(), Some("upstream-sol"));
    }

    #[test]
    fn filter_candidates_for_model_leaves_the_key_empty_without_a_requested_model() {
        let filtered = filter_candidates_for_model("codex", vec![candidate("cred-1", None, None)], None);
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0].model_key.is_none());
    }
```

`mod tests` 顶部需要补 `use` —— 检查现有的 `use super::*;` 是否已覆盖，若否则追加：

```rust
    use crate::models::route_credential_model::{
        RouteCredentialModelState, MODEL_STATUS_ERROR, MODEL_STATUS_OK, MODEL_STATUS_PAUSED,
    };
    use std::collections::HashMap;
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib route_proxy_service::tests::a_cooling_model`
Expected: FAIL —— `cannot find struct PoolCandidate` / `cannot find function partition_by_cooldown`。

- [ ] **Step 3: 新增 PoolCandidate 与 load_pool_candidates**

在 `route_proxy_service.rs` 的 `SelectedCredential` 定义（结束于 `:2530`）之后插入：

```rust
/// A pool row plus the state needed to decide whether it may serve *this*
/// request. `model_key` is filled in by `filter_candidates_for_model` and stays
/// `None` when the request carries no model (Gemini puts it in the path).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolCandidate {
    pub credential: SelectedCredential,
    pub cooldown_until: Option<String>,
    pub model_key: Option<String>,
}

/// Load the pool rows a request could use: SQL-level filters and quota only.
/// Cooldown partitioning deliberately happens later — which rows count as
/// cooling depends on the requested model, which is not known here.
pub async fn load_pool_candidates(
    pool: &SqlitePool,
    platform: &str,
) -> Result<Vec<PoolCandidate>, AppError> {
    let rows = sqlx::query(
        "SELECT c.id, c.platform, c.kind, c.display_name, c.status,
                c.route_priority, c.max_concurrency,
                c.secret_payload_json, c.config_json,
                c.next_retry_at, c.cooldown_until
         FROM route_pool_members rpm
         INNER JOIN route_credentials c ON c.id = rpm.route_credential_id
         WHERE rpm.platform = ?
           AND rpm.enabled = 1
           AND c.archived_at IS NULL
           AND c.status = 'ok'
           AND (c.primary_remain IS NULL OR c.primary_remain > 0)
           AND (c.weekly_remain IS NULL OR c.weekly_remain > 0)
         ORDER BY c.route_priority ASC, rpm.sort_order ASC, rpm.created_at ASC",
    )
    .bind(platform)
    .fetch_all(pool)
    .await
    .map_err(|err| AppError::Database {
        code: "database.route_proxy_credentials",
        message: "Could not load route credentials for proxy".to_string(),
        details: Some(err.to_string()),
        recoverable: true,
    })?;

    let mut candidates = Vec::with_capacity(rows.len());
    for row in rows {
        let next_retry_at: Option<String> = row.get("next_retry_at");
        let cooldown_until: Option<String> = row.get("cooldown_until");
        let credential = SelectedCredential {
            id: row.get("id"),
            platform: row.get("platform"),
            kind: row.get("kind"),
            display_name: row.get("display_name"),
            status: row.get("status"),
            route_priority: row.get("route_priority"),
            max_concurrency: row.get("max_concurrency"),
            secret_payload_json: row.get("secret_payload_json"),
            config_json: row.get("config_json"),
        };
        // Skip official accounts already known to have zero remaining quota.
        if !is_route_credential_quota_available(&credential.config_json) {
            continue;
        }
        candidates.push(PoolCandidate {
            credential,
            // The two account columns are always written the same value, so the
            // later of them is the single deadline that matters.
            cooldown_until: latest_deadline(next_retry_at.as_deref(), cooldown_until.as_deref()),
            model_key: None,
        });
    }
    Ok(candidates)
}

fn latest_deadline(left: Option<&str>, right: Option<&str>) -> Option<String> {
    [left, right]
        .into_iter()
        .flatten()
        .filter_map(|value| {
            DateTime::parse_from_rfc3339(value)
                .ok()
                .map(|parsed| (parsed.with_timezone(&Utc), value.to_string()))
        })
        .max_by_key(|(parsed, _)| *parsed)
        .map(|(_, value)| value)
}
```

- [ ] **Step 4: 写分桶与状态装载**

紧接着插入：

```rust
pub(crate) async fn load_candidate_model_states(
    pool: &SqlitePool,
    candidates: &[PoolCandidate],
) -> Result<HashMap<(String, String), RouteCredentialModelState>, AppError> {
    let keys: Vec<(String, String)> = candidates
        .iter()
        .filter_map(|candidate| {
            Some((
                candidate.credential.id.clone(),
                candidate.model_key.clone()?,
            ))
        })
        .collect();
    RouteCredentialModelRepository::load_states(pool, &keys).await
}

/// Split candidates into "may serve now" and "still cooling", then hand back the
/// usable set.
///
/// Order matters: `paused`/`error` models are dropped outright, because those are
/// verdicts rather than waits and must never be reached by the all-cooling probe
/// below. Only time-based cooldowns get that second chance — otherwise a pool
/// where everything is briefly cooling would fail requests it could still serve.
pub fn partition_by_cooldown(
    candidates: Vec<PoolCandidate>,
    model_states: &HashMap<(String, String), RouteCredentialModelState>,
    now: DateTime<Utc>,
) -> Vec<SelectedCredential> {
    let mut eligible = Vec::new();
    let mut cooling: Vec<(DateTime<Utc>, usize, SelectedCredential)> = Vec::new();

    for candidate in candidates {
        let state = candidate.model_key.as_ref().and_then(|model_key| {
            model_states.get(&(candidate.credential.id.clone(), model_key.clone()))
        });
        if state.is_some_and(|state| state.status != MODEL_STATUS_OK) {
            continue;
        }
        let model_cooldown = state.and_then(|state| state.cooldown_until.clone());
        let deadline = latest_deadline(
            candidate.cooldown_until.as_deref(),
            model_cooldown.as_deref(),
        )
        .and_then(|value| {
            DateTime::parse_from_rfc3339(&value)
                .ok()
                .map(|parsed| parsed.with_timezone(&Utc))
        });

        match deadline {
            Some(deadline) if deadline > now => {
                cooling.push((deadline, cooling.len(), candidate.credential));
            }
            _ => eligible.push(candidate.credential),
        }
    }

    if !eligible.is_empty() {
        return eligible;
    }

    cooling.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    cooling
        .into_iter()
        .take(1)
        .map(|(_, _, credential)| credential)
        .collect()
}

pub async fn select_pool_credentials(
    pool: &SqlitePool,
    platform: &str,
) -> Result<Vec<SelectedCredential>, AppError> {
    let candidates = load_pool_candidates(pool, platform).await?;
    Ok(partition_by_cooldown(
        candidates,
        &HashMap::new(),
        Utc::now(),
    ))
}
```

删除旧的 `select_pool_credentials` 函数体（`:2532-2610`）。`credential_is_retryable_now`（`:2265`）保留——`route_model_test_service.rs` 与既有测试仍在用它。

文件顶部补 `use`：

```rust
use crate::database::repositories::route_credential_model_repository::RouteCredentialModelRepository;
use crate::models::route_credential_model::{RouteCredentialModelState, MODEL_STATUS_OK};
```

`HashMap` 已在 `use std::collections::BTreeMap` 附近，检查是否需要补 `HashMap`。

- [ ] **Step 5: 改写两个 filter**

把 `filter_credentials_for_rule`（`:2612-2624`）与 `filter_credentials_for_model`（`:2626-2650`）整体替换为候选版本：

```rust
fn filter_candidates_for_rule(
    mut candidates: Vec<PoolCandidate>,
    rule: &CapabilityRule,
) -> Vec<PoolCandidate> {
    if !rule.credential_kinds.is_empty() {
        candidates.retain(|candidate| {
            rule.credential_kinds
                .iter()
                .any(|kind| kind == &candidate.credential.kind)
        });
    }
    candidates
}

/// Drop candidates that cannot serve the requested model, and record the model
/// key the survivors will be charged under. Both happen in one pass because both
/// need the same parsed capability.
fn filter_candidates_for_model(
    platform: &str,
    candidates: Vec<PoolCandidate>,
    requested_model: Option<&str>,
) -> Vec<PoolCandidate> {
    let Some(requested_model) = requested_model else {
        return candidates;
    };

    candidates
        .into_iter()
        .filter_map(|mut candidate| {
            let mut capability = parse_model_capability(&candidate.credential.config_json);
            if candidate.credential.kind == "official" {
                // build_official_upstream_request never applies model mappings, so a
                // synthetic alias would reach the vendor verbatim and 404. Ignoring
                // those entries here keeps official accounts on exactly their
                // pre-feature semantics (an alias-only config collapses to the
                // baseline-only wildcard).
                capability
                    .mappings
                    .retain(|mapping| !is_synthetic_route_alias(&mapping.from));
            }
            if !supports_requested_model(platform, &capability, Some(requested_model)) {
                return None;
            }
            candidate.model_key = Some(model_state_key(
                platform,
                &capability,
                &candidate.credential.kind,
                requested_model,
            ));
            Some(candidate)
        })
        .collect()
}
```

`use` 里的 `route_model_capability::{...}`（`:32-33`）补上 `model_state_key`。

- [ ] **Step 6: 跑测试确认通过**

Run: `cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib route_proxy_service`
Expected: 7 个新测试 PASS。既有的 `select_pool_credentials` 测试（`:7112`、`:7152`、`:7206`、`:7239`）也必须继续 PASS —— 它们是「账号级行为不变」的守卫。

若 `:580` 的 `/models` 路径与 `:619` 的转发路径此时编译不过（旧 filter 已删），先把它们改成候选形态：

```rust
        // /models 路径（原 :577-580）
        let candidates = load_pool_candidates(pool, &platform)
            .await
            .map_err(|err| err.to_string())?;
        let candidates = filter_candidates_for_rule(candidates, &routing_rule);
        let credentials = partition_by_cooldown(candidates, &HashMap::new(), Utc::now());
```

转发路径的完整改写在 Task 6，这里只需让它编译通过：把 `:616-624` 暂时改为 `load_pool_candidates` → `filter_candidates_for_rule` → `filter_candidates_for_model` → `partition_by_cooldown(candidates, &HashMap::new(), Utc::now())`，错误分支保持原样。

- [ ] **Step 7: 全量 Rust 测试**

Run: `cd src-tauri && CARGO_TARGET_DIR=target-codex cargo fmt && CARGO_TARGET_DIR=target-codex cargo test`
Expected: 全绿。这一步会暴露所有依赖旧函数签名的调用点。

- [ ] **Step 8: 提交**

```bash
git add src-tauri/src/services/route_proxy_service.rs
git commit -m "refactor: 选号拆分为装载与冷却分桶两步"
```

---

### Task 6: 转发链路按模型记账

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs`（`forward_request` 的选号段、`record_route_credential_failure`、9 处记账点、2 处成功清账点、`StreamCompletion`）
- Modify: `src-tauri/src/database/repositories/route_credential_repository.rs`（`record_transient_failure` 加 scope 参数并做升级判定；`clear_transient_failure` 加模型键参数）
- Test: 两个文件各自的 `mod tests`

**Interfaces:**
- Consumes: Task 2 的 `known_upstream_models`、Task 3 的全部仓储方法、Task 4 的 `is_account_scoped_failure`、Task 5 的 `PoolCandidate` 与 `partition_by_cooldown`。
- Produces:
  - `pub enum FailureScope<'a> { Account, Model { key: &'a str, siblings: &'a [String] } }`（定义在 `src-tauri/src/models/route_credential_model.rs`）
  - `RouteCredentialRepository::record_transient_failure(pool, id, kind, message, response_body, scope: FailureScope<'_>) -> Result<RetryState, AppError>`
  - `RouteCredentialRepository::clear_transient_failure(pool, id, model_key: Option<&str>) -> Result<(), AppError>`
  - `record_route_credential_failure(activity, platform, pool, credential: &SelectedCredential, model_key: Option<&str>, kind, message, response_body)`（签名从 `credential_id: &str` 改为整个 `&SelectedCredential`，因为升级判定需要 `config_json` 与 `kind` 才能算 siblings）
  - 新错误码字符串 `route_pool.model_unavailable`

- [ ] **Step 1: 写端到端失败测试**

在 `route_proxy_service.rs` 的 `mod tests` 内追加。先加一个按 model 分流的假上游 helper：

```rust
    /// An upstream that fails one model and serves another, i.e. exactly the
    /// relay behaviour that makes account-wide cooldown wrong.
    async fn start_per_model_upstream(failing_model: &'static str) -> String {
        let app = Router::new().fallback(
            move |body: axum::body::Bytes| async move {
                let model = serde_json::from_slice::<Value>(&body)
                    .ok()
                    .and_then(|value| {
                        value.get("model").and_then(Value::as_str).map(str::to_string)
                    })
                    .unwrap_or_default();
                if model == failing_model {
                    (StatusCode::TOO_MANY_REQUESTS, r#"{"error":{"message":"rate limited"}}"#)
                } else {
                    (StatusCode::OK, r#"{"choices":[{"message":{"content":"ok"}}]}"#)
                }
            },
        );
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind per-model upstream");
        let address = listener.local_addr().expect("per-model address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve per-model");
        });
        format!("http://{address}/v1")
    }

    #[tokio::test]
    async fn a_failing_model_does_not_take_out_its_sibling_on_the_same_account() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let upstream = start_per_model_upstream("upstream-sol").await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let credential = RouteCredentialRepository::create(
            &pool,
            "codex",
            "api",
            "Dual Model",
            None,
            "ok",
            None,
            r#"{"api_key":"sk-upstream"}"#,
            &json!({
                "base_url": upstream,
                "interface_format": "openai",
                "model_mappings": [
                    {"from": "gpt-5.6-sol", "to": "upstream-sol"},
                    {"from": "glm-5.3", "to": "upstream-glm"}
                ],
                "failure_policy": {"cooldown_enabled": true, "cooldown_seconds": 600, "retry_count": 0}
            })
            .to_string(),
            "{}",
        )
        .await
        .expect("create credential");
        RoutePoolRepository::replace_members(&pool, "codex", &[credential.id.clone()])
            .await
            .expect("pool members");
        let route_key =
            RouteProxyKeyRepository::ensure_platform_key(&pool, "codex", "sk-ai-switch-per-model")
                .await
                .expect("route key");
        let runtime = RouteProxyRuntimeState::default();
        let proxy = RouteProxyService::start(&runtime, pool.clone(), RouteProxyTransport::Http)
            .await
            .expect("start proxy");
        let endpoint = format!(
            "{}/v1/chat/completions",
            proxy.base_url.as_deref().expect("base url")
        );
        let client = reqwest::Client::new();
        let post = |model: &'static str| {
            let client = client.clone();
            let endpoint = endpoint.clone();
            let route_key = route_key.clone();
            async move {
                client
                    .post(&endpoint)
                    .bearer_auth(&route_key)
                    .header(ROUTE_PROXY_PLATFORM_HEADER, "codex")
                    .json(&json!({"model": model, "messages": []}))
                    .send()
                    .await
                    .expect("proxy response")
            }
        };

        // 1. The failing model parks itself.
        assert_eq!(
            post("gpt-5.6-sol").await.status(),
            reqwest::StatusCode::TOO_MANY_REQUESTS
        );
        let states =
            RouteCredentialModelRepository::list_for_credentials(&pool, &[credential.id.clone()])
                .await
                .expect("model states");
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].model_key, "upstream-sol");
        assert!(states[0].cooldown_until.is_some());

        // 2. The sibling still works — this is the regression this feature exists for.
        assert_eq!(post("glm-5.3").await.status(), reqwest::StatusCode::OK);

        // 3. The account itself was never parked, so the cooling model can still
        //    be probed as the last resort.
        let stored = RouteCredentialRepository::get(&pool, &credential.id)
            .await
            .expect("account row");
        assert!(stored.cooldown_until.is_none());
        assert_eq!(
            post("gpt-5.6-sol").await.status(),
            reqwest::StatusCode::TOO_MANY_REQUESTS
        );

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }

    #[tokio::test]
    async fn a_healthy_account_wins_over_one_whose_model_is_cooling() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let failing = start_per_model_upstream("upstream-sol").await;
        let healthy = start_fixed_upstream(StatusCode::OK, r#"{"route":"healthy"}"#).await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let cooling_id = create_proxy_api_credential_with_mappings(
            &pool,
            "cooling",
            &failing,
            json!([{"from": "gpt-5.6-sol", "to": "upstream-sol"}]),
        )
        .await;
        sqlx::query("UPDATE route_credentials SET config_json = json_set(config_json, '$.failure_policy', json('{\"cooldown_enabled\":true,\"cooldown_seconds\":600,\"retry_count\":0}')) WHERE id = ?")
            .bind(&cooling_id)
            .execute(&pool)
            .await
            .expect("enable cooldown");
        let healthy_id = create_proxy_api_credential_with_mappings(
            &pool,
            "healthy",
            &healthy,
            json!([{"from": "gpt-5.6-sol", "to": "upstream-sol"}]),
        )
        .await;
        RoutePoolRepository::replace_members(
            &pool,
            "codex",
            &[cooling_id.clone(), healthy_id.clone()],
        )
        .await
        .expect("pool members");
        let route_key = RouteProxyKeyRepository::ensure_platform_key(
            &pool,
            "codex",
            "sk-ai-switch-model-failover",
        )
        .await
        .expect("route key");
        let runtime = RouteProxyRuntimeState::default();
        let proxy = RouteProxyService::start(&runtime, pool.clone(), RouteProxyTransport::Http)
            .await
            .expect("start proxy");
        let endpoint = format!(
            "{}/v1/chat/completions",
            proxy.base_url.as_deref().expect("base url")
        );
        let client = reqwest::Client::new();

        // First request fails over from the cooling account to the healthy one.
        let first = client
            .post(&endpoint)
            .bearer_auth(&route_key)
            .header(ROUTE_PROXY_PLATFORM_HEADER, "codex")
            .json(&json!({"model": "gpt-5.6-sol", "messages": []}))
            .send()
            .await
            .expect("first response");
        assert_eq!(first.status(), reqwest::StatusCode::OK);

        // Second request skips the parked model outright.
        let second = client
            .post(&endpoint)
            .bearer_auth(&route_key)
            .header(ROUTE_PROXY_PLATFORM_HEADER, "codex")
            .json(&json!({"model": "gpt-5.6-sol", "messages": []}))
            .send()
            .await
            .expect("second response");
        assert_eq!(second.status(), reqwest::StatusCode::OK);
        assert_eq!(second.text().await.expect("body"), r#"{"route":"healthy"}"#);

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }

    #[tokio::test]
    async fn every_model_cooling_escalates_to_an_account_level_cooldown() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        // This upstream fails everything, so both mapped models get parked.
        let upstream = start_fixed_upstream(
            StatusCode::TOO_MANY_REQUESTS,
            r#"{"error":{"message":"rate limited"}}"#,
        )
        .await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let credential_id = create_proxy_api_credential_with_mappings(
            &pool,
            "all-models",
            &upstream,
            json!([
                {"from": "gpt-5.6-sol", "to": "upstream-sol"},
                {"from": "glm-5.3", "to": "upstream-glm"}
            ]),
        )
        .await;
        sqlx::query("UPDATE route_credentials SET config_json = json_set(config_json, '$.failure_policy', json('{\"cooldown_enabled\":true,\"cooldown_seconds\":600,\"retry_count\":0}')) WHERE id = ?")
            .bind(&credential_id)
            .execute(&pool)
            .await
            .expect("enable cooldown");
        RoutePoolRepository::replace_members(&pool, "codex", &[credential_id.clone()])
            .await
            .expect("pool members");
        let route_key =
            RouteProxyKeyRepository::ensure_platform_key(&pool, "codex", "sk-ai-switch-escalate")
                .await
                .expect("route key");
        let runtime = RouteProxyRuntimeState::default();
        let proxy = RouteProxyService::start(&runtime, pool.clone(), RouteProxyTransport::Http)
            .await
            .expect("start proxy");
        let endpoint = format!(
            "{}/v1/chat/completions",
            proxy.base_url.as_deref().expect("base url")
        );
        let client = reqwest::Client::new();
        for model in ["gpt-5.6-sol", "glm-5.3"] {
            let _ = client
                .post(&endpoint)
                .bearer_auth(&route_key)
                .header(ROUTE_PROXY_PLATFORM_HEADER, "codex")
                .json(&json!({"model": model, "messages": []}))
                .send()
                .await
                .expect("response");
        }

        let stored = RouteCredentialRepository::get(&pool, &credential_id)
            .await
            .expect("account row");
        // With nothing left to serve, the account itself backs off — otherwise a
        // fully-down relay would be re-probed once per model forever.
        assert!(stored.cooldown_until.is_some());

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }
```

`mod tests` 顶部补：

```rust
    use crate::database::repositories::route_credential_model_repository::RouteCredentialModelRepository;
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib a_failing_model_does_not_take_out`
Expected: FAIL —— `states.len()` 为 0（Task 5 只做了分桶，还没有人写模型行）。

- [ ] **Step 3: 定义 FailureScope**

在 `src-tauri/src/models/route_credential_model.rs` 末尾（`mod tests` 之前，该文件目前无测试）追加：

```rust
/// Where a failure should be charged.
///
/// `siblings` is every model key this account is known to serve, so the
/// repository can tell whether parking this one leaves nothing usable and the
/// account itself should back off. The service layer computes it — parsing
/// `model_mappings` is not the repository's job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureScope<'a> {
    Account,
    Model {
        key: &'a str,
        siblings: &'a [String],
    },
}
```

- [ ] **Step 4: 改写账号级仓储的记账与清账**

在 `route_credential_repository.rs` 把 `record_transient_failure`（`:1342`）改为接 scope。核心变化：账号级冷却只在 `FailureScope::Account` 或升级触发时写；模型级失败调 `RouteCredentialModelRepository::record_transient_failure`，然后在同一事务内判升级。

```rust
    pub async fn record_transient_failure(
        pool: &SqlitePool,
        id: &str,
        kind: &str,
        message: &str,
        response_body: Option<&[u8]>,
        scope: FailureScope<'_>,
    ) -> Result<RetryState, AppError> {
        let mut tx = pool.begin().await.map_err(|err| AppError::Database {
            code: "database.route_credential_retry_tx",
            message: "Could not start route credential retry update".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;
        let current = sqlx::query_as::<_, (i64, String)>(
            "SELECT transient_failure_count, config_json FROM route_credentials WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_credential_retry_read",
            message: "Could not read route credential retry state".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;
        let Some((current, config_json)) = current else {
            return Err(AppError::Validation {
                code: "validation.route_credential_not_found",
                message: "Route credential does not exist".to_string(),
                details: Some(id.to_string()),
                recoverable: true,
            });
        };
        let policy = RouteCredentialFailurePolicy::from_config_json(&config_json);
        let cooldown_seconds = policy.cooldown_enabled.then_some(policy.cooldown_seconds);

        // A model-scoped failure charges the model row first, then asks whether
        // anything is left to serve. Both happen inside this transaction:
        // concurrent requests must not each see "not all parked yet" and skip
        // the escalation.
        let escalate = match scope {
            FailureScope::Account => true,
            FailureScope::Model { key, siblings } => {
                RouteCredentialModelRepository::record_transient_failure(
                    &mut tx,
                    id,
                    key,
                    kind,
                    message,
                    response_body,
                    cooldown_seconds,
                    None,
                    i64::from(policy.semantic_error_threshold),
                    policy.error_status_enabled,
                )
                .await?;
                let now = Utc::now().to_rfc3339();
                let unavailable =
                    RouteCredentialModelRepository::unavailable_keys(&mut tx, id, &now).await?;
                let paused =
                    RouteCredentialModelRepository::paused_keys(&mut tx, id).await?;
                let serviceable = siblings
                    .iter()
                    .filter(|sibling| !paused.contains(sibling))
                    .count();
                let parked = siblings
                    .iter()
                    .filter(|sibling| {
                        !paused.contains(sibling) && unavailable.contains(sibling)
                    })
                    .count();
                // Only escalate when the account has run out of usable models.
                // Paused models are excluded from the denominator: pausing three
                // of four must not let the fourth's single failure fake an
                // account-wide outage.
                serviceable > 0 && parked >= serviceable
            }
        };

        let failure_count = current.saturating_add(1);
        // Every trigger uses the same account-configured window, so a flaky
        // account recovers predictably instead of sliding into a long backoff.
        let (retry_at, cooldown_until) = match (escalate, cooldown_seconds) {
            (true, Some(seconds)) => {
                let cooldown_until =
                    (Utc::now() + chrono::Duration::seconds(i64::from(seconds))).to_rfc3339();
                (Some(cooldown_until.clone()), Some(cooldown_until))
            }
            // A model-scoped failure leaves the account selectable for its other
            // models, so its own deadline must not be written.
            _ => (None, None),
        };
        let message = truncate_failure_message(message);
        let response = truncate_failure_response(response_body);
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE route_credentials
             SET transient_failure_count = ?, next_retry_at = ?, cooldown_until = ?,
                 semantic_failure_streak_count = 0, semantic_failure_streak_fingerprint = NULL,
                 last_failure_kind = ?, last_failure_message = ?, last_failure_response_json = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(failure_count)
        .bind(retry_at.as_deref())
        .bind(cooldown_until.as_deref())
        .bind(kind)
        .bind(&message)
        .bind(&response)
        .bind(&now)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_credential_retry_update",
            message: "Could not update route credential retry state".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;
        tx.commit().await.map_err(|err| AppError::Database {
            code: "database.route_credential_retry_commit",
            message: "Could not save route credential retry state".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(RetryState {
            failure_count,
            next_retry_at: retry_at,
            cooldown_until,
        })
    }
```

`FailureScope::Model` 分支用到 `paused_keys`，在 Task 3 的仓储里补上（与 `unavailable_keys` 相邻，同样接 `&mut SqliteConnection`）：

```rust
    /// Models the user explicitly paused. Excluded from the escalation
    /// denominator so a human decision cannot masquerade as an outage.
    pub async fn paused_keys(
        conn: &mut SqliteConnection,
        credential_id: &str,
    ) -> Result<Vec<String>, AppError> {
        sqlx::query_scalar::<_, String>(
            "SELECT model_key FROM route_credential_models
             WHERE route_credential_id = ? AND status = ?",
        )
        .bind(credential_id)
        .bind(MODEL_STATUS_PAUSED)
        .fetch_all(conn)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_model_paused",
                "Could not read paused models",
                err,
            )
        })
    }
```

`clear_transient_failure`（`:1428`）加模型键参数：

```rust
    /// A success clears this account's backoff and, when the request named a
    /// model, that model's row. Sibling models keep their own state: proving
    /// `glm-5.3` works says nothing about `gpt-5.6-sol`.
    pub async fn clear_transient_failure(
        pool: &SqlitePool,
        id: &str,
        model_key: Option<&str>,
    ) -> Result<(), AppError> {
        if let Some(model_key) = model_key {
            RouteCredentialModelRepository::clear(pool, id, model_key).await?;
        }
        let now = Utc::now().to_rfc3339();
        // ... 现有 UPDATE 原样保留 ...
    }
```

顶部 `use` 补 `FailureScope` 与 `RouteCredentialModelRepository`。

现有测试 `:2594`、`:2701`、`:2735` 的调用点补 `FailureScope::Account` 第六参数、`clear_transient_failure` 补 `None`。语义测试（`:2765-3056`）不动。

- [ ] **Step 5: 改写代理侧记账包装**

`record_route_credential_failure`（`:2280`）改为收整个候选凭证，自己算 scope 与 siblings：

```rust
async fn record_route_credential_failure(
    activity: &RouteCredentialActivityRegistry,
    platform: &str,
    pool: &SqlitePool,
    credential: &SelectedCredential,
    model_key: Option<&str>,
    kind: &str,
    message: &str,
    response_body: Option<&[u8]>,
) {
    let siblings = model_key.map(|_| {
        let capability = parse_model_capability(&credential.config_json);
        known_upstream_models(platform, &capability, &credential.kind)
    });
    let scope = match (model_key, siblings.as_deref()) {
        // Without a model name there is nothing to charge but the account —
        // Gemini keeps its model in the path, and some routes carry none at all.
        (Some(key), Some(siblings)) if !is_account_scoped_failure(kind, None) => {
            FailureScope::Model { key, siblings }
        }
        _ => FailureScope::Account,
    };
    if RouteCredentialRepository::record_transient_failure(
        pool,
        &credential.id,
        kind,
        message,
        response_body,
        scope,
    )
    .await
    .is_ok()
    {
        activity.notify_status_change(platform, &credential.id);
    }
}
```

`upstream_status` 与 `model_test_status` 需要状态码才能分级，所以这两处调用点改为直接构造 scope、不走上面的 `None` 判定 —— 新增一个变体包装：

```rust
#[allow(clippy::too_many_arguments)]
async fn record_route_credential_failure_with_status(
    activity: &RouteCredentialActivityRegistry,
    platform: &str,
    pool: &SqlitePool,
    credential: &SelectedCredential,
    model_key: Option<&str>,
    kind: &str,
    status: Option<u16>,
    message: &str,
    response_body: Option<&[u8]>,
) {
    let account_scoped = is_account_scoped_failure(kind, status);
    let siblings = (!account_scoped)
        .then(|| model_key.map(|_| {
            let capability = parse_model_capability(&credential.config_json);
            known_upstream_models(platform, &capability, &credential.kind)
        }))
        .flatten();
    let scope = match (model_key, siblings.as_deref()) {
        (Some(key), Some(siblings)) => FailureScope::Model { key, siblings },
        _ => FailureScope::Account,
    };
    if RouteCredentialRepository::record_transient_failure(
        pool, &credential.id, kind, message, response_body, scope,
    )
    .await
    .is_ok()
    {
        activity.notify_status_change(platform, &credential.id);
    }
}
```

9 处调用点逐一改造，`model_key` 一律传 `selected_model_key.as_deref()`（见 Step 6）：

| 行 | kind | 用哪个包装 |
|---|---|---|
| `:692` | `refresh` | `record_route_credential_failure`（分级判定得账号级） |
| `:772` | `request_build` | 同上 |
| `:897` | `transport` | 同上 |
| `:1139` | `transport` | 同上 |
| `:1190` | `response_transform` | 同上（得模型级） |
| `:1392` | `semantic_response_transient` | 同上（得模型级） |
| `:1412` | `upstream_status` | `..._with_status`，传 `Some(status.as_u16())` |
| `:1428` | `upstream_status` | 同上 |
| `:2006` | `transport` | `record_route_credential_failure` |
| `:2127` | `semantic_response_transient` | 同上（流式截断） |

- [ ] **Step 6: 改写转发链路的选号与成功清账**

`forward_request` 的 `:615-630` 改为：

```rust
    let requested_model = requested_model_from_body(&body_bytes);
    let candidates = load_pool_candidates(pool, &platform)
        .await
        .map_err(|err| err.to_string())?;
    let candidates = filter_candidates_for_rule(candidates, &routing_rule);
    if candidates.is_empty() {
        return Err("No enabled route credentials in pool".to_string());
    }
    let candidates =
        filter_candidates_for_model(&platform, candidates, requested_model.as_deref());
    if candidates.is_empty() {
        let model = requested_model.as_deref().unwrap_or("unknown");
        return Err(format!(
            "route_pool.model_unmatched: no enabled route credential supports model '{model}' on platform '{platform}'"
        ));
    }
    // Keyed by account id: two accounts may map the same requested model to
    // different upstream names, so the pair is the key, never the model alone.
    let model_keys: HashMap<String, String> = candidates
        .iter()
        .filter_map(|candidate| {
            Some((
                candidate.credential.id.clone(),
                candidate.model_key.clone()?,
            ))
        })
        .collect();
    let model_states = load_candidate_model_states(pool, &candidates)
        .await
        .map_err(|err| err.to_string())?;
    let credentials = partition_by_cooldown(candidates, &model_states, Utc::now());
    if credentials.is_empty() {
        let model = requested_model.as_deref().unwrap_or("unknown");
        return Err(format!(
            "route_pool.model_unavailable: every route credential for model '{model}' on platform '{platform}' is paused or marked unhealthy"
        ));
    }
```

重试循环内取当前凭证的键：

```rust
        let selected_model_key = model_keys.get(&credential.id).cloned();
```

放在 `:750` 附近（`failure_policy` 解析处）即可，之后的记账点都用它。

两处成功清账：

- `:1446` → `RouteCredentialRepository::clear_transient_failure(pool, &credential.id, selected_model_key.as_deref())`
- `:2140` → `StreamCompletion` 需要新字段。在 `StreamCompletion`（`:2025-2043`）加 `model_key: Option<String>`，在 `finish()` 的解构（`:2051-2064`）里加上，`:2140` 改为 `clear_transient_failure(&state.pool, &credential.id, model_key.as_deref())`。构造点（`:1065` 附近，`requested_model: requested_model.clone()` 那处）加 `model_key: selected_model_key.clone()`。`:2127` 的截断记账也用 `model_key.as_deref()`。

`:1351` 的配额耗尽分支不变 —— 配额是账号属性，`record_semantic_failure_with_status` 与 `mark_route_credential_error` 都继续作用于账号。

`:2006` 的流式首字节前失败在 `StreamCompletion` 之外，直接用 `selected_model_key.as_deref()`。

新错误码要接入 `route_proxy_error_status`（`:2340` 附近的错误码映射）：`route_pool.model_unavailable` 与 `route_pool.model_unmatched` 同样返回 400 系列。查看该函数现有分支，照 `model_unmatched` 的写法加一条。

- [ ] **Step 7: 跑端到端测试**

Run: `cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib route_proxy_service`
Expected: 3 个新端到端测试 PASS，既有 144 个测试继续 PASS。

- [ ] **Step 8: 全量 Rust 测试**

Run: `cd src-tauri && CARGO_TARGET_DIR=target-codex cargo fmt && CARGO_TARGET_DIR=target-codex cargo test`
Expected: 全绿。`route_model_test_service.rs` 的 3 处 `record_transient_failure` 与 1 处 `clear_transient_failure` 此时会编译失败 —— 先用 `FailureScope::Account` 与 `None` 占位让它编译通过，Task 7 再按模型改造。

- [ ] **Step 9: 提交**

```bash
git add src-tauri/src/services/route_proxy_service.rs src-tauri/src/database/repositories/route_credential_repository.rs src-tauri/src/database/repositories/route_credential_model_repository.rs src-tauri/src/models/route_credential_model.rs src-tauri/src/services/route_model_test_service.rs
git commit -m "feat: 转发失败按模型冷却并在全部模型不可用时升级整账号"
```

---

### Task 7: 模型测试与恢复调度按模型记账

**Files:**
- Modify: `src-tauri/src/services/route_model_test_service.rs:1278-1360`（`finish_outcome`）
- Modify: `src-tauri/src/services/route_recovery_service.rs:99-171`（`run_tick`、`needs_recovery`）
- Modify: `src-tauri/src/database/repositories/route_credential_repository.rs:1560`（`reactivate_credential` 清模型行）
- Modify: `src-tauri/src/models/route_credential.rs:158-166`（`RecoveryCandidate` 加一列）
- Modify: `src-tauri/src/database/repositories/route_credential_repository.rs:1324`（`list_recovery_candidates` 的投影）
- Test: 各自文件的 `mod tests`

**Interfaces:**
- Consumes: Task 2 `model_state_key`、Task 3 全部仓储方法、Task 4 `is_account_scoped_failure`、Task 6 `FailureScope`。
- Produces:
  - `RecoveryCandidate` 新增字段 `has_model_failures: i64`（SQLite 无布尔，`EXISTS` 返回 0/1）
  - `needs_recovery(status, next_retry_at, cooldown_until, has_model_failures: bool) -> bool`
  - `RouteCredentialRepository::reactivate_credential` 行为扩展（签名不变）

- [ ] **Step 1: 写失败的测试**

在 `route_model_test_service.rs` 的 `mod tests` 内追加：

```rust
    #[tokio::test]
    async fn a_successful_model_test_only_clears_the_model_it_tested() {
        use crate::database::repositories::route_credential_model_repository::RouteCredentialModelRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let upstream = start_fixed_upstream_ok(r#"{"choices":[{"message":{"content":"ai-switch-ok"}}]}"#).await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let credential = RouteCredentialRepository::create(
            &pool, "codex", "api", "Dual", None, "ok", None,
            r#"{"api_key":"sk-test"}"#,
            &serde_json::json!({
                "base_url": upstream,
                "interface_format": "openai",
                "model_mappings": [
                    {"from": "gpt-5.6-sol", "to": "upstream-sol"},
                    {"from": "glm-5.3", "to": "upstream-glm"}
                ]
            })
            .to_string(),
            "{}",
        )
        .await
        .expect("create");

        // Park both models, then test only one of them.
        let mut conn = pool.acquire().await.expect("conn");
        for key in ["upstream-sol", "upstream-glm"] {
            RouteCredentialModelRepository::record_transient_failure(
                &mut conn, &credential.id, key, "upstream_status", "boom", None,
                Some(600), Some(429), 10, true,
            )
            .await
            .expect("park");
        }
        drop(conn);

        RouteModelTestService::test_model(
            &pool,
            RoutePoolModelTestRequest {
                platform: "codex".to_string(),
                account_id: Some(credential.id.clone()),
                model: Some("glm-5.3".to_string()),
                interface_format: None,
            },
        )
        .await
        .expect("test");

        let states =
            RouteCredentialModelRepository::list_for_credentials(&pool, &[credential.id])
                .await
                .expect("states");
        // Proving glm-5.3 works says nothing about gpt-5.6-sol.
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].model_key, "upstream-sol");
    }

    #[tokio::test]
    async fn a_failing_model_test_parks_only_that_model() {
        use crate::database::repositories::route_credential_model_repository::RouteCredentialModelRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let upstream = start_fixed_status_upstream(
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            r#"{"error":{"message":"rate limited"}}"#,
        )
        .await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let credential = RouteCredentialRepository::create(
            &pool, "codex", "api", "Dual", None, "ok", None,
            r#"{"api_key":"sk-test"}"#,
            &serde_json::json!({
                "base_url": upstream,
                "interface_format": "openai",
                "model_mappings": [
                    {"from": "gpt-5.6-sol", "to": "upstream-sol"},
                    {"from": "glm-5.3", "to": "upstream-glm"}
                ],
                "failure_policy": {"cooldown_enabled": true, "cooldown_seconds": 600, "retry_count": 0}
            })
            .to_string(),
            "{}",
        )
        .await
        .expect("create");

        let _ = RouteModelTestService::test_model(
            &pool,
            RoutePoolModelTestRequest {
                platform: "codex".to_string(),
                account_id: Some(credential.id.clone()),
                model: Some("gpt-5.6-sol".to_string()),
                interface_format: None,
            },
        )
        .await;

        let states = RouteCredentialModelRepository::list_for_credentials(
            &pool, &[credential.id.clone()],
        )
        .await
        .expect("states");
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].model_key, "upstream-sol");
        let account = RouteCredentialRepository::get(&pool, &credential.id)
            .await
            .expect("account");
        // 429 is the upstream's verdict on one model, not on the key.
        assert!(account.cooldown_until.is_none());
    }
```

若 `start_fixed_upstream_ok` / `start_fixed_status_upstream` 这两个 helper 在该文件的测试模块中不存在，参照 `:1780`、`:1808` 已有的 `TcpListener::bind(("127.0.0.1", 0))` + `axum::serve` 写法各加一个。

在 `route_recovery_service.rs` 的 `mod tests` 内追加：

```rust
    #[test]
    fn an_account_with_only_model_failures_still_needs_recovery() {
        // Account-level columns look healthy, but one model is parked — without
        // this the scheduler would never probe it and only live traffic would.
        assert!(needs_recovery("ok", None, None, true));
        assert!(!needs_recovery("ok", None, None, false));
    }

    #[test]
    fn a_revoked_account_is_never_recovered_even_with_model_failures() {
        assert!(!needs_recovery("revoked", None, None, true));
    }
```

在 `route_credential_repository.rs` 的 `mod tests` 内追加：

```rust
    #[tokio::test]
    async fn reactivating_an_account_keeps_paused_models_paused() {
        use crate::database::repositories::route_credential_model_repository::RouteCredentialModelRepository;

        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let created = create_api_credential(&pool, "codex", "Reactivate").await;

        let mut conn = pool.acquire().await.expect("conn");
        RouteCredentialModelRepository::record_transient_failure(
            &mut conn, &created.id, "auto-parked", "upstream_status", "boom", None,
            Some(600), Some(429), 10, true,
        )
        .await
        .expect("park");
        drop(conn);
        RouteCredentialModelRepository::set_status(&pool, &created.id, "held", "paused")
            .await
            .expect("pause");

        RouteCredentialRepository::reactivate_credential(&pool, &created.id)
            .await
            .expect("reactivate");

        let states = RouteCredentialModelRepository::list_for_credentials(&pool, &[created.id])
            .await
            .expect("states");
        // Scheduled recovery clears what automation parked, never what the user did.
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].model_key, "held");
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib only_clears_the_model_it_tested`
Expected: FAIL —— `states.len()` 为 0（Task 6 的占位实现清了整账号）。

Run: `cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib an_account_with_only_model_failures`
Expected: FAIL —— `needs_recovery` 只接三个参数。

- [ ] **Step 3: 改写 finish_outcome**

在 `route_model_test_service.rs` 的 `finish_outcome`（`:1278`）开头算出模型键。函数已有 `credential` 与 `parts`，`parts` 里带着实际请求的模型名：

```rust
    // The key this test's result should be charged to. `parts.model` is what the
    // probe actually sent, which is what the upstream judged.
    let capability = parse_model_capability(&credential.config_json);
    let model_key = Some(model_state_key(
        platform,
        &capability,
        &credential.kind,
        &parts.model,
    ));
```

若 `parts` 结构里模型字段名不是 `model`，读 `build_model_test_request`（`:569`）确认后替换。

成功分支（`:1296-1300`）：

```rust
    if success {
        let _ =
            RouteCredentialRepository::clear_transient_failure(pool, &credential.id, model_key.as_deref())
                .await;
        if should_restore_model_test_account_status(&credential.status) {
            RouteCredentialRepository::update_status(pool, &credential.id, "ok").await?;
        }
    } else {
```

三处失败记账。共用一个局部闭包算 scope，避免重复三遍：

```rust
        let siblings = known_upstream_models(platform, &capability, &credential.kind);
        let scope_for = |kind: &str, status: Option<u16>| {
            match (&model_key, is_account_scoped_failure(kind, status)) {
                (Some(key), false) => FailureScope::Model {
                    key: key.as_str(),
                    siblings: &siblings,
                },
                _ => FailureScope::Account,
            }
        };
```

- `:1312`（`model_test_status`）：`scope_for("model_test_status", Some(status.as_u16()))`
- `:1321`（`semantic_response_transient`）：`scope_for("semantic_response_transient", Some(200))`
- `:1345`（`model_test`）：`scope_for("model_test", None)` —— 分级表判定为账号级，传输层失败与模型无关。

`quota_failure`（`:1306`）与 `Permanent`（`:1338`）继续 `update_status`，不动。

顶部 `use` 补 `model_state_key`、`known_upstream_models`、`FailureScope`、`is_account_scoped_failure`。

- [ ] **Step 4: 扩展 reactivate_credential**

`route_credential_repository.rs:1560` 的 `reactivate_credential` 改为事务，在现有 UPDATE 之后清模型行：

```rust
    pub async fn reactivate_credential(pool: &SqlitePool, id: &str) -> Result<(), AppError> {
        let mut tx = pool.begin().await.map_err(|err| {
            database_error(
                "database.route_credential_recover",
                "Could not start route credential recovery",
                err,
            )
        })?;
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            // ... 现有 UPDATE 原样，execute 改为 &mut *tx ...
        )
        .bind(&now)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(/* 现有 */)?;

        // Automation may undo automation, never a human decision.
        RouteCredentialModelRepository::clear_all_unpaused(&mut tx, id).await?;

        if result.rows_affected() == 0 {
            // ... 现有的存在性检查，fetch_one 改为 &mut *tx ...
        }
        tx.commit().await.map_err(|err| {
            database_error(
                "database.route_credential_recover",
                "Could not save route credential recovery",
                err,
            )
        })?;
        Ok(())
    }
```

- [ ] **Step 5: 扩展恢复候选投影**

`src-tauri/src/models/route_credential.rs` 的 `RecoveryCandidate`（`:158-166`）加一列：

```rust
pub struct RecoveryCandidate {
    pub id: String,
    pub platform: String,
    pub status: String,
    pub config_json: String,
    pub next_retry_at: Option<String>,
    pub cooldown_until: Option<String>,
    /// 1 when the account has non-paused model rows. An account can look healthy
    /// at the account level while one of its models is parked, and that case must
    /// still reach the scheduler.
    pub has_model_failures: i64,
}
```

`list_recovery_candidates`（`route_credential_repository.rs:1324`）的 SQL 加子查询：

```sql
SELECT id, platform, status, config_json, next_retry_at, cooldown_until,
       EXISTS (
         SELECT 1 FROM route_credential_models m
         WHERE m.route_credential_id = route_credentials.id AND m.status != 'paused'
       ) AS has_model_failures
FROM route_credentials
WHERE archived_at IS NULL
```

- [ ] **Step 6: 改写恢复调度**

`route_recovery_service.rs` 的 `needs_recovery`（`:166`）：

```rust
/// True when a non-revoked account is not fully healthy: status is not "ok"
/// (paused/error/warning), it still carries a retry/cooldown window, or one of
/// its models is parked.
fn needs_recovery(
    status: &str,
    next_retry_at: Option<&str>,
    cooldown_until: Option<&str>,
    has_model_failures: bool,
) -> bool {
    if status == "revoked" {
        return false;
    }
    status != "ok" || next_retry_at.is_some() || cooldown_until.is_some() || has_model_failures
}
```

`run_tick`（`:113`）的调用点补第四参数 `candidate.has_model_failures != 0`。

Healthcheck 分支（`:130-150`）显式挑探测模型：

```rust
                RecoveryMode::Healthcheck => {
                    let interval = rule
                        .probe_interval_minutes
                        .unwrap_or(DEFAULT_PROBE_INTERVAL_MINUTES)
                        .max(1);
                    if down && probe_is_due(probe_state, &candidate.id, interval, now_utc).await {
                        // Probe the model that has been parked longest rather than
                        // whichever mapping happens to be first: the account may be
                        // in recovery precisely because of the third one.
                        let model = RouteCredentialModelRepository::oldest_recoverable_key(
                            pool,
                            &candidate.id,
                            &now_utc.to_rfc3339(),
                        )
                        .await
                        .ok()
                        .flatten();
                        // A successful explicit test auto-recovers the account via
                        // recover_after_explicit_test inside test_model.
                        let _ = RouteModelTestService::test_model_with_activity(
                            pool,
                            activity,
                            RoutePoolModelTestRequest {
                                platform: candidate.platform.clone(),
                                account_id: Some(candidate.id.clone()),
                                model,
                                interface_format: None,
                            },
                        )
                        .await;
                    }
                }
```

注意：`oldest_recoverable_key` 返回的是**上游模型键**，而 `RoutePoolModelTestRequest.model` 期望的是**请求侧名字**（`request_model` 会拿它去匹配 `mapping.from`）。对 api 账号这两者不同。因此仓储查出键后，需在此处反查回请求侧别名 —— 在 `route_model_capability.rs` 加一个反查函数：

```rust
/// Map an upstream model key back to a client-facing alias, for places that must
/// speak the request vocabulary (the model test takes `mapping.from`).
pub(crate) fn alias_for_model_key(
    capability: &ModelCapability,
    model_key: &str,
) -> Option<String> {
    capability
        .mappings
        .iter()
        .find(|mapping| {
            !is_fallback_mapping(mapping)
                && strip_one_m_suffix_for_route_lookup(&mapping.to) == model_key
        })
        .map(|mapping| mapping.from.trim().to_string())
}
```

在 Healthcheck 分支里，取到键之后：

```rust
                        let capability = parse_model_capability(&candidate.config_json);
                        let model = model.and_then(|key| {
                            alias_for_model_key(&capability, &key).or(Some(key))
                        });
```

`.or(Some(key))` 覆盖 official 与空映射账号 —— 它们的键本身就是请求侧名字。

给 `alias_for_model_key` 补一个单测（放在 `route_model_capability.rs` 的 `mod tests`）：

```rust
    #[test]
    fn alias_for_model_key_maps_upstream_names_back_to_client_aliases() {
        let capability = parse_model_capability(
            r#"{"model_mappings":[{"from":"gpt-5.6-sol","to":"upstream-sol"}]}"#,
        );
        assert_eq!(
            alias_for_model_key(&capability, "upstream-sol").as_deref(),
            Some("gpt-5.6-sol")
        );
        assert!(alias_for_model_key(&capability, "unknown").is_none());
    }
```

- [ ] **Step 7: 跑测试确认通过**

Run: `cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib route_model_test_service route_recovery_service route_model_capability`
Expected: PASS

- [ ] **Step 8: 全量 Rust 测试并提交**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo fmt && CARGO_TARGET_DIR=target-codex cargo test
cd ..
git add src-tauri/src/services/route_model_test_service.rs src-tauri/src/services/route_recovery_service.rs src-tauri/src/services/route_model_capability.rs src-tauri/src/database/repositories/route_credential_repository.rs src-tauri/src/models/route_credential.rs
git commit -m "feat: 模型测试与自动恢复按模型记账"
```

---

### Task 8: 下发模型状态与两个新命令

**Files:**
- Modify: `src-tauri/src/models/route_credential.rs:136`（`RouteCredential` 加 `model_states`）
- Modify: `src-tauri/src/services/route_credential_service.rs:40-90`（三个读取路径批量填充）
- Modify: `src-tauri/src/commands/route_credential_commands.rs`
- Modify: `src-tauri/src/lib.rs:439-458`
- Modify: `src-tauri/src/web/handlers/mod.rs:495` 附近
- Modify: `src/lib/api/types.ts`、`src/lib/api/client.ts`
- Test: `src-tauri/src/services/route_credential_service.rs` 的 `mod tests`；`tests/transport/command-contract.test.ts`（自动守，无需改）

**Interfaces:**
- Consumes: Task 1 的 `RouteCredentialModelState`、Task 2 的 `known_upstream_models`、Task 3 的 `list_for_credentials` 与 `set_status`/`clear`。
- Produces:
  - `RouteCredential.model_states: Vec<RouteCredentialModelState>`（`#[sqlx(skip)]`，`#[serde(default)]`）
  - Tauri command `clear_route_credential_model_state(id: String, model_key: String) -> RouteCredential`
  - Tauri command `set_route_credential_model_status(id: String, model_key: String, status: String) -> RouteCredential`
  - TS `RouteCredentialModelState` 类型与两个 client 函数 `clearRouteCredentialModelState`、`setRouteCredentialModelStatus`

- [ ] **Step 1: 写失败的测试**

在 `route_credential_service.rs` 的 `mod tests` 内追加（若该文件没有 `mod tests`，新建一个，参考同目录其他服务的写法）：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::route_credential_model_repository::RouteCredentialModelRepository;
    use crate::database::{create_memory_pool, run_migrations};

    async fn dual_model_credential(pool: &SqlitePool) -> String {
        RouteCredentialRepository::create(
            pool, "codex", "api", "Dual", None, "ok", None,
            r#"{"api_key":"sk-test"}"#,
            &serde_json::json!({
                "base_url": "https://example.com",
                "interface_format": "openai",
                "model_mappings": [
                    {"from": "gpt-5.6-sol", "to": "upstream-sol"},
                    {"from": "glm-5.3", "to": "upstream-glm"}
                ]
            })
            .to_string(),
            "{}",
        )
        .await
        .expect("create")
        .id
    }

    #[tokio::test]
    async fn listing_accounts_reports_every_known_model_including_healthy_ones() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = dual_model_credential(&pool).await;

        let mut conn = pool.acquire().await.expect("conn");
        RouteCredentialModelRepository::record_transient_failure(
            &mut conn, &id, "upstream-sol", "upstream_status", "boom", None,
            Some(600), Some(429), 10, true,
        )
        .await
        .expect("park");
        drop(conn);

        let credentials = RouteCredentialService::list(&pool, "codex".to_string())
            .await
            .expect("list");
        let states = &credentials[0].model_states;
        // Healthy models must be listed too, otherwise the UI cannot offer to
        // pause a model that has never failed.
        assert_eq!(states.len(), 2);
        let parked = states
            .iter()
            .find(|state| state.model_key == "upstream-sol")
            .expect("parked model");
        assert!(parked.cooldown_until.is_some());
        let healthy = states
            .iter()
            .find(|state| state.model_key == "upstream-glm")
            .expect("healthy model");
        assert_eq!(healthy.status, MODEL_STATUS_OK);
        assert!(healthy.cooldown_until.is_none());
        assert_eq!(healthy.aliases, vec!["glm-5.3".to_string()]);
    }

    #[tokio::test]
    async fn an_orphan_row_survives_a_mapping_removal_with_no_aliases() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = dual_model_credential(&pool).await;

        RouteCredentialModelRepository::set_status(&pool, &id, "upstream-gone", MODEL_STATUS_PAUSED)
            .await
            .expect("pause a model that is not mapped any more");

        let credentials = RouteCredentialService::list(&pool, "codex".to_string())
            .await
            .expect("list");
        let orphan = credentials[0]
            .model_states
            .iter()
            .find(|state| state.model_key == "upstream-gone")
            .expect("orphan row is still reported");
        // Silently dropping the user's pause would be worse than showing a row
        // they can explicitly clear.
        assert!(orphan.aliases.is_empty());
        assert_eq!(orphan.status, MODEL_STATUS_PAUSED);
    }

    #[tokio::test]
    async fn setting_a_model_back_to_ok_removes_its_row() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = dual_model_credential(&pool).await;

        RouteCredentialService::set_model_status(
            &pool, id.clone(), "upstream-sol".to_string(), MODEL_STATUS_PAUSED.to_string(),
        )
        .await
        .expect("pause");
        let paused = RouteCredentialModelRepository::list_for_credentials(&pool, &[id.clone()])
            .await
            .expect("rows");
        assert_eq!(paused.len(), 1);

        RouteCredentialService::set_model_status(
            &pool, id.clone(), "upstream-sol".to_string(), MODEL_STATUS_OK.to_string(),
        )
        .await
        .expect("resume");
        assert!(RouteCredentialModelRepository::list_for_credentials(&pool, &[id])
            .await
            .expect("rows")
            .is_empty());
    }

    #[tokio::test]
    async fn setting_a_model_to_error_is_rejected() {
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let id = dual_model_credential(&pool).await;

        // `error` is reached only through a semantic failure streak.
        let error = RouteCredentialService::set_model_status(
            &pool, id, "upstream-sol".to_string(), MODEL_STATUS_ERROR.to_string(),
        )
        .await
        .expect_err("error is not a user-settable status");
        assert!(matches!(
            error,
            AppError::Validation {
                code: "validation.route_credential_model_status",
                ..
            }
        ));
    }
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib route_credential_service::tests`
Expected: FAIL —— `model_states` 字段与 `set_model_status` 方法都不存在。

- [ ] **Step 3: 扩展模型结构**

`src-tauri/src/models/route_credential_model.rs` 的 `RouteCredentialModelState` 加一个非持久化字段：

```rust
    /// Client-facing aliases pointing at this upstream model. Empty when the
    /// mapping was removed while the row lived on. Filled by the service layer,
    /// never stored.
    #[sqlx(default)]
    #[serde(default)]
    pub aliases: Vec<String>,
```

`Vec<String>` 无法从 SQLite 列直接解码，所以 `#[sqlx(default)]` 是必需的。

`src-tauri/src/models/route_credential.rs` 在 `active_request_count`（`:137`）之后插入：

```rust
    #[sqlx(default)]
    #[serde(default)]
    pub model_states: Vec<RouteCredentialModelState>,
```

顶部 `use crate::models::route_credential_model::RouteCredentialModelState;`。

`cpa_export_service.rs:724` 那处手工构造 `RouteCredential` 的地方需要补 `model_states: Vec::new()`。全量 `cargo check` 会指出所有构造点。

- [ ] **Step 4: 服务层批量填充**

在 `route_credential_service.rs` 加一个填充函数与两个新方法：

```rust
/// Attach per-model state to each account: every known model gets an entry, with
/// healthy ones synthesised so the UI can pause a model that has never failed.
async fn attach_model_states(
    pool: &SqlitePool,
    mut credentials: Vec<RouteCredential>,
) -> Result<Vec<RouteCredential>, AppError> {
    if credentials.is_empty() {
        return Ok(credentials);
    }
    let ids: Vec<String> = credentials
        .iter()
        .map(|credential| credential.id.clone())
        .collect();
    let mut rows = RouteCredentialModelRepository::list_for_credentials(pool, &ids).await?;

    for credential in &mut credentials {
        let capability = parse_model_capability(&credential.config_json);
        let known = known_upstream_models(&credential.platform, &capability, &credential.kind);
        let mut states: Vec<RouteCredentialModelState> = Vec::new();

        let (mine, rest): (Vec<_>, Vec<_>) = rows
            .into_iter()
            .partition(|row| row.route_credential_id == credential.id);
        rows = rest;

        for mut row in mine {
            row.aliases = aliases_for_model_key(&capability, &row.model_key);
            states.push(row);
        }
        for model_key in known {
            if states.iter().any(|state| state.model_key == model_key) {
                continue;
            }
            states.push(RouteCredentialModelState {
                route_credential_id: credential.id.clone(),
                aliases: aliases_for_model_key(&capability, &model_key),
                model_key,
                status: MODEL_STATUS_OK.to_string(),
                transient_failure_count: 0,
                cooldown_until: None,
                semantic_failure_streak_count: 0,
                semantic_failure_streak_fingerprint: None,
                last_failure_kind: None,
                last_failure_message: None,
                last_failure_response_json: None,
                created_at: credential.created_at.clone(),
                updated_at: credential.updated_at.clone(),
            });
        }
        states.sort_by(|left, right| left.model_key.cmp(&right.model_key));
        credential.model_states = states;
    }
    Ok(credentials)
}
```

`aliases_for_model_key` 是 Task 7 的 `alias_for_model_key` 的复数版，加在 `route_model_capability.rs`：

```rust
/// Every client-facing alias pointing at this upstream model. A relay config can
/// route two aliases to one upstream model, and the UI shows them all so users
/// recognise the row by the name they typed.
pub(crate) fn aliases_for_model_key(
    capability: &ModelCapability,
    model_key: &str,
) -> Vec<String> {
    capability
        .mappings
        .iter()
        .filter(|mapping| {
            !is_fallback_mapping(mapping)
                && strip_one_m_suffix_for_route_lookup(&mapping.to) == model_key
        })
        .map(|mapping| mapping.from.trim().to_string())
        .collect()
}
```

`alias_for_model_key`（Task 7）可改为 `aliases_for_model_key(...).into_iter().next()`，避免两份筛选逻辑。

三个读取路径接上：

- `list`（`:46`）：`Ok(attach_model_states(pool, RouteCredentialRepository::list_by_platform(...).await?).await?)`
- `get`（`:59`）：单条 —— `attach_model_states(pool, vec![credential]).await?.pop()`
- `page`（`:78`）：对 `page.items` 调用

`list_with_activity` / `get_with_activity` / `page_with_activity` 走的是上面这些，无需另改。

两个新服务方法：

```rust
    pub async fn set_model_status(
        pool: &SqlitePool,
        id: String,
        model_key: String,
        status: String,
    ) -> Result<RouteCredential, AppError> {
        // `error` is reached only through a semantic failure streak, so accepting
        // it here would let the UI fake a verdict the system never reached.
        if status != MODEL_STATUS_OK && status != MODEL_STATUS_PAUSED {
            return Err(AppError::Validation {
                code: "validation.route_credential_model_status",
                message: "Model status must be 'ok' or 'paused'".to_string(),
                details: Some(status),
                recoverable: true,
            });
        }
        RouteCredentialModelRepository::set_status(pool, &id, &model_key, &status).await?;
        Self::get(pool, id).await
    }

    pub async fn clear_model_state(
        pool: &SqlitePool,
        id: String,
        model_key: String,
    ) -> Result<RouteCredential, AppError> {
        RouteCredentialModelRepository::clear(pool, &id, &model_key).await?;
        Self::get(pool, id).await
    }
```

- [ ] **Step 5: 接线两个 command**

`src-tauri/src/commands/route_credential_commands.rs` 在 `set_route_credential_recovery`（`:110`）之后追加：

```rust
#[tauri::command]
pub async fn set_route_credential_model_status(
    state: State<'_, AppState>,
    id: String,
    model_key: String,
    status: String,
) -> Result<RouteCredential, ApiError> {
    let credential = RouteCredentialService::set_model_status(&state.pool, id, model_key, status)
        .await
        .map_err(ApiError::from)?;
    state
        .route_proxy
        .activity()
        .notify_status_change(&credential.platform, &credential.id);
    Ok(credential)
}

#[tauri::command]
pub async fn clear_route_credential_model_state(
    state: State<'_, AppState>,
    id: String,
    model_key: String,
) -> Result<RouteCredential, ApiError> {
    let credential = RouteCredentialService::clear_model_state(&state.pool, id, model_key)
        .await
        .map_err(ApiError::from)?;
    state
        .route_proxy
        .activity()
        .notify_status_change(&credential.platform, &credential.id);
    Ok(credential)
}
```

`notify_status_change` 是关键 —— 前端已订阅 `route-credential-status`（`AccountsScreen.tsx:2504`），手动操作也要触发刷新，否则其他窗口看到的是旧状态。

`src-tauri/src/lib.rs` 的 `generate_handler![]` 在 `set_route_credential_recovery,`（`:445`）之后加两行：

```rust
            set_route_credential_model_status,
            clear_route_credential_model_state,
```

`src-tauri/src/web/handlers/mod.rs` 在 `"set_route_credential_recovery"`（`:520`）分支之后加：

```rust
        "set_route_credential_model_status" => {
            let id = required_string_arg(&args, "id")?;
            let model_key = required_string_arg(&args, "model_key")?;
            let status = required_string_arg(&args, "status")?;
            to_value(
                RouteCredentialService::set_model_status(&state.pool, id, model_key, status)
                    .await
                    .map_err(to_error)?,
            )
        }
        "clear_route_credential_model_state" => {
            let id = required_string_arg(&args, "id")?;
            let model_key = required_string_arg(&args, "model_key")?;
            to_value(
                RouteCredentialService::clear_model_state(&state.pool, id, model_key)
                    .await
                    .map_err(to_error)?,
            )
        }
```

- [ ] **Step 6: 前端类型与封装**

`src/lib/api/types.ts` 在 `RouteCredential`（`:172`）之前插入：

```ts
export type RouteCredentialModelStatus = "ok" | "error" | "paused";

export type RouteCredentialModelState = {
  route_credential_id: string;
  model_key: string;
  aliases: string[];
  status: RouteCredentialModelStatus;
  transient_failure_count: number;
  cooldown_until?: string | null;
  semantic_failure_streak_count: number;
  last_failure_kind?: string | null;
  last_failure_message?: string | null;
  last_failure_response_json?: string | null;
  created_at: string;
  updated_at: string;
};
```

`RouteCredential` 在 `active_request_count?: number;`（`:203`）之后加：

```ts
  model_states?: RouteCredentialModelState[];
```

`src/lib/api/client.ts` 在 `setRouteCredentialRecovery`（`:345`）之后加：

```ts
export function setRouteCredentialModelStatus(
  id: string,
  modelKey: string,
  status: RouteCredentialModelStatus,
): Promise<RouteCredential> {
  return invoke("set_route_credential_model_status", {
    id,
    model_key: modelKey,
    status,
  });
}

export function clearRouteCredentialModelState(
  id: string,
  modelKey: string,
): Promise<RouteCredential> {
  return invoke("clear_route_credential_model_state", { id, model_key: modelKey });
}
```

`client.ts` 顶部的类型 import 补 `RouteCredentialModelStatus`。

- [ ] **Step 7: 跑测试**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo fmt && CARGO_TARGET_DIR=target-codex cargo test
cd .. && pnpm typecheck && pnpm vitest run tests/transport/command-contract.test.ts
```

Expected: 全绿。契约测试验证两个新 command 三处齐备。

- [ ] **Step 8: 提交**

```bash
git add src-tauri/src/models/route_credential.rs src-tauri/src/models/route_credential_model.rs src-tauri/src/services/route_credential_service.rs src-tauri/src/services/route_model_capability.rs src-tauri/src/services/cpa_export_service.rs src-tauri/src/commands/route_credential_commands.rs src-tauri/src/lib.rs src-tauri/src/web/handlers/mod.rs src/lib/api/types.ts src/lib/api/client.ts
git commit -m "feat: 下发模型状态并支持手动暂停与解除"
```

---

### Task 9: 前端徽章、悬停明细与抽屉区块

**Files:**
- Modify: `src/screens/AccountsScreen.tsx`
- Test: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes: Task 8 的 `RouteCredentialModelState` 类型、`RouteCredential.model_states`、`setRouteCredentialModelStatus`、`clearRouteCredentialModelState`。
- Produces: 无下游依赖（终点任务之一）。

新增的 testid 与 aria-label 契约：
- `credential-model-issues-<id>`：行内汇总徽章
- `credential-model-detail-<id>`：悬停明细容器
- `模型状态`：抽屉区块的 `aria-label`
- `暂停模型 <model_key>` / `恢复模型 <model_key>` / `解除模型 <model_key>` / `解除全部模型冷却`：抽屉内按钮的 `aria-label`

- [ ] **Step 1: 写失败的测试**

在 `tests/AccountsScreen.test.tsx` 内追加。`credentialsFixture`（`:125`）需要一个带 `model_states` 的账号 —— 在测试内就地覆盖而非改 fixture，避免影响既有 119 个测试：

```tsx
  const modelStatesFixture = [
    {
      route_credential_id: "cred-api-1",
      model_key: "upstream-sol",
      aliases: ["gpt-5.6-sol"],
      status: "ok" as const,
      transient_failure_count: 2,
      cooldown_until: new Date(Date.now() + 45_000).toISOString(),
      semantic_failure_streak_count: 0,
      last_failure_kind: "upstream_status",
      last_failure_message: "upstream returned 429",
      last_failure_response_json: null,
      created_at: "2026-09-02T00:00:00Z",
      updated_at: "2026-09-02T00:00:00Z",
    },
    {
      route_credential_id: "cred-api-1",
      model_key: "upstream-glm",
      aliases: ["glm-5.3"],
      status: "ok" as const,
      transient_failure_count: 0,
      cooldown_until: null,
      semantic_failure_streak_count: 0,
      last_failure_kind: null,
      last_failure_message: null,
      last_failure_response_json: null,
      created_at: "2026-09-02T00:00:00Z",
      updated_at: "2026-09-02T00:00:00Z",
    },
    {
      route_credential_id: "cred-api-1",
      model_key: "upstream-held",
      aliases: ["held-model"],
      status: "paused" as const,
      transient_failure_count: 0,
      cooldown_until: null,
      semantic_failure_streak_count: 0,
      last_failure_kind: null,
      last_failure_message: null,
      last_failure_response_json: null,
      created_at: "2026-09-02T00:00:00Z",
      updated_at: "2026-09-02T00:00:00Z",
    },
  ];

  function credentialsWithModelStates() {
    return credentialsFixture.map((credential) =>
      credential.id === "cred-api-1"
        ? { ...credential, model_states: modelStatesFixture }
        : credential,
    );
  }

  it("在账号行显示不可用模型的汇总徽章", async () => {
    vi.mocked(api.listRouteCredentials).mockResolvedValue(credentialsWithModelStates());
    await renderScreen();
    await selectAccountView("算力池");

    const badge = await screen.findByTestId("credential-model-issues-cred-api-1");
    // One cooling model plus one paused model; the healthy one is not counted.
    expect(badge).toHaveTextContent("模型 2 不可用");
  });

  it("不为全部模型健康的账号显示模型徽章", async () => {
    vi.mocked(api.listRouteCredentials).mockResolvedValue(
      credentialsFixture.map((credential) =>
        credential.id === "cred-api-1"
          ? { ...credential, model_states: [modelStatesFixture[1]] }
          : credential,
      ),
    );
    await renderScreen();
    await selectAccountView("算力池");

    expect(screen.queryByTestId("credential-model-issues-cred-api-1")).toBeNull();
  });

  it("悬停徽章时展示逐模型明细", async () => {
    vi.mocked(api.listRouteCredentials).mockResolvedValue(credentialsWithModelStates());
    await renderScreen();
    await selectAccountView("算力池");

    const detail = await screen.findByTestId("credential-model-detail-cred-api-1");
    expect(detail).toHaveTextContent("upstream-sol");
    // Aliases matter: the user configured "gpt-5.6-sol", not the upstream name.
    expect(detail).toHaveTextContent("gpt-5.6-sol");
    expect(detail).toHaveTextContent("upstream-held");
    expect(detail).toHaveTextContent("已暂停");
    expect(detail).not.toHaveTextContent("upstream-glm");
  });

  it("在编辑抽屉里列出全部已知模型并可暂停", async () => {
    vi.mocked(api.listRouteCredentials).mockResolvedValue(credentialsWithModelStates());
    vi.mocked(api.setRouteCredentialModelStatus).mockResolvedValue(
      credentialsWithModelStates()[1],
    );
    await renderScreen();
    await selectAccountView("算力池");
    await userEvent.click(await screen.findByLabelText("编辑 API Account"));

    const section = await screen.findByLabelText("模型状态");
    // Healthy models are listed too, so a model can be paused before it fails.
    expect(section).toHaveTextContent("upstream-glm");

    await userEvent.click(screen.getByLabelText("暂停模型 upstream-glm"));
    expect(api.setRouteCredentialModelStatus).toHaveBeenCalledWith(
      "cred-api-1",
      "upstream-glm",
      "paused",
    );
  });

  it("可恢复已暂停的模型", async () => {
    vi.mocked(api.listRouteCredentials).mockResolvedValue(credentialsWithModelStates());
    vi.mocked(api.setRouteCredentialModelStatus).mockResolvedValue(
      credentialsWithModelStates()[1],
    );
    await renderScreen();
    await selectAccountView("算力池");
    await userEvent.click(await screen.findByLabelText("编辑 API Account"));

    await userEvent.click(await screen.findByLabelText("恢复模型 upstream-held"));
    expect(api.setRouteCredentialModelStatus).toHaveBeenCalledWith(
      "cred-api-1",
      "upstream-held",
      "ok",
    );
  });

  it("可解除单个模型的冷却", async () => {
    vi.mocked(api.listRouteCredentials).mockResolvedValue(credentialsWithModelStates());
    vi.mocked(api.clearRouteCredentialModelState).mockResolvedValue(
      credentialsWithModelStates()[1],
    );
    await renderScreen();
    await selectAccountView("算力池");
    await userEvent.click(await screen.findByLabelText("编辑 API Account"));

    await userEvent.click(await screen.findByLabelText("解除模型 upstream-sol"));
    expect(api.clearRouteCredentialModelState).toHaveBeenCalledWith(
      "cred-api-1",
      "upstream-sol",
    );
  });

  it("一次解除全部非暂停模型", async () => {
    vi.mocked(api.listRouteCredentials).mockResolvedValue(credentialsWithModelStates());
    vi.mocked(api.clearRouteCredentialModelState).mockResolvedValue(
      credentialsWithModelStates()[1],
    );
    await renderScreen();
    await selectAccountView("算力池");
    await userEvent.click(await screen.findByLabelText("编辑 API Account"));

    await userEvent.click(await screen.findByLabelText("解除全部模型冷却"));
    // Only the cooling model: a paused one is the user's own decision.
    expect(api.clearRouteCredentialModelState).toHaveBeenCalledTimes(1);
    expect(api.clearRouteCredentialModelState).toHaveBeenCalledWith(
      "cred-api-1",
      "upstream-sol",
    );
  });

  it("已失效账号不显示模型徽章", async () => {
    vi.mocked(api.listRouteCredentials).mockResolvedValue(
      credentialsFixture.map((credential) =>
        credential.id === "cred-api-1"
          ? { ...credential, status: "revoked" as const, model_states: modelStatesFixture }
          : credential,
      ),
    );
    await renderScreen();
    await selectAccountView("算力池");

    // The account itself is dead; per-model detail would only add noise.
    expect(screen.queryByTestId("credential-model-issues-cred-api-1")).toBeNull();
  });
```

`vi.mock("../src/lib/api/client")`（`:62`）的 mock 对象需补两个新函数。检查该 mock 是自动 mock 还是手写对象：若手写，加 `setRouteCredentialModelStatus: vi.fn()`、`clearRouteCredentialModelState: vi.fn()`。

- [ ] **Step 2: 跑测试确认失败**

Run: `pnpm vitest run tests/AccountsScreen.test.tsx -t "模型"`
Expected: FAIL —— `Unable to find an element by: [data-testid="credential-model-issues-cred-api-1"]`。

- [ ] **Step 3: 写状态聚合辅助函数**

在 `AccountsScreen.tsx` 的 `transientFailureTag`（`:274-286`）之后插入：

```tsx
type ModelIssue = {
  state: RouteCredentialModelState;
  reason: "cooling" | "error" | "paused";
  remaining: number | null;
};

// A model is unavailable when it is paused, marked unhealthy, or still cooling.
// Cooling is time-based so it needs `now`; the other two are verdicts.
function credentialModelIssues(credential: RouteCredential, now: number): ModelIssue[] {
  if (terminalAccountStatuses.has(credential.status)) {
    return [];
  }
  const issues: ModelIssue[] = [];
  for (const state of credential.model_states ?? []) {
    if (state.status === "paused") {
      issues.push({ state, reason: "paused", remaining: null });
      continue;
    }
    if (state.status === "error") {
      issues.push({ state, reason: "error", remaining: null });
      continue;
    }
    if (!state.cooldown_until) {
      continue;
    }
    const deadline = new Date(state.cooldown_until).getTime();
    if (!Number.isFinite(deadline) || deadline <= now) {
      continue;
    }
    issues.push({ state, reason: "cooling", remaining: deadline - now });
  }
  return issues;
}

function modelIssueLabel(issue: ModelIssue): string {
  switch (issue.reason) {
    case "paused":
      return "已暂停";
    case "error":
      return "异常";
    default:
      return `冷却 ${formatCooldownRemaining(issue.remaining ?? 0)}`;
  }
}
```

`terminalAccountStatuses`（`:272`）已存在，直接复用 —— 已失效/异常/暂停的账号不显示模型徽章，与 `transientFailureTag` 的判断保持一致。

- [ ] **Step 4: 扩展倒计时**

`useCooldownCountdown`（`:1237`）的 `nextDeadline` 计算改为同时扫模型级时间戳：

```tsx
function useCooldownCountdown(credentials: RouteCredential[]) {
  const nextDeadline = useMemo(() => {
    const deadlines = credentials
      .flatMap((credential) => [
        credential.cooldown_until || credential.next_retry_at,
        // Model-level cooldowns tick on the same timer: one interval covers both.
        ...(credential.model_states ?? []).map((state) => state.cooldown_until),
      ])
      .filter((raw): raw is string => Boolean(raw))
      .map((raw) => new Date(raw).getTime())
      .filter((time) => Number.isFinite(time));
    return deadlines.length > 0 ? Math.max(...deadlines) : null;
  }, [credentials]);

  // ... 其余原样不动 ...
}
```

- [ ] **Step 5: 写行内徽章与悬停明细**

在 `credential.map` 循环内（`:5190` 附近，`modelMappings` 那行之后）加：

```tsx
                  const modelIssues = credentialModelIssues(credential, cooldownNow);
```

在冷却徽章（`:5380-5390`）之后插入：

```tsx
                        {modelIssues.length > 0 && (
                          <span
                            className="group relative inline-flex outline-none focus:ring-2 focus:ring-orange-300"
                            tabIndex={0}
                          >
                            <span
                              className="rounded-full bg-orange-50 px-2 py-0.5 text-[11px] font-semibold text-orange-800"
                              data-testid={`credential-model-issues-${credential.id}`}
                              title="部分模型暂不参与路由"
                            >
                              模型 {modelIssues.length} 不可用
                            </span>
                            {/* pt-1 rather than mt-1: a margin gap drops :hover
                                mid-travel and closes the panel before the pointer
                                arrives. */}
                            <span
                              className="absolute left-0 top-full z-50 hidden pt-1 group-hover:block group-focus-within:block"
                              data-testid={`credential-model-detail-${credential.id}`}
                            >
                              <span className="block w-[min(28rem,calc(100vw-2rem))] select-text rounded-lg border border-stone-700 bg-stone-900 px-3 py-2 text-left text-[11px] font-medium leading-5 text-white shadow-xl">
                                {modelIssues.map((issue) => (
                                  <span
                                    className="mt-1 block first:mt-0"
                                    key={issue.state.model_key}
                                  >
                                    <span className="font-semibold">{issue.state.model_key}</span>
                                    {issue.state.aliases.length > 0 ? (
                                      <span className="text-stone-400">
                                        （{issue.state.aliases.join("、")}）
                                      </span>
                                    ) : (
                                      <span className="text-stone-400">（已移除映射）</span>
                                    )}
                                    <span className="ml-1 text-orange-200">
                                      {modelIssueLabel(issue)}
                                    </span>
                                    {issue.state.last_failure_message ? (
                                      <span className="mt-0.5 block break-words text-stone-300">
                                        {issue.state.last_failure_message}
                                      </span>
                                    ) : null}
                                  </span>
                                ))}
                              </span>
                            </span>
                          </span>
                        )}
```

明细面板直接内联而非复用 `CredentialFailureTooltip` —— 后者绑定的是账号级 `last_failure_response_json`，语义不同。

- [ ] **Step 6: 写抽屉区块与 mutation**

在 `AccountsScreen` 组件内新增两个 mutation（放在 `recoveryMutation` 附近，`:3458` 前后）：

```tsx
  const modelStatusMutation = useMutation({
    mutationFn: ({
      credentialId,
      modelKey,
      status,
    }: {
      credentialId: string;
      modelKey: string;
      status: RouteCredentialModelStatus;
    }) => setRouteCredentialModelStatus(credentialId, modelKey, status),
    onSuccess: (credential) => {
      setEditingCredential(credential);
      void queryClient.invalidateQueries({ queryKey: ["route-credential-page"] });
      void queryClient.invalidateQueries({ queryKey: ["route-credentials-all"] });
    },
  });

  const clearModelStateMutation = useMutation({
    mutationFn: ({
      credentialId,
      modelKey,
    }: {
      credentialId: string;
      modelKey: string;
    }) => clearRouteCredentialModelState(credentialId, modelKey),
    onSuccess: (credential) => {
      setEditingCredential(credential);
      void queryClient.invalidateQueries({ queryKey: ["route-credential-page"] });
      void queryClient.invalidateQueries({ queryKey: ["route-credentials-all"] });
    },
  });
```

`invalidateQueries` 的 key 前缀照 `AccountsScreen.tsx:2347`、`:2409` 的既有定义写。

在编辑抽屉的 `失败处理策略` section（结束于 `:6700` 之后的 `</section>`）之后插入：

```tsx
              <section
                aria-label="模型状态"
                className="mt-3 rounded-xl border border-orange-100 bg-orange-50/50 p-3"
              >
                <div className="flex items-start justify-between gap-3">
                  <div>
                    <p className="text-[13px] font-semibold text-stone-900">模型状态</p>
                    <p className="mt-0.5 text-[11px] font-medium text-stone-500">
                      冷却与异常由失败自动写入，暂停只由你决定。
                    </p>
                  </div>
                  <button
                    aria-label="解除全部模型冷却"
                    className="shrink-0 rounded-full bg-white px-2 py-1 text-[10px] font-semibold text-orange-700 hover:bg-orange-100 disabled:opacity-50"
                    disabled={clearModelStateMutation.isPending}
                    onClick={() => {
                      for (const state of editingCredential.model_states ?? []) {
                        if (state.status === "paused") {
                          continue;
                        }
                        clearModelStateMutation.mutate({
                          credentialId: editingCredential.id,
                          modelKey: state.model_key,
                        });
                      }
                    }}
                    type="button"
                  >
                    全部解除
                  </button>
                </div>
                <div className="mt-3 space-y-2">
                  {(editingCredential.model_states ?? []).map((state) => {
                    const cooling =
                      state.cooldown_until &&
                      new Date(state.cooldown_until).getTime() > cooldownNow;
                    const paused = state.status === "paused";
                    return (
                      <div
                        className="flex items-center justify-between gap-2 rounded-lg bg-white px-2 py-1.5"
                        key={state.model_key}
                      >
                        <div className="min-w-0">
                          <p className="truncate text-[12px] font-semibold text-stone-900">
                            {state.model_key}
                          </p>
                          <p className="truncate text-[10px] font-medium text-stone-500">
                            {state.aliases.length > 0
                              ? state.aliases.join("、")
                              : "已移除映射"}
                            {paused
                              ? " · 已暂停"
                              : state.status === "error"
                                ? " · 异常"
                                : cooling
                                  ? ` · 冷却 ${formatCooldownRemaining(
                                      new Date(state.cooldown_until as string).getTime() -
                                        cooldownNow,
                                    )}`
                                  : " · 正常"}
                          </p>
                        </div>
                        <div className="flex shrink-0 items-center gap-1">
                          <button
                            aria-label={`${paused ? "恢复" : "暂停"}模型 ${state.model_key}`}
                            className="rounded-md border border-stone-200 px-2 py-1 text-[10px] font-semibold text-stone-700 hover:bg-stone-50 disabled:opacity-50"
                            disabled={modelStatusMutation.isPending}
                            onClick={() =>
                              modelStatusMutation.mutate({
                                credentialId: editingCredential.id,
                                modelKey: state.model_key,
                                status: paused ? "ok" : "paused",
                              })
                            }
                            type="button"
                          >
                            {paused ? "恢复" : "暂停"}
                          </button>
                          <button
                            aria-label={`解除模型 ${state.model_key}`}
                            className="rounded-md border border-orange-200 px-2 py-1 text-[10px] font-semibold text-orange-700 hover:bg-orange-50 disabled:opacity-50"
                            disabled={clearModelStateMutation.isPending}
                            onClick={() =>
                              clearModelStateMutation.mutate({
                                credentialId: editingCredential.id,
                                modelKey: state.model_key,
                              })
                            }
                            type="button"
                          >
                            解除
                          </button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </section>
```

顶部 import 补 `clearRouteCredentialModelState`、`setRouteCredentialModelStatus`（从 `../lib/api/client`）与 `RouteCredentialModelState`、`RouteCredentialModelStatus`（从 `../lib/api/types`）。具体路径照该文件既有 import 写法。

- [ ] **Step 7: 跑测试确认通过**

```bash
pnpm vitest run tests/AccountsScreen.test.tsx
pnpm typecheck
```

Expected: 8 个新测试 PASS，既有 119 个继续 PASS。

- [ ] **Step 8: 提交**

```bash
git add src/screens/AccountsScreen.tsx tests/AccountsScreen.test.tsx
git commit -m "feat: 界面展示模型冷却明细并支持暂停解除"
```

---

### Task 10: 文档同步与完整验证

**Files:**
- Modify: `docs-site/docs/guide/reliability.md`
- Modify: `docs-site/docs/en/guide/reliability.md`
- Modify: `docs-site/docs/guide/accounts.md:100-103`
- Modify: `docs-site/docs/en/guide/accounts.md`（对应位置）

**Interfaces:**
- Consumes: Task 1-9 的全部实现。
- Produces: 无（终点任务）。

- [ ] **Step 1: 更新 reliability.md**

`docs-site/docs/guide/reliability.md` 是与代码逐字对应的规格文档。四处必改：

1. **`:10-38` 字段表**：加 `route_credential_models` 一节，列出 12 列及含义，说明 `route_credentials` 上的 7 个字段现在表示**账号级**状态。
2. **`:117-124` 维护者警告**：删掉那段 HTML 注释。`semantic_error_threshold` 现在有消费者了（模型级语义连击），警告本身要求「若有人接线了要删掉」。
3. **`:179-206` 退避与冷却规则**：改为两层。贴新的 `is_account_scoped_failure` 分级表，说明模型级失败写模型行、账号级失败写账号行、全部模型不可用时升级。
4. **`:265-288` 选号如何跳过冷却账号**：改为新的五步链路（装载 → 规则 → 模型 → 查状态 → 分桶），说明 `paused`/`error` 硬排除不参与兜底探测，并补上新错误码 `route_pool.model_unavailable` 与 `route_pool.model_unmatched` 的区别。

另外 `:290-305`（成功清空哪些字段）要说明非对称清账：只清本模型 + 账号级，不动兄弟模型。`:307-389`（自动恢复）要说明 Healthcheck 现在探测 `updated_at` 最早的模型、`Scheduled` 不推翻 `paused`。

- [ ] **Step 2: 同步英文镜像**

`docs-site/docs/en/guide/reliability.md` 同位置同步，包括删除同一段维护者警告。UI 文案保留中文原文并附英译，照该文件既有做法（例如 `失败冷却（秒）` ("failure cooldown, seconds")）。新增文案：`模型 N 不可用` ("N models unavailable")、`模型状态` ("model status")、`已暂停` ("paused")。

- [ ] **Step 3: 更新 accounts.md 的调度说明**

`docs-site/docs/guide/accounts.md:100-103` 的调度四步改为五步，第 4 步「剔除冷却账号」拆成「按请求模型解析模型键并查状态」与「剔除账号级或模型级冷却，硬排除已暂停/异常的模型」。英文镜像同步。

- [ ] **Step 4: 跑 CI 的完整序列**

按 `.github/workflows/release.yml:192-219` 的权威顺序：

```bash
pnpm typecheck
pnpm test:run
pnpm release:manifest:test
pnpm build
cd sidecar/ai-switch-tsnet && go test ./... && cd ../..
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo fmt --check && cd ..
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo check && cd ..
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test && cd ..
```

Expected: 全绿。`pnpm rust:check` / `pnpm rust:test` 不带 `target-codex`，AI 执行时用上面的显式写法替代。

- [ ] **Step 5: 文档站构建**

```bash
cd docs-site && pnpm install --frozen-lockfile && pnpm docs:build && cd ..
```

Expected: 构建通过。若 `docs-site` 的依赖未安装且不便安装，跳过此步并在提交信息里说明。

- [ ] **Step 6: 提交**

```bash
git add docs-site/docs/guide/reliability.md docs-site/docs/en/guide/reliability.md docs-site/docs/guide/accounts.md docs-site/docs/en/guide/accounts.md
git commit -m "docs: 同步按模型冷却的行为说明"
```

- [ ] **Step 7: 手工验收**

自动化测试覆盖不到真实中转站的行为，这三项需在真环境跑一遍：

1. **账号级测试连接分模型**：给一个账号配两个模型映射，开启失败冷却。用账号行的「测试账号」分别测两个模型（弹窗里手填模型名）。断言：测失败的那个在编辑抽屉的「模型状态」里出现冷却，另一个仍显示正常；且账号行的「冷却 N 秒」徽章**不**出现（账号级未被写入）。
2. **算力池测试仍生效**：用算力池工具栏的「真实生成测试算力池路由」发一个已冷却模型的请求。该路径走本地代理，断言模型冷却在代理链路里同样生效（结果卡片里的「命中账号」应跳过冷却中的账号，或在只有一个账号时仍打到上游）。
3. **手动暂停生效**：抽屉里暂停一个模型，用真实客户端（Claude Code / Codex CLI）发该模型的请求，断言得到 `route_pool.model_unavailable`；发另一个模型正常。然后点「恢复」，确认该模型立即可用。

---

## Self-Review

**1. 规格覆盖**

| 规格章节 | 实现任务 |
|---|---|
| 1. 数据模型（建表、三态、单时间戳、行生命周期） | Task 1、Task 3 |
| 1.1 模型键（`to` / official 原名 / `[1m]` 归一化） | Task 2 |
| 1.2 已知模型集合 | Task 2 |
| 1.3 `RouteCredential.model_states` | Task 8 |
| 2. 失败分级表 | Task 4 |
| 2. 写入规则、`FailureScope`、升级同事务 | Task 6 |
| 2. 成功非对称清账（含 `:1446`、`:2140` 两个点） | Task 6 |
| 3. 选号拆分、`PoolCandidate`、判定顺序、兜底 | Task 5 |
| 3. 顺序缺陷修复、`route_pool.model_unavailable` | Task 5、Task 6 |
| 3. `/models` 不过滤暂停模型 | Task 5 Step 6（`/models` 路径传空状态表） |
| 4. 语义连击叠加冷却、阈值置 `error`、分母排除 `paused` | Task 3（叠加与阈值）、Task 6（分母） |
| 5. `finish_outcome` 三处改动 | Task 7 |
| 5. Healthcheck 探测模型选取 | Task 7 |
| 5. `needs_recovery` 加条件 | Task 7 |
| 5. `Scheduled` 不推翻 `paused` | Task 7 |
| 5. 两个新 command | Task 8 |
| 6. 下发全集 + `aliases` + 孤儿行 | Task 8 |
| 6. 行内徽章、悬停明细、抽屉区块、倒计时、实时刷新 | Task 9 |
| 7. 端到端 + 分层单测 | Task 5、6、7、8、9 |
| 7. 手工验收清单 | Task 10 Step 7 |
| 8. 文档同步 | Task 10 |

无遗漏。

**2. 占位符扫描**

无 TBD/TODO，无「add appropriate error handling」类空话，每个代码步骤都有可粘贴的代码块。三处需要执行者现场确认的地方均写明了确认方法与替代方案：`load_states` 的行值语法（Task 3 Step 7 给了 `OR` 退化写法）、`parts` 的模型字段名（Task 7 Step 3 指明读 `build_model_test_request` 确认）、测试 mock 的形态（Task 9 Step 1 指明检查是自动 mock 还是手写对象）。

**3. 类型一致性**

- `RouteCredentialModelState` 字段名在 Task 1 定义、Task 3/5/8/9 使用，一致；`aliases` 在 Task 8 追加，Task 9 使用。
- `FailureScope` 在 Task 6 定义于 `route_credential_model.rs`，Task 6/7 使用。
- `model_state_key` / `known_upstream_models` 在 Task 2 定义，Task 5/6/7/8 使用；`aliases_for_model_key` 在 Task 8 定义，`alias_for_model_key`（Task 7）改为它的单数包装 —— 已在 Task 8 Step 4 注明，避免两份筛选逻辑。
- `record_transient_failure` 的两个同名方法分属不同仓储：账号级 6 参（Task 6），模型级 10 参（Task 3），调用点始终带仓储前缀，无歧义。
- `clear_transient_failure` 的 `model_key: Option<&str>` 第三参在 Task 6 引入，Task 7 的模型测试与既有测试调用点均已注明补参。
- `partition_by_cooldown` 三参签名在 Task 5 定义，Task 5/6 使用一致。
- 前端 `RouteCredentialModelStatus` 在 Task 8 定义，Task 9 的 mutation 参数使用。

---

## Execution Handoff

计划已保存至 `docs/superpowers/plans/2026-09-02-per-model-cooldown.md`。






