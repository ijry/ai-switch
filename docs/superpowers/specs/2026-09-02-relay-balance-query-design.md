# 中转站余额查询设计

日期：2026-09-02

## 问题

中转站账号（`kind = "api"`）的钱是会用完的，而应用现在只能**事后**知道：上游回 `{"error":{"type":"new_api_error","message":"用户额度不足…"}}` 时 `is_quota_exhaustion_failure`（`response_failure_service.rs:52`）把账号置为异常。在那之前界面上没有任何信号——用户看不出池子里哪个号快没钱了，只能等它在一次真实请求里失败。

官方账号（`kind = "official"`）反而有主动查询：`RouteQuotaService` 拿 OAuth token 去厂商的 usage 接口取剩余额度，界面上有「主额度 / 周额度」徽标和「刷新额度」动作。中转站缺的就是这一份对称能力。

cc-switch 的做法是内嵌 QuickJS，让用户为每个 provider 写一段 `({request, extractor})` JS 脚本，并内置了一个 New API 模板（`{{baseUrl}}/api/user/self` + 用户 PAT + `New-Api-User` 头）。它没有 sub2api 支持，用户得手写自定义脚本。

## 目标形态

账号编辑表单的「高级」分区加一个分段器选查询方式：**关闭 / new-api / sub2api / 自定义**。选中后账号行出现「查余额」动作，查到的余额以徽标显示在额度徽标旁边，并持久化成快照，下次打开界面直接显示「更新于 X 分钟前」。

两个内置档都是**零配置**的：只用账号已有的 Base URL 和 API Key，不额外要用户填访问令牌或用户 ID。这一点比 cc-switch 的 New API 模板更省事，代价是换了个端点（见下）。

## 非目标

- **不内嵌 JS 引擎。** cc-switch 的脚本引擎能覆盖任意面板，但代价是 rquickjs 依赖、两个运行时、超时/内存/栈沙箱，以及「用户粘贴来的脚本能发任意请求」这一安全面。长尾面板改用声明式的「自定义」档覆盖：填 URL 与取值路径，表达力弱得多但够用，且没有执行任意代码这回事。
- **不做后台轮询。** 官方额度也是手动刷新的，两者保持一致。new-api 的 `/api/usage/token/` 带 20 次 / 20 分钟 每 key 的限流（`CriticalRateLimit`），而这里的对象是一整池账号而不是 cc-switch 的单个活跃 provider，自动轮询很容易把池子里每个号都撞上限流。
- **余额耗尽不联动账号状态。** 徽标变红并给出提示，但不写 `status`。查询链路上「面板 404 / 限流 / 字段改名」都会表现成「余额异常」，让它去改调度就等于给账号池加了一条新的误伤路径；真额度不足时现有的反应式路径已经会置异常。
- **不支持只能用用户 PAT 查的面板。** 见下面对端点的取舍。
- 不新增迁移。配置与快照都落在 `config_json`，与 `failure_policy`、`recovery` 一致。

## 1. 端点选择

### new-api：用 `/api/usage/token/`，不用 `/api/user/self`

| | `/api/user/self`（cc-switch 用的） | `/api/usage/token/`（本设计用的） |
| --- | --- | --- |
| 鉴权 | 用户 PAT（个人设置里另取），老版本还强制 `New-Api-User: <用户ID>` 头 | `Authorization: Bearer sk-…`，**账号已有的 key 就够** |
| 用户要填 | Base URL + 访问令牌 + 用户 ID | 什么都不用填 |
| 口径 | 用户级：整个账户的 `quota` / `used_quota` | **令牌级**：这把 key 的 `total_available` / `total_used` / `total_granted` |
| 信封 | `{"success":true,"data":{…}}` | `{"code":true,"message":"ok","data":{…}}` |
| 限流 | 全局 web 限流 | 20 次 / 20 分钟 每 key |

选令牌级是因为**这里问的问题就是令牌级的**：池子里一个成员就是一把 key，用户想知道的是「这把 key 还能不能用」，而不是「面板主人账户里还剩多少钱」。零配置是顺带的好处。

代价写清楚：若面板关掉或没有 `/api/usage/token/` 路由，这一档查不到，报错里带上试过的 URL 与面板返回的原文，用户可以改用「自定义」档。**PAT 路径 v1 不做**——它要多两个输入框、一个新的密文字段（PAT 属于 secret，得进 `secret_payload_json`），而且老版本还要那个用户 ID 头。

`quota` 是整数，要除以面板的 `QuotaPerUnit` 才是美元。这个值**面板管理员可改**，默认 500000。`GET /api/status`（无需鉴权）会回 `data.quota_per_unit`，所以先尽力取一次真值，取不到再退回 500000。cc-switch 把 500000 写死，在改过该值的面板上会算错。

### sub2api：`GET /v1/usage`

API key 鉴权，返回的就是美元浮点，不用换算。三种形态都要认：

- `mode = "quota_limited"`：`quota.{limit,used,remaining}`、`rate_limits[]`、`expires_at`
- `mode = "unrestricted"` + 订阅组：`planName`、`subscription.{daily,weekly,monthly}_{usage,limit}_usd`，`remaining` 取各窗口最小剩余
- `mode = "unrestricted"` + 钱包组：`planName = "钱包余额"`、`balance`

顶层已经有 `isValid` / `planName` / `remaining` / `unit`，直接读顶层即可，`mode` 只用来决定要不要附加显示订阅窗口。

### 自定义

填请求 URL + `remaining` 的取值路径（点号路径，如 `data.total_available`），可选 `used` / `limit` / `plan` 路径与除数。鉴权沿用账号自己的 key（与拉取模型列表同一套请求头：`Authorization: Bearer`，anthropic 方言额外带 `x-api-key`）。

## 2. Base URL 与面板根

账号的 Base URL 往往是 `https://panel.example.com/v1`，而面板接口挂在根上，直接拼就成了 `…/v1/api/usage/token/`（404）。cc-switch 不剥 `/v1`，这是它「查询失败」最常见的原因，还要求用户在脚本设置里单独再填一遍面板地址。

这里改成自动推导：剥掉尾部 `/`，再剥一层 `/v1`、`/v1beta`、`/openai/v1`，得到面板根，然后按「面板根」与「原始 Base URL」两个候选依次试，命中即止。用户不需要多填一个字段。

## 3. 数据模型

配置进 `config_json.relay_balance`，仿 `failure_policy`（`route_credential.rs:32-101`）：`#[serde(default)]` 结构 + `from_config_value` + `validate`。整块缺失即「关闭」。

```jsonc
{
  "relay_balance": {
    "provider": "new_api",        // new_api | sub2api | custom
    "endpoint": "",               // custom 必填
    "remaining_path": "",         // custom 必填，点号路径
    "used_path": "",              // custom 可选
    "limit_path": "",             // custom 可选
    "plan_path": "",              // custom 可选
    "unit": "",                   // custom 可选，留空按 USD
    "divisor": 1.0                // custom 可选，默认 1
  }
}
```

快照进 `config_json.relay_balance_snapshot`，字段是 `f64`：

```jsonc
{
  "relay_balance_snapshot": {
    "provider": "new_api",
    "plan_name": "default",
    "remaining": 37.7,
    "used": 12.3,
    "limit": 50.0,
    "unit": "USD",
    "unlimited": false,
    "expires_at": null,
    "source_url": "https://panel.example.com/api/usage/token/",
    "checked_at": "2026-09-02T12:00:00Z"
  }
}
```

**不复用 `quota_remaining` 等列。** 那几列是 `i64`（`quota_columns_from_config_json`，`route_credential_repository.rs:233`），$12.34 会被截成 12；而且它们是官方账号额度的口径，混进中转站余额会让「主额度」徽标和调度里读这些列的地方产生二义。快照因此只活在 `config_json` 里，前端解析 `config_json` 取用——账号行本来就在解析它。

## 4. 代码结构

| 单元 | 位置 | 职责 |
| --- | --- | --- |
| `RelayBalanceProvider` / `RelayBalanceConfig` / `RelayBalanceSnapshot` | 新增 `models/route_relay_balance.rs` | 枚举、配置解析校验、快照序列化 |
| 三个适配器 + 面板根推导 + 快照落库 | 新增 `services/route_relay_balance_service.rs` | 纯函数解析 + 出站请求 + `update_secret_and_config` |
| 两个命令 | `commands/route_credential_commands.rs` | `refresh_route_credential_relay_balance(id)`、`refresh_route_credentials_relay_balance(platform)` |
| 分段器与徽标 | `screens/AccountsScreen.tsx` | 「高级」分区的 `RelayBalanceFields`、行内「查余额」动作、余额徽标 |

出站请求照 `route_model_fetch_service.rs` 的形状：`build_outbound_http_client(Some(15s))`、候选 URL 依次试、404/405 继续、错误体截断到 512 字符、错误码走 `validation.route_relay_balance_*`。落库照 `RouteQuotaService::refresh_credential`（`route_quota_service.rs:89`）：算出新 `config_json` → 与旧值相同则 `updated: false` 直接返回 → 否则 `update_secret_and_config` 后重读整行，返回 `RelayBalanceRefreshOutcome { credential, updated, source, message }`。

**HTTP 200 不等于成功。** new-api 的鉴权失败有若干分支是 200 + `success: false`，`/api/usage/token/` 的信封键还是 `code` 而不是 `success`。适配器一律先看信封再看数据，取不到数字就报错并把面板的 `message` 带出来。`unlimited_quota: true` 时数字无意义，显示「不限额度」。

## 5. 界面

- 分段器：`fieldset` + 药丸轨道，照 `AccountsScreen.tsx:6244` 的测试接口分段器；四档横排，选中态 `bg-white text-stone-950 shadow-sm`。选「自定义」时下方展开三个输入框，其余档不展开任何字段。
- 位置：编辑抽屉与新增弹窗的「高级」分区（User-Agent 那一组旁边）。新增弹窗要走 Rust 的结构化 input，因此 `CreateApiRouteCredentialInput` 加一个 `relay_balance_provider`；自定义档的三个字段只在编辑抽屉里配，新增时先选内置档或事后再补。
- 行内动作：`kind === "api"` 且配置非关闭时出现「查余额」，图标沿用 `RefreshCw` + 旋转态，与官方账号「刷新额度」（`AccountsScreen.tsx:5585`）对称。
- 批量动作：刷新菜单里加一条「查中转站余额」，对应 `refresh_route_credentials_relay_balance`，与已有的「刷新账号额度」并列。
- 关掉查询时连快照一起删，徽标跟着设置一起消失，而不是冻在最后一次读数上。
- 徽标：额度徽标之后（`AccountsScreen.tsx:5850` 之后）加一枚，`余额 $37.70`，`title` 里写清口径与更新时间。余额 ≤ 0 用红色，> 0 用青色，`unlimited` 显示「不限额度」。
- 该屏不走 i18n（全屏零 `useI18n`），新文案沿用硬编码中文。

## 6. 测试

- Rust 纯函数：面板根推导（带 `/v1`、带尾斜杠、已是根）、new-api 信封（`code:true` 正常 / 200+`success:false` / `unlimited_quota`）、除数取自 `/api/status` 与退回 500000、sub2api 三种 `mode`、自定义点号路径取值与除数、错误体截断。
- Rust 服务：内存池 + 本地固定响应上游（照 `start_fixed_upstream`）跑通「查到并写进 `config_json`」「同值不写库」「关闭档跳过」「归档账号拒绝」。
- 前端：`vi.mock` 加两个 client 函数；测「在高级分区选 new-api 后保存，`updateRouteCredential` 收到的 `config_json` 带 `relay_balance.provider`」「选自定义但没填 URL 时报错并跳到高级分区」「有快照时账号行出现余额徽标」。

## 7. 这四个取舍是我定的，等你确认

提问没等到回答，按下面的默认实现了，每一个都好翻：

1. **给了「自定义」档**，用声明式的 URL + 取值路径，而不是 cc-switch 的 JS 引擎。不想要这一档就删掉枚举分支与那三个输入框。
2. **余额 ≤ 0 不改账号状态**，只让徽标变红。要联动就在 `refresh_credential` 落库后按快照调 `update_status`。
3. **不做自动轮询**，只有手动动作。要加就在前端给 `useQuery` 挂 `refetchInterval`，并把间隔存进 `relay_balance.auto_query_interval_minutes`；注意 20 次 / 20 分钟的限流。
4. **新增弹窗也放了分段器**，代价是 `CreateApiRouteCredentialInput` 多一个字段。只想放编辑抽屉就把该字段和弹窗里那一组去掉。

## 8. 2026-09-03 实测修正

拿生产库 `~/.ai-switch/ai-switch.db` 里的 8 个中转站账号逐个跑真实请求，结果是一个都没成功。三个原因，两个是代码的：

| 账号 | 面板实际是 | 当时的档 | 当时的结果 |
| --- | --- | --- | --- |
| 千刀站 `api.zzzcoding.org` | 真的 Sub2API | 关闭 | 没查；打开就能读到 `planName: 钱包余额` + `remaining` |
| goRouter `gorouter.app` | New API | 关闭 | 没查 |
| kktoken `kktoken.cc` | New API | sub2api | 失败 |
| worldclawpro `worldclawpro.ai/v1` | New API | sub2api | 失败 |
| justwoker `api.justwoker.icu` | New API | sub2api | 失败 |
| muyuan `muyuan.do/v1` | 被 Cloudflare 挑战墙挡住，看不到 | sub2api | 403 `Just a moment...` |
| AR `ps.air-outer.com/v1` | new-api 分支，没有 `/api/usage/token/` | new-api | 全 404 |
| 肖恩 `free.supxh.xin/v1` | 自研 Next.js 站 | 关闭 | 两个内置档都不适用 |

端点选择本身没问题：`/api/usage/token/` 在四个 New API 面板上都是 200，`/v1/usage` 在唯一的真 sub2api 上也是 200，字段与第 1 节写的形状一致（`isValid` / `planName` / `remaining` / `unit`，钱包组走 `balance`）。改掉的是三件事：

1. **面板 SPA 的 200 不再算命中。** 中转站面板的兜底路由对任何未知路径都回 200 + `index.html`，候选扫描把它当成命中后 JSON 解析失败并**终止整轮扫描**。这让 kktoken 报「余额接口没有返回 JSON」，而真因是这个面板没有这个接口；更糟的是它会掩埋排在后面、本来能答的候选。现在非 JSON 的 200 与 404 同等对待，继续试下一个，全部失败时在详情里说明面板回的是网页。

2. **选错档自动兜住。** Base URL 看不出面板软件是哪套，所以这个设置本质是用户从外部猜的，而猜错是健康账号读成「查询失败」的头号原因。两个内置档都零配置、同一把 key，因此在**选定档的所有候选都没有该接口**时（仅此一种情况，401/403/错误信封都不重试，那些意味着接口在、问题在别处）改按另一个档再试一轮，命中就用它，并在快照的 `notes` 里写明「面板实际按 X 应答（账号里选的是 Y）」——不静默替用户改设置。`RelayBalanceRefreshOutcome.source` 随之改为**实际应答**的档。「自定义」档不参与：那个 URL 是用户自己填的。
   重试沿用同一个 15 秒预算。曾试过给它 8 秒的短绳，结果 kktoken.cc 与 worldclawpro.ai 这两个确实会答 `/api/usage/token/` 的面板双双超时——正是这条回退要消灭的那种失败。中转站慢是常态。

3. **批量失败不再无迹可寻。** `refresh_platform` 把每个失败折成 `message` 时只取了 `Display`，丢掉带 URL 与面板原文的 `details`（`AppError` 因此补了 `details()`）；前端又只统计「失败 N」而不写回行状态。两处都补上后，批量查一次余额，每个失败的账号行都能看到与单账号动作一样的原因。

改完再跑同一批账号：kktoken / justwoker / worldclawpro 三个不动配置就能读到数（都是 New API 的不限额度令牌，所以徽标是「余额 不限」+ tooltip 里的已用金额），AR 与 muyuan 仍然失败但报错已能指向真因。

**还没做的两件事**，都需要新的用户输入，因此留在这里而不是偷偷实现：

- AR 这类删掉了 `/api/usage/token/` 的 new-api 分支，只剩 `/v1/dashboard/billing/usage`（回 `{"object":"list","total_usage":…}`）。它能填进「自定义」档，但 `total_usage` 的量级要面板主人对一次才知道除数是 100 还是 `QuotaPerUnit`，猜错会把金额报错一百倍。
- New API 面板发的多是 `unlimited_quota: true` 的令牌，令牌级接口只有「已用」没有「剩余」。要看剩余得走用户 PAT 的 `/api/user/self`，即第 1 节明确排除的那条路。

## 9. 2026-09-04：不限额度的令牌改查面板账户余额

上一节最后那件「还没做的事」做了，第 1 节排除 `/api/user/self` 的取舍随之改一半。

排除的理由有两条：「这里问的问题是令牌级的」和「零配置」。实测推翻了前一条——四个 New API 面板发的都是 `unlimited_quota: true` 的令牌，令牌级接口**结构上就没有「剩余」这个数**，徽标只能永远显示「余额 不限」。用户问的是「还剩多少钱」，而对这些令牌，答案只存在于账户口径。第二条理由仍然成立，所以改法是分层而不是换路：

- 令牌级仍是默认，仍然零配置。
- 只有在**令牌被面板标记为不限额度**且用户填了面板访问令牌时，才改查 `/api/user/self`。没填就保持「不限」，并在详情里写一句「填写面板访问令牌后可显示账户剩余额度」。
- 有自己额度的令牌即便填了访问令牌也走原路：那个数才是用户要的。

两个新输入框与两个新密文字段，正是第 1 节记下的代价：`relay_balance_access_token`、`relay_balance_access_token_user_id`，都进 `secret_payload_json`。访问令牌能签发和吊销 key，权限比一把中转 key 大，必须继承 `api_key` 那套掩码（`MASKED_SECRET_PAYLOAD` 整块替换，因此自动覆盖）。用户 ID 本身不是密文，但离了令牌没有意义、也只跟它一起用，成对存放成对掩码，而不是把一个凭据劈到两列去。

**用户 ID 头不是老版本的遗留。** v0.13.2——当前整条稳定线——依旧拒绝不带 `New-Api-User` 的访问令牌请求，并把值与令牌归属比对；只有 `main` 删掉了这个检查。所以：

- 不填就不发这个头。错的 ID 与缺 ID 一样被拒，塞个占位符正是 cc-switch 把「用户没填」变成「莫名 401」的做法。
- 未填 ID 时的失败在详情里点名要补哪个框——面板自己的话术里根本不提这个头。
- 已填 ID 时不加这句提示：那时候的拒绝是别的原因，猜一次就会把用户指到已经填好的字段上去。
- 非数字的 ID 在发请求前就拦下（面板用 `strconv.Atoi` 解析），前端输入框也只收数字。

**口径要说出口。** `RelayBalanceSnapshot.account_level` 让徽标写「账户余额」而不是「余额」，`notes` 里写明这是面板账户的钱、同一面板下的多个账号会显示同一个数。少了这一步，账户级的数字会被读成这把 key 的额度。

**失败不退回「不限」。** 填了访问令牌还查不到就报错。静默降级到「余额 不限」等于把这条路要替换的读数又摆回去。

面板压根没有令牌级接口（第 8 节的 AR 那类分支）时也会追问账户接口，但只在「路径不存在」而非「主机不可达」时——与第 8 节第 2 点的方言回退同一条判据。

**导出不带这两个字段。** CPA 格式没有它们的位置，导出时丢掉并给一条 `transfer.relay_balance_secret_dropped` 告警（整块 `relay_balance` 配置本来也不随导出转移，走的是 `transfer.api_config_field_ignored`）。把面板访问令牌写进导出文件，会让一次分享泄露一个能签发 key 的凭据，比泄露一把中转 key 严重得多——所以这里是刻意为之，不是漏了。

**还没拿真面板对过成功的那一半。** 手上没有任何面板的账户访问令牌，所以 `/api/user/self` 的 **200** 响应形状仍然只对着文档与 cc-switch 的用法写，跑的是本地 fixture。拿到真令牌后先对一次字段名（`data.quota` / `data.used_quota` / `data.group` / `data.username`）再信任屏幕上的金额。

失败的那一半 2026-09-04 拿假令牌探过真面板，三条结论：

- **api.justwoker.icu、worldclawpro.ai**：`/api/user/self` 在，回 **HTTP 401** + `{"code":"AUTH_UNAUTHORIZED","message":"Unauthorized, invalid access token","success":false}`。所以拒绝走的是状态码那条路而不是「200 + `success:false`」——`code` 在这里还是个**字符串**，不能当信封布尔读。信封检查仍然保留：`/api/usage/token/` 拒绝 key 用的正是 200 + false。
- **带不带 `New-Api-User: 1`，这两个面板的 401 一字不差**。假令牌下本来就分不出「缺头」和「令牌错」，但这恰好证明了提示存在的理由：面板的话术不会告诉用户少填了哪个框。
- **kktoken.cc、gorouter.app 这条路 403**，回 Cloudflare 挑战页（换 `ai-switch/0.8.1` UA 也一样），而第 8 节里 kktoken 的 `/api/usage/token/` 是 200。挡的是路由不是站点，所以这两个面板的用户填了访问令牌也读不到账户余额，只能停在「余额 不限」。

