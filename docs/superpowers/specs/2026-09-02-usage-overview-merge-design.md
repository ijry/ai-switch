# 用量总览合并设计

## 背景

统计页当前把两套互不相干的数字并列展示：

- **路由请求统计** 来自 `usage_events` 表，只看得见走本应用代理的流量。
- **本机会话用量** 来自 `session_usage_service` 扫描的 CLI 会话记录，包含未经代理的请求。

两套数字各有六张汇总卡、各有独立的时间口径，且下方的请求列表只反映其中一套。用户看到的是两个数，而问题只有一个：我到底花了多少。

同时按模型的明细表常驻展开，面板里散落四段说明文字，请求列表铺开九列，整体信息密度失衡。

统计视图的全部实现内联在 `src/screens/AccountsScreen.tsx`（7242 行）中，与账号列表共用状态、共用底部状态栏的分页控件，且 `get_route_pool` 的 query key 把统计筛选条件与算力池成员绑在一起。

## 目标

1. 会话记录与数据库记录进入同一个列表，匹配上的合并为一行。
2. 顶部一套数字，语义为「我的总花费」：本机会话 ∪ 代理流量，去重后。
3. 按模型等维度的统计改为分段器，默认收起，点击后展示。
4. 顶部数字使用万 / 百万 / 亿单位。

## 非目标

- 不改动 `usage_events` 已有行的语义，不做历史数据 backfill。
- 不把统计面板提升为独立页面（入口保持在账号页）。
- 不引入新的图表库或可视化形态。
- 不改动模型价格表的配置方式与估算公式。

## 已确认的决策

| 决策点 | 选定 |
| --- | --- |
| 顶部数字语义 | 我的总花费：本机会话 ∪ 代理流量，去重后 |
| Codex 会话行粒度 | 逐轮拆分（每次请求一行） |
| 分段器维度 | 模型 / 平台 / 账号 / 来源，默认全收起 |
| 数字单位 | 严格三档：≥1万 用万，≥100万 用百万，≥1亿 用亿 |

## 关联键的可行性验证

在本机真实数据上验证（只读副本，未改动原库）：

- `usage_events` 共 3430 条 `metric_type='request'` 行。其中 claude 成功行 2921 条，2916 条的 `response_body` 预览含 `message_start` 帧，可解析出 `msg_<hex>`。
- `~/.claude/projects` 的 assistant 行携带 `message.id`，现有去重逻辑已在使用。
- **交集 2905 / 2933 ≈ 99.0%。**
- codex 侧：数据库存储的 `response.created` 中的 uuid，在 rollout 的 `rs_<uuid>` / `fc_<uuid>` 中可定位，抽样 6/6 命中。

结论：两侧存在可用的关联键，不需要依赖时间戳 + 模型 + token 的启发式配对。

### 规模事实

合并后列表中多数行来自会话记录：本机代理行 3430 条，而会话语料含 6 万余条带用量的 Claude 行与 1087 个 codex rollout。重叠区约 2900 行，会话独有部分大一个数量级，因此多数行不带账号归属。

## 设计

### 合并层位置

合并、去重、分页全部在 Rust 侧完成，前端只负责渲染。

理由是量级：把 6 万余行传输到前端再合并分页不可行。`session_usage_service.rs` 的 `PARSE_CACHE`（按文件缓存，以 mtime + size 失效）存储的正是未过滤未去重的逐条记录，复用它比重建一套解析更省。

新增命令：

```
get_usage_overview(since, page, page_size)
  -> { totals, rows, groups, row_count, page, page_size, integrity }
```

命令不接受 `platform` 参数——顶部数字跨 provider，per-platform 视角由 `groups` 中的平台分组提供。这与「查询 key 不再包含 `activePlatform`」一致。

`groups` 一次返回全部四个维度的分组结果，不设 `group_by` 参数。四个维度的基数都很小（模型、账号、平台、来源各为个位到十位数），一次算完可避免切换分段器时重新请求。

**`totals` 与 `groups` 计算于时间窗内的全量行，而非当前页**——分页只影响 `rows`。

`rows` 按时间倒序（`created_at` / 会话记录的 `timestamp`），与现有请求列表一致。

`integrity` 承载数据完整性说明所需的事实，供前端组装文案：会话扫描是否截断、扫描文件数、无价格模型的请求数、按本地价格表估算的请求数、以及缺失去重键因而可能双算的行数。

沿用 `TimeWindow::contains` 的既有语义：无时间戳的会话记录只在无边界窗口（「累计」）中计入，时间筛选不会静默吸收无日期行。

### 解耦 get_route_pool

统计搬出后，`get_route_pool` 的 query key 从 `["route-pool", platform, statsSince, requestPage, pageSize]` 退回 `["route-pool", platform]`。

这消除了一处既有纠缠：该查询同时为 `draftPoolIds` 与模型测试的乐观缓存写入供数，而它的 key 里却编入了统计筛选条件；退化后与 `invalidateAccountData` 及实时事件失效逻辑使用的 key 前缀对齐。

`RoutePoolStats` 中的统计字段（`requests`、`recent_logs`、`request_row_count`、`request_page`、`request_page_size`、各 token 与 cost 汇总）不再被前端消费。本次不删除这些字段，以免牵动 route proxy 与 model test 的既有测试；由新命令承担统计职责。

### 去重键的落地

不在查询时解析 SSE 文本抠取 id——那意味着每次刷新都要扫描全部 `metadata_json`，开销随库增长恶化。

分两段：

**新行** — 代理写入时抽取上游响应 id，存入新列。写入点本来就持有完整响应字节，是最省的位置。

```sql
ALTER TABLE usage_events ADD COLUMN upstream_response_id TEXT;
CREATE INDEX IF NOT EXISTS idx_usage_events_upstream_response_id
  ON usage_events (upstream_response_id);
```

抽取规则按响应形态：Anthropic 取 `message_start` 帧或非流式响应体的 `message.id`；OpenAI Responses 取 `response.created` 的 `response.id`；OpenAI Chat Completions 取 `id`（`chatcmpl-*`）。

**历史行** — 该列为 NULL 时，回退到查询时解析 `response_body` 预览。这个集合在升级那一刻冻结，且随旧数据滑出所选时间窗而缩小；默认「当日」视图在升级一天后不再走该路径。因此不需要 backfill 任务，也不需要开关。

**已知边界**：Claude 非流式响应的 id 可能落在 2 KB 成功预览（`ROUTE_PROXY_SUCCESS_BODY_LIMIT`）之外。本机现存 5 行属于此类，占 0.2%。这些行匹配不上，对应请求会双算。如实在数据完整性说明中标注，不为 0.2% 增加一列存储原始 id。

### Codex 逐轮拆分

`total_token_usage` 是全会话累计值。现有实现刻意只取每个 rollout 的最后一条——早期版本对其求和，单文件多算 350 倍。要让 codex 请求进入合并列表并参与去重，必须拆成逐轮。

拆分规则对相邻两次 `total_token_usage` 做差：

1. 首个事件按原值计入。
2. 连续重复的事件跳过（同一累计值会连发 2-3 次）。
3. 任一字段出现负差，判定为会话重置 / fork，该事件按原值重新起算。

**排除 `last_token_usage`**：新版记录中存在该字段，直觉上可直接使用，但抽样 58 个可比文件中仅 12 个的 `Σ(last)` 等于最终累计值，46 个偏高——与 cc-switch #2571（codex fork 重复统计父会话历史 token）现象一致。做差法在 77 个可比文件中 76 个完全相等，唯一不等的亦为 fork 场景。

**守住 350 倍教训的机制**是把它写成不变量测试：对任意 rollout，`Σ(拆出的各轮) == 最终累计值`（重置点除外）。该断言存在，即不可能退回求和的错。

codex 行的去重键取自 rollout 中 `response_item` 的 id：`rs_<uuid>` / `msg_<uuid>` / `fc_<uuid>` 均携带上游响应 uuid，且出现在该轮 `token_count` 之前（已 dump 顺序确认）。排除 `fco_` 前缀——那是客户端侧的 function_call_output，非响应 id。

### 合并算法

1. 取时间窗内的会话记录（复用 `PARSE_CACHE`，沿用 Claude 按 `message.id` 的跨文件去重）与代理行（`metric_type='request'`）。
2. 以响应 id 建索引，两侧同键的合为一行，标记为**匹配**。
3. 无键或未命中的会话记录标记为**仅会话**；未命中的代理行标记为**仅代理**。
4. 汇总与分组基于合并后的行集，因此一次请求只计一次。

一次请求同时出现在两侧，正是 CLI 走本应用代理的情形——这是设计上的正常路径，不是异常。仅会话即该 CLI 直连上游；仅代理即请求方不是被扫描的两种 CLI（如模型测试、或指向本代理的其他工具）。

### 合并行的字段取舍

三种行共存，以来源徽标区分：**匹配**（两侧均有）、**仅会话**（未走本应用代理）、**仅代理**（代理有记录而会话无，如模型测试）。该徽标同时作为分段器「来源」维度的分组依据。

匹配行的字段来源：

| 字段 | 取自 | 理由 |
| --- | --- | --- |
| Token | 会话侧 | 会话记录将缓存拆为写入 / 读取，而数据库只有合并的 `cache_tokens`；两者定价差 12.5 倍（写入按输入价 1.25 倍、读取 0.1 倍）。且代理侧遇流截断会漏最后一个 delta。 |
| 费用 | 数据库的上游价格优先（`price_source='upstream'`），否则按会话 token 用本地价格表估算 | 上游价格是真实计费数据 |
| 账号、状态码、路径、上游响应体 | 数据库 | 会话记录不含这些 |

### 请求列表

列数从 9 降至 6 + 详情按钮：时间 / 模型 / Token / 费用 / 账号 / 来源。路径收进详情。

**状态码不占独立列**：失败行在「来源」徽标旁内联一个红色 chip，成功行不显示。会话行本无 HTTP 状态（transcript 不记录），该值天生稀疏，常驻一列只制造空白。完整状态码与路径在详情中。

无账号归属的行（仅会话）在账号列显示为「未经代理」。

分页控件从底部共享状态栏移入面板内。面板独立后自带分页比与账号分页共用一个三元表达式更自洽。

### 分段器

一张表，五列：分组名 / 请求数 / 输入 / 输出 / 费用。切换 模型 / 平台 / 账号 / 来源 只更换分组依据。默认整块收起，仅保留分段器一行。

结构上等同于现有 `by_model` 表加一个分组维度。

行数上限沿用现有 `by_model` 的 `.slice(0, 12)`；超出时标注被省略的行数，而非静默截断。账号维度中无归属的行归入「未经代理」一栏。

### 顶部卡片

从两套共 12 张压缩至 5 张：请求 / 输入 / 输出 / 缓存 / 费用。

- 移除「Token 总计」——它是输入与输出之和，加单位格式化后单看已足够。
- 缓存卡显示合计，写入 / 读取拆分放悬浮提示。

现散落的四段说明（本地价格表脚注、扫描截断警告、无价格模型提示、估算免责）合为一行数据完整性说明。既然语义是「我的总花费」，该数完整与否必须说清，包括上文 0.2% 匹配不上可能双算的行。

### 单位格式化

新建 `src/lib/usageFormat.ts`：

| 区间 | 形态 | 小数位 |
| --- | --- | --- |
| < 10,000 | `9,999` | 千分位，无缩写 |
| ≥ 10,000 | `25.0万` | 1 位 |
| ≥ 1,000,000 | `2.50百万` | 2 位 |
| ≥ 100,000,000 | `55.85亿` | 2 位 |

小数位沿用现有 M / B 的习惯。精确值继续置于 `title`，现有 `getByTitle("5,584,802,591")` 断言无需改动。

三档同时存在打断了中文按 1e4 分组的进制（万 → 亿），因此列表中会并存「万」与「百万」两种词。这是明确选定的取舍，非疏漏。

小数位截断不四舍五入到进上一档：99,999,999 显示为 `100.00百万` 而非 `1.00亿`，档位由原值决定。

费用不套此单位，仍用货币格式（如 `$16,248.91`）——金额量级依靠货币符号与小数位判断，套「万」反而更难读。`formatCostMicros` 现有的小额多位小数分支保留。

单独建文件的原因：这些格式化函数当前均未导出，只能通过渲染整个界面间接测试；提取后可直接测边界值。

### 从 AccountsScreen 拆出

新建 `src/components/accounts/UsageOverviewPanel.tsx`，迁入统计相关的 state、查询、effect 与 JSX。

`AccountsScreen` 保留 `accountView === "stats"` 的开关逻辑：分段控件（`accountViewOptions`）、账号列表查询的 `enabled: !statsOpen`、`openExport` 与 `setSelectedAccountsStatus` 的两处 guard、外层条件渲染。

**两处不可误伤**（已核对调用点）：

- `formatUsageTime` 亦被实时日志弹窗使用；`prettyJsonOrText` 亦被实时日志、凭证失败提示、模型测试面板使用。两者不随迁移移动，保留在共享位置。
- `LiveLogStage` 与 `liveLogStagesIdentical` 名称近似统计，实际只服务实时日志弹窗。

### 平台归属的连带影响

跨 provider 的总花费语义使该面板不再属于单一平台：切换 Codex 页与 Claude 页将看到相同数字，而现副标题为「统计当前 Codex 的历史路由请求」。

- 副标题改为不绑定平台的表述。
- per-platform 视角改由分段器的「平台」维度提供。
- 查询 key 不再包含 `activePlatform`，切平台不触发重查。

代价是同一份内容出现在每个平台的账号页中。本次保持入口不动；是否提升为独立页面留待后续决定。

### 刷新间隔

合并后只剩一个查询，需重新确定间隔（现为代理侧 5 秒、会话侧 60 秒）。会话扫描是较贵的一侧，暖缓存实际耗时无法在当前 worktree 内测量（缺 `node_modules`、`dist` 与 go sidecar 二进制，`cargo` 被 tauri build script 拦截）。

先按 10 秒设计，并将「实测暖扫描耗时，过慢则退让」列入实施计划。

### 错误与降级

- 会话扫描失败或语料为空：仍渲染代理侧数据，在完整性说明中标明会话侧缺失。
- 数据库查询失败：整个面板报错，与现有 `formatApiError` 行为一致。
- 单个 rollout 解析失败：跳过该文件，不影响其余（沿用 `malformed_lines_do_not_abort_the_file` 的既有策略）。
- `metadata_json` 无法解析：沿用现有的逐行 UI 回退。

## 测试

**新增 Rust 测试**

- 不变量：对任意 rollout，`Σ(拆出的各轮) == 最终累计值`（重置点除外）。此为守住 350 倍教训的锁。
- codex 拆分：连续重复事件跳过、负差触发重置起算。
- 上游响应 id 抽取：Anthropic 流式 / 非流式、OpenAI Responses、Chat Completions 四种形态。
- 合并：msg id 命中时合为一行；字段优先级（token 取会话、上游价格优先）。
- 历史行回退：`upstream_response_id` 为 NULL 时从 `response_body` 预览解析。
- 分组：四个维度各自的分组正确性。

**新增前端测试**

- `usageFormat` 边界值：9,999 / 10,000 / 999,999 / 1,000,000 / 99,999,999 / 100,000,000。
- 合并列表渲染三种来源徽标；状态码 chip 仅失败行出现。
- 分段器默认收起，点击后展开并按所选维度分组。

**需修改的既有测试**（`tests/AccountsScreen.test.tsx`）

- `renders filtered route request statistics, expands request details, and paginates request rows`（约 200 行，2928-3135）——列结构、数据源与分页位置全变；随统计实现迁移至新面板的测试文件。
- `renders local session usage alongside route statistics`（3137-3203）——「本机会话用量」独立区块消失，断言改为合并后的单套数字；`5.58B` 改为 `55.85亿`，`getByTitle("5,584,802,591")` 保留。
- `auto refreshes route statistics only while the panel is open`（3237-3282）——查询与间隔变更。
- `shows sub-cent route costs instead of rounding them to $0.00`（3205-3235）——断言本身不变（费用不套万/亿单位），但需随面板迁移并改用新命令的 mock。

**应原样通过**

- `switches between pooled, unpooled, and statistics segments with scoped actions`（1272-1303）——只断言切到统计后账号列表消失。
- `tests the credential pool route …`（3284-3331）——只断言非统计视图下「请求统计」不出现。

面板迁出后需为新组件建立测试文件，mock 新命令而非 `getRoutePool` + `getSessionUsageStats` 两个。`tests/DeepLinkImportDialog.test.tsx` 内联构造 `RoutePoolStats` 仅为满足 `getRoutePool` mock，不受影响。

## 预期文件

**新建**

- `src-tauri/migrations/202609020002_usage_upstream_response_id.sql`
- `src/lib/usageFormat.ts` 与对应测试
- `src/components/accounts/UsageOverviewPanel.tsx` 与对应测试
- Rust 侧的合并服务（`services/usage_overview_service.rs`）与命令

**修改**

- `src-tauri/src/services/session_usage_service.rs`——暴露逐条记录，codex 逐轮拆分
- `src-tauri/src/services/route_proxy_service.rs`——写入时抽取上游响应 id
- `src-tauri/src/services/route_model_test_service.rs`——同样写入新列
- `src-tauri/src/database/repositories/route_pool_repository.rs`——新列读写
- `src-tauri/src/models/route_pool.rs`、`src/lib/api/types.ts`、`src/lib/api/client.ts`
- `src-tauri/src/lib.rs`、`src-tauri/src/web/handlers/mod.rs`——注册新命令
- `src/screens/AccountsScreen.tsx`——移出统计实现，保留开关逻辑
- `tests/AccountsScreen.test.tsx`

## 待实施阶段确认

- 暖扫描实测耗时，据此定刷新间隔（先按 10 秒设计）。
- 合并后单次请求的端到端耗时；若分组计算成为瓶颈，考虑将 `groups` 拆为按需请求。

两项均需可运行的构建环境，当前 worktree 缺 `node_modules`、`dist` 与 go sidecar 二进制。
