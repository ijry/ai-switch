# 算力池模型清单：聚合模式与精确模式

2026-09-06

## 问题

池对外只有一种模型词表：`advertised_model_catalog_entries` 把池内所有账号的别名去重合并成一份，客户端选 `gpt-5.6-sol`，代理按游标在池内轮换。这是池的价值所在，也意味着用户无法说「这一轮走某个账号」——排查某家中转站是否掉线、对比两家质量、把长上下文任务钉在窗口更大的那家，今天只能靠临时把其他账号踢出池子。

## 目标形态

每个平台（智能体标签页）各有一个模式，默认聚合：

- **聚合模式**：今天的行为，输出一字不改。
- **精确模式**：清单按账号展开。每个 **API 类型**账号出一组 `账号前缀/别名`，请求钉死在该账号；**所有官方账号合并为一份**，统一挂在保留前缀 `official/` 下，继续在官方账号之间轮换。

官方账号不逐个展开有两个理由：它们的请求体不套模型映射（`build_official_upstream_request` 从不改写 `model`），所以每个官方账号能服务的模型集合完全相同，逐个展开只是 N 份一模一样的噪音；而池化官方账号的全部意义就是配额轮换，钉住一个正好把它抵消掉。

代理在**任何模式下都接受**带前缀的模型名，所以切模式不会让已写出去的配置失效，客户端里手打前缀名也能用。

## 非目标

- 不做会话粘性路由，沿用 `2026-09-05-route-proxy-thinking-signature-recovery-design.md` 的结论。
- Claude Code 的四个 `/model` 槽位不变：槽位写的是单个别名而非清单，精确模式无从下手，它继续走轮换。
- gemini 原生路径把模型放在 URL 里（`/v1beta/models/{model}:generateContent`），本次不解析前缀，文档写明。
- 不给账号加第二把 key，也不加「选账号」请求头——聊天类客户端和 CLI 都改不了请求头，模型选择器里也看不见。

## 1. 模式存在哪里

新表，migration `202609060001_route_pool_model_modes.sql`：

```sql
CREATE TABLE IF NOT EXISTS route_pool_model_modes (
  platform TEXT PRIMARY KEY,
  mode TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

不放 `settings.json`，尽管「写入哪些客户端」在那里：`/v1/models` 必须知道模式，而代理全链路只拿得到 `SqlitePool`——`RouteProxyService::start` 的每个调用点（含 `route_proxy_https_service` 里的十余处）都没有 `AppPaths`，为一个开关把它穿进去不划算。

`RoutePoolRepository::model_mode` / `save_model_mode`，缺行时返回 `Aggregate`，无法识别的字符串也返回 `Aggregate`：一个开关坏掉不该让池不可用。模式顺带挂到已有的 `RoutePoolState.model_mode` 上，弹窗读现成的 `routePoolQuery`，不新增读命令。写用新命令 `set_route_pool_model_mode(platform, mode) -> RoutePoolState`，Tauri `invoke_handler` 与 `web/handlers` 派发表都要注册。

## 2. 前缀怎么算

新模块 `src-tauri/src/services/route_pool_model_mode.rs`，独立于目录生成与代理，便于单测：

- `PoolModelMode`：`Aggregate` / `Precise`，`parse` 宽容、`as_str` 稳定。
- `OFFICIAL_MODEL_PREFIX = "official"`。
- `account_model_prefix(display_name, credential_id)`：去首尾空白；非「Unicode 字母 / 数字 / `.` / `_` / `-`」的字符（含空格、`/`、`:`）一律换 `-`；压掉连续 `-`；去掉首尾 `-`；截到 32 字符后再去尾 `-`。中文保留。结果为空时用 `acct-{id 前 8 位}`。
- 结果（忽略大小写）等于 `official` 时强制加 `-{id 前 6 位}`，保留前缀不能被账号名顶掉。
- `assign_member_prefixes(members)`：同名（忽略大小写）时**两边都加** `-{id 前 6 位}`，不是只给第二个加。前缀因此是单账号的纯函数，与遍历顺序无关，也与「谁此刻健康」无关——否则目录写入时算出的后缀，会在某个账号变 `error` 之后与代理侧算出的不一致。
- `accepted_prefixes(display_name, credential_id)`：代理侧接受的集合。同时接受 `base` 与 `base-{短码}`，所以目录当时加没加后缀都能解回来；被保留字挤掉的账号只接受带后缀那个。

同名账号都用裸 `base` 请求时会匹配到两个候选，于是在这两个同名账号之间轮换。这是对歧义输入的定义好的行为，不是错误。

## 3. 目录生成

`route_model_capability.rs` 的入口从「一串 `ModelCapability`」改为「一串成员」加模式：

```rust
pub(crate) struct ModelCatalogMember {
    pub(crate) kind: String,      // "api" | "official"
    pub(crate) prefix: String,    // 精确模式前缀；官方成员固定 "official"
    pub(crate) capability: ModelCapability,
}

pub(crate) fn catalog_members(inputs: &[CatalogMemberInput<'_>]) -> Vec<ModelCatalogMember>;
pub(crate) fn advertised_model_catalog_entries(
    platform: &str,
    members: &[ModelCatalogMember],
    mode: PoolModelMode,
) -> Vec<AdvertisedModel>;
```

聚合模式完全忽略 `kind` 与 `prefix`，输出与今天逐字节一致——现有断言全部保留，是这次改动的回归网。

精确模式沿用现有两趟结构（基准趟、映射趟）与 `push_unique_model` 的去重合并，只把每条 id 换成 `{prefix}/{id}`：

- API 成员：空映射或带兜底映射的账号照旧贡献平台基准模型，只是带上自己的前缀；非兜底映射逐条出 `前缀/from`；Claude 的 `supports_1m` 孪生条目变成 `前缀/别名[1m]`。因为 id 带账号前缀，跨账号不再合并——上下文窗口与 Codex 推理档位就是该账号自己声明的，不再取最大值、取交集，这比聚合模式更准。
- 官方成员：先按 `is_synthetic_route_alias` 剔掉合成别名（`claude-model`、`claude-subagent`），与 `filter_candidates_for_model` 对官方账号的处理一致，advertise 的东西才等于 routing 真接受的东西；剩下的以 `official` 为前缀贡献，多个官方账号的 id 天然相同，于是被现有去重合并成一份。

`advertised_model_ids` 与 `codex_model_catalog_payload` 跟着改签名。四个调用点都拿得到 `kind`/`display_name`/`id`：`route_proxy_service` 的 `/v1/models`（`SelectedCredential`）、`route_config_service` 的 `resolve_client_models` 与 `write_codex_model_catalog`、`agent_launch_service::platform_models`（都是 `RouteCredential`）。

## 4. 代理选路

在 `forward_request` 里，`filter_candidates_for_rule` 之后、`filter_candidates_for_model` 之前插一步：

```rust
fn resolve_account_scoped_model(
    platform: &str,
    candidates: Vec<PoolCandidate>,
    requested_model: Option<String>,
    body: Vec<u8>,
) -> (Vec<PoolCandidate>, Option<String>, Vec<u8>)
```

1. 模型名按**第一个** `/` 拆成 `(prefix, rest)`，`rest` 为空则原样返回。
2. `prefix` 等于 `official`（忽略大小写）→ 候选只留 `kind == "official"`；否则在 `kind == "api"` 的候选里按 `accepted_prefixes` 匹配。
3. 命中集合为空 → 原样返回，整串继续当模型名。
4. 命中集合里没有一个支持 `rest`（用与 `filter_candidates_for_model` 共享的 `candidate_capability`，官方成员同样剔合成别名）→ 原样返回。
5. 否则：候选集换成命中集合，请求模型名换成 `rest`，并把请求体里所有等于原前缀名的 `model` 值递归改写成 `rest`（与 `rewrite_model_value` 同形状的遍历）。

**顺序是两个安全性的关键。** 带兜底映射的账号接受任何模型名，若先试整串，`Grox/gpt-5.6-sol` 会被它劫走并原样发给上游；反过来，中转站常见的 `z-ai/glm-5.3` 这类带厂商路径的真实模型名，靠第 3、4 步「解不出就回退整串」救回来。

**裸名化必须发生在这里**，因为官方账号那条路根本不套映射，带前缀的名字会直达厂商拿 404。改写之后，`model_state_key`、每模型冷却、失败归因（`FailureScope::Model`）、用量统计全部照旧：精确模式与聚合模式记在同一个 `(账号, 上游模型)` 账上，不会把冷却状态劈成两半。

`[1m]` 后缀不受影响：`TaBiAI/claude-opus-alias[1m]` 拆出的 `rest` 仍带后缀，交给现有逻辑。

`/v1/models` 从库里读模式后走同一套目录函数；它的候选集仍是 `partition_by_cooldown(candidates, &HashMap::new(), now)`，即不按每模型状态过滤——沿用 `reliability.md` 里已记录的理由。

## 5. 界面

`ConfigWriteTargetsDialog` 在 tab 之上加一个两档分段控件（`role="radiogroup"`，两个 `role="radio"`），因为它同时影响「其他 Agent」那一栏客户端拉到的 `/v1/models`：

- 聚合模式：同名模型合并成一条，请求在池内轮换。
- 精确模式：每个 API 账号各出 `账号/模型`，请求钉在该账号；官方账号合并为 `official/模型` 继续轮换。

切换即存（自己的 mutation，成功后 invalidate `route-pool` 与 `route-config-stale`），不等「写入」。理由：它是池级设置而不是写入参数，`/v1/models` 要立刻生效；而已有的「配置已过期」提示会随即亮起，正好把用户推去重写客户端配置。写入本身不新增参数，`resolve_client_models` 从库里读。

## 6. 测试

Rust：

- `route_pool_model_mode`：安全化（空格/`/`/`:`、中文保留、截断）、空名回退、保留字 `official` 被挤走、同名两边都加后缀、`accepted_prefixes` 两种写法都收。
- 目录：聚合模式现有断言全绿；精确模式 API 逐账号带前缀且跨账号不合并、官方合并成一份 `official/*`、`[1m]` 孪生带前缀、Codex 窗口取该账号声明值。
- 选路：钉到指定 API 账号；`official/` 只落官方；未知前缀回退；`z-ai/glm-5.3` 在「无同名账号」与「有同名账号但不支持余下别名」两种情形都回退整串；带兜底映射的账号不能劫走 `Grox/gpt-5.6-sol`；请求体嵌套 `model` 一并改写；官方账号收到的是裸名。
- 仓储：默认 `aggregate`、脏值回落、set/get 往返。
- 写入：精确模式下 ZCode 配置与 Codex 目录里都是带前缀的 id。

前端：扩 `tests/ConfigWriteTargetsDialog.test.tsx`（控件渲染、当前模式回显、切换回调），`tests/AccountsScreen.test.tsx` 补一条切换后落到 `set_route_pool_model_mode`。

命令：`pnpm typecheck`、`pnpm test:run`、`CARGO_TARGET_DIR=target-codex cargo test`（工作目录 `src-tauri`）。

## 7. 文档同步

`docs-site/docs/guide/protocol-routing.md`、`quick-start.md` 及 `docs-site/docs/en/` 的对应两篇：模型清单一节补精确模式，写明 `official/` 保留前缀、账号改名后需重写配置、gemini 原生路径不支持前缀。
