---
title: AI Switch 是什么
description: AI Switch 是一个开源的 AI 供应商与账号切换工具，用一个本地代理统一管理 Codex、Claude Code、Gemini CLI、Grok 等 7 个平台的账号，提供算力池调度与四种上游协议桥接。
---

# AI Switch 是什么

AI Switch 是一个开源的 AI 供应商与账号切换工具，有桌面端和自托管 Web 服务两种形态。它在你的机器上跑一个本地路由代理，把各个 AI CLI 的请求接下来，再按你配置的优先级和并发规则转发到某个上游账号。

对你来说，最直接的变化是：**CLI 只需要指向一个固定的本地地址，账号、供应商、协议的差异都在 AI Switch 里解决。**

## 它解决什么问题

如果你同时用几个 AI CLI，大概会遇到这些情况。

**每个 CLI 各自管账号。** Codex 的配置在 `~/.codex/config.toml`，Claude Code 在 `~/.claude/settings.json`，Gemini CLI 在 `~/.gemini/settings.json`，Grok 在 `~/.grok/settings.json`。同一个中转站的 key 要在几个地方分别填一遍，改一次要改几个文件，格式还各不相同。

**换供应商就要改配置。** 想从一个中转站换到另一个，得找到对应的配置文件、改 base URL、改 key，改错了 CLI 直接起不来。想临时试一下另一家，成本高到你根本不会去试。

**额度用完要手动切。** 某个账号被限流或者额度耗尽，CLI 那边只会给你一个 429 或者 5xx。你得自己意识到发生了什么，然后回到配置文件里换成另一个账号，再重启 CLI。

**协议不通就用不了。** 手上有一个只支持 OpenAI Chat Completions 的中转账号，但你想用 Codex —— Codex 说的是 Responses 协议，两边对不上，这个账号就闲置了。

AI Switch 的做法是把这些差异收敛到一个地方：账号集中管理，切换由代理自动完成，协议不一致的时候在代理里做转换。CLI 的配置写一次就不用再动。

## 核心概念

理解这五个概念，基本就理解了 AI Switch 的工作方式。

### 路由账号

一个**路由账号**是一份可以用来发请求的上游凭据。最常见的是 API 账号：base URL + API key + 上游协议（接口格式）三件套。

每个账号自己带一组路由参数：

- **路由优先级** 1-5，默认 3，数字越小越优先
- **最大并发数** 默认 1，最小 1
- **失败处理策略**：额外重试次数、重试间隔、异常触发次数
- **模型映射**：把客户端请求的模型名映射到上游真实支持的模型
- **自动恢复**：关闭 / 每日定时 / 探活恢复

账号除了 API 类型，还有从官方登录态导入的类型。两者都能进入算力池参与路由。

详见 [账号与算力池](/guide/accounts)。

### 算力池

**算力池**是当前平台参与路由的账号集合。请求进来的时候，代理从池子里挑一个账号用。

挑选规则是**严格优先级分层 + 层内轮转**：

1. 先按 `route_priority` 升序分组，优先级 1 的一批账号排在优先级 2 前面
2. 同一优先级组内部按游标做轮转（round-robin），游标每完成一个请求前进一位，同层账号的负载因此摊平
3. 每次尝试都要先拿到并发额度。账号已经跑满 `max_concurrency` 就跳过它，换下一个
4. 状态不是正常、已归档、额度耗尽、正在冷却的账号会被过滤掉
5. 请求的模型如果没有任何池内账号支持，直接返回 `route_pool.model_unmatched`

失败的时候不用你介入：可重试的错误（连接失败、超时、408 / 429 / 5xx）按账号自己的策略重试，重试次数用完就换下一个账号。401 / 403 不在同一账号上重试，直接走切换逻辑。

所以「额度用完手动切」这件事在 AI Switch 里是自动的。你只需要把备用账号放进池子、设好优先级。

### 本地代理

**本地路由代理**是 CLI 实际连接的对象，绑定在 `127.0.0.1`，默认端口 **19527**。

它按平台区分本地入口协议：

- **Codex** 走 `/responses`（OpenAI Responses）
- **Claude** 走 `/v1/messages`（Anthropic Messages）
- **Gemini CLI** 保持 Gemini native

代理对每个平台生成一个本地代理 key，形如 `sk-ai-switch-<uuid>`。CLI 用这个 key 认证（`Authorization: Bearer`、`x-api-key` 或 `x-goog-api-key`），代理据此识别是哪个平台的请求。这个 key 只用于本地认证，**永远不会转发到上游**。

代理也可以启用本地 HTTPS（在设置里配置，会生成并导入根证书）。

### 协议桥接

**协议桥接**是 AI Switch 让「协议不通」的账号可用的方式。

上游协议（dialect）有四种：

| dialect | 上游接口 |
| --- | --- |
| `openai` | Chat Completions |
| `openai-responses` | Responses API |
| `anthropic` | Messages API |
| `gemini` | generateContent |

本地入口协议和上游账号协议不一致时，代理做请求和响应的双向转换，一共 7 条链路：

`ResponsesToChat`、`ResponsesToResponses`、`ResponsesToAnthropic`、`ResponsesToGemini`、`ClaudeToChat`、`ClaudeToResponses`、`ClaudeToGemini`。

举个具体的例子：你只有一个 Chat Completions 的中转账号，但想用 Codex。Codex 发 `/responses`，代理走 `ResponsesToChat`，把请求改写成 Chat Completions 发给上游，再把响应转回 Responses 格式。Codex 那边完全不知道中间发生了什么。

流式响应同样会被转换。详见 [协议路由与桥接](/guide/protocol-routing)。

### 平台

**平台**是 AI Switch 认识的目标 CLI，一共 7 个：

- **原生支持**：Codex、Claude Code、Gemini CLI、Grok
- **通用 API 路由**：OpenCode、OpenClaw、Hermes

区别在于能力范围。原生支持的四个平台可以由 AI Switch 直接写入配置文件、导入官方账号；后三个只做通用 API 路由，需要你显式提供 base URL 和接口格式。

完整的 7 平台 × 10 能力对照表见 [平台支持矩阵](/guide/platform-support)。

## 适合谁用

**同时用多个 AI CLI 的人。** 账号在一个地方管，四个 CLI 的配置由 AI Switch 写，不用记哪个文件是什么格式。

**手上有多个中转账号的人。** 把它们全放进算力池，设好优先级，限流和额度耗尽的切换交给代理。主力账号优先级设 1，备用设 3，主力挂了自动降级。

**账号协议和想用的 CLI 不匹配的人。** 协议桥接让 Chat Completions 的账号也能喂给 Codex，Gemini 的账号也能喂给 Claude Code。

**关心花了多少钱的人。** 每个请求都记录输入 / 输出 / 缓存 token 和价格，按账号、按时间窗口统计。见 [用量与请求统计](/guide/usage-stats)。

**需要在浏览器或手机上访问的人。** Web 服务模式让同一套界面在浏览器里跑。

**想在终端里直接干活的人。** 内置终端工作区、会话管理、MCP 服务器管理和技能管理。见 [Vibe 终端与皮肤](/features/vibe) 和 [会话管理](/features/sessions)。

## 桌面端和 Web 服务的关系

这是同一个程序的两种用法，**不是两个产品**。

桌面端和浏览器共用同一套 React 界面，也共用同一个 Rust 核心。区别只在传输层：

- **桌面端**通过 Tauri IPC 调用核心
- **浏览器模式**通过 HTTP 调用：`POST /api/:command` 和 `GET /ws/events`，两个端点都需要访问令牌

Web 服务默认绑定 `127.0.0.1:3090`。这里有一条硬性的安全约束：**在非 loopback 地址上监听而没有启用 TLS，服务会直接拒绝启动**，报 `web.sensitive_transport_requires_tls`。想让局域网或者远程访问到，要么配好 TLS，要么走 Tailscale 这类私有网络。

有两种跑 Web 服务的方式：

1. **桌面端内置** —— 在桌面应用的设置里开启 Web 服务，适合「桌面端常开，偶尔用手机看一眼」
2. **独立服务器** —— 单独的 `ai-switch-server` 二进制，不需要桌面环境，适合放在 NAS 或者服务器上

两种方式共享同一个数据目录格式。详见 [Web 服务模式](/deploy/web-service) 和 [独立服务器](/deploy/standalone-server)。

## 技术构成

- **前端**：Tauri 2 + React 18 + TypeScript
- **核心**：Rust crate `ai_switch_lib`，用到 axum、sqlx、rustls、rcgen、reqwest、portable-pty
- **数据**：SQLite，23 个迁移，位于 `src-tauri/migrations`
- **Sidecar**：Go 写的 `ai-switch-tsnet`，基于 Tailscale tsnet + Funnel，用于私有远程访问
- **独立二进制**：`ai-switch-server`，给浏览器和移动端用

架构细节见 [架构总览](/dev/architecture)。

## 一个说明

AI Switch 支持导入其他工具的账号格式（兼容导入协议），但这是通过研究**公开行为、公开文档和公开文件格式**实现的。项目本身不复用第三方代码。

## 下一步

- [安装](/guide/installation) —— 下载对应平台的安装包，了解数据存在哪里
- [快速开始](/guide/quick-start) —— 从添加第一个账号到跑通第一条请求
- [平台支持矩阵](/guide/platform-support) —— 确认你的 CLI 支持到什么程度
