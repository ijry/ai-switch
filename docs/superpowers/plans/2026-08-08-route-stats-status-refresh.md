# 统计请求展示、分页与账号状态刷新实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**目标：** 修复统计请求列表的 Token/模型展示、分页切换和算力池账号状态实时刷新。

**架构：** 继续使用 `usage_events.metadata_json` 保存请求模型上下文，不新增统计表字段；代理运行时通过现有事件出口发送账号状态变更事件。前端使用 TanStack Query 保留分页旧数据并在状态事件到达后失效相关缓存。

**技术栈：** Rust、SQLite/sqlx、Tauri 2、Axum、React、TypeScript、TanStack Query、Vitest。

## 全局约束

- 统计 Token 合并只作用于请求列表，顶部汇总卡片保持现状。
- Token 总值沿用当前口径：输入 Token + 输出 Token，缓存 Token 单独在悬停明细中显示。
- 模型映射显示格式固定为 `请求模型->上游模型`。
- 旧统计记录缺少模型字段时显示 `-`。
- 不新增统计数据库列，不改变既有统计汇总口径。
- 文档使用中文，直接修改 `main`，不创建分支或 worktree。
- 不提交代码，除非用户明确要求。

---

### 任务 1：补充代理统计模型元数据

**文件：**
- 修改：`src-tauri/src/services/route_proxy_service.rs`
- 修改：`src-tauri/src/services/route_model_test_service.rs`
- 修改：`src-tauri/src/database/repositories/route_pool_repository.rs`

- [ ] **步骤 1：增加失败测试 fixture**

在代理集成测试中构造一个请求模型为 `gpt-5.6-sol`、映射目标为 `sol-upstream` 的账号，发送一次请求并读取对应 `usage_events.metadata_json`。

断言 JSON 包含：

```json
{
  "requested_model": "gpt-5.6-sol",
  "upstream_model": "sol-upstream"
}
```

再使用无映射账号断言两个字段相同。

- [ ] **步骤 2：提取请求模型和上游模型**

在代理请求入口保存 `requested_model`；构建上游请求后从最终 outbound body 提取 `upstream_model`，并把两个字段传给所有请求事件元数据生成路径。构建请求失败时至少保存 `requested_model`。

- [ ] **步骤 3：扩展模型测试事件元数据**

模型测试写入请求统计时，同样写入 `requested_model` 和 `upstream_model`；显式测试无法识别模型时省略字段，不写入伪造值。

- [ ] **步骤 4：保持统计类型兼容**

保持 `RoutePoolUsageLog` 和 `RoutePoolStats` 的 Rust/TypeScript 类型不变，模型展示统一从已有 `metadata_json` 解析。旧 JSON、旧数据库记录和非 JSON 请求均按缺失模型处理并显示 `-`。

- [ ] **步骤 5：运行定向 Rust 测试**

运行：

```text
pnpm rust:test -- route_proxy
pnpm rust:test -- route_pool
```

预期：模型元数据和既有统计测试通过。

---

### 任务 2：优化请求列表 Token 和模型列

**文件：**
- 修改：`src/screens/AccountsScreen.tsx`
- 修改：`tests/AccountsScreen.test.tsx`

- [ ] **步骤 1：添加前端失败断言**

扩展统计 fixture 的 `metadata_json`，加入请求模型和上游模型；断言请求行显示 `gpt-5.6-sol->sol-upstream`，无映射时显示单模型，缺少字段时显示 `-`。

同时断言请求行只显示一个 Token 主值，并通过 hover/无障碍内容查看输入、输出、缓存三项明细。

- [ ] **步骤 2：增加统计元数据模型解析**

扩展 `ParsedUsageMetadata`，解析 `requested_model` 和 `upstream_model`，增加统一的模型显示值和 Token 明细辅助函数。

- [ ] **步骤 3：合并请求行 Token 列**

删除请求列表行中的输入、输出、缓存三个独立列，增加“Token”列。主值按输入+输出显示，悬停内容显示：

```text
输入：120
输出：30
缓存：80
```

保留请求详情中的三项 Token 明细。

- [ ] **步骤 4：增加请求模型列**

在请求列表中增加“模型”列，使用 `requested_model`/`upstream_model` 规则显示 `A->B`；为长模型名添加截断和完整 `title`。

- [ ] **步骤 5：运行前端定向测试**

运行：

```text
pnpm test:run -- tests/AccountsScreen.test.tsx
pnpm typecheck
```

预期：统计展示和既有账号页面测试通过。

---

### 任务 3：修复统计分页切换

**文件：**
- 修改：`src/screens/AccountsScreen.tsx`
- 修改：`tests/AccountsScreen.test.tsx`

- [ ] **步骤 1：补充真实页面内容断言**

让统计 mock 根据 `requestPage` 返回不同的请求 ID和模型，并在点击下一页后断言：

```text
请求 2/3
request-page-2
```

同时断言上一页的请求行不再显示。

- [ ] **步骤 2：保留切页期间的上一页数据**

为 `routePoolQuery` 增加 `placeholderData: keepPreviousData`，避免页码 key 切换时统计总数变成零、页数变成一并禁用下一页按钮。

- [ ] **步骤 3：处理统计查询失败状态**

切页请求失败时保留上一页内容，在现有统计反馈区域显示错误；成功后自动清理错误并展示新页数据。统计周期变化继续将 `requestPage` 重置为 `1`。

- [ ] **步骤 4：运行分页定向测试**

运行：

```text
pnpm test:run -- tests/AccountsScreen.test.tsx -t "paginat"
```

预期：下一页请求参数、页码和内容全部切换。

---

### 任务 4：增加账号状态变更事件并刷新界面

**文件：**
- 修改：`src-tauri/src/services/route_credential_activity.rs`
- 修改：`src-tauri/src/services/route_proxy_service.rs`
- 修改：`src-tauri/src/services/route_model_test_service.rs`
- 修改：`src/screens/AccountsScreen.tsx`
- 修改：`tests/AccountsScreen.test.tsx`
- 修改：`tests/transport/transport.test.ts`

- [ ] **步骤 1：增加状态事件契约测试**

定义事件名 `route-credential-status` 和载荷：

```rust
pub struct RouteCredentialStatusEvent {
    pub platform: String,
    pub credential_id: String,
}
```

测试事件发送器在状态变化通知时收到正确平台和账号 ID。

- [ ] **步骤 2：接入代理状态变化通知**

在代理的瞬时失败、清除冷却、语义失败、撤销和成功恢复路径中，数据库写入成功后调用状态通知。通知失败不影响原请求；数据库写入失败不发送通知。

- [ ] **步骤 3：接入模型测试状态变化通知**

让 `finish_outcome` 使用同一活动/事件出口，在模型测试导致账号异常、撤销或恢复后发送状态事件。

- [ ] **步骤 4：前端订阅并失效缓存**

在 `AccountsScreen` 订阅 `route-credential-status`，过滤当前平台后失效：

```text
["route-credential-page", activePlatform]
["route-credentials-all", activePlatform]
["route-pool", activePlatform]
```

保留当前账号页、统计周期和请求页状态，不直接拼接不完整的账号对象。

- [ ] **步骤 5：增加界面刷新测试**

模拟状态事件，断言 `invalidateQueries` 收到账号分页、全量账号和算力池三个 Query key；模拟活动事件仍只更新并发计数，不重复触发状态刷新。

- [ ] **步骤 6：运行事件定向测试**

运行：

```text
pnpm test:run -- tests/AccountsScreen.test.tsx tests/transport/transport.test.ts
pnpm rust:test -- activity
pnpm rust:test -- route_proxy
pnpm rust:test -- route_model_test
```

预期：状态事件和原有活动事件测试全部通过。

---

### 任务 5：全量验证和文档同步

**文件：**
- 修改：`docs/superpowers/specs/2026-08-08-route-stats-status-refresh-design.md`
- 修改：`docs/superpowers/plans/2026-08-08-route-stats-status-refresh.md`

- [ ] **步骤 1：运行全量前端验证**

运行：

```text
pnpm test:run
pnpm typecheck
pnpm build
```

- [ ] **步骤 2：运行全量 Rust 验证**

运行：

```text
pnpm rust:check
CARGO_TARGET_DIR=%TEMP%\ai-switch-cargo-target pnpm rust:test
```

- [ ] **步骤 3：检查差异**

运行：

```text
git diff --check
git status --short
```

确认没有新的测试生成目录，且改动只包含本任务相关文件。
