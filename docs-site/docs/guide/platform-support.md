---
title: 平台支持矩阵
description: AI Switch 支持的 7 个平台与 10 种能力的完整对照表，说明原生支持与通用 API 路由的区别、部分支持的具体含义，以及原生配置写入的安全直写机制。
---

# 平台支持矩阵

AI Switch 认识 7 个目标平台，每个平台在 10 种能力上的支持情况是**在代码里显式声明的**，不是运行时猜的。这一页把完整矩阵列出来，并解释每种能力和每个状态的实际含义。

## 两类支持级别

7 个平台分成两类：

**原生支持**（`supported`）—— Codex、Claude Code、Gemini CLI、Grok

AI Switch 认识这些工具的配置文件格式和官方登录态格式。除了通用 API 路由，它还能直接写入原生配置、导入官方账号、用官方账号路由、处理 deeplink 导入。

**通用 API 路由**（`partial`）—— OpenCode、OpenClaw、Hermes

> OpenCode、OpenClaw、Hermes 保持可见，用于通用 API 路由、终端启动和会话流程，但 AI Switch 不声称对它们支持原生配置、官方账号导入或额度查询。

也就是说，你依然可以给它们配 API 账号并通过本地代理路由，也依然可以从 AI Switch 启动终端、管理会话。但配置需要你自己填，AI Switch 不会去动它们的配置文件。

## 完整矩阵

三种状态：

- **✅ 支持** —— 完整可用
- **◐ 部分支持** —— 可用，但有额外前置条件
- **✕ 不支持** —— 调用会被拒绝

| 能力 | Codex | Claude Code | Gemini CLI | Grok | OpenCode | OpenClaw | Hermes |
| --- | :-: | :-: | :-: | :-: | :-: | :-: | :-: |
| `route_credentials` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `generic_api_routing` | ✅ | ✅ | ✅ | ✅ | ◐ | ◐ | ◐ |
| `config_write` | ✅ | ✅ | ✅ | ✅ | ✕ | ✕ | ✕ |
| `official_import` | ✅ | ✅ | ✅ | ✅ | ✕ | ✕ | ✕ |
| `official_account_routing` | ✅ | ✅ | ✅ | ✅ | ✕ | ✕ | ✕ |
| `deeplink_import` | ✅ | ✅ | ✅ | ✅ | ✕ | ✕ | ✕ |
| `official_quota` | ✅ | ✅ | **✕** | ✅ | ✕ | ✕ | ✕ |
| `model_test` | ✅ | ✅ | ✅ | ✅ | ◐ | ◐ | ◐ |
| `terminal_launch` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `session_resume` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |

::: warning 一个例外：Gemini CLI 的额度查询
矩阵里唯一不按「原生支持 = 全绿」规律的格子是 **Gemini CLI 的 `official_quota`**。

Gemini CLI 是原生支持平台，配置写入、官方导入、官方账号路由都可用，但**官方额度查询不可用**（`capability.quota_unavailable`）。所以 Gemini 账号不会显示官方额度信息，也不能刷新额度。

其余三个原生平台（Codex、Claude Code、Grok）10 项能力全部支持。
:::

## 10 种能力分别是什么

### `route_credentials`

管理该平台的路由账号：创建、编辑、删除、加入或移出算力池。

**7 个平台全部支持。** 这是最基础的能力 —— 任何平台都可以配路由账号。

### `generic_api_routing`

通过本地路由代理转发该平台的 API 请求。

原生四平台完整支持。OpenCode、OpenClaw、Hermes 是**部分支持**，原因码 `capability.api_credentials_only`：

> 仅支持已配置 Base URL 和接口格式的 API 账号。

具体限制有三条：

1. 只接受 `api` 类型的账号，官方登录态类型的账号不参与路由
2. **必须**显式提供 base URL
3. **必须**显式提供接口格式（上游 dialect）

原生四平台有默认 dialect（Codex 和 Grok 是 `openai`，Claude 是 `anthropic`，Gemini 是 `gemini`），这三个平台**没有默认值**，所以你不填就没法用。

注意「部分支持」不等于不能用 —— 它照样能路由，只是不给你省这两个字段。

### `config_write`

把 CLI 的原生配置文件指向本地路由代理。

原生四平台支持，对应的目标文件：

| 平台 | 目标文件 | 格式 |
| --- | --- | --- |
| Codex | `~/.codex/config.toml`（外加 `~/.codex/ai-switch-model-catalog.json`） | TOML |
| Claude Code | `~/.claude/settings.json` | JSON |
| Gemini CLI | `~/.gemini/settings.json` | JSON |
| Grok | `~/.grok/settings.json` | JSON |

后三个平台不支持，原因码 `capability.native_config_unavailable`：

> 该平台的原生配置写入尚未实现。

它们的配置需要你手动填（从界面复制 Base URL 和代理 key）。AI Switch 不会解析也不会修改它们的配置文件。

### `official_import`

导入平台官方的登录态或账号凭据。AI Switch 支持多种输入：OAuth CPA、API Key CPA、session JSON、`auth.json`、Sub2API JSON、accessToken、refresh_token。

原生四平台支持。后三个不支持，原因码 `capability.official_account_unavailable`：

> 该平台不支持官方账号导入或官方账号路由。

### `official_account_routing`

用导入的官方账号（而不是 API key 账号）来路由请求。

原生四平台支持，后三个不支持，同样是 `capability.official_account_unavailable`。这也是为什么它们的 `generic_api_routing` 限定只接受 `api` 类型账号。

### `deeplink_import`

通过 deeplink 导入账号。桌面端注册了 `aiswitch` 这个 URL scheme。

原生四平台支持。后三个不支持，原因码 `capability.deeplink_unavailable`：

> 该平台不支持 Deeplink 导入。

### `official_quota`

查询和刷新官方账号的额度信息。

**Codex、Claude Code、Grok 支持。Gemini CLI 不支持。** OpenCode、OpenClaw、Hermes 也不支持。不支持的原因码统一是 `capability.quota_unavailable`：

> 该平台不支持官方账号额度刷新。

算力池的路由逻辑会参考账号的剩余额度过滤（剩余额度为 0 的账号不参与选择），但这个信息依赖额度查询能力。Gemini 账号拿不到官方额度，所以只能靠失败反馈来发现账号不可用。

### `model_test`

对账号发起真实生成测试。这不是可达性探测 —— AI Switch 会真的让上游生成一段内容，然后展示模型输出和完整的请求链路。

原生四平台完整支持。后三个是**部分支持**，同样是 `capability.api_credentials_only` —— 只能测试配好了 base URL 和接口格式的 `api` 账号。

详见 [模型连通性测试](/guide/model-test)。

### `terminal_launch`

从 AI Switch 启动系统终端并运行该平台的 CLI。

**7 个平台全部支持。**

### `session_resume`

恢复该平台之前的会话，可以在系统终端里恢复，也可以复制恢复命令自己执行。

**7 个平台全部支持。**

详见 [会话管理](/features/sessions)。

## 「部分支持」和「不支持」在行为上的区别

这两个状态的差别很关键，因为它决定了操作会不会被拒绝。

**部分支持（`partial`）的操作是可以调用的。** AI Switch 只是附带了额外约束（必须是 `api` 类型账号、必须有 base URL、必须有接口格式），满足条件就正常执行。界面上会显示原因码对应的提示文字，告诉你为什么有限制。

**不支持（`unavailable`）的操作会被拒绝。** 调用直接返回 `capability.unavailable` 校验错误，消息形如 `Hermes does not support config_write`，并附带具体原因码。界面上对应的按钮会被禁用，鼠标悬停显示原因。

这套检查在服务端强制执行，不只是界面上的置灰 —— 即使绕过界面直接调命令，也会被同一套规则拦住。

## 原生配置写入的安全性

原生四平台的配置写入不是简单的覆盖文件：

> 原生配置写入采用安全直写：变更前建立快照、原子写入、检测并发修改、支持带守卫的回滚。

四层保障：

**变更前建立快照。** 每次写入前先把原文件存一份到 `~/.ai-switch/backups/config-snapshots/`（Unix 上权限 `0700`）。写入结果面板会显示对应的快照 id。

**原子写入。** 不会出现写一半的配置文件。

**检测并发修改。** 如果文件在 AI Switch 读取之后、写入之前被别人改了（你手动编辑、另一个工具改了），会被检测出来，而不是默默覆盖掉。写入结果里带变更前后的哈希。

**带守卫的回滚。** 出问题可以回滚到快照，回滚本身也有守卫检查，不会盲目覆盖当前状态。

另外，写入是**增量**的：AI Switch 只增改自己管理的字段，你在这些文件里的其他配置项会被保留。比如 Claude Code 的 `settings.json` 里已有的 `env` 项和其他设置不会被清掉。

## 平台标识和别名

命令和 API 里用的平台 id 是：`codex`、`claude`、`gemini`、`grok`、`opencode`、`openclaw`、`hermes`。

解析时接受一些别名：

| 平台 id | 接受的别名 |
| --- | --- |
| `codex` | `openai`、`chatgpt` |
| `claude` | `anthropic`、`claude_code`、`claude_desktop`、`claude-code` |
| `gemini` | `google`、`gemini_cli`、`gemini-cli` |
| `grok` | `xai`、`x_ai`、`x.ai` |
| `opencode` | `open_code`、`open-code` |
| `openclaw` | `open_claw`、`open-claw` |
| `hermes` | —— |

解析大小写不敏感，空格和连字符会被规范化成下划线。但**只接受显式别名** —— 像 `my-claude-wrapper` 这种包含平台名的字符串会被拒绝，返回 `platform.unknown`，不会被模糊匹配到 Claude。

## 相关页面

- [账号与算力池](/guide/accounts) —— 账号类型、优先级、并发和池调度
- [协议路由与桥接](/guide/protocol-routing) —— 四种上游协议和 7 条桥接链路
- [模型连通性测试](/guide/model-test) —— 真实生成测试的细节
- [用量与请求统计](/guide/usage-stats) —— 按账号的用量和计费
- [稳定性与自动恢复](/guide/reliability) —— 失败处理和自动恢复
- [会话管理](/features/sessions) —— 终端启动和会话恢复
