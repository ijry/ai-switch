---
title: 协议路由与桥接
description: AI Switch 本地代理如何把 Codex、Claude Code、Gemini CLI 的原生请求，桥接成 Chat Completions、Responses、Anthropic Messages、Gemini generateContent 四种上游方言。
---

# 协议路由与桥接

大部分账号切换工具解决的是"改配置文件"的问题：把 CLI 的 base URL 和 key 换成另一个供应商的。这条路的天花板很低——**Codex 只会说 Responses API，Claude Code 只会说 Anthropic Messages API**，如果你手上的第三方 key 是 OpenAI Chat Completions 格式的，改配置也没用，CLI 发出去的请求上游根本不认。

AI Switch 的做法是在中间插一个本地 HTTP 代理：CLI 继续讲它的母语，代理负责把请求改写成上游账号听得懂的方言，再把上游的回答改写回 CLI 期待的形状。**同一个 Codex 客户端，因此可以打到 Chat Completions 网关、Responses 网关、Anthropic 网关，甚至 Gemini 上去。**

## 本地代理

代理只监听回环地址：

```rust
const BIND_HOST: &str = "127.0.0.1";
const DEFAULT_ROUTE_PROXY_PORT: u16 = 19527;
```

默认端口 **19527**。如果端口已被占用，绑定逻辑会从 19527 向上递增扫描直到找到可用端口，所以实际端口需要以运行时状态里的 `base_url` 为准。

开启本地 HTTPS 后，HTTPS **不与 HTTP 共用端口**：HTTP 继续在原端口服务，HTTPS 从「HTTP 端口 + 1」起再绑一个（常态即 19527/19528），两个监听器共享同一套路由逻辑与凭据池。写入客户端配置的一直是 HTTP 地址——自带 CA bundle 的客户端（macOS/Linux 的 curl、Node 版 CLI）读不到装进系统信任库的本地根证书，写 `https://` 会让它们直接失效。HTTPS 地址（运行时状态里的 `https_base_url`）留给确实需要 TLS 的场景手动填写。HTTPS 起不来只记录原因，HTTP 不受影响。

### 平台识别

代理是所有平台共用一个端口的，所以进来的每个请求都要先判断"这是哪个 CLI"。识别顺序是：

1. **本地代理 key**。从 `Authorization: Bearer`、`x-api-key` 或查询参数里取出入站 key，查平台映射表。key 表在内存里缓存，TTL 30 秒；缓存里没命中会再查一次数据库，所以刚写入的新 key 不必等 TTL 过期。
2. **`x-ai-switch-platform` 请求头**。手工调试或自定义客户端可以直接指定平台。
3. **都没有就报错**。带了 key 但查不到映射，返回 `route_proxy.key_invalid`；连 key 都没带，返回 `route_proxy.platform_unresolved`。两者都是 401，带 `WWW-Authenticate: Bearer`。

入站 key **只用于本地认证，绝不会转发到上游**：转发前会显式剥掉代理认证请求头和查询参数，再按上游账号的方言重新装上真正的凭据。

### 各 CLI 的接入方式

写入 CLI 配置时，AI Switch 只覆盖自己那一块，其余配置原样保留（TOML 用 `toml_edit` 保留注释与未托管段落，JSON 保留原有缩进风格）。

| 平台 | 配置文件 | 写入内容 |
| --- | --- | --- |
| Codex | `~/.codex/config.toml` | `model_provider = "ai-switch"`、`model_catalog_json = "ai-switch-model-catalog.json"`，以及 `[model_providers.ai-switch]` 段：`base_url = "<proxy>/v1"`、`wire_api = "responses"`、`experimental_bearer_token = "<代理 key>"` |
| Claude Code | `~/.claude/settings.json` | `env.ANTHROPIC_BASE_URL`、`env.AI_SWITCH_ROUTE_PROXY`、`env.AI_SWITCH_ROUTE_PROXY_API_KEY`，以及 `aiSwitch.routeProxy.{enabled,baseUrl,platform,apiKey}` |
| Gemini CLI | `~/.gemini/settings.json` | 同上结构，base URL 环境变量为 `GEMINI_API_BASE_URL` 与 `GOOGLE_GEMINI_BASE_URL` |
| Grok | `~/.grok/settings.json` | 同上结构，base URL 环境变量为 `XAI_API_BASE_URL` 与 `GROK_API_BASE_URL` |

Codex 的 provider 段落里刻意用 `experimental_bearer_token` 而不是 `api_key`：渲染时会主动删掉遗留的 `api_key` 键，避免两种写法同时存在。

OpenCode、OpenClaw、Hermes **没有配置写入适配器**，只能通过路由凭据参与算力池，需要自行把客户端指向代理地址。详见 [平台支持矩阵](/guide/platform-support)。

## 四种上游方言

上游账号的协议方言由凭据的 `interface_format` 字段决定，只有四个合法值：

| 方言值 | 协议 | 上游路径 | 鉴权方式 |
| --- | --- | --- | --- |
| `openai` | Chat Completions | `/chat/completions` | `Authorization: Bearer <key>` |
| `openai-responses` | Responses API | `/responses` | `Authorization: Bearer <key>` |
| `anthropic` | Messages API | `/v1/messages` | `x-api-key` 或 `Authorization: Bearer`（按 `api_key_field`） |
| `gemini` | generateContent | `/v1beta/models/{model}:generateContent` | URL 查询参数 `?key=<key>` |

各方言还带了自己的附加处理：

- **`anthropic`**：自动补 `anthropic-version: 2023-06-01`（客户端已提供则不覆盖）；对 messages 路径追加 `?beta=true`；套用 Claude Code 的客户端标识，避免做客户端指纹校验的网关把请求当成未知客户端拒掉。
- **`openai` / `openai-responses`**：套用 Codex CLI 的客户端标识，同样是为了过指纹校验；请求路径会剥掉前导版本段（`/v1/...` → `/...`），再由 base URL 拼回去。标识是整套替换而不是逐项补齐：客户端自己就是官方 Codex（`user-agent` 属于官方客户端家族、且能解出 `X.Y.Z` 引擎版本）时原样保留，只补上与它配套的 `originator`；否则整套换成我们的，并清掉客户端带来的 Anthropic SDK 标识——半个 Codex 身份比任何一种完整身份都更容易被拒。另外补一个 `x-codex-window-id` 引擎指纹头（客户端已带任意 `x-codex-*` 时不补）：中转站的「仅官方 Codex 客户端」限制默认要求它，缺了整个账号每轮都会拿到 `This account only allows Codex official clients`。账号自己配了 `User-Agent` 时这套伪装整体让路，不给它拼半个身份。
- **`gemini`**：key 放查询参数，不放请求头。流式请求走 `:streamGenerateContent` 并附加 `alt=sse`。

不管走哪个方言，出站请求都会被强制加上 `accept-encoding: identity`。原因写在代码注释里：出站 HTTP 客户端是不带任何解压特性编译的，一旦上游返回 gzip/br/zstd，中转和解析环节看到的就是乱码。

## 七条桥接链路

桥接种类由一个枚举穷举，只有七条：

```rust
pub enum ProtocolBridgeKind {
    ResponsesToChat,
    ResponsesToResponses,
    ResponsesToAnthropic,
    ResponsesToGemini,
    ClaudeToChat,
    ClaudeToResponses,
    ClaudeToGemini,
}
```

按"本地入口 × 上游方言"排成矩阵：

| 本地入口 | 上游 `openai` | 上游 `openai-responses` | 上游 `anthropic` | 上游 `gemini` |
| --- | --- | --- | --- | --- |
| Codex `/responses` | `ResponsesToChat` | `ResponsesToResponses` | `ResponsesToAnthropic` | `ResponsesToGemini` |
| Claude `/v1/messages` | `ClaudeToChat` | `ClaudeToResponses` | 直通（无桥接） | `ClaudeToGemini` |
| 其他入口 / 其他路径 | 直通 | 直通 | 直通 | 直通 |

每条链路都要做双向转换：请求方向把入口格式改写成上游格式，响应方向把上游回答改写回入口格式。响应转换是按 `kind` 分发的，所以每条链路都有自己成对的转换实现。

### 桥接在什么时候发生

判定条件很窄，只有两个分支会命中桥接：

```rust
if platform == PlatformId::Codex && is_responses { /* … */ }
if platform == PlatformId::Claude && is_messages { /* … */ }
```

其余所有组合都落到 `passthrough_request`——请求体原样转发，只做路径规范化。这意味着：

- **入口路径判定会跳过版本段。** `/responses`、`/v1/responses`、`/v1/v1/responses` 都算 Responses 创建路径；messages 同理。
- **只有创建端点会被桥接。** Codex 打 `/v1/chat/completions` 这类非创建路径时不桥接。
- **Claude → `anthropic` 是纯直通**，桥接种类为空。同协议不需要翻译。
- **Codex → `openai-responses` 仍然算一条桥接**（`ResponsesToResponses`）。虽然两边都是 Responses API，但第三方 Responses 网关的实现差异需要一层清洗，所以它不是简单的直通。
- **Gemini CLI 入口的流量永远不桥接。** 判定分支里没有 `PlatformId::Gemini`，所以 Gemini CLI 发出的请求一律直通。这也是当前的能力边界：Gemini CLI 只能路由到 `gemini` 方言的账号。模型测试的方言校验里同样写死了这一点——平台 `gemini` 只允许 `gemini` 方言。

### 流式与非流式

桥接同时处理两种响应形态。是否流式由请求体里的 `stream` 字段决定（缺省视为 `false`）。

非流式走 JSON 结构转换；流式走 SSE 事件流转换，把上游的事件序列重放成入口协议的事件序列。以桥接到 Responses 入口为例，转换后发出的事件包括 `response.created`、`response.in_progress`、`response.output_text.delta`、`response.output_text.done`、`response.completed`，工具调用有 `response.function_call_arguments.delta` / `.done`，推理摘要有 `response.reasoning_summary_text.delta` / `.done`；上游的 `[DONE]` 标记会被剥掉，因为 Responses 协议不用这个终止符。

**非 2xx 响应不做转换**，原样透传。上游报错时你看到的是上游的原始错误体，而不是被改写过的形状——这对排查问题很关键。

## 桥接之外的请求改写

桥接只负责协议形状。同一条转发路径上还叠了几层与协议正交的改写：

### 模型映射

凭据里的 `model_mappings` 在桥接之前生效：请求体里的模型名按 `from` → `to` 替换。这样 CLI 里选的 `gpt-5` 可以被换成上游实际提供的模型 ID，而不用改 CLI 配置。

### 工具命名空间展平

Responses 协议支持把工具组织进 `namespace` 分组，而 Chat Completions 之类的协议只有平铺的函数列表。桥接会把分组展平成 `namespace__tool` 形式的名字，并把映射关系带在请求上下文里；响应回来时按同一张映射表还原回分组结构，客户端察觉不到中间发生了什么。

### 自定义工具兼容与托管工具剥离

第三方 Responses 网关经常不支持 Codex 的自定义工具（custom tool）写法，也不支持 OpenAI 自己托管的那批工具。于是有两层处理：

- `config_json.responses_custom_tool_compat` 打开时，自定义工具被改写成普通 function tool；响应回来时再还原。
- 同一条件下会剥掉网关跑不了的托管工具类型。代码里穷举了七种：`web_search`、`web_search_preview`、`file_search`、`computer_use_preview`、`code_interpreter`、`image_generation`、`container_file_citation`。如果 `tool_choice` 正好钉住了一个被剥掉的工具，会被放宽成 `"auto"`。

**Codex + `openai` 方言 + Responses 路径会自动开启这两层处理**，不需要手工勾选——因为 Responses→Chat 的桥接本身就意味着上游是一个 Chat 网关，一定不认这些写法。

### 推理内容回填

Chat 系的推理模型（DeepSeek、MiMo 一类）要求带工具调用的 assistant 消息上必须有 `reasoning_content`，否则直接 400。但客户端在多轮工具调用里往往会把这个字段丢掉。代理侧维护了一份推理内容缓存，在 Responses→Chat 转换之前把真实的 `reasoning_content`（必要时连整个 `function_call`）回填到工具调用轮次上，避免模型在工具调用之间丢失自己的计划而卡住。

### 模型列表聚合

`/models` 和 `/v1/models` 两个路径不会被转发到上游。代理会把池内所有可用凭据的对外模型 ID 聚合去重，直接以 OpenAI 模型列表格式返回，`owned_by` 固定为 `ai-switch`。Codex 平台还会额外附上 `supported_reasoning_levels` 与 `default_reasoning_level`。这个路径只接受 `GET`，其他方法返回 405 与 `route_proxy.method_not_allowed`。

拉取**上游**账号的真实模型列表是另一回事，见 [模型连通性测试](/guide/model-test)。

## 一次转发的完整顺序

```text
CLI 请求
  ├─ 识别平台（代理 key → x-ai-switch-platform）
  ├─ /models 路径？→ 聚合池内模型列表并直接返回
  ├─ 读取请求体（上限 32 MiB），解析请求的模型名
  ├─ 选号：池内 status=ok、未归档、未冷却、配额未耗尽，按优先级分组轮转
  ├─ 按平台能力规则与模型名再过滤一遍
  ├─ 剥离代理认证头/查询参数，剥离逐跳头，强制 accept-encoding: identity
  └─ 重试队列循环，每个候选账号：
        ├─ 取并发租约（max_concurrency），取不到就跳到下一个
        ├─ 官方凭据按需刷新 access token
        ├─ 模型映射 → 自定义工具兼容 → 托管工具剥离 → 推理内容回填
        ├─ 协议桥接（七条链路之一，或直通）
        ├─ 按方言装配鉴权与客户端标识，拼出目标 URL
        ├─ 发出上游请求
        ├─ 响应经桥接反向转换后返回客户端
        └─ 记录用量事件、更新账号状态、推送实时日志
```

选号规则的细节见 [账号与算力池](/guide/accounts)，失败与退避规则见 [稳定性与自动恢复](/guide/reliability)。

## 实时请求日志

调试桥接问题时，光看客户端收到什么是不够的——你需要看到中间那三步。代理内置了一个实时日志，捕获每个转发请求的**四个阶段**：

1. 客户端原始请求
2. 改写后的上游请求
3. 上游原始响应
4. 返回给客户端的最终响应

日志条目还带 trace ID、命中的账号 ID 与名称、第几次尝试、路径、目标 URL、请求模型与上游模型、状态码、是否成功、错误信息、耗时、命中的桥接种类，以及诊断备注和截断标记。

两个重要限制：

- **完全不落盘。** 这是一个内存环形缓冲，容量 100 条（整个代理共享，不是每平台 100 条），单个阶段的正文上限 64 KiB，超出即截断。
- **只在有人看的时候才推送。** 至少有一个订阅者时才发事件；订阅某个平台会先拿到该平台的历史条目。

正文里的长字符串与敏感字段在写入日志前会被脱敏处理。

## 下一步

- [账号与算力池](/guide/accounts)：凭据字段与选号规则
- [模型连通性测试](/guide/model-test)：用一次真实请求验证整条桥接链路
- [用量与请求统计](/guide/usage-stats)：每次转发记录了哪些指标
- [架构总览](/dev/architecture)：代理在整个系统里的位置
