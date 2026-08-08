# 算力池账号优先级与并发控制实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**目标：** 为算力池账号增加 `1-5` 路由优先级、进程内并发上限、暂停状态、批量状态设置和实时请求活动动效。

**架构：** 将路由优先级和最大并发数持久化到 `route_credentials`，通过账号服务和 Tauri/Web command 暴露；新增进程内 `RouteCredentialActivityRegistry`，用租约方式原子占用并发额度，路由选择按优先级分组后在同级内轮询。活动租约获取/释放通过现有 Tauri/Web 事件桥推送到前端，账号列表初始数据同时补充当前活动数。

**技术栈：** Rust、SQLite/sqlx、Tokio、Tauri 2、Axum Web command、React、TypeScript、TanStack Query、Vitest。

## 全局约束

- 直接修改 `main`，不创建分支或 worktree。
- 文档使用中文，版本保持 `0.4.0`。
- `route_priority` 只允许 `1-5`，数字越小优先级越高，默认值为 `3`。
- `max_concurrency` 只允许大于等于 `1` 的整数，默认值为 `1`。
- `paused` 保留算力池成员关系，但所有自动路由选择都跳过。
- 活动并发数只保存在当前 AI Switch 进程内，不写入 SQLite。
- 请求失败重试、响应转换失败、传输失败、读取失败和取消都必须释放活动租约。
- 所有状态批量更新必须在一个数据库事务中完成，任一账号不存在或状态非法时整体回滚。
- 实现和验证已完成，功能提交等待用户明确确认。

---

### 任务 1：增加账号路由配置字段和状态契约

**文件：**
- 创建：`src-tauri/migrations/202608080002_route_credential_priority_concurrency.sql`
- 修改：`src-tauri/src/models/route_credential.rs`
- 修改：`src-tauri/src/database/repositories/route_credential_repository.rs`
- 修改：`src-tauri/src/services/route_credential_service.rs`
- 修改：`src/lib/api/types.ts`

**接口：**
- `RouteCredential` 新增 `route_priority: i64`、`max_concurrency: i64`、`active_request_count: i64`。
- `UpdateRouteCredentialInput` 新增 `route_priority: number`、`max_concurrency: number`。
- `AccountStatus` 从 `"ok" | "warning" | "error" | "revoked"` 扩展为包含 `"paused"`。
- `RouteCredentialService::validate_route_priority(value: i64)` 返回 `Result<i64, AppError>`。
- `RouteCredentialService::validate_max_concurrency(value: i64)` 返回 `Result<i64, AppError>`。

- [x] **步骤 1：编写数据库迁移**

创建以下迁移，保持旧账号自动获得兼容默认值：

```sql
PRAGMA foreign_keys = ON;

ALTER TABLE route_credentials
  ADD COLUMN route_priority INTEGER NOT NULL DEFAULT 3
  CHECK (route_priority BETWEEN 1 AND 5);

ALTER TABLE route_credentials
  ADD COLUMN max_concurrency INTEGER NOT NULL DEFAULT 1
  CHECK (max_concurrency >= 1);

CREATE INDEX IF NOT EXISTS idx_route_credentials_routing_priority
  ON route_credentials(platform, route_priority, status, next_retry_at, cooldown_until);
```

- [x] **步骤 2：扩展 Rust 和 TypeScript 模型**

在 Rust 账号模型中将持久化字段放在 `sort_order` 后面，将活动字段放在失败/重试字段附近，并为动态字段增加 `#[sqlx(default)]`：

```rust
pub route_priority: i64,
pub max_concurrency: i64,
#[sqlx(default)]
pub active_request_count: i64,
```

在 TypeScript 中同步增加：

```ts
route_priority: number;
max_concurrency: number;
active_request_count?: number;
```

同时把 `"paused"` 加入 `AccountStatus`。

- [x] **步骤 3：更新账号查询和写入字段**

在 `route_credential_repository.rs` 的 `PAGE_SELECT`、`get`、`list_by_ids`、`list_by_platform`、`page` 所有查询中追加 `rc.route_priority` 和 `rc.max_concurrency`，并确保列顺序与 `RouteCredential` 的 `FromRow` 顺序一致。

更新账号写入和编辑 SQL：

```sql
UPDATE route_credentials
SET display_name = ?, email = ?, status = ?, route_priority = ?,
    max_concurrency = ?, secret_payload_json = ?, config_json = ?,
    preview_json = ?, updated_at = ?
WHERE id = ?
```

创建、导入和复制继续省略这两个字段，依赖迁移默认值 `3/1`；如果现有 `create` SQL 显式列出字段，则显式绑定 `3` 和 `1`。

- [x] **步骤 4：增加服务端输入校验**

在 `route_credential_service.rs` 增加以下校验函数，并在 `update` 前调用：

```rust
fn validate_route_priority(value: i64) -> Result<i64, AppError> {
    if (1..=5).contains(&value) {
        Ok(value)
    } else {
        Err(AppError::Validation {
            code: "validation.route_credential_priority",
            message: "Route priority must be between 1 and 5".to_string(),
            details: Some(value.to_string()),
            recoverable: true,
        })
    }
}

fn validate_max_concurrency(value: i64) -> Result<i64, AppError> {
    if value >= 1 {
        Ok(value)
    } else {
        Err(AppError::Validation {
            code: "validation.route_credential_concurrency",
            message: "Max concurrency must be at least 1".to_string(),
            details: Some(value.to_string()),
            recoverable: true,
        })
    }
}
```

增加单元测试覆盖 `1`、`5`、`0`、`6` 和负数，并确认创建/复制账号仍返回 `route_priority = 3`、`max_concurrency = 1`。

- [x] **步骤 5：运行账号字段定向测试**

运行：

```text
pnpm rust:test -- route_credential
```

预期：迁移、账号 repository 和 service 测试通过；如果命令无法按测试名过滤，则运行 `pnpm rust:test` 并确认失败范围只涉及本任务字段。

- [ ] **步骤 6：提交数据契约改动**

```text
git add src-tauri/migrations/202608080002_route_credential_priority_concurrency.sql src-tauri/src/models/route_credential.rs src-tauri/src/database/repositories/route_credential_repository.rs src-tauri/src/services/route_credential_service.rs src/lib/api/types.ts
git commit -m "feat: add route account priority and concurrency fields"
```

---

### 任务 2：实现批量账号状态更新

**文件：**
- 修改：`src-tauri/src/database/repositories/route_credential_repository.rs`
- 修改：`src-tauri/src/services/route_credential_service.rs`
- 修改：`src-tauri/src/commands/route_credential_commands.rs`
- 修改：`src-tauri/src/lib.rs`
- 修改：`src-tauri/src/web/handlers/mod.rs`
- 修改：`src/lib/api/client.ts`
- 修改：`tests/transport/command-contract.test.ts`

**接口：**
- Rust command：`set_route_credential_statuses(state, ids: Vec<String>, status: String) -> Result<(), ApiError>`。
- 前端 client：`setRouteCredentialStatuses(ids: string[], status: AccountStatus): Promise<void>`。
- Repository：`RouteCredentialRepository::set_statuses(pool, ids, status) -> Result<(), AppError>`。

- [x] **步骤 1：先添加批量状态 repository 测试**

在 `route_credential_repository.rs` 测试中创建三个账号，调用：

```rust
RouteCredentialRepository::set_statuses(
    &pool,
    &vec![first.id.clone(), second.id.clone()],
    "paused",
)
.await
```

断言两个账号变为 `paused`，第三个保持原状态；再传入空 ID 列表和不存在 ID，分别断言 `validation.route_credential_selection_empty` 和 `validation.route_credential_not_found`。

- [x] **步骤 2：实现事务化状态更新**

去重并清理 ID 后，开启 SQLite transaction，先执行：

```sql
SELECT COUNT(*) FROM route_credentials WHERE id IN (...)
```

只有计数等于去重后的 ID 数量时才执行：

```sql
UPDATE route_credentials
SET status = ?, updated_at = ?
WHERE id IN (...)
```

状态只允许 `ok`、`warning`、`error`、`revoked`、`paused`；非法状态返回 `validation.route_credential_status`。任何校验或 SQL 错误都回滚。

- [x] **步骤 3：接入 Tauri 和 Web command**

在 `route_credential_commands.rs` 增加 `#[tauri::command] set_route_credential_statuses`，在 `src-tauri/src/lib.rs` 的 `generate_handler!` 中注册。

在 Web dispatch 的账号 command 分支中加入同名分支，解析 `ids` 和 `status` 字段并调用同一个 service，保证桌面端与 Web 端行为一致。

- [x] **步骤 4：补充 TypeScript client 和 command 契约**

在 `src/lib/api/client.ts` 增加：

```ts
export function setRouteCredentialStatuses(
  ids: string[],
  status: AccountStatus,
): Promise<void> {
  return invoke("set_route_credential_statuses", { ids, status });
}
```

在 `tests/transport/command-contract.test.ts` 断言 Tauri command 已注册、Web handler 已分派、client 使用 `{ ids, status }` 参数。

- [x] **步骤 5：运行批量状态定向测试**

运行：

```text
pnpm rust:test -- set_statuses
pnpm test:run -- tests/transport/command-contract.test.ts
```

预期：事务更新、非法状态、空选择、不存在账号和双 transport 契约全部通过。

---

### 任务 3：实现进程内活动注册表和事件载荷

**文件：**
- 创建：`src-tauri/src/services/route_credential_activity.rs`
- 修改：`src-tauri/src/services/mod.rs`
- 修改：`src-tauri/src/services/route_proxy_service.rs`
- 修改：`src-tauri/src/app_state.rs`
- 修改：`src-tauri/src/server.rs`
- 修改：`src-tauri/src/lib.rs`

**接口：**
- `RouteCredentialActivityRegistry::try_acquire(platform, credential_id, max_concurrency) -> Option<RouteCredentialActivityLease>`。
- `RouteCredentialActivityRegistry::snapshot(credential_id) -> i64`。
- `RouteCredentialActivityRegistry::set_emitter(emitter: EventEmitter)`。
- `RouteCredentialActivityLease` 在 `Drop` 时释放账号活动数。
- 事件名固定为 `route-credential-activity`。
- 事件载荷结构：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialActivityEvent {
    pub platform: String,
    pub credential_id: String,
    pub active_request_count: i64,
    pub max_concurrency: i64,
}
```

- [x] **步骤 1：先写注册表单元测试**

测试以下行为：

```rust
let registry = RouteCredentialActivityRegistry::default();
let first = registry.try_acquire("codex", "credential-a", 1).await;
assert!(first.is_some());
assert!(registry.try_acquire("codex", "credential-a", 1).await.is_none());
drop(first);
assert!(registry.try_acquire("codex", "credential-a", 1).await.is_some());
```

另外覆盖同账号并发上限 `2`、不同账号互不影响、租约释放后活动数归零、动态下调上限时不允许新请求超过新上限。

- [x] **步骤 2：实现带事件通知的租约**

使用 `Arc<tokio::sync::Mutex<HashMap<String, ActivityState>>>` 保存账号活动数。获取租约时在锁内完成检查和递增，释放时在锁内递减并删除归零条目。

事件发送必须发生在活动数变更后，获取事件携带递增后的值，释放事件携带递减后的值；没有设置 emitter 的单元测试环境只更新计数不发送事件。

- [x] **步骤 3：把注册表挂到 RouteProxyRuntimeState**

为 `RouteProxyRuntimeState` 增加可 clone 的 registry 访问方法：

```rust
pub fn activity(&self) -> RouteCredentialActivityRegistry;
```

`ProxyAppState` 保存 registry clone，使运行中的 Axum handler 与 Tauri/Web command 使用同一活动状态。

- [x] **步骤 4：配置桌面端和独立 Web 端事件出口**

在 Tauri `setup` 中，在自动启动代理前执行：

```rust
state.route_proxy
    .activity()
    .set_emitter(EventEmitter::Tauri(app.handle().clone()));
```

在 `server::run_from_env` 创建 `AppState` 后、构建 router 前执行：

```rust
state.route_proxy
    .activity()
    .set_emitter(EventEmitter::Web(Arc::clone(&state.event_broadcaster)));
```

更新相关测试状态构造，使测试不需要 emitter 也能运行。

- [x] **步骤 5：运行注册表测试**

运行：

```text
pnpm rust:test -- activity
pnpm rust:check
```

预期：活动计数、租约释放和事件结构测试通过，Rust 编译检查通过。

---

### 任务 4：接入优先级路由和并发租约

**文件：**
- 修改：`src-tauri/src/services/route_proxy_service.rs`
- 修改：`src-tauri/src/services/route_model_test_service.rs`
- 修改：`src-tauri/src/services/route_pool_service.rs`
- 修改：`src-tauri/src/commands/route_pool_commands.rs`
- 修改：`src-tauri/src/web/handlers/mod.rs`
- 修改：`src-tauri/src/database/repositories/route_pool_repository.rs`
- 修改：`src-tauri/src/models/route_pool.rs`

**接口：**
- `SelectedCredential` 增加 `route_priority: i64`、`max_concurrency: i64`。
- `select_pool_credentials` 返回账号配置并按 `route_priority ASC, rpm.sort_order ASC, rpm.created_at ASC` 排序。
- 新增可测试的优先级排序函数：

```rust
fn credential_indexes_by_priority(
    credentials: &[SelectedCredential],
    cursor: i64,
) -> Vec<usize>
```

该函数只按优先级从小到大生成同级内轮询顺序，不获取租约；请求循环按该顺序逐个调用 `try_acquire`，确保一次入站请求最多持有一个账号租约。

- [x] **步骤 1：先添加优先级和并发选择失败测试**

在 `route_proxy_service.rs` 增加测试 fixture，构造优先级 `1`、`2`、`3` 的账号，覆盖：

1. 优先级 `1` 健康账号始终先于优先级 `2`。
2. 优先级 `1` 全部暂停/冷却/异常时选择优先级 `2`。
3. 优先级 `1` 并发已满时选择优先级 `2`。
4. 优先级 `1` 同级两个账号仍按游标轮询。
5. 所有账号并发已满时返回 `route_pool.concurrency_exhausted`。

- [x] **步骤 2：扩展池成员查询**

将 `RoutePoolRepository::member_accounts` 的查询从 `id/display_name` 扩展为 `id/display_name/route_priority/max_concurrency/status`，并让 `RoutePoolMemberAccount` 增加对应字段；统计查询的 `member_count` 保持统计所有未归档池成员，不因当前状态改变成员数量。

- [x] **步骤 3：实现账号候选顺序**

在 `select_pool_credentials` 中：

- 排除 `status != 'ok'`，因此 `paused` 自动被排除。
- 继续排除未到期的 `next_retry_at` 和 `cooldown_until`。
- 保留模型映射所需的 `config_json`。
- 追加优先级和并发配置。

在请求处理前先执行现有 `filter_credentials_for_model`，再按优先级组获取活动租约。不能先选中高优先级账号再发现并发已满，否则会错误绕过同级其他可用账号。

- [x] **步骤 4：改造普通 route proxy 重试循环**

在 `forward_request` 中：

1. 读取候选账号和模型过滤结果。
2. 使用当前平台游标生成“优先级优先、同级轮询”的索引顺序。
3. 每次尝试前调用 `try_acquire`；拿不到租约就跳过该账号。
4. 将单个租约变量放在单次尝试作用域中，成功响应返回前和所有 `continue` 路径都自动释放。
5. 所有候选都拿不到租约时返回 `route_pool.concurrency_exhausted`。
6. 上游失败后保存现有失败状态，释放当前租约，再尝试下一个账号。

成功或失败都继续更新现有 route proxy 游标和 usage event，不改变现有错误响应结构。

- [x] **步骤 5：让模型测试和一次性池路由复用规则**

修改 `RouteModelTestService` 的池内自动选账号路径，使用同一 registry 和优先级顺序；显式账号测试在请求开始时仍需获取租约，暂停账号返回校验错误。

修改 `RoutePoolService::route_once` 及其 Tauri/Web 调用方，接受 registry 参数，使用同一可用性与优先级选择函数；租约仅覆盖该一次操作的执行区间。

- [x] **步骤 6：补充路由集成测试**

新增代理集成测试：启动两个固定上游，优先级 `1` 账号设置并发 `1`，保持第一个响应挂起，再发起第二个请求，断言第二个请求落到优先级 `2` 账号；释放第一个响应后，后续请求重新回到优先级 `1`。

另外断言暂停账号保留在 `get_route_pool.account_ids` 中但不会成为 `selected_account_id`，以及所有候选并发已满时返回预期错误。

- [x] **步骤 7：运行 Rust 路由测试**

运行：

```text
pnpm rust:test -- route_proxy
pnpm rust:test -- route_pool
pnpm rust:check
```

预期：优先级、冷却、暂停、并发租约和重试链路全部通过。

---

### 任务 5：补充账号接口初始活动数和前端编辑状态

**文件：**
- 修改：`src-tauri/src/services/route_credential_service.rs`
- 修改：`src-tauri/src/database/repositories/route_credential_repository.rs`
- 修改：`src-tauri/src/commands/route_credential_commands.rs`
- 修改：`src-tauri/src/web/handlers/mod.rs`
- 修改：`src/lib/api/client.ts`
- 修改：`src/screens/AccountsScreen.tsx`
- 修改：`tests/AccountsScreen.test.tsx`

**接口：**
- `RouteCredentialService::list/page/get` 在返回账号前填充 registry 的 `active_request_count`；对应 Tauri command 和 Web handler 将 `state.route_proxy.activity()` 透传给 service。
- 前端事件类型：

```ts
export type RouteCredentialActivityEvent = {
  platform: string;
  credential_id: string;
  active_request_count: number;
  max_concurrency: number;
};
```

- [x] **步骤 1：扩展账号查询服务注入活动数**

让账号 service 接收 `&RouteCredentialActivityRegistry`，例如：

```rust
pub async fn list(
    pool: &SqlitePool,
    activity: &RouteCredentialActivityRegistry,
    platform: String,
) -> Result<Vec<RouteCredential>, AppError>
```

分页和单账号读取使用相同的 `activity` 参数；对应 command/Web handler 传入 `state.route_proxy.activity()`。保持 repository 只负责数据库字段，避免将运行时状态写入 SQL。

列表、分页和单账号读取都必须填充活动数；数据库旧账号没有动态值时返回 `0`。

- [x] **步骤 2：先补前端 fixture 和失败测试**

在 `tests/AccountsScreen.test.tsx` 的 fixture 中增加：

```ts
route_priority: 1,
max_concurrency: 2,
active_request_count: 0,
```

新增测试断言：

- 编辑账号后 payload 包含 `route_priority` 和 `max_concurrency`。
- 状态下拉包含“暂停 (paused)”。
- 收到 `route-credential-activity` 的 `1/2` 事件时显示活动标签。
- 收到同账号 `0/2` 事件后隐藏活动标签。

同步在 client mock 中加入 `setRouteCredentialStatuses`。

- [x] **步骤 3：增加编辑面板字段**

在 `AccountsScreen.tsx` 增加 `editPriority` 和 `editMaxConcurrency` state。打开编辑时从账号填充，提交时转换为整数并调用：

```ts
updateRouteCredential(editingCredential.id, {
  display_name: editName.trim(),
  email: editingCredential.kind === "api" ? null : editEmail.trim() || null,
  status: editStatus,
  route_priority: editPriority,
  max_concurrency: editMaxConcurrency,
  secret_payload_json: nextSecretJson,
  config_json: nextConfigJson,
  preview_json: nextPreviewJson,
});
```

优先级使用 `select`，并发使用 `input type="number" min={1} step={1}`；前端在提交前拒绝非整数或小于 `1` 的值。

- [x] **步骤 4：订阅账号活动事件并更新 Query cache**

在账号页面 effect 中调用：

```ts
const unsubscribe = await getTransport().subscribe<RouteCredentialActivityEvent>(
  "route-credential-activity",
  (event) => {
    if (event.platform !== activePlatform) return;
    updateCredentialActivityInCaches(queryClient, event);
  },
);
```

组件卸载、平台切换和订阅失败时清理 listener；更新分页和全量账号两个 cache 中匹配 ID 的 `active_request_count` 与 `max_concurrency`，不触发整页重新请求。

- [x] **步骤 5：实现账号行活动动效**

在账号名称右侧增加仅在 `active_request_count > 0` 时渲染的标记：

```tsx
<span
  aria-label={`正在处理请求，当前 ${activeCount}/${credential.max_concurrency}`}
  className="inline-flex items-center gap-1 text-[10px] text-emerald-700"
  data-testid={`credential-activity-${credential.id}`}
>
  <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-emerald-500" />
  {activeCount}/{credential.max_concurrency}
</span>
```

账号名称区域继续保留模型映射标签和状态 tooltip，不改变行布局的拖拽、选择和编辑操作。

- [x] **步骤 6：运行前端定向测试**

运行：

```text
pnpm test:run -- tests/AccountsScreen.test.tsx
pnpm typecheck
```

预期：账号编辑、活动事件、状态显示和现有账号/算力池测试全部通过。

---

### 任务 6：实现批量状态操作和优先级/并发展示

**文件：**
- 修改：`src/screens/AccountsScreen.tsx`
- 修改：`tests/AccountsScreen.test.tsx`
- 修改：`src/lib/api/client.ts`
- 修改：`src/lib/api/types.ts`

**接口：**
- `setRouteCredentialStatuses(ids: string[], status: AccountStatus): Promise<void>`。
- 批量操作状态 state 使用 `AccountStatus | ""`，空值不发请求。

- [x] **步骤 1：增加批量状态 mutation**

在 `AccountsScreen.tsx` 增加：

```ts
const batchStatusMutation = useMutation({
  mutationFn: ({ ids, status }: { ids: string[]; status: AccountStatus }) =>
    setRouteCredentialStatuses(ids, status),
  onSuccess: async () => {
    setSelectedAccountIds(new Set());
    setBatchStatus("");
    await invalidateAccountData();
  },
});
```

错误时保留当前选择并显示现有 route pool feedback 的错误样式。

- [x] **步骤 2：在选中工具栏增加状态选择器**

在“已选 N 个账号”工具栏中增加带边框的下拉框，选项标签为“批量设置状态”“正常”“暂停”“警告”“异常”“revoked”；选择非空状态后显示确认按钮“应用状态”。

归档页的恢复/删除操作保持不变，批量状态操作可以在所有非统计账号页面使用。

- [x] **步骤 3：增加优先级和并发标签**

在账号名称或模型映射标签区域旁显示：

- `P1` 至 `P5` 优先级标签。
- `并发 1` 或 `并发 2` 等并发上限标签。

活动标签使用独立颜色，不与优先级和账号状态颜色混淆。

- [x] **步骤 4：补充前端批量测试**

新增测试：

1. 选择两个账号，选择“暂停”，点击“应用状态”，断言 `setRouteCredentialStatuses(["id-1", "id-2"], "paused")`。
2. mutation 成功后断言“已选 N 个账号”消失。
3. mutation 失败后断言选择仍存在且显示错误。
4. 账号行显示 `P1`、`并发 2`，活动事件显示 `1/2`。

- [x] **步骤 5：运行批量 UI 测试**

运行：

```text
pnpm test:run -- tests/AccountsScreen.test.tsx
```

预期：批量状态、优先级标签、并发标签和活动动效测试通过。

---

### 任务 7：全量验证和文档同步

**文件：**
- 修改：`docs/superpowers/specs/2026-08-08-route-account-priority-concurrency-design.md`
- 修改：`docs/superpowers/plans/2026-08-08-route-account-priority-concurrency.md`

- [x] **步骤 1：更新 spec/plan 的实现状态**

实现完成后，将 plan 中已完成的步骤标记为 `[x]`，并在 spec 的测试策略中补充实际执行的命令和结果；不修改已确认的路由语义。

- [x] **步骤 2：运行前端全量验证**

运行：

```text
pnpm typecheck
pnpm test:run
pnpm build
```

预期：类型检查、全部 Vitest 测试和生产构建通过。

- [x] **步骤 3：运行 Rust 全量验证**

运行：

```text
pnpm rust:check
pnpm rust:test
```

预期：Rust 编译检查和全部测试通过；不执行全量 `cargo fmt`，避免引入仓库既有格式差异。

- [x] **步骤 4：检查差异和生成物**

运行：

```text
git diff --check
git status --short
```

确认未留下 `target-rust-*` 等测试生成目录，迁移文件存在，工作区只包含本功能相关改动。

- [ ] **步骤 5：创建功能提交**

```text
git add src-tauri/migrations/202608080002_route_credential_priority_concurrency.sql src-tauri/src/models/route_credential.rs src-tauri/src/database/repositories/route_credential_repository.rs src-tauri/src/services/route_credential_service.rs src-tauri/src/services/route_credential_activity.rs src-tauri/src/services/route_proxy_service.rs src-tauri/src/services/route_model_test_service.rs src-tauri/src/services/route_pool_service.rs src-tauri/src/database/repositories/route_pool_repository.rs src-tauri/src/models/route_pool.rs src-tauri/src/commands/route_credential_commands.rs src-tauri/src/commands/route_pool_commands.rs src-tauri/src/web/handlers/mod.rs src-tauri/src/server.rs src-tauri/src/lib.rs src-tauri/src/services/route_proxy_https_service.rs src/lib/api/types.ts src/lib/api/client.ts src/lib/transport/index.ts src/screens/AccountsScreen.tsx tests/AccountsScreen.test.tsx tests/transport/command-contract.test.ts tests/transport/transport.test.ts docs/superpowers/specs/2026-08-08-route-account-priority-concurrency-design.md docs/superpowers/plans/2026-08-08-route-account-priority-concurrency.md
git commit -m "feat: add route account priority and concurrency controls"
```
