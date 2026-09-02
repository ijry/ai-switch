# 按模型维度冷却设计

日期：2026-09-02

## 问题

一个账号可以通过 `config_json.model_mappings` 同时支持多个模型（例如 `gpt-5.6-sol` 与 `glm-5.3`）。现有冷却机制只有账号一个维度：`gpt-5.6-sol` 连续失败触发冷却时，`route_credentials.cooldown_until` 被写入，选号时整个账号被跳过，`glm-5.3` 明明健康却一起不可用。

中转站按模型分别限流或某个模型的后端单独故障，是这套机制最常遇到的失败形态，因此账号级冷却的误伤面很大。

## 目标形态

两层冷却：

- **模型级**（默认）：上游针对单个模型返回的失败，只冷却触发它的 `(账号, 模型)` 对。
- **账号级**（升级）：明确属于凭证或网络的失败，仍然冷却整个账号；此外当一个账号的全部已知模型都不可用时，自动升级为账号级冷却。

模型也支持手动暂停，与时间冷却正交。

## 非目标

- 不支持按模型单独配置冷却秒数/阈值。`failure_policy` 仍是账号级一份，对该账号所有模型生效。真需要差异化时拆成两个账号即可，代价远低于改动 `model_mappings` 结构及其导入导出。
- 不改 `route_pool_service.rs:116` 那条独立选号路径（Tauri 命令 `route_pool_route_once`）。它今天既不看冷却也不做模型过滤，扩大范围没有收益。
- 不新增出站限流器。

## 1. 数据模型

新增 migration `202609020001_route_credential_models.sql`：

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

设计取舍：

**三态而非五态。** `ok` / `error`（语义连击自动置位）/ `paused`（仅手动）。不设 `revoked`——凭证失效是账号概念；不设 `warning`——账号上它也只是手动标记，模型上没有触发源。

**`paused` 与 `cooldown_until` 正交。** 前者是用户决定，后者是时间退避。仿照 `record_semantic_failure_with_status`（`route_credential_repository.rs:1477`）对 `revoked`/`paused` 的处理：暂停中的模型照常记失败账，但 `status` 不被自动改写。

**冷却可被兜底探测，暂停与异常不可以。** 「全部候选都在冷却就试最早恢复的那个」只对时间冷却生效。`paused` 与 `error` 是硬排除，与账号级 `status != 'ok'` 被选号 SQL 直接剔除一致。

**只有一个时间戳。** 账号级 `next_retry_at` 与 `cooldown_until` 被 `record_transient_failure`（`:1380-1387`）赋成同一个值，是冗余，新表不复制。

**行生命周期。** 首次失败或手动暂停时建行。成功时：`paused` 的行保留状态、只清失败字段；其余整行删除。健康系统里这张表只剩手动暂停的行。行数上限为「账号数 × 每账号映射模型数」，有界，无需 GC；删账号由 `ON DELETE CASCADE` 清理。

**账号级 7 个字段一列不动**，语义不变，只是写入时机变少。这样 `/models` 聚合、算力池测试、恢复调度、导入导出、CPA 导出等读账号级字段的位置都不需要理解新概念。

### 1.1 模型键

新增 `model_state_key(platform, capability, kind, requested_model) -> String`：

- `api` 账号：取 `resolve_mapping_target`（`route_model_capability.rs:186`）解析出的上游 `to`。
- `official` 账号与空映射账号：取请求原名（`build_official_upstream_request` 从不改写 model）。
- 两者都再过一遍 `strip_one_m_suffix_for_route_lookup`（`:326`）。

因此 `claude-sonnet-alias[1m]` 与 `claude-sonnet-alias` 共享一条冷却记录——它们是同一个上游模型，只差一个 beta 头，且 `model_mapping_matches`（`:301`）本来就视二者相等。

用上游 `to` 而非请求侧 `from` 做键，带来天然收敛：catch-all 映射（`FALLBACK_MODEL_ALIAS`）下客户端发任意模型名都汇聚到同一个 `to`，一次失败就冷却到位，表也不会被客户端乱发的模型名撑大。

### 1.2 已知模型集合

新增 `known_upstream_models(platform, capability, kind) -> Vec<String>`：

- 空映射或 `official` 账号：返回 `default_client_models(platform)`（`route_model_capability.rs:255`）。
- 否则：非 fallback 条目的 `to` 去重集合；存在 catch-all 时加上它的 `to`。

两处用途：界面需要列出「还健康、可供暂停」的模型（它们没有表行），以及升级规则的分母。模型过滤已拦掉集合外的请求，所以请求产生的键一定落在集合内。

### 1.3 结构体

`RouteCredential` 新增 `#[sqlx(skip)]` 的 `model_states: Vec<RouteCredentialModelState>`，仿 `active_request_count`（`models/route_credential.rs:137`）由服务层在 `list` / `get` / `page` 之后批量填充。前端拿账号列表就顺带拿到明细，不需要额外 command，也不会出现列表与明细两个数据源不一致。

## 2. 失败分级与写入路径

`is_account_scoped_failure(kind, status) -> bool`，一处定义，全部记账点共用：

| `last_failure_kind` | 触发点 | 归属 |
|---|---|---|
| `refresh` | OAuth 刷新失败 `route_proxy_service.rs:692` | 账号 |
| `request_build` | 缺 api_key/base_url、dialect 非法 `:772` | 账号 |
| `transport` | 连不上、读响应中断、流式首字节前 `:897 :1139 :2006` | 账号 |
| `upstream_status` 401/403 | `:1428` | 账号 |
| `upstream_status` 其他（400/404/408/429/5xx） | `:1412` | 模型 |
| `response_transform` | `:1190` | 模型 |
| `semantic_response_transient` | `:1392 :2127` | 模型 |
| `model_test_status` 401/403 | `route_model_test_service.rs:1312` | 账号 |
| `model_test_status` 其他 | 同上 | 模型 |
| `model_test` | `:1345`（传输层失败） | 账号 |
| 配额耗尽 | `route_proxy_service.rs:1351` | 账号（配额是账号属性） |

拿不到请求模型名时（Gemini 把 model 放在路径里，以及其他不带 model 的路由），模型级降级为账号级：选号不查模型冷却，失败写账号级。保底不丢保护，且与 `filter_credentials_for_model`「无 model 就不过滤」的现有语义一致。

**写入规则。** 每次失败都照旧更新账号级 `transient_failure_count` 与 `last_failure_kind/message/response_json`；**只有账号级失败才额外写账号级冷却时间戳**（`next_retry_at` 与 `cooldown_until`，现有实现赋同值）。模型级失败把冷却写进模型行，同时递增模型行自己的 `transient_failure_count`——两级计数器都涨。这样「错误 N 次」徽章与失败悬停面板不改一行就继续正确——它们展示的是「这个账号最近一次失败」。

`cooldown_enabled` 默认 `false` 的语义不变：关着时两级都只累计次数、不写时间戳。

**接口形态。** 给 `record_transient_failure` 加 scope 参数，而非另起一套函数：

```rust
pub enum FailureScope<'a> {
    Account,
    Model { key: &'a str, siblings: &'a [String] },
}
```

`siblings` 由服务层用 `known_upstream_models` 算好传入——仓储层不应知道 `model_mappings` 怎么解析。

**升级判定**（siblings 全部处于冷却或非 `ok` → 顺带写账号级冷却）必须与模型行写入在**同一个事务**内，否则并发请求会各自看到「还没全冷」而都不升级。`record_transient_failure` 本来就已开事务（`:1349`），沿用。

**成功时的非对称清账。** 删除该模型的行（`paused` 的行只清失败字段、保留状态；`error` 的行整行删除——一次成功证明该模型可用），同时清空账号级冷却与计数——一次成功响应证明凭证与网络都好；但**绝不动其他模型的行**。

转发链路有两个成功清账点，都要传模型键：`route_proxy_service.rs:1446`（缓冲响应成功）与 `:2140`（流式正常结束）。流式上下文里 `requested_model` 已在作用域内（`:2035`）。

已知代价：`official` 账号与空映射账号的 siblings 是平台基线模型集合（codex 与 claude 各 4 个，gemini 与 grok 各 1 个），所以中转站或厂商整体故障时，codex/claude 账号需要 4 个模型各失败一次才升级到账号级。有上限，可接受。

## 3. 选号链路改造

拆分 `select_pool_credentials`（`route_proxy_service.rs:2532`）为三个函数：

```rust
pub struct PoolCandidate {
    pub credential: SelectedCredential,
    pub cooldown_until: Option<String>,   // 账号级，两个时间戳取较晚者
    pub model_key: Option<String>,        // 由 filter_candidates_for_model 填入
}

pub async fn load_pool_candidates(pool, platform) -> Result<Vec<PoolCandidate>>;

pub fn filter_candidates_for_rule(Vec<PoolCandidate>, rule: &CapabilityRule) -> Vec<PoolCandidate>;

// 过滤的同时为每个留下的候选算出 model_key（requested_model 为 None 时保持 None）
pub fn filter_candidates_for_model(platform, Vec<PoolCandidate>, Option<&str>) -> Vec<PoolCandidate>;

pub async fn load_model_states(pool, &[PoolCandidate])
    -> Result<HashMap<(String, String), RouteCredentialModelState>>;

pub fn partition_by_cooldown(
    Vec<PoolCandidate>,
    &HashMap<(String, String), RouteCredentialModelState>,
    now: DateTime<Utc>,
) -> Vec<SelectedCredential>;

// 保留：load + partition(空状态表)。供 route_model_test_service.rs:100 与 :409 使用。
pub async fn select_pool_credentials(pool, platform) -> Result<Vec<SelectedCredential>>;
```

`model_key` 挂在候选上而非另开一个平行数组，因为它是逐候选解析的（见下），与候选同生命周期。`load_model_states` 只查 `model_key` 为 `Some` 的候选，按 `(账号 id, model_key)` 对批量查询。

`load_pool_candidates` 只做现有 SQL 过滤与配额过滤（`:2536-2549` 与 `:2579`），不分桶。`partition_by_cooldown` 承接现有的 eligible/cooling 分桶与全冷却兜底（`:2582-2609`）。

现有 `filter_credentials_for_rule`（`:2612`）被 `filter_candidates_for_rule` 取代，它只有两个调用点（`:580` 的 `/models` 聚合与 `:619` 的转发），都改为候选形态，因此规则过滤仍只有一份实现。`/models` 路径变为 `load_pool_candidates` → `filter_candidates_for_rule` → `partition_by_cooldown`（状态表传空）→ 取出 credentials，与今天的可见行为一致。`route_model_test_service.rs` 里的 `filter_model_test_credentials`（`:1530`）继续作用于 `Vec<SelectedCredential>`，不动。

转发链路改为：

```rust
let candidates = load_pool_candidates(pool, &platform).await?;
let candidates = filter_candidates_for_rule(candidates, &routing_rule);      // 空 → "No enabled route credentials in pool"
let candidates = filter_candidates_for_model(&platform, candidates, model);  // 空 → route_pool.model_unmatched
let states = load_model_states(pool, &candidates).await?;
let credentials = partition_by_cooldown(candidates, &states, Utc::now());    // 空 → route_pool.model_unavailable
```

**顺序变更修掉一个现存缺陷。** 今天冷却分桶（`:2582`）发生在模型过滤（`:624`）之前。后果：A 号只支持 `glm-5.3` 且健康、B 号只支持 `gpt-5.6-sol` 但在冷却中，此时来一个 `gpt-5.6-sol` 请求——`select_pool_credentials` 见 eligible 非空直接返回 `[A]`，B 连兜底机会都没有；接着模型过滤把 A 也筛掉，请求以 `route_pool.model_unmatched` 失败。而 B 是唯一能服务它的号，本应走兜底探测。冷却按模型分之后，该不该判为冷却取决于请求哪个模型，不知道模型就无法分桶，所以这个顺序必须调整。

**`model_key` 逐候选计算。** 两个账号可以把同一个请求模型映射到不同的 `to`，所以键必须按候选各算一次，查询也按 `(账号 id, model_key)` 对进行，不能只按 `model_key`。`filter_candidates_for_model` 已在遍历每个候选的 capability 做匹配判断，键在同一次遍历里顺手算出，不额外多一趟解析。

**`partition_by_cooldown` 的判定顺序：**

1. 模型状态为 `paused` 或 `error` → 直接丢弃，不进任何桶、不参与兜底。
2. 账号级 `cooldown_until` 未过期，或模型级 `cooldown_until` 未过期 → 进 cooling 桶。
3. 否则进 eligible 桶。

eligible 非空则只用它；全在冷却则取最早恢复的 1 个探测（沿用 `:2600-2609`，「最早恢复」在两级时间戳取较晚者之后再比较）。

**新增错误码 `route_pool.model_unavailable`**，区别于 `route_pool.model_unmatched`：前者是「有账号支持这个模型，但都被暂停或判定异常」，后者是「没有账号映射这个模型」。两者处置不同（去解除暂停 vs 去加映射），合并会让排查变成猜谜。

**重试循环不动。** `credential_indexes_by_priority`、并发租约、同号重试、游标推进都作用在最终列表上，模型冷却只影响谁能进这个列表。

**`/models` 聚合不过滤暂停模型。** 暂停是用户刚做的临时动作，让客户端模型列表悄悄缩短反而更难排查；真发请求会得到 `route_pool.model_unavailable`，那个错误码本身就是答案。代价是目录与可用性短暂不一致。

## 4. 语义连击与升级规则

事实前提：`semantic_error_threshold` 至今**没有消费者**。`semantic_response_transient` 走的是 `record_transient_failure`（`:1392`），不动 `status`；唯一置 `status='error'` 的是配额耗尽那条（`:1353`，阈值硬编码 1）。`docs-site/docs/guide/reliability.md:117-124` 记录了这件事并留有维护者提醒。

因此本节不是「把已有的整账号升级改成按模型」，而是**给这个配置接上第一个消费者，且直接接在模型维度上**。

**为什么需要它。** 只有冷却的话，账号根本不支持的模型会永久 churn：冷却 10 秒 → 重试 → 同样报错 → 再冷却 10 秒。连击加阈值把「反复同样的失败」变成「别再试了」。指纹沿用 `semantic_failure_fingerprint`（`route_credential_repository.rs:1748`，状态码 + 归一化消息的 SHA-256）。

**与账号级的一处刻意分歧。** 账号级两个函数互斥：`record_transient_failure` 把 streak 归零（`:1394`），`record_semantic_failure_with_status` 把冷却清空（`:1487`）。互斥的后果是冷却中的模型 streak 永远攒不起来，阈值永远用不上。所以模型行**两者叠加**：每次模型级失败都写冷却，同时按指纹累计 streak；同指纹达到 `semantic_error_threshold` 就置 `status='error'`。账号级两个函数的现有互斥关系不动。

**升级分母排除 `paused`。** 第 2 节的「siblings 全部不可用则升级账号冷却」，分母只算非 `paused` 的模型——用户手动暂停 3 个模型，不该让第 4 个的一次失败伪造出「整账号坏了」的结论。推论：只映射一个模型的账号，行为与今天完全一致。

`error_status_enabled` 继续作为总开关，现在同时管两级的自动置异常。

## 5. 模型测试与恢复调度

模型测试的记账全在 `finish_outcome`（`route_model_test_service.rs:1278`），只有直连路径执行；算力池测试走 `finish_proxy_outcome`（`:1414`），零记账（代理已记过），不改。

**`finish_outcome` 三处改动：**

- 成功分支（`:1296-1300`）今天调 `clear_transient_failure` 清整账号。改为先算出本次测试的 `model_key`，按第 2 节的非对称清账规则处理。`should_restore_model_test_account_status`（`error|warning` → `ok`）与 `recover_after_explicit_test`（`:336-342`）都不动——那是账号状态复原，与模型无关。
- `:1312`（`model_test_status`）与 `:1321`（`semantic_response_transient`）传模型 scope。
- `:1345`（`model_test`，传输层失败）传账号 scope。`quota_failure` 与 `Permanent` 两条继续动账号 `status`。

**探测模型选取必须修正。** `request_model`（`:880`）在用户未填模型时取「第一条非 fallback 映射的 `from`」。冷却按模型分之后，这会让 Healthcheck 永远只探第一个模型，而恢复候选恰恰可能是因为第三个模型异常才进来的。改为：`RecoveryMode::Healthcheck`（`route_recovery_service.rs:130`）调用时，先挑该账号当前最需要探测的模型（有表行、非 `paused`、冷却已过期，取 `updated_at` 最早者），作为 `model` 显式传入 `RoutePoolModelTestRequest`；没有这样的行才回落到今天的逻辑。UI 手动测试的默认值不改。

**`needs_recovery`（`:166`）新增条件。** 今天是 `status != 'ok' || next_retry_at.is_some() || cooldown_until.is_some()`，只看账号级字段——账号级健康、只有某个模型异常的账号进不了恢复流程，会一直靠转发请求撞。加上「存在非 `paused` 的模型行」。`list_recovery_candidates`（`route_credential_repository.rs:1324`）用 `EXISTS` 子查询补一列即可，不必把明细捞出来。

**`Scheduled` 模式的 `reactivate_credential`（`:1560`）** 语义是「到点无条件复活」，扩展为同时清空该账号的全部非 `paused` 模型行。`paused` 是用户意志，定时任务不该推翻它。

**新增两个 command：** `clear_route_credential_model_state`（解除单个模型的冷却/异常：删除该行；若为 `paused` 则只清失败字段并保留暂停）与 `set_route_credential_model_status`（在 `ok` 与 `paused` 间切换；目标模型没有表行时按需建行，`ok` 意味着删行）。`error` 不是这个 command 的合法入参——它只由语义连击自动置位。界面上的「全部解除」复用 `clear_route_credential_model_state` 逐个调用，不新增第三个 command。

三处契约必须同步——`src/lib/api/client.ts`、`src-tauri/src/lib.rs` 的 `generate_handler![]`、`src-tauri/src/web/handlers/mod.rs`，否则 `tests/transport/command-contract.test.ts` 失败。

## 6. 前端界面

`AccountsScreen.tsx` 无 i18n、全中文硬编码，沿用该约定（不新建 i18n key）。

**后端下发形状。** `model_states` 包含该账号**全部已知模型**，不只有表行的：服务层用 `known_upstream_models` 取全集，与表行左连接，健康模型合成一条 `status: "ok"` 且无冷却的记录。这样抽屉能统一渲染（健康模型也要能被暂停），前端不需复刻映射解析逻辑。数量有界。

用户改过映射后可能留下集合外的孤儿行（例如暂停了某模型，之后把它从 `model_mappings` 删掉）。这些行照样下发，`aliases` 为空数组，界面按「已移除映射」渲染并只提供解除。不自动清理——静默删掉用户的暂停意图比留一条可见的孤儿行更糟，而解除入口就在旁边。

```ts
export type RouteCredentialModelState = {
  model_key: string;
  aliases: string[];              // 指向这个 to 的所有 from
  status: "ok" | "error" | "paused";
  transient_failure_count: number;
  cooldown_until?: string;
  last_failure_kind?: string;
  last_failure_message?: string;
};
```

`aliases` 解决用 `to` 做键带来的展示落差：界面显示上游真实名，用户配的是别名，需要让两者对得上。

**账号行新增一个徽章**，仅在有模型不可用时出现，橙色，与现有 `冷却 N 秒`（`:5380-5390`）同色系：

```
模型 2 不可用
```

计数 = 冷却未过期 + `error` + `paused` 三者之和。悬停展开逐模型明细（模型名、别名、原因、剩余时间、失败消息），复用 `CredentialFailureTooltip`（`:1817`）的 `pt-1` 悬停模式（`mt-1` 会在指针移动中途丢 `:hover`，已在 b6c9beb 踩过）。

只加一个徽章而非冷却/异常/暂停各一个：该行已有归档、状态、并发脉冲、映射摘要、账号冷却、订阅、主额度、周额度、重置等徽章。一个入口加悬停明细，行高不变。

现有 `冷却 N 秒` 徽章保留，现在明确表示**账号级**冷却。两个徽章语义不重叠：一个是「整号退避中」，一个是「部分模型不可用」。

**编辑抽屉新增 `模型状态` 区块**，放在现有 `失败处理策略`（`:6583`）之后——策略是配置，状态是运行时，相邻但分开。逐模型一行：名字 + 别名 + 状态 chip + 剩余时间 + 失败摘要，右侧两个动作：`暂停`/`恢复` 切换、`解除冷却`。区块头部一个 `全部解除`（清掉该账号所有非 `paused` 的模型行）。

**倒计时。** `useCooldownCountdown`（`:1237`）扩展为同时扫模型级 `cooldown_until`，仍是每秒一 tick，一个 ticker 覆盖两级。

**实时刷新。** 后端 `notify_status_change` 已在失败记账时调用（`:2299`），前端已订阅 `route-credential-status`（`:2504`）。模型状态变更（含手动暂停/解除）需补上该通知调用，前端零改动。

## 7. 测试策略

**端到端**（新增于 `route_proxy_service.rs` 测试模块，复用现有假上游套路 `:5304` 起的 `TcpListener::bind(("127.0.0.1", 0))` + `axum::serve`）：

假上游按 `body.model` 分流，`gpt-5.6-sol` 返 429、`glm-5.3` 返 200；一个账号同时映射两者，`cooldown_enabled: true`。真的过 `forward_request`：

1. 发 `gpt-5.6-sol` → 失败，模型行写入冷却。
2. 立刻发 `glm-5.3` → **仍然 200**（今天此处会失败，是本次回归的核心断言）。
3. 立刻再发 `gpt-5.6-sol` → 走全冷却兜底（只有一个账号），断言仍打到上游而非直接报错。
4. 加入第二个只映射 `gpt-5.6-sol` 的健康账号，重发 → 命中新账号，冷却中的被跳过。

补一个升级测试：两个模型都失败后，账号级 `cooldown_until` 被写入。

**分层单测：**

- 仓储层（仿 `route_credential_repository.rs:2594` 现有风格）：模型行写入/删除、`paused` 成功后保留状态、语义连击达阈值置 `error`、升级分母排除 `paused`、`ON DELETE CASCADE`。
- `partition_by_cooldown` 纯函数：三条判定的优先级、`paused`/`error` 不参与兜底、两级时间戳取较晚者。
- `model_state_key`：api 取 `to`、official 取原名、`[1m]` 归一化、catch-all 收敛到同一个键。
- `known_upstream_models`：空映射→基线、catch-all 的 `to` 计入。
- `finish_outcome`：成功只清本模型、`model_test_status` 401 归账号 / 429 归模型。
- 恢复调度：`needs_recovery` 认出「账号健康但有模型异常」、Healthcheck 探测挑 `updated_at` 最早的模型、`Scheduled` 复活不动 `paused`。
- `tests/AccountsScreen.test.tsx`：徽章计数与文案、悬停明细、抽屉逐模型渲染、暂停/解除/全部解除的 mutation 载荷、`revoked` 账号不显示模型徽章。
- `tests/transport/command-contract.test.ts`：两个新 command 三处同步（该测试自动守）。

**命令**（AGENTS.md 要求 AI 用 `target-codex`）：

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test route_credential_model
pnpm vitest run tests/AccountsScreen.test.tsx
pnpm typecheck
```

**手工验收清单：**

1. 账号配两个模型映射、开启失败冷却，用「测试连接」分别测两个模型，确认互不影响。
2. 算力池测试确认走代理路径后模型冷却仍生效。
3. 抽屉里暂停一个模型，确认客户端发它得到 `route_pool.model_unavailable`、发另一个正常。

## 8. 文档同步

- `docs-site/docs/guide/reliability.md`：与代码逐字对应的规格文档，冷却规则、字段表、失败分类表均需同步；删掉 `:117-124` 那条「`semantic_error_threshold` 没有消费者」的维护者警告——它现在有了。
- `docs-site/docs/en/guide/reliability.md`：英文镜像同步，包括同位置的警告删除。
- `docs-site/docs/guide/accounts.md:100-103`：调度四步说明更新。英文镜像同步。
