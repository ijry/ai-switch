---
title: 稳定性与自动恢复
description: AI Switch 的失败分类、指数退避与冷却窗口的精确规则，账号如何自动恢复进算力池，以及定时恢复与健康探测两种恢复模式怎么配。
---

# 稳定性与自动恢复

多号池的价值不在"号多"，而在**某个号出问题时流量能自己绕过去，问题过去之后号能自己回来**。这一页把 AI Switch 的失败判定、退避时长、冷却窗口和自动恢复规则逐条写清楚，所有数字都来自代码，不是建议值。

## 数据库里的失败状态

失败状态记在 `route_credentials` 表的一组列上，由三个迁移逐步补齐：

```sql
-- 202607300001_route_credential_retry.sql
ALTER TABLE route_credentials ADD COLUMN transient_failure_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE route_credentials ADD COLUMN next_retry_at TEXT;
ALTER TABLE route_credentials ADD COLUMN cooldown_until TEXT;
ALTER TABLE route_credentials ADD COLUMN last_failure_kind TEXT;
ALTER TABLE route_credentials ADD COLUMN last_failure_message TEXT;

-- 202608080001_route_credential_failure_response.sql
ALTER TABLE route_credentials ADD COLUMN last_failure_response_json TEXT;

-- 202608130001_route_credential_semantic_failure_streak.sql
ALTER TABLE route_credentials ADD COLUMN semantic_failure_streak_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE route_credentials ADD COLUMN semantic_failure_streak_fingerprint TEXT;
```

| 列 | 作用 |
| --- | --- |
| `transient_failure_count` | 连续瞬时失败次数，决定退避档位 |
| `next_retry_at` | 下次可以再试的时间点 |
| `cooldown_until` | 冷却截止时间点 |
| `last_failure_kind` | 失败分类标签，见下文 |
| `last_failure_message` | 失败信息，截断保存 |
| `last_failure_response_json` | 上游原始失败响应，最多 **8192** 字符，超出部分以 `…` 结尾 |
| `semantic_failure_streak_count` / `_fingerprint` | 同一种语义失败的连击次数与其指纹 |

## 三类失败

代理拿到上游结果之后，先分类再决定怎么处理。分类函数只有三种返回值：

```rust
pub enum ProxyFailureKind {
    Transient,
    Permanent,
    None,
}
```

### 永久性失败（Permanent）

判定完全基于错误文本，命中以下任一子串即为永久失败：

- `invalid_grant`
- `refresh token has been revoked`
- `token has been revoked`
- `官方 oauth 凭证已失效`

这类失败意味着凭据本身已经作废，重试没有意义。账号状态**直接写 `revoked`**，不设退避窗口——因为它不会自己好起来。

### 瞬时失败（Transient）

以下情形算瞬时失败：

- HTTP 状态码为 **408 请求超时**、**401 未授权**、**403 禁止**、**429 请求过多**
- 任意 **5xx** 服务端错误
- **没有 HTTP 状态码**（连接失败、DNS 失败、超时、TLS 错误等传输层问题）

401/403 被算作瞬时失败，是因为第三方网关经常用它们表达"这个 key 临时被限流/风控"，而不一定是"key 已废"。真正的凭据作废由上面的永久性判定捕获。

### 不算失败（None）

有 HTTP 状态码，但既不在可重试列表里、也不是 5xx——例如 400 参数错误、404 路径不存在。这类是**客户端请求本身的问题**，跟账号健康无关，所以不累加失败计数，账号状态不动。

::: tip 为什么要区分
如果把 400 也算成账号失败，一个写错模型名的客户端能在几十秒内把整个池子打成全冷却。区分之后，参数错误只会返回给客户端，池子毫发无伤。
:::

## 语义失败：HTTP 200 但内容是失败

第三方网关有个常见毛病：额度耗尽、上游报错、内容审核拦截，返回的却是 **HTTP 200**，失败信息藏在响应体里。只看状态码会把这些请求当成功，账号也就永远不会被退避。

所以还有一层语义失败检测，直接解析响应体判断"这实际上是一次失败"。两种触发方式：

1. **响应体结构上是失败**（由响应体失败检测服务判定）。
2. **流式请求在完成事件之前断流**——SSE 流断在半路，没有收到终止事件。

语义失败在统计事件里会被标记成失败（`metadata_json.success = false`），并按下面的规则处理账号状态。

### 配额耗尽走单独通道

语义失败如果被进一步识别为**配额耗尽**，处理方式和普通语义失败不同：它会以阈值 **1** 记一次语义失败——也就是**一次就把状态翻成 `error`**，不给连击机会。理由很直接：配额耗尽是确定性事实，重试只是浪费时间。

同时这条通道会清空 `transient_failure_count`、`next_retry_at` 和 `cooldown_until`：账号不是"稍后重试"，而是"这个周期内别再用了"。

### 指纹连击机制

语义失败的连击计数不是简单加一，而是**按指纹匹配**才加：

```rust
fn semantic_failure_fingerprint(response_status: Option<u16>, message: &str) -> String {
    // sha256("semantic_response_failed|{status 或 none}|{空白归一化后的小写消息}")
}
```

消息先按空白切分再用单个空格拼回、转小写，然后连同状态码算 SHA-256。规则是：

- **指纹和上次相同** → 连击计数加一（上限为阈值）
- **指纹不同** → 连击计数重置为 1，并记录新指纹
- 连击计数**达到阈值**时状态翻成 `error`
- 状态已经是 `revoked` 或 `paused` 的账号**不会被这条规则改动**

指纹的意义在于区分"同一个病反复发作"和"每次都是不同的偶发错误"。前者说明账号真的坏了，后者更可能是上游抖动。

<!-- 维护提示：下面这段描述的是「当前行为」，不是永久性质。
     如果有人把 semantic_error_threshold 接进了转发路径或模型测试路径，这个 warning 就变成错的，需要删掉或重写。
     判断方法：grep semantic_error_threshold，若在 route_proxy_service.rs / route_model_test_service.rs
     的非测试代码里出现，说明已接线。英文版 en/guide/reliability.md 同一位置需同步修改。 -->

::: warning 阈值当前不可调
账号的失败策略里有一个 `semantic_error_threshold` 字段（默认 10，可填 1–1000），创建/更新账号时会做范围校验并持久化。但在当前代码里，**转发路径与模型测试路径都没有读取这个值**：唯一使用连击机制的地方是配额耗尽通道，它把阈值硬编码为 1。其余语义失败走的是下面的瞬时失败退避，不经过连击计数。所以这个字段目前只是被保存下来，不影响实际行为。
:::

## 退避与冷却的精确规则

瞬时失败的处理是一段很短的代码，值得原样看：

```rust
let failure_count = current.saturating_add(1);
let base_seconds = match failure_count {
    1 => 30,
    2 => 120,
    _ => 600,
};
let jitter_seconds = jitter_seconds(id, failure_count, base_seconds);
let retry_at = Utc::now() + chrono::Duration::seconds(jitter_seconds);
let cooldown_until = if failure_count >= 3 { Some(retry_at.clone()) } else { None };
```

| 连续第几次瞬时失败 | 基础退避 | 是否设冷却 |
| --- | --- | --- |
| 第 1 次 | 30 秒 | 否 |
| 第 2 次 | 120 秒（2 分钟） | 否 |
| 第 3 次及以后 | 600 秒（10 分钟） | **是**，冷却截止时间等于 `next_retry_at` |

三点要注意：

- **退避阶梯到第 3 次就到顶了**，不会无限翻倍。持续坏下去也就是每 10 分钟被试一次。
- **前两次只设 `next_retry_at`，不设 `cooldown_until`。** 两个字段在调度时的效果一样（都要求已经过期才可用），但只有 `cooldown_until` 会被界面呈现成"冷却中"。
- **每次瞬时失败都会清空语义连击计数**（`semantic_failure_streak_count = 0`、指纹置空），两套计数互不叠加。

### 抖动是确定性的

退避时长不是精确的 30/120/600 秒，而是乘上一个 80%–120% 的系数：

```rust
fn jitter_seconds(id: &str, failure_count: i64, base_seconds: i64) -> i64 {
    let seed = id.bytes().fold(failure_count as u64, |value, byte| {
        value.wrapping_mul(31).wrapping_add(byte as u64)
    });
    let jitter_percent = 80 + (seed % 41) as i64;
    (base_seconds * jitter_percent / 100).max(1)
}
```

种子由**账号 ID 加失败次数**算出，不用随机数。这带来两个性质：

- **不同账号的退避时长不同**，即使它们在同一秒一起失败。这避免了"整池账号同时冷却、同时解冻"的惊群效应——上游被瞬间打满的可能性小了很多。
- **同一账号同一档位的退避时长是可复现的**，方便排查与写测试。

实际范围：第 1 次 24–36 秒，第 2 次 96–144 秒，第 3 次及以后 480–720 秒。

## 失败分类标签

`last_failure_kind` 记的是失败发生在链路哪一步，界面上和排错时都用得到：

| 标签 | 触发时机 |
| --- | --- |
| `refresh` | 官方凭据刷新 access token 失败（且被判为瞬时） |
| `request_build` | 构造上游请求失败（桥接、鉴权装配等环节） |
| `transport` | 请求发不出去（连接、DNS、TLS、超时） |
| `response_read` | 请求发出了，但读不完响应体 |
| `response_transform` | 上游有响应，但桥接的反向转换失败 |
| `upstream_status` | 上游返回可重试的非 2xx 状态码 |
| `semantic_response_transient` | 语义失败（走瞬时退避这条路时） |
| `semantic_response_failed` | 语义失败（走指纹连击那条路时，即配额耗尽通道） |
| `model_test_status` | 模型测试收到非 2xx 状态码 |
| `model_test` | 模型测试的其他可重试失败 |

## 同号重试还是换号

失败之后不一定立刻换号。转发逻辑维护一个重试队列，规则是：

1. 读取账号的失败策略（`config_json.failure_policy`），拿到 `retry_count` 与 `retry_interval_ms`。
2. 如果这个账号的重试次数还没用完，**等待 `retry_interval_ms` 后把它塞回队列头部**，也就是同号再试一次；这一次不写失败计数。
3. 重试次数用完了，才记一次瞬时失败并顺延到下一个候选账号。

**401 / 403 是例外：永不同号重试。** 判定就一行：

```rust
pub(crate) fn should_retry_same_credential_status(status: StatusCode) -> bool {
    !status.is_success() && !matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
}
```

鉴权失败重试只会更快触发上游风控，所以直接记失败、换号。

### 失败策略的边界

| 字段 | 默认值 | 上限 |
| --- | --- | --- |
| `retry_count` | 2 | 10 |
| `retry_interval_ms` | 200 | 60000 |
| `semantic_error_threshold` | 10 | 1000（下限 1，填 0 被拒） |

只写部分字段时，其余字段各自回落到默认值。整个 `failure_policy` 都不写就全用默认值。超范围的值在创建/更新账号时返回 `validation.route_credential_failure_policy`。

## 冷却中的账号怎么被跳过

调度时的可用判定要求 `next_retry_at` **和** `cooldown_until` 都已经过期（为空视为已过期）：

```rust
pub fn credential_is_retryable_now(
    next_retry_at: Option<&str>,
    cooldown_until: Option<&str>,
    now: DateTime<Utc>,
) -> bool { /* 两个时间戳都必须 <= now */ }
```

选号 SQL 本身也已经把 `status = 'ok'` 之外的账号排除在外，所以被翻成 `error` 或 `revoked` 的账号根本不会进入候选集。

### 全池冷却时的兜底

如果筛完发现**一个可用账号都不剩**，调度器不会直接让请求失败，而是**挑出最快恢复的那一个冷却账号**去试：

```rust
cooling.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
Ok(cooling.into_iter().take(1).map(|(_, _, credential)| credential).collect())
```

排序键是"恢复时间最早"，同时间则按原顺序稳定排序。这是一个有意的取舍：宁可用一个可能还在冷却的号试一次，也不要在客户端侧直接报错——毕竟退避时长本身是估算，上游可能已经恢复了。

## 成功会清空什么

一次成功的转发会调用清空逻辑，把这些字段一次性归零：

```sql
UPDATE route_credentials
SET transient_failure_count = 0, next_retry_at = NULL, cooldown_until = NULL,
    semantic_failure_streak_count = 0, semantic_failure_streak_fingerprint = NULL,
    last_failure_kind = NULL, last_failure_message = NULL,
    last_failure_response_json = NULL, updated_at = ?
WHERE id = ?
```

注意这一步**不改 `status`**，只清失败痕迹。因为能被选中转发的账号本来就是 `ok`。

模型测试成功时多做一步：如果账号当前是 `error` 或 `warning`，把状态拉回 `ok`；针对单个账号的显式测试还会额外执行完整恢复（见下节）。

## 自动恢复

失败的账号靠什么回到池子里？两条路：

1. **退避窗口自然过期**——`next_retry_at` / `cooldown_until` 过去之后，账号自动重新成为候选。这条路不需要任何配置，但只适用于状态仍是 `ok` 的账号。
2. **自动恢复调度器**——针对已经被翻成 `error`、`warning` 或被手工 `paused` 的账号。状态不是 `ok` 的账号不会被选号 SQL 选中，光等时间是回不来的，必须有人把状态改回去。

### 三种恢复模式

恢复规则按账号配置，存在 `config_json.recovery` 里：

```rust
pub enum RecoveryMode {
    #[default]
    Off,
    Scheduled,
    Healthcheck,
}
```

| 模式 | 行为 |
| --- | --- |
| `off` | 不做任何自动恢复（默认）。设成 `off` 会把 `recovery` 键整个从配置里删掉 |
| `scheduled` | 按每天的固定时刻，无条件把账号重新激活 |
| `healthcheck` | 按固定间隔跑一次真实的模型连通性测试，**测通了才恢复** |

调度器每 **30 秒**跑一个 tick（`RECOVERY_TICK_SECONDS = 30`），遍历所有未归档的账号（不分平台），对每个账号判断"是否需要恢复"再按其模式处理。

### 需要恢复的判定

```rust
fn needs_recovery(status: &str, next_retry_at: Option<&str>, cooldown_until: Option<&str>) -> bool {
    if status == "revoked" {
        return false;
    }
    status != "ok" || next_retry_at.is_some() || cooldown_until.is_some()
}
```

- `revoked` 的账号**永不参与自动恢复**。重新激活的 SQL 也带 `WHERE id = ? AND status != 'revoked'`，双重保险。
- 状态不是 `ok`，或者身上还挂着任何一个退避/冷却时间戳，都算"需要恢复"。注意即使状态是 `ok`，只要还带着退避窗口就会被恢复流程清掉。

### 定时恢复（`scheduled`）

- 时刻用 `HH:MM` 表示，按**本地时区**判定。
- 保存时会规范化：补零成两位、去重、排序。`3:00` 和 `03:00` 视为同一个时刻。
- **至少要有一个时刻**，否则返回 `validation.recovery_times_required`。
- 格式非法返回 `validation.recovery_times`。

触发判定是"某个时刻是否落在上一 tick 与本次 tick 之间"这个左开右闭区间内，并且逐日枚举日期。这样两种边界情况都能正确处理：

- **跨零点**：上一 tick 在 23:59:50、本次在次日 00:00:20，配了 `00:00` 会正常触发。
- **机器睡了好几天**：上一 tick 在 8 月 11 日 16:00、本次在 8 月 13 日 10:00，配了 `15:00` 也会触发（一次，不是补齐每一天）。

触发之后执行的是**无条件重新激活**：状态写回 `ok`，清空所有失败计数、退避窗口、连击计数和失败详情。这条路不验证账号是否真的可用——它假定"过了一晚上，限流应该解了"。

### 健康探测（`healthcheck`）

- 探测间隔以分钟为单位，默认 **30**，合法范围 **1–1440**（一天）。超范围返回 `validation.recovery_probe_interval`。
- 探测上次执行时间记在内存里，按账号 ID 分别计时；应用重启后重新开始计时。
- 到点后执行的是**针对该账号的显式模型连通性测试**。测试成功会通过"显式测试恢复"把账号完整恢复；测试失败则什么都不改，等下一个间隔再试。

也就是说 healthcheck 模式恢复账号的唯一途径是**真的打通一次生成请求**。它比定时恢复更可信，代价是每次探测都会消耗一点配额，并且会在统计里留下一条请求事件。

### 两种模式怎么选

| 场景 | 建议 |
| --- | --- |
| 上游按自然日重置额度 | `scheduled`，把时刻设在重置之后一小会儿 |
| 上游限流窗口不确定 | `healthcheck`，间隔 15–30 分钟 |
| 号很多、不想每个都探测消耗配额 | 主力号用 `healthcheck`，备用号用 `scheduled` |
| 想完全手工控制 | `off`，需要时手动点一次测试 |

::: tip 手动测试就是最快的恢复手段
针对单个账号点一次模型连通性测试，成功即执行完整恢复。`paused` 的账号也可以被测试——代码里明确注释了这一点：显式测一次正是用户判断暂停中的账号是否已恢复的方式。
:::

### 配置写坏了会怎样

如果账号的 `config_json` 不是合法 JSON 对象，设置恢复规则会返回 `validation.recovery_config_json`，并且**不会覆盖原有配置**。这是有意的：宁可拒绝写入，也不要把用户手工编辑过的配置冲掉。

读取侧则很宽容：解析不出 `recovery` 键、或者内容不合规范，都回落到 `off`，不会让一条坏配置把整个恢复循环打断。

## 一次故障的完整时间线

假设某个主力号的上游开始返回 429：

```text
T+0s     第 1 次请求 429 → 同号重试（间隔 200 ms）→ 仍 429
         → 重试次数用尽 → 记瞬时失败 #1
         → next_retry_at = T+24s ~ T+36s（无冷却）
         → 换到同优先级组的下一个号，客户端正常拿到响应

T+30s    退避过期，主力号重新进入候选
         → 又 429 → 记瞬时失败 #2 → next_retry_at = T+126s ~ T+174s

T+150s   再试 → 又 429 → 记瞬时失败 #3
         → next_retry_at = cooldown_until = T+630s ~ T+870s
         → 界面显示「冷却中」

T+700s   冷却过期，再试 → 这次成功
         → 清空 transient_failure_count / next_retry_at / cooldown_until
         → 主力号完全恢复，流量回到它身上
```

整个过程里客户端**没有感知到任何失败**——每一次退避都伴随一次换号，只要池里还有别的可用号。这就是多号池的意义。

## 下一步

- [账号与算力池](/guide/accounts)：状态机、优先级与并发上限
- [模型连通性测试](/guide/model-test)：手动恢复与健康探测背后的同一套测试逻辑
- [用量与请求统计](/guide/usage-stats)：失败的请求也会被记录，怎么查
- [协议路由与桥接](/guide/protocol-routing)：失败发生在转发链路的哪一步
