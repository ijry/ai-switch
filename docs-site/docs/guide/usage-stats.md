---
title: 用量与请求统计
description: AI Switch 如何按请求记录输入/输出/缓存 token 与双币价格，统计面板的四个时间范围与六个指标口径，以及请求列表里能展开看到什么。
---

# 用量与请求统计

用多号池跑一段时间之后，你会想知道三件事：**打了多少次、烧了多少 token、花了多少钱**。AI Switch 把每一次转发都记成一条用量事件，统计口径完全由数据库里的聚合查询决定——本页把这些口径逐条摊开，让你看到的数字有明确含义。

## 一次请求记一条事件

所有用量都落在 `usage_events` 表。这张表在早期迁移里就存在，后来补上了路由凭据关联与拆分列：

| 迁移 | 补充内容 |
| --- | --- |
| `202607130004_routing_usage.sql` | 建表：`source_label`、`metric_type`、`amount`、`unit`、`metadata_json`、`created_at` |
| `202607130011_route_credentials.sql` | 增加 `route_credential_id` 列与索引 |
| `202608060002_route_usage_breakdown.sql` | 增加 6 个拆分列 |

拆分列就是 token 与费用的全部来源：

```sql
ALTER TABLE usage_events ADD COLUMN input_tokens INTEGER;
ALTER TABLE usage_events ADD COLUMN output_tokens INTEGER;
ALTER TABLE usage_events ADD COLUMN cache_tokens INTEGER;
ALTER TABLE usage_events ADD COLUMN price_usd_micros INTEGER;
ALTER TABLE usage_events ADD COLUMN price_cny_micros INTEGER;
ALTER TABLE usage_events ADD COLUMN price_currency TEXT;
```

价格用**微单位**（micros）存整数，1 美元 = 1,000,000 micros。这样避免浮点累加误差，同时保留六位小数的精度。USD 与 CNY 各占一列，`price_currency` 记录上游到底报的是哪种货币。

### 谁在写这张表

`source_label` 标明事件来源，这也是区分"真实流量"和"手工测试"的唯一依据：

| `source_label` | 来源 | 写入时机 |
| --- | --- | --- |
| `route_proxy` | 本地代理转发 | 每次转发结束（成功或失败都记） |
| `route_pool_model_test` | 模型连通性测试 | 每次点测试 |
| `route_pool` | 池内单次路由调用 | 调用路由接口时 |

写入的形状是固定的：`metric_type = 'request'`、`amount = 1`、`unit = 'count'`，一次请求一行。token 与价格不再单独开行，而是直接填进同一行的拆分列。

::: warning 统计里包含你手工点的测试
模型连通性测试的事件和真实转发写在同一张表、同一个 `metric_type`，**统计面板不会把它们分开**。所以频繁点测试会推高请求数。要区分只能看请求列表里的"来源"列。
:::

## 上游 usage 怎么解析成 token

第三方网关返回 usage 的字段名各不相同，代理侧做了别名兜底，按顺序取第一个可用值：

| 指标 | 依次尝试的字段 |
| --- | --- |
| 输入 token | `usage.input_tokens` → `usage.prompt_tokens` → `usageMetadata.promptTokenCount` |
| 输出 token | `usage.output_tokens` → `usage.completion_tokens` → `usageMetadata.candidatesTokenCount` |
| 缓存 token | `usage.input_tokens_details.cached_tokens` → `usage.prompt_tokens_details.cached_tokens` → `usage.prompt_cache_hit_tokens` → `cache_read_input_tokens + cache_creation_input_tokens` 之和 → `usageMetadata.cachedContentTokenCount` |

这三条链路分别覆盖了 Responses、Chat Completions、Gemini、Anthropic 以及 DeepSeek 系网关的写法。数值支持整数和字符串两种类型，负数被当作无效值丢弃。

**没有 usage 就是空，不做估算。** 上游不回 usage 时对应列留 `NULL`，界面上显示 `-`。AI Switch 不会用字符数反推 token 数去填一个看起来更好看的数字。

## 上游价格怎么解析

价格解析同样是别名链，并且区分"已经是微单位"和"需要乘 1,000,000"两种情形：

| 目标列 | 微单位字段 | 普通单位字段（自动 ×1,000,000） |
| --- | --- | --- |
| `price_usd_micros` | `price_usd_micros`、`cost_usd_micros`、`cost_micros` | `price_usd`、`cost_usd` |
| `price_cny_micros` | `price_cny_micros`、`cost_cny_micros` | `price_cny`、`cost_cny` |

还有一层泛化处理：如果上游只给了 `price` 或 `cost`（可能是对象，带自己的 `currency` / `unit`），会先识别货币再归到对应列。货币识别是宽松匹配——包含 `usd`、`dollar` 或等于 `$` 归为 USD；包含 `cny`、`rmb`、`yuan` 或等于 `¥` 归为 CNY。

`price_currency` 的确定顺序是：显式货币字段 → 泛化价格对象里的货币 → 只有一列有值时按那一列推断。三者都无法确定时留空。

## 统计面板的口径

统计面板在账号页的"统计"视图里，按平台展示。它有**四个时间范围**和**六个指标卡**。

### 四个时间范围

| 选项 | `since` 取值 |
| --- | --- |
| 当日 | 本地时间今天 00:00:00 |
| 本周 | 本周一（周日算上周）本地时间 00:00:00 |
| 本月 | 本月 1 日本地时间 00:00:00 |
| 累计 | 不传 `since`，不设下界 |

`since` 以 RFC 3339 字符串传给后端，非法格式返回 `validation.route_pool_since`。SQL 里对应的条件就是一句 `AND ue.created_at >= ?`——**只有起点，没有终点**，所以这四个选项本质上是"最近某个时刻之后的累计值"，而不是分桶统计。

### 六个指标卡

| 指标卡 | 后端字段 | 计算口径 |
| --- | --- | --- |
| 请求 | `request_count` | `metric_type = 'request'` 的事件求和；`amount > 0` 取 `amount`，否则按 1 计 |
| 输入 Token | `input_token_count` | request 行的 `input_tokens` 求和（`NULL` 按 0） |
| 输出 Token | `output_token_count` | request 行的 `output_tokens` 求和 |
| 缓存 Token | `cache_token_count` | request 行的 `cache_tokens` 求和 |
| Token 总计 | `token_count` | request 行的 `input_tokens + output_tokens` 求和，**再加上**历史遗留的 `metric_type = 'token'` 或 `unit = 'token'` 事件的 `amount` |
| 总费用（USD） | `cost_micros` | 见下方换算规则，展示时除以 1,000,000 保留两位小数 |

注意两点容易误读的地方：

- **Token 总计不包含缓存 token。** 它是输入加输出，缓存 token 单独一个卡。
- **Token 总计带遗留口径。** 早期版本把 token 记成独立事件行，这部分历史数据仍会被计入总计，但不会出现在输入/输出/缓存三个卡里。

除这六个指标外，后端还返回一个 `member_count`（该平台池内、未归档的成员数），用于界面上其他位置。

### 费用是怎么换算的

`cost_micros` 统一折算成美元微单位：

```sql
COALESCE(SUM(CASE
    WHEN ue.metric_type = 'request' AND ue.price_currency = 'usd' THEN COALESCE(ue.price_usd_micros, 0)
    WHEN ue.metric_type = 'request' AND ue.price_currency = 'cny' THEN CAST(ROUND(COALESCE(ue.price_cny_micros, 0) / 7.1) AS INTEGER)
    WHEN ue.metric_type = 'cost' AND ue.unit = 'usd_micros' THEN ue.amount
    ELSE 0
END), 0) AS cost_micros
```

::: warning 汇率是写死的 7.1
人民币计价的事件按固定除数 **7.1** 折算成美元，代码里没有汇率接口也没有配置项。所以混用 USD 与 CNY 计价的网关时，总费用是个参考值而非账单值。请求列表里每一行展示的是**原始币种的原始金额**，那个数字才是精确的。
:::

### 哪些账号会被统计

所有统计查询都是 `INNER JOIN route_credentials`，条件为 `a.platform = ? AND a.archived_at IS NULL`。因此：

- **按平台隔离。** 每个平台的统计互不相干，没有跨平台汇总视图。
- **归档账号被排除。** 归档一个账号，它的历史事件立刻从统计里消失；取消归档又会回来。
- **删除账号后其历史事件不再出现在统计里。** 事件行本身没有随账号级联删除，但因为 join 不上，聚合结果里就没有它了。

## 请求列表

指标卡下面是逐条请求列表，分页返回，按 `created_at DESC, id DESC` 排序。

- 只包含 `metric_type = 'request'` 的行。
- 界面固定每页 **20** 条；后端接受 1–100 的页大小，超范围会被夹到区间内，页码小于 1 会被抬到 1。
- 面板打开期间每 **5 秒**自动刷新一次，关闭后停止轮询。

列表每行展示：时间、账号名、状态码、路径、模型、token 合计、价格、来源。其中：

- **模型列**在请求模型与上游模型不同时显示成 `请求模型->上游模型`，一眼就能看出模型映射生效了。
- **token 合计**是输入加输出；悬浮可以看到输入、输出、缓存三个分项。
- **价格**按 `price_currency` 决定符号，CNY 显示 `¥`、USD 显示 `$`，都保留六位小数；无价格显示 `-`。

### 展开详情

点"详情"展开一行，可以看到账号名、账号 ID、来源、指标（`amount` + `unit`）、输入/输出/缓存 token、价格、时间，以及两块原文：

- **上游原始响应**：来自 `metadata_json.response_body`。成功请求只留前 **2 KiB**，失败请求留前 **16 KiB**——排错更需要看完整的错误体，成功的响应留一小段够定位就行。
- **完整的 `metadata_json`**：格式化输出。解析失败时原样显示并给出提示。

`metadata_json` 里由代理写入的字段包括：

| 字段 | 内容 |
| --- | --- |
| `platform` | 平台 |
| `route_credential_id` / `route_credential_name` | 命中的账号 |
| `entry_path` / `path` | 入口路径 |
| `target_url` | 最终请求的上游 URL |
| `status` | HTTP 状态码 |
| `success` | 是否成功 |
| `duration_ms` | 耗时 |
| `trace_id` | 追踪 ID（模型测试经代理路径时用它反查命中账号） |
| `error_message` | 错误信息 |
| `requested_model` / `upstream_model` | 客户端请求的模型与实际发给上游的模型 |
| `response_body` | 截断后的上游响应体 |

模型测试写入的 `metadata_json` 字段更多，见 [模型连通性测试](/guide/model-test)。

## 账号列表上的成功率

账号列表里每行带的请求数与成功率是另一套聚合，口径和统计面板不同：

```sql
LEFT JOIN usage_events ue
  ON ue.route_credential_id = rc.id
 AND ue.source_label IN ('route_proxy', 'route_pool_model_test')
 AND ue.metric_type = 'request'
```

- **只算 `route_proxy` 与 `route_pool_model_test` 两种来源**，池内单次路由调用不计入。
- 成功与失败靠 `json_extract(ue.metadata_json, '$.success') = 1` 判定。
- 成功率 = 成功数 × 100 ÷ 总数；没有请求时为 `NULL`（界面显示 `-`）。
- **没有时间范围。** 这是账号的全历史累计，不跟着统计面板的时间选择变化。

所以同一个账号，在统计面板里（选"当日"）和在账号列表里看到的请求数很可能不一样——这不是 bug，是两套口径。

## 想看实时流量怎么办

用量事件是**结果记录**：一次请求结束后写一行，带的正文是截断过的。如果你要看的是请求正在被怎么改写，用代理的实时请求日志——它按四个阶段捕获每个转发请求，但只在内存里保留 100 条且不落盘。两者互补：一个用来算账，一个用来排错。细节见 [协议路由与桥接](/guide/protocol-routing)。

## 下一步

- [账号与算力池](/guide/accounts)：账号列表、归档与批量操作
- [模型连通性测试](/guide/model-test)：测试事件是怎么写进统计的
- [稳定性与自动恢复](/guide/reliability)：失败的请求会怎样影响账号状态
- [常见问题](/faq)
