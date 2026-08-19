---
title: 模型连通性测试
description: AI Switch 的模型测试会发一次真实的生成请求，记录改写后的上游请求体、原始响应与提取的文本；同时说明如何从上游拉取真实模型列表。
---

# 模型连通性测试

配好一条凭据之后，最想知道的是"它到底能不能出话"。AI Switch 的模型连通性测试给出的答案是确定的：**它发的是一次真实的生成请求，不是探活。**

请求内容写死在代码里：

```rust
pub const MODEL_TEST_PROMPT: &str = "Reply with exactly: ai-switch-ok";
```

模型被要求原样回一句 `ai-switch-ok`，输出上限 16 token，温度 0。这样既能确认整条链路（鉴权、协议桥接、模型名映射、响应解析）真的通了，又几乎不消耗配额。

## 请求长什么样

请求的形状由**平台**决定，而不是由上游方言决定——因为测试要模拟的是本地 CLI 的入口请求，剩下的改写交给协议桥接完成。

| 平台 | 入口路径 | 请求体 |
| --- | --- | --- |
| `codex` | `/responses` | `{"model": …, "input": "<prompt>", "temperature": 0, "max_output_tokens": 16}` |
| `claude` | `/v1/messages` | `{"model": …, "messages": [{"role": "user", "content": "<prompt>"}], "max_tokens": 16}` |
| `gemini` | `/v1beta/models/{model}:generateContent` | `{"contents": [...], "generationConfig": {"temperature": 0, "maxOutputTokens": 16}}` |

其余平台（Grok、OpenCode、OpenClaw、Hermes）没有固定入口协议，按凭据的 `interface_format` 选形状：

| `interface_format` | 路径 | 请求体形状 |
| --- | --- | --- |
| `openai` | `/chat/completions` | `messages` + `temperature: 0` + `max_tokens: 16` |
| `openai-responses` | `/responses` | `input` + `temperature: 0` + `max_output_tokens: 16` |
| `anthropic` | `/v1/messages` | `messages` + `max_tokens: 16` |
| `gemini` | `/v1beta/models/{model}:generateContent` | `contents` + `generationConfig` |

模型名的选择顺序是：请求里显式指定的模型 → 凭据的模型映射 → 平台/方言的内置默认值（`anthropic` → `claude-sonnet-4-20250514`、`gemini` → `gemini-2.5-flash`、`grok` 平台 → `grok-4.5`、其余 → `gpt-5.5`）。占位映射（空值或字面量 `upstream-model`）会被剔除，不参与选择。

### 测试和真实流量走同一条路

关键的一点：构造好入口请求之后，测试调用的是**代理转发用的同一个上游请求构造函数**。也就是说模型映射、自定义工具兼容、托管工具剥离、协议桥接、方言鉴权装配全都会照常执行。

所以结果里记录的 `request_body_json` **是桥接之后的上游请求体**，不是你在界面上看到的那份入口请求。这正是排查桥接问题时最需要的东西——你能直接看到 Responses 请求被改写成 Chat Completions 之后到底长什么样。

响应回来时同样先过一遍桥接的反向转换，再去提取文本。

### 方言覆盖

测试时可以临时指定一个方言，用来试探"这个网关到底说哪种协议"，但可选范围是受限的：

| 平台 | 允许覆盖成 |
| --- | --- |
| `codex`、`claude` | `openai`、`openai-responses`、`anthropic`、`gemini`（四种全开） |
| `gemini` | 只允许 `gemini` |
| `grok`、`opencode`、`openclaw`、`hermes` | 只允许 `openai` |

超出范围返回 `validation.route_model_test_interface_format`。

## 两种测试路径

### 直连上游

默认路径。选号之后直接从应用进程发出 HTTP 请求到上游，30 秒超时。

选号逻辑跟真实转发一致：指定了账号 ID 就测那一个；没指定就走池内候选，按优先级分组轮转，逐个尝试取并发租约。所有账号都占满时返回 `route_pool.concurrency_exhausted`；池里没有可用账号时返回 `validation.route_pool_empty`。

### 经本地代理

另一条路径把请求打到**本地代理的入口地址**上，用平台的本地代理 key 鉴权，额外带一个 `x-ai-switch-test-trace-id` 请求头。

这条路径验证的是"CLI 打过来会不会通"，而不只是"凭据能不能用"——代理监听、平台识别、选号、桥接全都在链路里。它只在不指定具体账号时生效；指定了账号会退回直连模式。

因为请求是代理内部选号的，测试端事先并不知道会命中哪个账号。所以请求结束后会用 trace ID 反查：扫最近 50 条 `source_label = 'route_proxy'` 的请求事件，找到 `metadata_json.trace_id` 匹配的那一条，从里面读出实际命中的账号 ID、账号名和目标 URL。

## 重试与失败判定

测试的重试次数、间隔和语义失败阈值来自账号的失败策略（`config_json.failure_policy`），默认重试 2 次、间隔 200 ms。

重试规则里有两条硬性例外：

- **401 / 403 永不重试。** 鉴权失败重试没有意义，只会更快触发上游风控。
- **确定性的配额耗尽不重试。** 语义失败被识别为配额耗尽时直接短路，不再尝试。

除此之外，非 2xx 状态码和语义失败（响应体结构上是失败，但 HTTP 状态是 200 那种）都会触发重试。流式请求如果在完成事件之前断流，也算一次语义失败。

### 成功与失败分别做什么

**成功：**

- 清空瞬时失败计数与退避窗口
- 如果账号当前是 `error` 或 `warning`，拉回 `ok`
- 如果是针对单个账号的显式测试，额外执行"显式测试恢复"，把账号完整恢复进池

**失败**按类型分流：

| 判定 | 结果 |
| --- | --- |
| 配额耗尽 | 状态直接写 `error` |
| 非 2xx HTTP | 记一次 `model_test_status` 瞬时失败 |
| 语义失败 | 记一次 `semantic_response_transient` 瞬时失败 |
| 永久性失败（如凭据已吊销） | 状态写 `revoked` |
| 其他可重试失败 | 记一次 `model_test` 瞬时失败 |

瞬时失败会带上退避窗口，具体阈值和时长见 [稳定性与自动恢复](/guide/reliability)。

**`paused` 的账号可以被测试。** 代码里对此有明确注释：显式测一次正是用户判断暂停中的账号是否已恢复的方式，成功即恢复。

## 结果里有什么

一次测试返回的结果字段：

| 字段 | 内容 |
| --- | --- |
| `platform` | 平台 |
| `selected_account_id` / `selected_account_name` | 实际命中的账号 |
| `via_route_proxy` | 是否走的本地代理路径 |
| `route_proxy_entry_url` / `route_proxy_entry_path` / `route_proxy_trace_id` | 代理路径专属信息 |
| `interface_format` | 实际使用的上游方言 |
| `request_path` | 入口路径 |
| `base_url` / `target_url` | 凭据的 base URL 与最终请求的完整 URL |
| `request_body_json` | **桥接之后的上游请求体**，格式化输出 |
| `response_status` | HTTP 状态码（传输层失败时为空） |
| `response_body` | 原始响应体，上限 16 KiB |
| `response_text` | 从响应里提取出的模型回复文本 |
| `error_message` | 错误信息 |
| `success` / `duration_ms` | 是否成功、耗时 |
| `stats` | 该平台的完整用量统计快照 |

`response_text` 的提取路径按方言不同：

| 方言 | JSON 指针（按顺序尝试） |
| --- | --- |
| `openai` / `openai-responses` | `/choices/0/message/content` → `/output_text` → 遍历 `/output[]/content[]/text` |
| `anthropic` | `/content/0/text` |
| `gemini` | `/candidates/0/content/parts/0/text` |

### 敏感值脱敏

响应体、错误信息在写入数据库之前都会做替换式脱敏：从凭据的 secret 载荷里取出所有敏感键的值（`api_key`、`access_token`、`refresh_token`、`id_token`、`authorization`、`x-api-key`），在文本里逐一替换成 `[redacted]`。所以哪怕上游把 key 原样回显在错误信息里，也不会落库。

### 每次测试都会记一条用量事件

测试结果会写进 `usage_events` 表，`source_label` 为 `route_pool_model_test`，`metadata_json` 里带：

```json
{
  "source": "ui_model_connectivity_test",
  "request_kind": "model_connectivity",
  "platform": "codex",
  "route_credential_id": "…",
  "route_credential_name": "…",
  "interface_format": "openai",
  "path": "/responses",
  "base_url": "…",
  "target_url": "…",
  "status": 200,
  "success": true,
  "duration_ms": 812,
  "request_body_json": "…",
  "response_body": "…",
  "response_text": "ai-switch-ok",
  "error_message": null
}
```

上游返回的 usage 信息也会被解析成 token 与费用拆分一并入库。因为测试事件和真实转发事件写在同一张表里，统计页面上看到的请求数会包含你手工点的每一次测试——这一点在看数据时要留意。详见 [用量与请求统计](/guide/usage-stats)。

## 拉取上游模型列表

模型测试需要一个模型名，而第三方网关提供哪些模型往往只有它自己知道。所以还有一个独立的模型列表拉取功能，直接问上游要清单。

要求 base URL 和 API Key 都不为空，15 秒超时。候选 URL 按方言依次尝试：

| 方言 | 候选 URL（按顺序） | 鉴权 |
| --- | --- | --- |
| `openai` / `openai-responses`（默认） | `{base}/models` | `Authorization: Bearer` + Codex CLI 客户端标识 |
| `anthropic` | `{base}/v1/models` → `{base}/models` | `x-api-key` 或 `Authorization: Bearer`，加 `anthropic-version: 2023-06-01`、`anthropic-beta` 与 Claude Code 客户端标识 |
| `gemini` | base 已以 `/v1beta` 或 `/v1` 结尾时用 `{base}/models`；否则 `{base}/v1beta/models` → `{base}/v1/models` | key 放查询参数，主动移除 `Authorization` 与 `x-api-key` |

失败处理很克制：**只有 404 和 405 会继续尝试下一个候选**，其他非 2xx 立刻返回 `validation.route_models_http`，不做无意义的重试。所有候选都失败返回 `validation.route_models_all_failed`。

### 响应解析

上游返回的结构五花八门，解析逻辑做了递归归一：

- 容器键依次识别 `data`、`models`、`items`，递归展开
- 模型 ID 依次尝试 `id`、`name`、`model`、`slug`
- 归属信息依次尝试 `owned_by`、`ownedBy`、`provider`、`display_name`、`displayName`
- 长上下文标记识别 `supports_1m` / `supports1m`
- Gemini 风格的 `models/gemini-2.5-flash` 前缀会被剥掉
- 纯字符串数组也能解析
- 结果按 ID 排序并去重

拉取到的列表会写进凭据的 `config_json.fetched_models`，编辑账号时可以直接从下拉里挑模型，不必每次重新拉。

## 实时请求日志

模型测试给你的是一次请求的结果快照。如果要看**持续的**流量，代理侧还有一个实时请求日志，按四个阶段捕获每个转发请求：客户端原始请求、改写后的上游请求、上游原始响应、返回给客户端的最终响应。

它是内存环形缓冲，容量 100 条，单阶段正文上限 64 KiB，完全不落盘，且只在有订阅者时才推送事件。细节见 [协议路由与桥接](/guide/protocol-routing)。

## 下一步

- [协议路由与桥接](/guide/protocol-routing)：理解 `request_body_json` 里那份上游请求体是怎么来的
- [账号与算力池](/guide/accounts)：模型映射与凭据字段
- [稳定性与自动恢复](/guide/reliability)：测试失败之后账号会怎样
- [常见问题](/faq)
