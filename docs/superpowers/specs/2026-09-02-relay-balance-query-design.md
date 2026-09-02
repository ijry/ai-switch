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
