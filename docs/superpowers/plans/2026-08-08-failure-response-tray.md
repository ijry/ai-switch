# 异常请求结果提示与托盘支持实施计划

> **For agentic workers:** 本计划在当前 `main` 工作区内执行，按复选框逐项完成并在每个阶段运行对应验证。

**目标：** 保存导致账号异常/冷却的上游响应，并为桌面应用增加关闭隐藏到托盘的行为。

**架构：** 后端在现有账号失败记录链路中增加一个受限的响应体字段，沿现有 SQL 查询和 Tauri command 序列化到前端；前端将状态标签包装为可悬停浮层。托盘使用 Tauri 2 内置 tray-icon feature 和现有图标资源，窗口关闭事件统一拦截并由托盘菜单负责恢复或退出。

**技术栈：** Rust、SQLite/sqlx、Tauri 2、React、TypeScript、Vitest、Cargo tests。

## 全局约束

- 版本保持 `0.4.0`。
- 文案使用中文，响应内容不保存请求头、API Key 或请求体。
- 失败响应保存长度上限为 8192 个字符，空内容保存为 `NULL`。
- 主窗口关闭默认隐藏到托盘；只有托盘退出菜单真正结束应用。
- 直接修改 `main`，不创建分支或 worktree。

---

### 任务 1：增加失败响应字段和迁移

**文件：**
- 新建：`src-tauri/migrations/202608080001_route_credential_failure_response.sql`
- 修改：`src-tauri/src/models/route_credential.rs`
- 修改：`src/lib/api/types.ts`

**接口：**
- 新增数据库列：`last_failure_response_json TEXT`。
- 新增 Rust 字段：`pub last_failure_response_json: Option<String>`。
- 新增前端字段：`last_failure_response_json?: string | null`。

- [x] **步骤 1：编写迁移和模型字段**

```sql
PRAGMA foreign_keys = ON;

ALTER TABLE route_credentials ADD COLUMN last_failure_response_json TEXT;
```

将 Rust/TypeScript 字段紧跟在 `last_failure_message` 后面，保持失败字段连续。

- [x] **步骤 2：运行数据库相关 Rust 测试确认迁移可加载**

运行：`pnpm rust:test`

预期：现有数据库仓库测试通过；若测试在后续字段读写测试前失败，先修正迁移或 FromRow 字段顺序。

---

### 任务 2：保存失败请求响应并在恢复时清理

**文件：**
- 修改：`src-tauri/src/database/repositories/route_credential_repository.rs`
- 修改：`src-tauri/src/database/repositories/route_credential_repository.rs` 中所有账号查询字段列表
- 修改：`src-tauri/src/services/route_proxy_service.rs`

**接口：**
- `record_transient_failure` 增加 `response_body: Option<&[u8]>` 参数。
- `record_semantic_failure` 增加 `response_body: Option<&[u8]>` 参数。
- `record_route_credential_failure` 增加同名响应参数并向仓库透传。
- 新增私有截断函数，将 UTF-8 响应去空白后限制为 8192 字符。

- [x] **步骤 1：先添加仓库失败响应测试**

在现有 retry repository 测试中调用：

```rust
RouteCredentialRepository::record_transient_failure(
    &pool,
    &created.id,
    "upstream_status",
    "upstream returned 401",
    Some(br#"{"error":{"message":"bad key"}}"#),
)
```

断言 `get` 返回的 `last_failure_response_json` 等于原始 JSON，并断言 `clear_transient_failure` 后为空。

- [x] **步骤 2：运行测试确认新签名和字段尚未完成**

运行：`pnpm rust:test`

预期：测试因新参数或字段实现尚未完成而失败；失败范围应限于账号仓库相关代码。

- [x] **步骤 3：实现保存、截断和查询字段**

在所有包含 `last_failure_message` 的账号查询中追加 `last_failure_response_json`；更新 transient/semantic SQL：

```sql
last_failure_kind = ?, last_failure_message = ?, last_failure_response_json = ?, updated_at = ?
```

`clear_transient_failure` 和 `recover_after_explicit_test` 同时设置 `last_failure_response_json = NULL`。非法 UTF-8、空响应或超过上限的响应按“去空白后截断”处理。

- [x] **步骤 4：把有响应体的代理失败路径接入**

在 `route_proxy_service.rs` 中：

- 响应转换失败调用传入 `Some(&response_bytes)`。
- 语义失败调用传入 `Some(&response_bytes)`。
- 可重试 HTTP 状态调用传入 `Some(&response_bytes)`。
- 刷新、请求构建、传输、读取响应失败传入 `None`。

- [x] **步骤 5：运行 Rust 测试确认保存链路通过**

运行：`pnpm rust:test`

预期：账号仓库、路由代理和现有服务测试全部通过。

---

### 任务 3：增加账号列表响应详情浮层

**文件：**
- 修改：`src/screens/AccountsScreen.tsx`
- 修改：`tests/AccountsScreen.test.tsx`

**接口：**
- 新增 `CredentialFailureTooltip` 组件，接收 `credential: RouteCredential` 和 `children`。
- 使用 `prettyJsonOrText` 格式化 `last_failure_response_json`，显示失败类型、消息和响应内容。

- [x] **步骤 1：增加前端失败账号 fixture 和失败测试**

覆盖以下两种账号：

```ts
{
  status: "error",
  last_failure_kind: "semantic_response_failed",
  last_failure_message: "upstream rejected the request",
  last_failure_response_json: '{"error":{"message":"bad key"}}'
}
```

以及 `status: "ok"` 但有 `cooldown_until` 和 `last_failure_response_json` 的账号。断言隐藏 tooltip 中包含 `bad key`，无响应字段的账号不渲染 tooltip。

- [x] **步骤 2：运行定向测试确认组件尚未实现**

运行：`pnpm test:run -- tests/AccountsScreen.test.tsx`

预期：新增 tooltip 断言失败。

- [x] **步骤 3：实现无布局跳动的 hover/focus 浮层**

将状态标签和冷却标签分别包在相对定位容器中；浮层使用 `role="tooltip"`、`group-hover:block` 和 `group-focus-within:block`，响应区域设置最大宽高、滚动和等宽字体。无 `last_failure_response_json` 时只保留现有 `title` 文案。

- [x] **步骤 4：运行前端定向测试**

运行：`pnpm test:run -- tests/AccountsScreen.test.tsx`

预期：账号列表相关测试全部通过。

---

### 任务 4：配置 Tauri 托盘图标和菜单

**文件：**
- 修改：`src-tauri/Cargo.toml`
- 修改：`src-tauri/tauri.conf.json`
- 修改：`src-tauri/src/lib.rs`

**接口：**
- Tauri 依赖启用 `tray-icon` feature。
- `app.trayIcon` 使用 `icons/32x32.png`、tooltip `AI Switch`、`showMenuOnLeftClick: false`。
- 菜单 ID 固定为 `tray-show` 和 `tray-quit`。

- [x] **步骤 1：添加 Tauri tray feature 和配置**

在 `Cargo.toml` 使用：

```toml
tauri = { version = "2", features = ["tray-icon"] }
```

在 `tauri.conf.json` 的 `app` 下增加：

```json
"trayIcon": {
  "iconPath": "icons/32x32.png",
  "iconAsTemplate": true,
  "showMenuOnLeftClick": false,
  "tooltip": "AI Switch"
}
```

- [x] **步骤 2：实现托盘菜单和窗口关闭拦截**

在 `lib.rs` 中用 `MenuBuilder`/`MenuItemBuilder` 设置“显示主窗口”和“退出 AI Switch”；用 `WindowEvent::CloseRequested` 调用 `api.prevent_close()` 后隐藏主窗口。用 `Arc<AtomicBool>` 标记托盘退出，避免退出菜单触发隐藏逻辑。

- [x] **步骤 3：运行 Rust 编译检查**

运行：`pnpm rust:check`

预期：Tauri tray API、菜单 API 和现有平台条件编译均通过。

---

### 任务 5：全量验证

**文件：**
- 检查：`src-tauri/tauri.conf.json`
- 检查：`src-tauri/icons/32x32.png`

- [x] **步骤 1：运行前端类型和测试**

运行：`pnpm typecheck`、`pnpm test:run`

- [x] **步骤 2：运行生产构建**

运行：`pnpm build`

- [x] **步骤 3：运行 Rust 测试和检查**

运行：`pnpm rust:test`、`pnpm rust:check`

- [x] **步骤 4：检查差异和工作区**

运行：`git diff --check`、`git status --short`

预期：所有命令通过，托盘图标路径存在，未生成无关文件。
