# SaaS 身份与账务实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans task-by-task. Track steps with checkboxes.

**Goal:** 提供真实 OAuth、用户资源、分组、美元交易、充值审核与兑换。

**Architecture:** SQLx 仓储与纯定价规则分离；HTTP/桌面适配器只负责认证和分派，不能复制账务规则。

**Tech Stack:** Rust、SQLx SQLite、reqwest、chrono、sha2、uuid。

**Spec:** `docs/superpowers/specs/2026-09-07-saas-plugin-design.md`，第 5–8 节。

## Global Constraints

- 继承 `2026-09-07-saas-implementation.md` 全部约束和 operation/camelCase 契约。
- 仅写 `src-tauri/src/saas/config/`、`auth/`、`domain/`、`repository/`、`billing/`、`migrations/`；宿主集成由总计划负责。
- 表含专属迁移表均 `saas_`；使用整数金额，事务处理并发，秘密不进入用户输出。
- 一个 Key 固定分组；不改变既有宿主账号/Key。

## Task 1: 独立迁移与配置

**Files:** `repository/mod.rs`、`migrations/0001_saas.sql`、`config/mod.rs`、`config/tests.rs`。

**Interfaces:** `repository::migrate(pool: &SqlitePool) -> Result<(), AppError>`；`config::load(pool: &SqlitePool) -> Result<SaasConfig, AppError>`；`SaasConfig` camelCase 序列化，公开字段含 enabled、registrationEnabled、siteName、publicBaseUrl、githubClientId、exchangeRateMicros（每美元人民币微单位）、日志配置；秘密单独读取/更新，API 仅显示已配置状态。

- [ ] 先写内存数据库幂等迁移、默认禁用、表前缀、缺失/负数汇率拒绝和秘密脱敏测试。

```rust
assert!(!config::load(&pool).await.unwrap().enabled);
assert!(tables.iter().all(|name| name.starts_with("saas_") || name.starts_with("sqlite_")));
assert!(!serde_json::to_string(&public_config).unwrap().contains("client-secret-value"));
```

- [ ] 使用唯一 Cargo target 跑 `cargo test --lib saas::config`，观察新行为失败。
- [ ] 实现事务/checksum 迁移，schema 与设计表清单一致；配置校验、脱敏、秘密保留规则。
- [ ] 同组测试转绿；提交相关文件。

## Task 2: GitHub 与独立 session

**Files:** `auth/mod.rs`、`auth/tests.rs`。

**Interfaces:** OAuth 起始结果含 authorizeUrl、浏览器 binding Cookie；回调结果含 session token、csrf token、用户安全信息。HTTP adapter 传入真实 code/state/binding；所有数据库时间用 UTC。认证服务依赖显式 GitHub HTTP 客户端，测试注入本地 mock，不把测试绕过开关暴露生产配置。

- [ ] 覆盖 365 天边界、state 重放/跨浏览器、注册关闭已有用户、封禁和 token 摘要的失败测试。

```rust
assert!(!eligible_at(now - chrono::Duration::days(364), now));
assert!(eligible_at(now - chrono::Duration::days(365), now));
assert!(consume_state(&pool, &state, "foreign-browser").await.is_err());
assert!(consume_state(&pool, &state, &binding).await.is_ok());
assert!(consume_state(&pool, &state, &binding).await.is_err());
```

`eligible_at(created: DateTime<Utc>, now: DateTime<Utc>) -> bool`，`consume_state(pool, state, binding) -> Result<OAuthState, AppError>`；OAuthState 是存储的有效期、verifier、回调地址快照。

- [ ] 跑 `cargo test --lib saas::auth`，确认因缺失身份限制失败。
- [ ] PKCE/state、GitHub code 交换、稳定 ID 开户、哈希 session/CSRF、注销/封禁实现。
- [ ] mock GitHub 集成转绿，无可操作真实 OAuth App 时不报告真人登录通过。

## Task 3: 用户、分组、Key 与管理员操作

**Files:** `domain/mod.rs`、`domain/groups.rs`、`domain/keys.rs`、`domain/tests.rs`。

**Interfaces:** `domain::admin(pool, operation: &str, payload: Value) -> Result<Value, AppError>`；`domain::user(pool, user_id: &str, operation: &str, payload: Value) -> Result<Value, AppError>`。覆盖总计划列出的对应操作；config/logs 由宿主适配器分派。分页返回 `{items,total}`，列表数据不能泄漏秘密。

- [ ] 失败测试覆盖默认批次、自定义 ID、空组、账号范围、Key 撤销、跨用户资源与模型定价必填。

```rust
assert!(domain::user(&pool, &second_user, "keys.update", first_user_key).await.is_err());
assert_eq!(permitted_accounts(&pool, &empty_group).await.unwrap().len(), 0);
assert!(!listed_key_json.contains(&plain_key));
```

`permitted_accounts(pool, group_id) -> Result<Vec<String>, AppError>` 返回当前允许且可用的 route credential ID。

- [ ] 定向测试确认失败后实现参数化查询、分页、固定 Key 分组、软删除和实时状态检查。
- [ ] 模型 catalog 来自既有账号配置和批次，不回传账号秘密。
- [ ] 所有 UI 所需操作转绿；无匹配操作返回明确错误，不返回模拟成功。

## Task 4: 财务、用量、充值与兑换

**Files:** `billing/mod.rs`、`billing/pricing.rs`、`billing/tests.rs`，`domain/mod.rs` 分派业务。

**Interfaces:** `BillableUsage { input_tokens, cache_read_tokens, cache_write_tokens, output_tokens: i64 }` 表示互斥 token 分类。`ModelPrice` 三类微美元/百万 token 与倍率定点值。`ApiPrincipal` 包含 user_id、key_id、group_id、platform。`Reservation` 包含 request_id、价格快照与预占金额。

代理消费 `authenticate_key(pool, plaintext_key)`、`reserve(pool, principal, model, input_estimate, output_limit)`、`settle(pool, request_id, usage: Option<BillableUsage>, success: bool)`；结果返回实际微美元、结算状态、用户/Key/分组引用，供日志记录。

- [ ] 写并发兑换/审核、双结算、冻结余额、超支、整数舍入、汇率快照和 missing-usage 测试。

```rust
assert_eq!(successful_redemptions, 1);
assert_eq!(wallet_after - wallet_before, code_credit_micros);
assert_eq!(ledger_rows_for_request, 1);
assert_eq!(price_usd_micros, expected_integer_micros);
assert_eq!(unconfirmed_settlement.status, "pending_review");
```

- [ ] 跑 `cargo test --lib saas::billing` 并观察失败。
- [ ] 用原子条件更新和事务实现预占/结算、Key 累计、小时聚合、订单/兑换、人工核对；不往 SQLite 写 HTTP/token 请求明细。
- [ ] 覆盖 overview、usage、ledger 和充值/兑换所有 operation，测试转绿。
- [ ] 提交本子项目并给出导出 API 与测试证据。
