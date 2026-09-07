# SaaS 完整功能实施计划

> 分组部分暂停执行：用户已改为核心动态分组、普通路由单组激活、SaaS 使用所有非内部组。以 `../specs/2026-09-07-shared-agent-groups-design.md` 为准；本文原有独立分组、批次授权和 `groups.delete` 契约已失效，待修订设计审阅后更新实施步骤。其余已实现文件保留，不回滚。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 完整交付独立 SaaS 模块、用户站点、管理员控制、真实计费与可切换日志驱动。

**Architecture:** Rust 模块复用 SQLite、既有代理和管理员命令，用户 API 独立鉴权。React 在入口层分流，内部管理页保留现有交互。身份/账务、日志、前端分开实施，宿主接线统一集成。

**Tech Stack:** React 18、TypeScript、Vitest、Rust、Axum 0.7、SQLx 0.8、Tokio；日志可选 Redis/PostgreSQL。

**Spec:** `docs/superpowers/specs/2026-09-07-saas-plugin-design.md`

## Global Constraints

- 当前分支 `task/164`；不切换、不合并、不变基、不推送；提交只包含本任务文件。
- AI Cargo 目录仅 `src-tauri/target-codex/`；命令从 `src-tauri` 执行并设置 `$env:CARGO_TARGET_DIR='target-codex'`。
- 默认关闭；桌面始终管理入口；Web `/ai-switch-admin` 固定，关闭时 `/` 临时跳转，开启时用户站点。
- 仅 GitHub，账号至少 365 天；管理员主 token、用户 session、用户 API Key 三者权限隔离。
- 一个 Key 固定分组；Codex/Claude、默认与自定义批次、模型白名单、三类价格和倍率。
- 微美元账务、人民币整数分充值、固定汇率快照；不做在线支付。
- SaaS 表与迁移表均 `saas_` 前缀；请求明细不能写 SQLite 或旧 `usage_events`。
- 日志默认 `~/ai-switch/logs/yyyy-mm-dd/hh.log`，队列内存/Redis，存储文件/PostgreSQL。
- 内存崩溃丢失窗口明确；账务不能依赖日志队列。
- 不使用无业务实现的占位页面或驱动枚举作为交付。

## 文件边界与执行顺序

| 子计划 | 主要文件责任 | 依赖 |
| --- | --- | --- |
| 身份/业务 | `src-tauri/src/saas/{config,auth,domain,repository,billing,migrations}/` | 宿主 DB/路径 |
| 日志 | `src-tauri/src/saas/logs/` | 独立 DTO，Tokio/SQLx |
| 前端 | `src/saas/`、`tests/saas/` | 下列 HTTP/命令契约 |
| 宿主集成 | `saas/mod.rs`、`saas/transport/`、`saas/proxy/`、既有 App/路由/代理 | 前三项 |

分计划：`2026-09-07-saas-business.md`、`2026-09-07-saas-logs.md`、`2026-09-07-saas-ui.md`。本计划负责宿主集成和总体验收。

## 公共传输契约

管理员沿用 `getTransport().call('saas_admin', { operation, payload })`，只让主 token 或桌面调用。

用户通过 `/api/saas/user/:operation` JSON POST 调用；用户 ID 从 HttpOnly session 推导，写请求必须带 session 对应 CSRF token。用户客户端不使用宿主 WebTransport。

```ts
type SaasEnvelope = { code: string; message: string; details?: unknown };
type AdminOperation = 'config.get' | 'config.save' | 'overview' | 'catalog'
  | 'users.list' | 'users.status' | 'groups.list' | 'groups.save' | 'groups.delete'
  | 'recharges.list' | 'recharges.review' | 'codes.list' | 'codes.create'
  | 'codes.disable' | 'ledger.list' | 'ledger.reconcile' | 'logs.query';
type UserOperation = 'overview' | 'usage' | 'groups' | 'keys.list'
  | 'keys.create' | 'keys.update' | 'keys.rotate' | 'recharges.list'
  | 'recharges.create' | 'recharges.cancel' | 'redeem' | 'logs.query';
```

另设 GET `/api/saas/public/config`、`/api/saas/auth/github`、`/api/saas/auth/github/callback`、`/api/saas/auth/session`，POST `/api/saas/auth/logout`。JSON 字段统一 camelCase，错误统一稳定 code/message，不含 SQL/秘密。

## Task 1: 入口分流与插件宿主

**Files:** 新增 `saas/mod.rs`、`saas/transport/mod.rs`；修改 `app_state.rs`、`server.rs`、`desktop.rs`、`lib.rs`、`database/mod.rs`、`web/router.rs`、`web/handlers/mod.rs`、`src/App.tsx`、`vite.config.ts`。

**Interfaces:** 服务通过 AppState 获取 pool/paths；共享的 `SaasRuntime` 负责后台任务和启停。管理员 `saas_admin` operation/payload 调用同一个业务服务。用户 API 仅从经验证 session 获取 userId。

- [ ] 写现有 router fixture 的失败测试：默认 `/` 302 且 Location 正确、admin 返回 HTML，公开配置不含 token；现有未配置 SaaS 数据库也默认关闭。

```rust
assert_eq!(root.status(), StatusCode::FOUND);
assert_eq!(root.headers()[header::LOCATION], "/ai-switch-admin");
assert_eq!(root.headers()[header::CACHE_CONTROL], "no-store");
```

- [ ] `cargo test --lib web::router::tests` 观察新断言失败。
- [ ] 挂接 SaaS 迁移/运行时，路由显式先于静态 fallback，原管理 App 包装为独立入口。
- [ ] 定向测试转绿，验证旧管理员/移动配对权限不变。
- [ ] 只提交此任务相关文件。

## Task 2: 受限代理和统一完成回调

**Files:** 修改 `services/route_proxy_service.rs`；新增 `saas/proxy/mod.rs`、`saas/proxy/tests.rs`。

**Interfaces:** 从真实 SaaS Key 得到用户/分组；请求上下文携带允许账号集合、模型/价格快照、预占 ID。上下文作为代理状态可选字段，只对当前请求 clone，不污染宿主共享状态。普通/SSE 完成都走幂等结算，失败释放或待核对。

- [ ] mock 上游测试写出同批次允许、另一批次不得访问、平台请求头不能绕过、不会新增 SQLite usage 行的断言。

```rust
assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
assert_eq!(foreign_group_upstream_hits.load(Ordering::SeqCst), 0);
assert_eq!(legacy_usage_after, legacy_usage_before);
assert_eq!(ledger_count_for_request, 1);
```

- [ ] `cargo test --lib saas::proxy` 观察未接线的行为失败。
- [ ] 在候选池选择之前过滤；模型路径白名单；禁用宿主逐请求事件和原文 live log，但保留账号健康。
- [ ] 普通与 SSE 完成得到原始规范化 usage，结算一次并投递脱敏日志；客户端断连执行有界清理。
- [ ] 验证成功、重试、失败、断流、未知 usage 和并发；只提交代理相关文件。

## Task 3: 全部接线、回归与用户文档

**Files:** `README.md`、`README-EN.md`、新模块测试、配置/设置/系统菜单挂载点。

- [ ] 检查每个 operation 均有真实后端和页面调用；对不存在的操作明确 404。
- [ ] 端到端检查：启用→OAuth→兑换→Key→普通/SSE 请求→扣费→日志查询→人工充值审核→停用。
- [ ] 在配置页说明 GitHub 回调、365 天门槛、日志路径、驱动切换历史、默认内存丢失边界。
- [ ] 执行 `pnpm typecheck`、`pnpm test:run`、`pnpm build`。
- [ ] 从 `src-tauri` 使用唯一 target 执行 `cargo check`、`cargo test --lib`、`cargo check --no-default-features --features standalone-server --bin ai-switch-server`、`cargo fmt --check`。
- [ ] 用真实 Redis/PostgreSQL 服务验证；无服务时如实标记未验证，不宣称真实连接通过。
- [ ] `git diff --check`；只提交相关文件，不推送。交付报告区分已完成功能、测试证据和剩余问题。
