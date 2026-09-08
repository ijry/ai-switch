# 共享智能体动态分组实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将智能体账号三视图升级为动态分组，并让 SaaS 只读取核心分组与扩展配置。

**Architecture:** SQLite 继续作为核心分组真相，`route_pool_groups` 与既有 `route_pool_members` 保存组、成员和激活状态；普通代理从平台激活组取账号，SaaS 直接按 Key 绑定组取账号。React 底栏按核心组渲染并调用新命令；SaaS 管理页删除组 CRUD，只保存 `saas_` 扩展。

**Tech Stack:** Rust、SQLx SQLite、Axum/Tauri 命令、React 18、TypeScript、Vitest。

**Spec:** `docs/superpowers/specs/2026-09-07-shared-agent-groups-design.md`。

## Global Constraints

- 分支保持 `task/164`；不合并、不变基、不推送。
- Rust 验证仅使用 `src-tauri/target-codex/`。
- 核心表不使用 `saas_` 前缀；SaaS 扩展表必须使用 `saas_` 前缀。
- 账号在同平台只属于一个分组；每个平台普通路由只有一个激活组。
- 旧账号归档状态不得因迁移、改名、激活或移动分组而自动清除。
- SaaS 使用所有非内部组，不受普通路由激活状态限制；内部组拒绝新 SaaS 请求。
- 每个行为先写失败测试，再实现；不得提交未验证的大范围改动。

## Task 1: 核心分组模型与迁移

**Files:**

- Create: `src-tauri/migrations/202609080001_route_pool_groups.sql`
- Modify: `src-tauri/src/models/route_pool.rs`
- Modify: `src-tauri/src/database/repositories/route_pool_repository.rs`
- Modify: `src-tauri/src/database/mod.rs`

**Interfaces:**

- `RoutePoolGroup { id, platform, name, sort_order, is_internal, is_active, account_count, created_at, updated_at }`
- `RoutePoolRepository::list_groups(pool, platform, include_deleted) -> Result<Vec<RoutePoolGroup>, AppError>`
- `RoutePoolRepository::active_group_id(pool, platform) -> Result<Option<String>, AppError>`
- `RoutePoolRepository::group_id_for_account(pool, platform, credential_id) -> Result<Option<String>, AppError>`
- 迁移为 7 个支持平台各创建默认组、未入池、已归档，并一次性迁移既有账号。

- [ ] 写仓储失败测试：三类旧账号进入正确组、默认组唯一激活、归档时间不变、重复迁移不重复建组。
- [ ] 新增迁移和模型，按测试实现分组查询。
- [ ] 将 `route_pool_groups`、`route_pool_members` 纳入用户数据表清单。
- [ ] 运行定向 Rust 测试并提交。

## Task 2: 分组操作服务与命令

**Files:**

- Modify: `src-tauri/src/services/route_pool_service.rs`
- Modify: `src-tauri/src/commands/route_pool_commands.rs`
- Modify: `src-tauri/src/desktop.rs`
- Modify: `src-tauri/src/web/handlers/mod.rs`
- Modify: `src/lib/api/types.ts`
- Modify: `src/lib/api/client.ts`

**Interfaces:**

- `RoutePoolService::get(pool, platform, group_id, since, page, page_size)`
- `CreateRoutePoolGroupInput { platform, name, is_internal }`
- `UpdateRoutePoolGroupInput { id, platform, name, is_internal, activate, sort_order }`
- `DeleteRoutePoolGroupInput { id, platform }`
- `SetRoutePoolGroupMembersInput { platform, group_id, account_ids }`
- 命令：`create_route_pool_group`、`update_route_pool_group`、`delete_route_pool_group`、`set_route_pool_group_members`；旧 `set_route_pool_members` 兼容为写当前激活组。

- [ ] 写服务测试：创建不抢占激活、激活原子替换、空组排序、非空/激活组删除拒绝、跨平台账号拒绝、移动后旧组无成员。
- [ ] 实现服务、命令和 API 客户端。
- [ ] 桌面与 Web 命令分派转绿。
- [ ] 提交本任务。

## Task 3: 智能体账号页动态分组

**Files:**

- Modify: `src/screens/AccountsScreen.tsx`
- Modify: `tests/AccountsScreen.test.tsx`
- Modify: `src/lib/api/types.ts`

**Interfaces:**

账号分页请求携带 `group_id`；底栏从 `RoutePoolState.groups` 渲染，统计入口右侧是 `+`；分组按钮右键和菜单均可编辑。

- [ ] 写前端失败测试：动态组渲染、`+` 打开创建、右键打开编辑、激活标识与选中标识区分、切组清空选择。
- [ ] 移除硬编码三视图状态，改为当前组 ID。
- [ ] “加入/移出算力池”改成“移动到分组”，保留批量名筛选。
- [ ] 创建/导入账号落入当前组；旧无界面入口落入激活组。
- [ ] 运行相关 Vitest 并提交。

## Task 4: 代理路由与账号查询接入

**Files:**

- Modify: `src-tauri/src/database/repositories/route_pool_repository.rs`
- Modify: `src-tauri/src/database/repositories/route_credential_repository.rs`
- Modify: `src-tauri/src/services/route_pool_service.rs`
- Modify: `src-tauri/src/services/route_proxy_service.rs`
- Modify: `src-tauri/src/services/route_model_test_service.rs`

**Interfaces:**

- `RoutePoolRepository::member_accounts(pool, platform)` 只返回激活组账号。
- `RoutePoolRepository::member_accounts_for_group(pool, group_id)` 返回指定组账号，供 SaaS 和组内测试使用。
- 代理候选选择在组过滤后继续沿用健康、优先级、并发、重试和冷却逻辑。

- [ ] 写代理失败测试：切换激活组后不再选旧组；空激活组不回退默认/其他组；SaaS 作用域可访问非激活非内部组。
- [ ] 更新所有 `route_pool_members` 查询为组语义，移除对 `enabled` 的授权依赖。
- [ ] 运行代理与仓储定向测试并提交。

## Task 5: SaaS 扩展改造

**Files:**

- Modify: `src-tauri/src/saas/migrations/0001_saas.sql`
- Modify: `src-tauri/src/saas/domain/groups.rs`
- Modify: `src-tauri/src/saas/domain/keys.rs`
- Modify: `src-tauri/src/saas/billing/*`
- Modify: `src/saas/admin/Groups.tsx`
- Modify: `src/saas/types.ts`
- Modify: `tests/saas/admin.test.tsx`

**Interfaces:**

- 删除 `saas_groups`、`saas_group_batches` 设计，改为 `saas_group_settings` 一对一引用核心组。
- `groups.list` 读取核心组并合并扩展；`groups.save` 仅保存倍率、请求限制、模型映射/价格；不再创建、改名、删除组或选择批次成员。

- [ ] 写 SaaS 失败测试：非激活非内部组可用；内部组拒绝；核心组不存在不能伪造；缺模型/价格未就绪；历史 Key 不因改名失效。
- [ ] 调整迁移、域服务、Key 授权和账务外键。
- [ ] 前端删除分组 CRUD 和批次成员编辑，显示核心组只读信息。
- [ ] 运行 SaaS 后端与前端定向测试并提交。

## Task 6: 入口、日志与总体验收

**Files:**

- Modify: `src/App.tsx`
- Modify: `vite.config.ts`
- Modify: `src-tauri/src/saas/mod.rs`
- Modify: `src-tauri/src/saas/transport/mod.rs`
- Modify: `README.md`、`README-EN.md`
- Test: 相关 Rust/Vitest 与集成测试。

- [ ] 完成根入口、SaaS 菜单、设置页与用户站挂载。
- [ ] 代理完成回调统一预占、结算、日志投递；确认不写 SQLite 明细。
- [ ] 端到端验证启用、登录、兑换、Key、普通/SSE 请求、扣费、日志、充值审核和停用。
- [ ] 运行 `pnpm typecheck`、`pnpm test:run`、`pnpm build`。
- [ ] 从 `src-tauri` 运行 `cargo fmt --check`、`cargo check`、`cargo test --lib`。
- [ ] `git diff --check` 后提交；如实报告未验证的真实 Redis/PostgreSQL/Git项。
