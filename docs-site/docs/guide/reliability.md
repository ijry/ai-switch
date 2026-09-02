---
title: 稳定性与自动恢复
description: AI Switch 的失败分类、账号级与模型级两层冷却的精确规则，账号与单个模型如何自动恢复进算力池，以及定时恢复与健康探测两种恢复模式怎么配。
---

# 稳定性与自动恢复

多号池的价值不在"号多"，而在**某个号出问题时流量能自己绕过去，问题过去之后号能自己回来**。这一页把 AI Switch 的失败判定、退避时长、冷却窗口和自动恢复规则逐条写清楚，所有数字都来自代码，不是建议值。

冷却有两层：**模型级**是默认，**账号级**是升级。中转站常常只对某一个模型限流，把整个账号打成冷却会误伤同一个号上健康的其他模型，所以默认只冷却触发失败的那个 `(账号, 模型)` 对。

## 数据库里的失败状态

失败状态分两层记录：**账号级**在 `route_credentials` 表的一组列上，**模型级**在 `route_credential_models` 表里，一行对应一个 `(账号, 模型)` 对。一个账号常常通过 `config_json.model_mappings` 同时支持多个模型，而中转站往往只对其中一个限流，所以两层必须分开记。

### 账号级：`route_credentials` 的失败列

这组列由三个迁移逐步补齐：

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
| `transient_failure_count` | 连续瞬时失败次数，界面据此显示 `错误 N 次` |
| `next_retry_at` | 下次可以再试的时间点 |
| `cooldown_until` | 冷却截止时间点 |
| `last_failure_kind` | 失败分类标签，见下文 |
| `last_failure_message` | 失败信息，截断保存 |
| `last_failure_response_json` | 上游原始失败响应，最多 **8192** 字符，超出部分以 `…` 结尾 |
| `semantic_failure_streak_count` / `_fingerprint` | 同一种语义失败的连击次数与其指纹 |

这些列现在明确表示**账号级**状态：只有被判为账号级的失败才会写它们的冷却时间戳（见下文的失败归属表）。

### 模型级：`route_credential_models`

```sql
-- 202609020002_route_credential_models.sql
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
```

| 列 | 作用 |
| --- | --- |
| `route_credential_id` | 所属账号，随账号删除级联清理 |
| `model_key` | **上游模型名**，即映射的 `to`；与账号 ID 共同构成主键 |
| `status` | `ok` / `error`（语义连击自动置位） / `paused`（只由用户设置） |
| `transient_failure_count` | 这个模型自己的连续失败次数 |
| `cooldown_until` | 这个模型的冷却截止时间点 |
| `semantic_failure_streak_count` / `_fingerprint` | 这个模型的语义连击次数与指纹 |
| `last_failure_kind` / `last_failure_message` / `last_failure_response_json` | 这个模型最近一次失败的分类、消息与原始响应 |
| `created_at` / `updated_at` | 建行与最后更新时间；`updated_at` 决定健康探测先挑谁 |

**只有一个时间戳。** 账号级的 `next_retry_at` 与 `cooldown_until` 被赋成同一个值，是冗余，模型行不复制这份冗余。

**模型键取上游名而非请求名。** `api` 账号取 `model_mappings` 解析出的 `to`；`official` 账号与空映射账号取请求原名（官方请求从不改写 model）。两者都会剥掉 `[1m]` 后缀，所以 `claude-sonnet-alias[1m]` 与 `claude-sonnet-alias` 共享同一条记录——它们是同一个上游模型，只差一个 beta 头。用 `to` 做键还带来天然收敛：catch-all 映射下客户端发任意模型名都汇聚到同一个键，一次失败就冷却到位，表也不会被乱发的模型名撑大。

**行的生命周期。** 首次失败或手动暂停时建行；成功时 `paused` 的行保留状态、只清失败字段，其余整行删除。所以健康系统里这张表只剩手动暂停的行。行数上限是「账号数 × 每账号映射的模型数」，有界，不需要 GC。

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

不过状态码只是其中一层：401/403 的响应体如果本身就是确定性的失败（例如下文的 new-api 余额耗尽），会先被语义规则接住并直接置异常，不再按瞬时失败退避。

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

识别配额耗尽有两种依据：

1. **错误码或错误文本**——`error.code` 归一化后含 `insufficientquota` / `quotaexhausted` / `usageexhausted`，或消息里出现 `额度已耗尽`、`配额耗尽`、`quota exhausted`、`insufficient quota` 等固定说法。
2. **`error.type` 加消息开头**——`error.type` 为 `new_api_error`（大小写不敏感）**且**消息以 `用户额度不足` 开头。

第二条是为 new-api 系中转站单独加的。它们的余额耗尽响应长这样，常带 401 / 403 状态码：

```json
{
  "error": {
    "type": "new_api_error",
    "message": "用户额度不足, 剩余额度: ＄-0.398052 (request id: 20260902...)"
  },
  "type": "error"
}
```

整个信封里没有 `error.code`，所以第一条依据看不到它；而消息文本单独用又不够安全——同一个中转站也会把上游的 `用户额度不足` 原样转发出来。两个条件同时要求才把范围收窄到"这个中转站账号自己的余额没了"。判定只看 `error` 对象里的 `type`，顶层那个 `type` 描述的是信封而不是错误类别。走到这一步时剩余额度已经是负数，和额度重置边界一样确定，所以直接置异常而不是退避重试。

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

### 阈值管的是模型，不是账号

账号的失败策略里有一个 `semantic_error_threshold` 字段（默认 10，可填 1–1000），它的**唯一消费者是模型级连击**：转发或模型测试记下一次模型级失败时，按指纹累计该模型行的连击次数，同指纹达到阈值就把这个模型的 `status` 置成 `error`，此后它被硬排除在选号之外，直到手动解除或恢复调度把它清掉。

账号级的连击仍然只有配额耗尽一条通道在用，它把阈值硬编码为 1（一次就把账号翻成 `error`），不读这个字段。`error_status_enabled` 是两级共用的总开关，关掉之后连击照常累计、但不再自动置 `error`。

::: tip 为什么模型级需要阈值，账号级不需要
只有冷却的话，账号根本不支持的模型会永久 churn：冷却 10 秒 → 重试 → 同样报错 → 再冷却 10 秒。连击加阈值把"反复同样的失败"变成"别再试了"。
:::

**模型级与账号级在这里有一处刻意分歧。** 账号级两个函数互斥：记瞬时失败会把连击归零，记语义失败会把冷却清空。互斥的后果是冷却中的对象永远攒不起连击，阈值永远用不上。模型行**两者叠加**：每次模型级失败都写冷却，同时按指纹累计连击。

## 出站超时：让卡死的上游变成一次失败

前面所有的失败判定都有一个前提：上游得先**给出**一个结果，无论是状态码还是错误。但上游还有第三种表现——**卡死**：TCP 连上了，然后再也不发一个字节，既不报错也不断开。

这种情况下如果没有超时，转发根本拿不到 `Err`，于是同号重试、换号、退避一个都不会发生，客户端无限期挂着。所以出站请求带两个上限：

| 上限 | 值 | 管什么 |
| --- | --- | --- |
| 连接超时 | 20 秒 | TCP/TLS 握手阶段。握不上手就没什么可等的 |
| 读取间隔超时 | 180 秒 | 两次成功读取之间的最大间隔，**每次读到数据后重置** |

触发之后就是一次普通的传输层瞬时失败：记 `transport`，按 `failure_policy` 决定同号重试还是换号，并按账号配置的失败冷却窗口退避。失败消息里会明确写出是哪个上限、设的是多少秒，方便和"连接被拒绝"区分开。

::: tip 为什么不设总时限
一次正常的长回答本身就可能耗时数分钟：缓冲转发要等整个响应体到齐，流式透传则要等上游把话说完。如果设一个"从连接到读完"的总时限，就会把正常生成误杀掉。

读取间隔超时不会有这个问题：只要上游还在吐字节（SSE 增量、心跳），计时就一直被重置。它管的是"字节彻底停了"，而不是"总共花了多久"。
:::

## 流式透传与截断

代理默认把响应体**整体缓冲**下来再返回。这么做是为了让重试循环能在"客户端还没收到任何字节"的前提下换号——包括那些只有读完整个响应体才暴露的失败。

代价是没有 TTFT：客户端要等上游把话说完才看到第一个字。所以当整个响应体对下游都没有必要时，代理会改成**流式透传**。五个条件必须同时满足：

| 条件 | 为什么 |
| --- | --- |
| 不需要协议桥接 | 桥接要整体改写响应体，而且七个桥接里有五个是"先聚合整流、再重新合成事件序列" |
| 客户端请求了流式 | 非流式回复只有一份 JSON，没有可增量检查的帧，也无从流式 |
| 状态码是 2xx | 非 2xx 的响应体决定重试分类，必须在交付客户端之前完成 |
| 没有 custom tools | 自定义工具还原要在出站方向改写帧 |
| 不是官方账号 | 官方账号要解析响应体里的订阅/配额信号 |

也就是说：**Claude 直连 Anthropic 上游、Codex 直连 Responses 上游**这两条最常见的路径会流式透传，其余全部沿用缓冲路径。

### 首包之前仍然可以换号

流式路径不会一拿到响应头就交付。它先在重试循环内**等第一个数据块**，只有拿到之后才把响应交给客户端。所以这三种情况仍然会换号，行为与缓冲路径一致：

- 首包之前连接就断了或超时；
- 上游给了 200 却在首包前直接关流；
- 首包本身就是个失败信封（网关常把错误放在开头那一帧）。

### 首包之后的截断只记账，不重试

字节一旦发给客户端就收不回来，所以此后暴露的失败无法再换号。但**账号健康度照记**：流结束时如果发现"有数据帧却始终没有终止事件"（`response.completed` / `[DONE]` / `message_stop` / `finish_reason: stop` / `finishReason: STOP`），就按 `semantic_response_transient` 记一次失败，并按账号配置的失败冷却窗口退避。

判定用的终止标记与缓冲路径的 `stream_disconnected_before_completion` 逐字节一致，两条路的结论不会分叉。差别只在处置：缓冲路径能重试，流式路径只能记账——而这恰好会让下一次选号避开长期截断的上游。

::: tip 用量不会因为流式而丢
token 与费用是在流结束时结算的，客户端提前断开也一样会记——半截响应同样计入统计，而不是凭空消失。
:::

## 退避与冷却的精确规则

冷却分两层：**模型级**是默认，**账号级**是升级。判定只看失败分类与状态码，一处定义、所有记账点共用：

```rust
pub(crate) fn is_account_scoped_failure(kind: &str, status: Option<u16>) -> bool {
    match kind {
        // The credential itself, or the path to the upstream, is at fault.
        "refresh" | "request_build" | "transport" | "model_test" => true,
        // A rejected key rejects every model, so settle it once at the account
        // level; every other status is the upstream's verdict on one model.
        "upstream_status" | "model_test_status" => matches!(status, Some(401) | Some(403)),
        // The upstream answered about this specific model.
        "semantic_response_transient" | "response_transform" => false,
        // Unknown kinds park the account: over-parking is recoverable, letting a
        // broken credential keep serving is not.
        _ => true,
    }
}
```

| 失败分类 | 归属 | 为什么 |
| --- | --- | --- |
| `refresh` / `request_build` / `transport` / `model_test` | 账号 | 凭证本身或到上游的通路有问题，与请求哪个模型无关 |
| `upstream_status` / `model_test_status` 的 **401 / 403** | 账号 | 被拒的 key 对所有模型都会被拒，分模型记只会让账号失败 N 次才落定 |
| `upstream_status` / `model_test_status` 的其他状态码（400/404/408/429/5xx） | 模型 | 这是上游对这一个模型的裁决 |
| `semantic_response_transient` / `response_transform` | 模型 | 上游针对这个模型的响应内容有问题 |
| 配额耗尽 | 账号 | 配额是账号属性 |
| 未知分类 | 账号 | 宁可多冷却一点：过度冷却可恢复，让坏凭证继续服务不可恢复 |

**拿不到请求模型名时降级为账号级。** Gemini 把模型放在 URL 路径里，还有些路由本来就不带模型；这种请求选号时不查模型状态，失败也写账号级，保底不丢保护。

### 两层各写什么

- **模型级失败**：写 `route_credential_models` 里那一行——递增它的 `transient_failure_count`、写它的 `cooldown_until`、按指纹累计连击。账号级只更新 `transient_failure_count` 与 `last_failure_kind/message/response_json`，**不写账号级冷却时间戳**，所以这个账号的其他模型照常可用。
- **账号级失败**：与改动前完全一样——账号的 `next_retry_at` 与 `cooldown_until` 都写成 `现在 + 失败冷却（秒）`。
- **升级**：一次模型级失败之后，如果这个账号的**全部可服务模型都已不可用**，就顺带写账号级冷却。判定与模型行写入在**同一个事务**内完成，否则并发请求会各自看到"还没全冷"而都不升级。

升级的分母**排除手动暂停的模型**：用户暂停了 3 个模型，不该让第 4 个的一次失败伪造出"整账号坏了"的结论。推论是只映射一个模型的账号，行为与改动前完全一致。

`official` 账号与空映射账号的分母是平台基线模型集合（codex 与 claude 各 4 个，gemini 与 grok 各 1 个），所以中转站整体故障时，这类账号需要 4 个模型各失败一次才升级到账号级。有上限，可接受。

::: warning 未升级的模型级失败会清掉账号上已有的退避
写账号级列的那条 `UPDATE` 总是同时写 `next_retry_at` 与 `cooldown_until` 两列，所以一次不触发升级的模型级失败会把它们置为 `NULL`——包括之前某次账号级失败留下的窗口。实际后果有限：账号在冷却期间本来就只能通过全冷却兜底探测被选中，而这样一次探测拿到模型级失败，恰好也说明凭证本身连得上上游。
:::

### 冷却窗口的配置

瞬时失败写冷却的那段代码，两层用同一份配置：

```rust
let policy = RouteCredentialFailurePolicy::from_config_json(&config_json);
let cooldown_seconds = policy.cooldown_enabled.then_some(policy.cooldown_seconds);
```

冷却是**按账号开关**的（默认关闭），时长也**按账号配置**，在账号编辑面板的「故障处理 → 失败处理策略 → 失败冷却（秒）」里改，默认 **10 秒**，取值范围 1–86400 秒。**没有按模型单独配冷却秒数或阈值**：`failure_policy` 是账号级一份，对该账号的所有模型生效。真需要差异化时拆成两个账号。

| 账号配置 | 每次瞬时失败的效果 |
| --- | --- |
| 未开启「启用失败冷却」 | 两层都只累加失败次数，不写任何冷却时间戳，账号与模型都立刻还能被选中 |
| 已开启「启用失败冷却」 | 模型级失败写模型行的 `cooldown_until`；账号级失败（或升级）把账号的 `next_retry_at` 与 `cooldown_until` 都设为 `现在 + 失败冷却（秒）` |

三点要注意：

- **冷却时长是固定的，不再阶梯式增长。** 以前是第 1 / 2 次约 30 秒 / 2 分钟、第 3 次起 10 分钟；现在每次触发都只等配置的那个短窗口，一次抖动的代价是秒级而不是分钟级。持续坏下去的账号会反复触发同一个短冷却，而不是被越推越远。
- **账号级的两个时间戳一起写。** 它们在调度时效果相同（都要求已过期才可用），一起写入是为了界面第一次失败就能显示"冷却中"和剩余时间。
- **每次账号级瞬时失败都会清空账号级的语义连击计数**（`semantic_failure_streak_count = 0`、指纹置空）。模型行不受这条影响，它的冷却与连击是叠加的。

### 界面上的失败计数与模型徽章

只要账号的 `transient_failure_count` 大于 0，账号列表的状态标签就会显示成 `错误 N 次`；最近一次请求成功后计数清零，标签立刻回到原来的状态文案。已失效、异常、暂停这类状态会保留自己的标签，不被失败计数覆盖。

账号行的 `冷却 N 秒` 徽章现在明确表示**账号级**冷却。此外有模型不可用时会多出一个橙色徽章 `模型 N 不可用`，计数是"冷却未过期 + `error` + `paused`"三者之和。两个徽章语义不重叠：一个是「整号退避中」，一个是「部分模型不可用」。悬停 `模型 N 不可用` 会展开逐模型明细：上游模型名、括号里的客户端别名（映射被删掉的行显示 `已移除映射`）、原因与剩余时间、最近一次失败消息。

编辑抽屉的「故障处理」分区里有一个 `模型状态` 区块，逐模型列出全部已知模型——包括还没失败过的，这样一个健康模型也能被提前暂停。每行右侧两个动作：`暂停` / `恢复` 切换，和 `解除`（清掉这个模型的冷却与异常）。区块头部的 `全部解除` 只对**真的有东西可清**的模型发请求：跳过 `paused`（那是用户自己的决定），也跳过既无冷却又无失败计数的健康模型。

### 敏感词检测提醒

如果 `last_failure_response_json` 或 `last_failure_message` 里出现 `sensitive_words_detected` 这个错误码，状态标签的悬停提示里会多出一条友情提醒：

> 友情提醒：当前中转站似乎对项目存在关键词检测，您的项目可能存在敏感词，也不排除是中转站误判。

这类错误来自中转站自己的关键词过滤，不是账号坏了。看到提醒可以先检查提示词与代码里有没有触发词，也可以换一个中转站验证是否误判。

## 失败分类标签

`last_failure_kind` 记的是失败发生在链路哪一步，界面上和排错时都用得到：

| 标签 | 触发时机 | 冷却归属 |
| --- | --- | --- |
| `refresh` | 官方凭据刷新 access token 失败（且被判为瞬时） | 账号 |
| `request_build` | 构造上游请求失败（桥接、鉴权装配等环节） | 账号 |
| `transport` | 请求发不出去、读不完响应体，或上游卡死（连接、DNS、TLS、连接超时 20 秒、读取间隔超时 180 秒） | 账号 |
| `response_transform` | 上游有响应，但桥接的反向转换失败 | 模型 |
| `upstream_status` | 上游返回可重试的非 2xx 状态码 | 401/403 归账号，其余归模型 |
| `semantic_response_transient` | 语义失败（走瞬时退避这条路时） | 模型 |
| `semantic_response_failed` | 语义失败（走指纹连击那条路时，即配额耗尽通道） | 账号 |
| `model_test_status` | 模型测试收到非 2xx 状态码 | 401/403 归账号，其余归模型 |
| `model_test` | 模型测试的其他可重试失败（传输层） | 账号 |

同一个标签会同时出现在账号级与模型级的 `last_failure_kind` 上：账号级记的是"这个账号最近一次失败"，模型级记的是"这个模型最近一次失败"。

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

## 冷却中的账号与模型怎么被跳过

选号是五步，顺序不能换：

1. **装载候选**（`load_pool_candidates`）：只做 SQL 层过滤与配额过滤——在池内且启用、未归档、`status = 'ok'`、配额字段为空或大于 0。这一步**不判冷却**。
2. **按平台能力规则过滤**（`filter_candidates_for_rule`）：例如某些平台只接受 `api` 凭据。筛空则返回 `No enabled route credentials in pool`。
3. **按请求模型过滤并解析模型键**（`filter_candidates_for_model`）：一趟遍历里既判断这个账号支不支持请求的模型，又算出它该被记到哪个 `model_key` 上。筛空则返回 `route_pool.model_unmatched`。
4. **批量查模型状态**（`load_candidate_model_states`）：按 `(账号 ID, model_key)` 对一次查完整个池子。两个账号可以把同一个请求模型映射到不同的上游名，所以键必须是这个对，不能只按模型名。
5. **分桶**（`partition_by_cooldown`）：见下。筛空则返回 `route_pool.model_unavailable`。

**为什么模型过滤必须在冷却分桶之前。** 该不该判为冷却取决于请求哪个模型，不知道模型就无法分桶。这个顺序调整顺手修掉一个既有缺陷：假设 A 号只支持 `glm-5.3` 且健康、B 号只支持 `gpt-5.6-sol` 但在冷却中，此时来一个 `gpt-5.6-sol` 请求——旧顺序下分桶先看到"eligible 非空"直接返回 `[A]`，接着模型过滤把 A 也筛掉，请求以 `route_pool.model_unmatched` 失败，而 B 是唯一能服务它的号、本应走兜底探测。

### 分桶的判定顺序

```rust
pub fn partition_by_cooldown(
    candidates: Vec<PoolCandidate>,
    model_states: &HashMap<(String, String), RouteCredentialModelState>,
    now: DateTime<Utc>,
) -> Vec<SelectedCredential>
```

1. 该模型的状态是 `paused` 或 `error` → **直接丢弃**，不进任何桶，也不参与下面的兜底探测。
2. 账号级 `cooldown_until` 未过期，**或**模型级 `cooldown_until` 未过期 → 进 cooling 桶（两级时间戳取较晚者作为恢复时间）。
3. 否则进 eligible 桶。

eligible 非空就只用它。选号 SQL 本身也已经把 `status = 'ok'` 之外的账号排除在外，所以被翻成 `error` 或 `revoked` 的**账号**根本不会进入候选集。

### 全池冷却时的兜底

如果筛完发现**一个可用账号都不剩**，调度器不会直接让请求失败，而是**挑出最快恢复的那一个冷却账号**去试：

```rust
cooling.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
Ok(cooling.into_iter().take(1).map(|(_, _, credential)| credential).collect())
```

排序键是"恢复时间最早"，同时间则按原顺序稳定排序。这是一个有意的取舍：宁可用一个可能还在冷却的号试一次，也不要在客户端侧直接报错——毕竟退避时长本身是估算，上游可能已经恢复了。

**只有时间冷却能被兜底探测。** `paused` 与 `error` 是裁决而不是等待，硬排除到底——这与账号级 `status != 'ok'` 被选号 SQL 直接剔除是一致的。

### 两个模型相关的错误码，别搞混

| 错误码 | 含义 | 怎么处置 |
| --- | --- | --- |
| `route_pool.model_unmatched` | **没有任何账号映射这个模型**，第 3 步就筛空了 | 去账号的模型映射里加上它 |
| `route_pool.model_unavailable` | 有账号支持这个模型，但这些账号上它**全被暂停或判为异常** | 去编辑抽屉「故障处理」分区的「模型状态」里解除暂停/异常 |

只有冷却不会触发 `model_unavailable`：冷却中的候选还能被兜底探测挑出来，只有硬排除才会把候选集清空。合并成一个码会让排查变成猜谜，所以它们是两个码。

::: warning 客户端看到的状态码是 502
代理不透传上游状态码。上述两个错误码和「所有账号都失败」都以 HTTP **502** 返回，错误体里的 `error.code` 才是可判定的信息。所以上游返回 429 时客户端看到的是 502，不是 429。
:::

**`/models` 列表不过滤暂停模型。** 暂停是用户刚做的临时动作，让客户端模型列表悄悄缩短反而更难排查；真发请求会得到 `route_pool.model_unavailable`，那个错误码本身就是答案。代价是模型目录与实际可用性会短暂不一致。

## 成功会清空什么

一次成功的转发做两件事，**不对称**：

```sql
-- 账号级：整套失败痕迹归零
UPDATE route_credentials
SET transient_failure_count = 0, next_retry_at = NULL, cooldown_until = NULL,
    semantic_failure_streak_count = 0, semantic_failure_streak_fingerprint = NULL,
    last_failure_kind = NULL, last_failure_message = NULL,
    last_failure_response_json = NULL, updated_at = ?
WHERE id = ?

-- 模型级：只删这一个模型的行
DELETE FROM route_credential_models
WHERE route_credential_id = ? AND model_key = ? AND status != 'paused'
```

账号级全清是因为一次成功响应证明凭证与网络都好。模型级**只清本次请求命中的那个模型，绝不动兄弟模型**——测通 `glm-5.3` 不能说明 `gpt-5.6-sol` 也好了。

`paused` 的行是例外：它不被删除，只把失败字段清零、保留 `paused` 状态。一次成功不该悄悄推翻用户手动暂停的决定。

注意账号级这一步**不改 `status`**，只清失败痕迹。因为能被选中转发的账号本来就是 `ok`。

模型测试成功时多做一步：如果账号当前是 `error` 或 `warning`，把状态拉回 `ok`；针对单个账号的显式测试还会额外执行完整恢复（见下节）。

## 自动恢复

失败的账号靠什么回到池子里？两条路：

1. **退避窗口自然过期**——账号级或模型级的 `cooldown_until` 过去之后，对应的对象自动重新成为候选。这条路不需要任何配置，但只适用于状态仍是 `ok` 的账号与模型。
2. **自动恢复调度器**——针对已经被翻成 `error`、`warning` 或被手工 `paused` 的账号，以及带着模型行的账号。状态不是 `ok` 的账号不会被选号 SQL 选中，被判 `error` 或 `paused` 的模型也不会被分桶选中，光等时间是回不来的，必须有人把状态改回去。

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
fn needs_recovery(
    status: &str,
    next_retry_at: Option<&str>,
    cooldown_until: Option<&str>,
    has_model_failures: bool,
) -> bool {
    if status == "revoked" {
        return false;
    }
    status != "ok" || next_retry_at.is_some() || cooldown_until.is_some() || has_model_failures
}
```

- `revoked` 的账号**永不参与自动恢复**。重新激活的 SQL 也带 `WHERE id = ? AND status != 'revoked'`，双重保险。
- 状态不是 `ok`，或者身上还挂着任何一个退避/冷却时间戳，都算"需要恢复"。注意即使状态是 `ok`，只要还带着退避窗口就会被恢复流程清掉。
- **最后一个条件是这次新增的**：账号级字段全都健康、但存在非 `paused` 的模型行时，也算需要恢复。否则一个只有某个模型异常的账号进不了恢复流程，只能靠真实转发请求去撞。候选查询用一个 `EXISTS` 子查询补出这一列，不必把明细捞出来。

### 定时恢复（`scheduled`）

- 时刻用 `HH:MM` 表示，按**本地时区**判定。
- 保存时会规范化：补零成两位、去重、排序。`3:00` 和 `03:00` 视为同一个时刻。
- **至少要有一个时刻**，否则返回 `validation.recovery_times_required`。
- 格式非法返回 `validation.recovery_times`。

触发判定是"某个时刻是否落在上一 tick 与本次 tick 之间"这个左开右闭区间内，并且逐日枚举日期。这样两种边界情况都能正确处理：

- **跨零点**：上一 tick 在 23:59:50、本次在次日 00:00:20，配了 `00:00` 会正常触发。
- **机器睡了好几天**：上一 tick 在 8 月 11 日 16:00、本次在 8 月 13 日 10:00，配了 `15:00` 也会触发（一次，不是补齐每一天）。

触发之后执行的是**无条件重新激活**：状态写回 `ok`，清空所有失败计数、退避窗口、连击计数和失败详情，并**删掉该账号所有非 `paused` 的模型行**。这条路不验证账号是否真的可用——它假定"过了一晚上，限流应该解了"。`paused` 是用户意志，定时任务不推翻它：自动化只能撤销自动化。

### 健康探测（`healthcheck`）

- 探测间隔以分钟为单位，默认 **30**，合法范围 **1–1440**（一天）。超范围返回 `validation.recovery_probe_interval`。
- 探测上次执行时间记在内存里，按账号 ID 分别计时；应用重启后重新开始计时。
- **探测哪个模型**：挑该账号当前最该被探的那个——有表行、不是 `paused`、冷却已过期，取 `updated_at` 最早者。仓储层说的是上游模型键，模型测试要的是请求侧别名，所以中间会把键换回一个 `model_mappings` 里的 `from`（官方账号与空映射账号的键本来就是请求名，原样用）。找不到这样的行才回落到默认行为（取第一条非 fallback 映射）。
- 到点后执行的是**针对该账号的显式模型连通性测试**。测试成功会通过"显式测试恢复"把账号完整恢复；测试失败则什么都不改，等下一个间隔再试。

::: tip 为什么要挑 `updated_at` 最早的
用户没填模型时，默认取的是"第一条非 fallback 映射"。冷却按模型分之后这会让健康探测永远只探第一个模型，而这个账号进恢复流程恰恰可能是因为第三个模型异常。
:::

也就是说 healthcheck 模式恢复账号的唯一途径是**真的打通一次生成请求**。它比定时恢复更可信，代价是每次探测都会消耗一点配额，并且会在统计里留下一条请求事件。

### 显式测试恢复只管账号，不管兄弟模型

针对单个账号的显式测试通过之后，走的是另一条路：它**只清账号级的列**，不碰模型行——因为这次测试命中的那个模型，它自己的行已经在成功清账里删掉了。

这个区别是有意的。如果显式测试也顺带清空所有模型行，那么"测通 `glm-5.3`"就会顺带抹掉 `gpt-5.6-sol` 的冷却，等于宣称上游对一个从未被问到的模型作出了回答——而这正是这套两层机制要消灭的行为。

### 两种模式怎么选

| 场景 | 建议 |
| --- | --- |
| 上游按自然日重置额度 | `scheduled`，把时刻设在重置之后一小会儿 |
| 上游限流窗口不确定 | `healthcheck`，间隔 15–30 分钟 |
| 号很多、不想每个都探测消耗配额 | 主力号用 `healthcheck`，备用号用 `scheduled` |
| 想完全手工控制 | `off`，需要时手动点一次测试 |

::: tip 手动测试就是最快的恢复手段
针对单个账号点一次模型连通性测试，成功即执行完整恢复。`paused` 的账号也可以被测试——代码里明确注释了这一点：显式测一次正是用户判断暂停中的账号是否已恢复的方式。想解除单个模型的冷却或异常，编辑抽屉「故障处理」分区的「模型状态」里点一次 `解除` 更直接，不消耗配额。
:::

### 配置写坏了会怎样

如果账号的 `config_json` 不是合法 JSON 对象，设置恢复规则会返回 `validation.recovery_config_json`，并且**不会覆盖原有配置**。这是有意的：宁可拒绝写入，也不要把用户手工编辑过的配置冲掉。

读取侧则很宽容：解析不出 `recovery` 键、或者内容不合规范，都回落到 `off`，不会让一条坏配置把整个恢复循环打断。

## 一次故障的完整时间线

假设某个主力号同时映射了 `gpt-5.6-sol` 与 `glm-5.3`，上游只对 `gpt-5.6-sol` 返回 429：

```text
T+0s     第 1 次 gpt-5.6-sol 请求 429 → 同号重试（间隔 200 ms）→ 仍 429
         → 重试次数用尽 → 记一次模型级失败
         → route_credential_models 里 upstream-sol 那行：冷却至 T+10s（该号开了失败冷却，默认 10 秒）
         → 账号级只累加 transient_failure_count，不写账号冷却
         → 界面显示「错误 1 次」和「模型 1 不可用」，账号行的「冷却 N 秒」不出现
         → 换到同优先级组的下一个号，客户端正常拿到响应

T+2s     glm-5.3 请求 → 主力号照常被选中 → 成功
         → 清空账号级失败痕迹，删掉 upstream-glm 的行（本来就没有）
         → upstream-sol 的冷却不动：这次成功没有回答关于它的任何问题

T+10s    upstream-sol 冷却过期，主力号重新可以服务这个模型
         → 又 429 → 模型级失败 #2 → 该行冷却至 T+20s

T+20s    再试 gpt-5.6-sol → 这次成功
         → upstream-sol 的行被删除，账号级失败痕迹清零
         → 徽章消失，主力号对两个模型都完全恢复
```

如果上游是整体故障，两个模型各失败一次之后，第二次失败会触发升级：账号级 `cooldown_until` 被写入，整个号退避——避免一个全挂的中转站被逐模型反复探测。

整个过程里客户端**没有感知到任何失败**——每一次退避都伴随一次换号，只要池里还有别的可用号。这就是多号池加两层冷却的意义。

## 下一步

- [账号与算力池](/guide/accounts)：状态机、优先级与并发上限
- [模型连通性测试](/guide/model-test)：手动恢复与健康探测背后的同一套测试逻辑
- [用量与请求统计](/guide/usage-stats)：失败的请求也会被记录，怎么查
- [协议路由与桥接](/guide/protocol-routing)：失败发生在转发链路的哪一步
