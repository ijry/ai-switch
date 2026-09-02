---
title: 账号与算力池
description: 详解 AI Switch 的账号字段、状态机、优先级与并发上限调度规则，以及池内/池外/归档视图、批量操作和凭据导入导出的安全约束。
---

# 账号与算力池

在 AI Switch 里，"账号"是一条**路由凭据**（route credential）。每条凭据都归属某一个平台（Codex、Claude Code、Gemini CLI、Grok、OpenCode、OpenClaw、Hermes），被加入该平台的**算力池**之后，本地代理就可以在多条凭据之间自动挑选、轮换和退避。

本页说明凭据由哪些字段构成、状态如何流转、调度器按什么顺序选号，以及批量维护与导入导出时需要注意什么。

## 两种凭据来源

数据库对凭据类型做了硬约束，只有两种取值：

```sql
kind TEXT NOT NULL CHECK (kind IN ('official','api'))
```

| 类型 | 来源 | 典型内容 |
| --- | --- | --- |
| `official` | 从官方登录态导入（粘贴文本或选择文件） | 官方 OAuth 凭据、订阅信息、配额窗口 |
| `api` | 手工填写或导入的第三方 API 凭据 | Base URL + API Key + 上游协议方言 |

导入官方凭据时必须提供**批次名称**（batch name）。服务层会先创建一个 `route_credential_import` 来源的批次记录，再把导入结果挂到该批次上；批次名为空会直接返回 `validation.batch_name_required`。批次让"一次导入 30 个账号"之后还能按来源批量归档或改状态。

不同平台对凭据类型的支持程度不同：OpenCode、OpenClaw、Hermes 只接受 `api` 类型凭据，且必须显式指定 Base URL 和协议方言。详见 [平台支持矩阵](/guide/platform-support)。

## API 凭据的字段

创建 API 凭据时，敏感值与非敏感配置分开存放：API Key 落在 `secret_payload_json`，其余落在 `config_json`。

| 字段 | 存放位置 | 说明 |
| --- | --- | --- |
| 显示名称 | `display_name` | 列表与日志里的标识，必填 |
| Base URL | `config_json.base_url` | 上游接口根地址，必填 |
| API Key | `secret_payload_json.api_key` | 必填；不会随普通列表接口返回明文 |
| 上游协议方言 | `config_json.interface_format` | `openai` / `openai-responses` / `anthropic` / `gemini` |
| 模型映射 | `config_json.model_mappings` | 数组，每项含 `from`、`to`，可选 `label` 与 `supports_1m` |
| 已拉取模型列表 | `config_json.fetched_models` | 由模型列表拉取写入，见 [模型连通性测试](/guide/model-test) |
| 自定义工具兼容 | `config_json.responses_custom_tool_compat` | 布尔，默认 `false` |
| 自定义 User-Agent | `config_json.headers["User-Agent"]` | 可选；填写后作为固定请求头下发 |
| 每轮纠偏提醒 | `config_json.turn_reminder` | 布尔；开启后每轮在最后一条用户消息后追加一句要求。关闭时该键不写入 |
| 提醒内容 | `config_json.turn_reminder_text` | 可选；留空则用默认「请用简体中文回复。」 |
| 失败策略 | `config_json.failure_policy` | 单账号覆盖重试与语义失败阈值，见 [稳定性与自动恢复](/guide/reliability) |
| 自动恢复规则 | `config_json.recovery` | 定时或探测恢复，见 [稳定性与自动恢复](/guide/reliability) |

**Anthropic 方言的鉴权字段可选。** `api_key_field` 只接受两个值：`ANTHROPIC_API_KEY`（默认，走 `x-api-key` 请求头）或 `ANTHROPIC_AUTH_TOKEN`（走 `Authorization: Bearer`）。填其他值会被拒绝。这一项对应很多第三方 Claude 兼容网关只认其中一种头的现实情况。

协议方言的含义、以及它如何决定请求被改写成什么形状，见 [协议路由与桥接](/guide/protocol-routing)。

## 账号状态机

状态字段只允许五种取值（校验函数 `validate_route_credential_status`）：

| 状态 | 含义 | 是否参与调度 | 能否自动恢复 |
| --- | --- | --- | --- |
| `ok` | 健康 | 是 | — |
| `warning` | 观察中，出现过问题 | 否 | 能 |
| `error` | 已判定不可用 | 否 | 能 |
| `paused` | 手动暂停 | 否 | 能（需显式触发） |
| `revoked` | 凭据已失效/被吊销 | 否 | **不能** |

数据库默认值是 `ok`。状态迁移由三类事件驱动：

- **请求成功**：清空瞬时失败计数与退避窗口；如果当前是 `error` 或 `warning`，会被拉回 `ok`。
- **请求失败**：按失败类型分流。永久性失败（如 refresh token 已被吊销）直接写 `revoked`；可重试失败累加瞬时失败计数并设置退避窗口；语义失败按指纹累加连击计数，达到阈值才翻成 `error`。
- **显式测试成功**：对单个账号手动跑一次模型连通性测试并通过，会执行"显式测试恢复"，清空所有失败计数与退避窗口。

`revoked` 是唯一的终态：重新激活的 SQL 带 `WHERE id = ? AND status != 'revoked'`，自动恢复调度器也会跳过它。要让被吊销的账号复活，只能重新导入或改凭据内容。

**`paused` 的账号仍然可以被显式测试。** 这是有意设计：手动测一次，正是用户判断暂停中的账号是否已经恢复的方式。

**账号状态之外还有一层模型状态。** 同一个账号上的每个模型有自己的 `ok` / `error` / `paused` 三态与冷却窗口，存在 `route_credential_models` 表里。账号级 `paused` 让整个号退出调度，模型级 `paused` 只让这一个模型退出；两者互不影响。编辑抽屉的「模型状态」区块可以逐模型暂停、恢复或解除冷却。

失败分类的完整规则、退避时长与阈值，见 [稳定性与自动恢复](/guide/reliability)。

## 优先级与并发上限

两个调度参数在数据库层就带了约束：

```sql
ALTER TABLE route_credentials
  ADD COLUMN route_priority INTEGER NOT NULL DEFAULT 3
    CHECK (route_priority BETWEEN 1 AND 5);
ALTER TABLE route_credentials
  ADD COLUMN max_concurrency INTEGER NOT NULL DEFAULT 1
    CHECK (max_concurrency >= 1);
```

建表时的列默认值仍是 1，但创建账号的写入路径会显式绑定 5（`DEFAULT_ROUTE_CREDENTIAL_MAX_CONCURRENCY`），所以新账号的实际默认并发是 5。已存在的账号不受影响。

| 参数 | 范围 | 默认 | 作用 |
| --- | --- | --- | --- |
| `route_priority` | 1–5 | 3 | 数值越小越优先。同一数值的账号构成一个优先级组 |
| `max_concurrency` | ≥ 1 | 5 | 该账号同时在飞的请求上限；占满则本轮跳过它 |

### 调度顺序

一次代理请求的选号过程是：

1. **筛出候选**。SQL 条件要求：在池内且启用（`route_pool_members.enabled = 1`）、未归档（`archived_at IS NULL`）、状态为 `ok`，且配额字段（`primary_remain`、`weekly_remain`）为空或大于 0。这一步不判冷却。
2. **排序**。`ORDER BY route_priority ASC, sort_order ASC, created_at ASC` —— 先按优先级，再按池内手工排序，最后按创建时间。
3. **分组轮转**。候选按 `route_priority` 分组，组内从持久化游标（`route_pool_cursors` 表按平台记录 `next_index`）开始轮询。游标持久化意味着重启应用之后不会每次都从第一个账号打起。
4. **按模型过滤并解析模型键**。按平台能力规则与请求里的模型名筛一遍，同一趟为每个留下的候选算出它该被记账的上游模型键（`model_mappings` 的 `to`；官方账号取请求原名）。筛空返回 `route_pool.model_unmatched`。
5. **查模型状态并剔除冷却**。按 `(账号 ID, 模型键)` 对批量读 `route_credential_models`，然后：被暂停或判为异常的模型**硬排除**，不参与任何兜底；账号级或模型级冷却未过期的进冷却桶。如果这样一来一个可用账号都不剩，调度器**只保留最快恢复的那一个冷却账号**去试，而不是让请求直接失败；如果连兜底都没有（全被硬排除），返回 `route_pool.model_unavailable`。
6. **取并发租约**。逐个尝试 `try_acquire(platform, id, max_concurrency)`；拿不到租约就顺延到重试队列里的下一个账号。

**第 4 步必须在第 5 步之前。** 该不该判为冷却取决于请求哪个模型，不知道模型就无法分桶。完整规则见 [稳定性与自动恢复](/guide/reliability)。

支撑这套查询的复合索引是：

```sql
CREATE INDEX IF NOT EXISTS idx_route_credentials_routing_priority
  ON route_credentials(platform, route_priority, status, next_retry_at, cooldown_until);
```

索引列顺序也正是调度语义：**池按平台切分，组内按优先级排序，冷却中的账号被排除在外。** 模型级状态另有一个索引：

```sql
CREATE INDEX IF NOT EXISTS idx_route_credential_models_lookup
  ON route_credential_models(route_credential_id, status, cooldown_until);
```

### 怎么配这两个值

- **主力号 + 备用号**：主力设 1 或 2，备用设 4 或 5。只有主力全部冷却或占满时，流量才会落到备用号。
- **同权重摊平**：所有号设同一优先级，靠组内轮转均摊，配额消耗更平均。
- **并发上限对齐上游限制**：上游按 key 限制并发时，把 `max_concurrency` 设成它允许的值。新账号默认 5；对并发敏感的上游改成 1，就退回到同一个账号任意时刻只有一个在飞请求。

## 列表视图与批量操作

账号列表按三个互斥的范围（`RouteCredentialPoolScope`）分页展示：

| 范围 | 含义 |
| --- | --- |
| `in_pool` | 已加入该平台算力池 |
| `out_of_pool` | 已存在但未入池（默认范围） |
| `archived` | 已归档 |

分页大小只接受 `20`、`50`、`100` 三个值，其他值会被 `page_size must be 20, 50, or 100` 拒绝。

可用的批量与单条操作：

| 操作 | 行为 |
| --- | --- |
| 归档 / 恢复归档 | 批量写入或清空 `archived_at`；归档账号不参与任何调度 |
| 批量设置状态 | 对选中 ID 统一写入某个合法状态 |
| 拖拽排序 | 按"移动到某两个账号之间"重算 `sort_order`，在当前筛选与范围内生效 |
| 复制 | 复制一条凭据，新名称是原名加上 `YYYY-MM-DD` 日期戳 |
| 删除 | 物理删除；池成员表带 `ON DELETE CASCADE`，成员记录同步消失 |

**归档 vs 删除**：归档是可逆的软隐藏，凭据与历史统计都还在；删除不可逆。定期轮换一批号时，归档比删除更合适。

归档专用索引同样是复合的：

```sql
CREATE INDEX IF NOT EXISTS idx_route_credentials_archive
  ON route_credentials(platform, archived_at, sort_order);
```

## 导出与导入

导出对话框提供两种格式：**JSON 文件**和**方案链接**（scheme link）。

::: danger 导出即泄露风险
此导出内容包含凭据。请妥善保管，并删除不再需要的副本。
:::

### JSON 导出

- 建议文件名形如 `ai-switch-<platform>-route-credentials-20260819-101530.json`，平台与 UTC 时间戳都写在名字里。
- 载荷带 `schema_version: 1`，以及来源实例 ID、来源凭据 ID、平台、类型等元数据；可以关闭"增强元数据"只导出核心字段。
- 单次导出最多 **2000** 条凭据，序列化后最大 **8 MiB**（`8 * 1024 * 1024`）。超限返回 `transfer.selection_too_large` 或 `transfer.export_too_large`。
- 桌面端走系统保存对话框；Web 服务模式下走浏览器下载。

### 方案链接

方案链接是形如 `aiswitch://v1/import?...` 的深链，**只对 `api` 类型凭据生成**（官方凭据没有可放进 URL 的等价形式）。

::: warning 复制方案链接
复制方案链接会将 API 密钥放入系统剪贴板。
:::

点击复制按钮时会先弹确认框，文案是：

> 此方案链接包含 API 密钥。是否复制到系统剪贴板？

确认之后才会真正写入剪贴板。关闭导出对话框会立即清掉界面上持有的敏感状态。

### 导入去重

导入端会记录每条凭据的来源身份，避免同一份导出被反复导入成重复账号：

```sql
CREATE TABLE IF NOT EXISTS route_credential_transfer_origins (
  route_credential_id TEXT PRIMARY KEY,
  source_instance_id TEXT NOT NULL,
  source_credential_id TEXT NOT NULL,
  source_platform TEXT NOT NULL,
  source_kind TEXT NOT NULL,
  source_schema_version INTEGER NOT NULL,
  source_fingerprint TEXT NOT NULL,
  imported_at TEXT NOT NULL,
  UNIQUE(source_instance_id, source_credential_id, source_platform)
);
```

每个安装实例自己也有一个稳定身份（`transfer_installation_identity`），所以"从 A 机器导出、导入到 B 机器"和"在同一台机器上重复导入"是可区分的。

除自身格式之外，导入还兼容其他账号切换工具的导出格式（兼容导入协议）。`schema_version` 不匹配时返回 `transfer.schema_version_unsupported`，不会尝试猜测字段含义。

## 存储位置

所有凭据数据都在应用的 SQLite 数据库里：数据目录固定为用户主目录下的 `~/.ai-switch`，数据库文件名为 `ai-switch.db`（开发构建用独立的 `ai-switch-dev.db`，互不干扰）。

::: danger 数据目录就是凭据目录
API Key 与官方登录凭据都存放在这个 SQLite 数据库里（`route_credentials.secret_payload_json` 列），**没有额外的静态加密**。请把整个 `~/.ai-switch` 目录当作凭据目录对待：

- 不要把它放进公开仓库、未加密的同步盘或共享目录
- 备份时注意备份介质本身的安全性，并收紧目录的文件权限
- 建议在开启全盘加密的磁盘上使用
:::

表结构由 `src-tauri/migrations` 下的 **23 个**只进式迁移脚本定义。与本页相关的主要迁移：

| 迁移 | 内容 |
| --- | --- |
| `202607130011_route_credentials.sql` | `route_credentials` 主表、`route_pool_members` 池成员表 |
| `202607300001_route_credential_retry.sql` | 瞬时失败计数、`next_retry_at`、`cooldown_until` |
| `202608040001_route_credential_transfer.sql` | 安装实例身份与导入来源表 |
| `202608050001_route_credential_archive.sql` | `archived_at` 与归档索引 |
| `202608060002_route_usage_breakdown.sql` | `usage_events` 的 token 与价格拆分列 |
| `202608080002_route_credential_priority_concurrency.sql` | `route_priority`、`max_concurrency` 与调度索引 |
| `202608130001_route_credential_semantic_failure_streak.sql` | 语义失败连击计数与指纹 |

## 下一步

- [协议路由与桥接](/guide/protocol-routing)：账号的协议方言如何决定请求被改写成什么形状
- [模型连通性测试](/guide/model-test)：怎么验证一条新凭据真的能出话
- [用量与请求统计](/guide/usage-stats)：每条请求的 token 与费用记在哪里
- [稳定性与自动恢复](/guide/reliability)：退避、冷却与自动恢复的精确规则
