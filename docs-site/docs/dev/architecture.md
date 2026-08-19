---
title: 架构总览
description: AI Switch 的分层架构说明：React 界面层、Rust 核心 crate ai_switch_lib、Tauri IPC 与 axum HTTP 双传输、SQLite 存储层与数据目录布局，以及 Go 编写的 Tailscale sidecar。
---

# 架构总览

AI Switch 的核心设计目标是**一份业务逻辑，两种运行形态**。同一套 React 界面既能作为 Tauri 桌面应用运行，也能由浏览器加载；同一个 Rust crate 既被桌面进程链接，也被独立的 HTTP 服务器复用。两者之间只有传输层不同。

```text
┌──────────────────────────────────────────────────────────┐
│  界面层   React 18 + TypeScript + Vite（桌面与浏览器共用） │
└───────────────────────┬──────────────────────────────────┘
                        │  Transport 抽象（运行时探测）
        ┌───────────────┴───────────────┐
        ▼                               ▼
┌────────────────┐            ┌────────────────────────┐
│  Tauri IPC     │            │  axum HTTP + WebSocket │
│  invoke()      │            │  POST /api/:command    │
│  桌面进程内     │            │  GET  /ws/events       │
└───────┬────────┘            └───────────┬────────────┘
        └───────────────┬─────────────────┘
                        ▼
┌──────────────────────────────────────────────────────────┐
│  核心层   Rust crate `ai_switch_lib`                      │
│  services / models / database / mcp / skills / ...        │
└───────────────────────┬──────────────────────────────────┘
                        ▼
┌──────────────────────────────────────────────────────────┐
│  存储层   SQLite（23 个迁移） + `~/.ai-switch` 数据目录     │
└──────────────────────────────────────────────────────────┘
                        │
                        ▼
┌──────────────────────────────────────────────────────────┐
│  sidecar  ai-switch-tsnet（Go + Tailscale tsnet）         │
└──────────────────────────────────────────────────────────┘
```

## 界面层

前端位于仓库根目录的 `src/`，技术栈为 React 18 + TypeScript，由 Vite 5 打包，UnoCSS 提供样式原子类，TanStack Query 管理服务端状态。

| 目录 | 内容 |
| --- | --- |
| `src/screens/` | 15 个顶层界面：账号、批次、仪表盘、导入、MCP、OCR、操作日志、供应商、会话、设置、技能、目标应用、更新、Vibe、加密小工具 |
| `src/components/` | 按域划分的组件目录：accounts、auth、batches、brand、deeplink、imports、layout、mcp、platform、settings、skills、terminal、ui、updates、vibe |
| `src/lib/transport/` | 传输抽象层，桌面/浏览器差异只在这里 |
| `src/lib/api/` | 命令客户端、命令可用性探测、错误映射 |
| `src/lib/ocr/`、`src/lib/query/` | 本地 OCR 与查询客户端配置 |
| `src/skins/` | 3 个内置 Vibe 皮肤：`codex-2007-blue`、`rescue-pups-adventure-bay`、`starship-cockpit` |

终端界面基于 `@xterm/xterm`，Vibe 皮肤的三维场景使用 `three`，图标来自 `lucide-react`，导入导出的压缩包处理用 `jszip`。

界面代码不感知自己跑在哪里。`src/lib/transport/detect.ts` 通过 `window.__TAURI_INTERNALS__` 判定运行时，`getTransport()` 据此返回 `TauriTransport` 或 `WebTransport`。所有业务组件只调用统一的 transport 接口，因此新增一个命令时前端只需写一次。

## 传输层

### 桌面：Tauri IPC

`src-tauri/src/lib.rs` 的 `run()` 里通过 `tauri::generate_handler!` 注册了 **87 个命令**，覆盖设置、账号与凭据、算力池、路由代理、HTTPS 证书、会话、目标应用、终端、Web 服务、Tailscale、MCP 与技能。命令实现集中在 `src-tauri/src/commands/` 的 13 个模块中，它们大多只做参数解析，真正的逻辑落在 `services/`。

`setup()` 阶段还会启动几个常驻任务：托盘菜单与窗口隐藏行为、按配置自动拉起 Web 服务、按配置恢复路由代理、以及 `RouteRecoveryService::run_loop`（周期性按恢复规则重新启用账号）。

### 浏览器：axum HTTP + WebSocket

`src-tauri/src/web/` 提供等价的 HTTP 表面：

| 端点 | 方法 | 说明 |
| --- | --- | --- |
| `/api/:command` | `POST` | 与 Tauri 命令同名同参，由 `web/handlers/mod.rs` 的 `dispatch_command` 分发 |
| `/ws/events` | `GET` | 事件推送通道，对应桌面端的 Tauri 事件 |
| `/health` | `GET` | 健康检查，唯一无需令牌的端点 |
| 其他路径 | `GET` | 回落到静态资源，用于托管前端 `dist` |

安全相关的实现细节：

- `web/auth.rs` 的 `authorize_api_request` 中间件对 `/api/*` 与 `/ws/events` 强制校验 Bearer 令牌，且在 JSON 提取器之前生效——非法请求不会被解析正文。
- **11 个敏感命令**（凭据导出/导入预览/导入、读取代理密钥、MCP 的四个写入命令、技能的三个写入命令）额外经过 `gate_sensitive_commands` 闸门；当传输不满足安全要求时它们直接返回 404 而不是 403，避免暴露命令存在性。
- 请求体上限 12 MiB（`SENSITIVE_COMMAND_BODY_LIMIT`）。
- 所有 `/api/*` 响应带 `Cache-Control: no-store` 与 `Pragma: no-cache`。
- CORS 只允许 `GET`/`POST`/`OPTIONS` 与 `Authorization`/`Content-Type` 头，预检不绕过鉴权。

`web/event_bridge.rs` 的 `EventEmitter` 是双传输的关键抽象：同一段服务代码调用 `emit`，桌面下走 Tauri 事件，浏览器下走 `WebEventBroadcaster` 的 broadcast 通道（容量 4096）再落到 WebSocket。

## 核心层：`ai_switch_lib`

`src-tauri/Cargo.toml` 把 crate 命名为 `ai_switch_lib`，同时产出 `staticlib`/`cdylib`/`rlib`，并定义两个可执行目标：

- `ai-switch`（`src/main.rs`）——Tauri 桌面应用
- `ai-switch-server`（`src/bin/ai_switch_server.rs`）——独立 HTTP 服务器，主函数只有一行 `server::run_from_env()`

顶层模块划分（`src-tauri/src/lib.rs`）：

| 模块 | 职责 |
| --- | --- |
| `app_state` | 全局状态容器：数据库池、路径、各运行时状态、终端管理器、事件广播器 |
| `commands` | Tauri 命令入口（13 个模块） |
| `web` | axum 路由、鉴权、WebSocket、静态资源、事件桥 |
| `server` | 独立服务器的引导逻辑、环境变量解析、回环地址判定 |
| `services` | 业务服务层，占代码量的绝大部分 |
| `models` | 领域模型与序列化契约（12 个模块） |
| `database` | SQLite 连接池、迁移执行与 11 个仓储 |
| `adapters` | 各 CLI 的配置文件读写适配器 |
| `config_writer` | 带快照与哈希校验的安全写入原语 |
| `core` | 桌面与 Web 共用的 `*_core` 函数（会话、设置、终端） |
| `mcp` | MCP 服务器管理与 11 个客户端适配器 |
| `skills` | 技能包的读写、frontmatter 解析与路径校验 |
| `importers` | 示例 JSON 等外部格式导入 |
| `session_manager` | 扫描各 CLI 的本地会话文件并解析消息 |
| `terminal_manager` | 基于 `portable-pty` 的 PTY 会话池 |
| `security` | `SecretStore` trait 与一个未接线的 keyring 实现（当前未生效） |
| `paths` | `~/.ai-switch` 目录布局 |
| `error` | 统一的 `AppError` / `ApiError` 结构化错误 |

### services 分组

`src-tauri/src/services/` 是业务重心，按关注点大致分为：

**账号与凭据**
`route_credential_service`、`route_credential_activity`（并发与活动登记）、`route_quota_service`、`route_recovery_service`、`route_credential_transfer_service`、`route_credential_transfer_codec`、`route_credential_transfer_import_service`、`batch_service`

**路由与代理**
`route_proxy_service`（本地入口，7000 余行，含账号选择、失败判定、重试与用量记账）、`route_pool_service`、`route_config_service`、`route_preview_service`、`route_proxy_live_log`、`route_proxy_https_service`、`route_proxy_https_trust`、`route_protocol_bridge/`

**模型能力**
`route_model_fetch_service`、`route_model_test_service`、`route_model_capability`、`codex_reasoning_cache`

**导入与互操作**
`import_service`、`cpa_import_service`、`cpa_export_service`、`sub2api_import_service`、`deeplink_service`、`deeplink_protocol_service`

**平台与配置写入**
`platform_capability_service`、`config_write_service`、`target_service`、`official_agent_identity_service`、`client_identity`

**网络与远程**
`web_service`、`tailscale_service`、`tailscale_sidecar`、`tailscale_types`、`http_client`

**其他**
`settings_service`、`response_failure_service`（响应级失败与额度耗尽判定）

### 平台与协议模型

`src-tauri/src/models/platform.rs` 定义了三组关键枚举：

- `PlatformId` —— **7 个平台**：`Codex`、`Claude`、`Gemini`、`Grok`、`OpenCode`、`OpenClaw`、`Hermes`。前四个原生支持，后三个只提供通用 API 路由。
- `ApiDialect` —— **4 种上游协议**：`openai`、`openai-responses`、`anthropic`、`gemini`。`default_api_credential_dialect()` 对 OpenCode / OpenClaw / Hermes 返回 `None`，因此这三个平台的 API 账号必须显式填写 base URL 与协议。
- `PlatformOperation` —— **10 种平台能力**：`route_credentials`、`generic_api_routing`、`config_write`、`official_import`、`official_account_routing`、`deeplink_import`、`official_quota`、`model_test`、`terminal_launch`、`session_resume`。每种能力用 `CapabilityRule` 描述可用性（`Supported` / `Partial` / `Unavailable`）、所需凭据类型，以及是否强制 base URL 与协议。

界面上的每个按钮是否可用，都由 `list_platform_capabilities` 返回的这张能力表驱动，而不是散落在前端的硬编码判断。详见[平台支持矩阵](/guide/platform-support)。

`services/route_protocol_bridge/mod.rs` 的 `ProtocolBridgeKind` 定义了 **7 条桥接链路**：`ResponsesToChat`、`ResponsesToResponses`、`ResponsesToAnthropic`、`ResponsesToGemini`、`ClaudeToChat`、`ClaudeToResponses`、`ClaudeToGemini`。每条链路都有独立的请求改写、响应改写与 SSE 流式转换模块。原理说明见[协议路由与桥接](/guide/protocol-routing)。

### 两个容易混淆的端口

| | 本地路由代理 | Web 服务 |
| --- | --- | --- |
| 默认地址 | `127.0.0.1:19527` | `127.0.0.1:3090` |
| 定义位置 | `services/route_proxy_service.rs` 的 `DEFAULT_ROUTE_PROXY_PORT` | `services/web_service.rs` 默认配置 |
| 服务对象 | 本机的 AI CLI（Codex、Claude Code……） | 浏览器/手机上的 AI Switch 界面 |
| 流量内容 | 模型推理请求，会被改写并转发到上游 | 应用自身的 API 与事件 |
| 鉴权方式 | 路由代理密钥（写入各 CLI 配置） | Web 访问令牌（Bearer） |

这两者互不依赖：只用桌面端管理账号可以不开 Web 服务；只在浏览器里看统计也不必开路由代理。

## 存储层

### SQLite

`database/mod.rs` 用 sqlx 打开连接池（最多 5 连接，开启 `foreign_keys`），`open_migrated_pool` 在启动时执行迁移，失败时会尝试用 `backups/` 做修复。

数据库文件位于 `~/.ai-switch/`。debug 构建刻意使用独立的 `ai-switch-dev.db`，release 构建使用 `ai-switch.db`，因此 `pnpm tauri:dev` 不会污染已安装版本的数据。

`src-tauri/migrations/` 下共 **23 个迁移**，文件名本身就是一部功能演进史：

**第一批（2026-07-13）：奠定基础**

| 迁移 | 引入内容 |
| --- | --- |
| `202607130001_foundation.sql` | `target_apps`、`providers`、`official_accounts`、`batches`、`batch_items`、`import_jobs`、`target_app_states`、`config_snapshots`、`quota_snapshots`、`secure_secrets` |
| `202607130002_mcp_servers.sql` | `mcp_servers` |
| `202607130003_prompt_assets.sql` | `prompt_assets` |
| `202607130004_routing_usage.sql` | `proxy_profiles`、`failover_policies`、`usage_events`、`route_pool_members`、`route_pool_cursors` |
| `202607130005_sync_foundation.sql` | `sync_profiles`、`sync_snapshots` |
| `202607130006_sessions.sql` | `sessions`、`session_events` |
| `202607130007_updater.sql` | `update_channels`、`update_checks` |
| `202607130008_managed_instances.sql` | `managed_instances` |
| `202607130009_wakeup_tasks.sql` | `wakeup_tasks`、`wakeup_runs` |
| `202607130010_bulk_tags_plugins.sql` | `tags`、`item_tags`、`plugin_links`、`bulk_operations` |
| `202607130011_route_credentials.sql` | `route_credentials` 主表，重建 `route_pool_members`，`usage_events` 关联凭据 |

**第二批：路由代理与额度**

| 迁移 | 引入内容 |
| --- | --- |
| `202607210001_route_proxy_keys.sql` | `route_proxy_keys`（本地入口访问密钥） |
| `202607220001_route_credential_quota.sql` | `subscription_type`、`quota_remaining/limit/used`、`quota_updated_at` |
| `202607220002_route_credential_quota_windows.sql` | `primary_remain`、`weekly_remain`、`reset_primary`、`reset_weekly`（多重额度窗口） |
| `202607300001_route_credential_retry.sql` | `transient_failure_count`、`next_retry_at`、`cooldown_until`、`last_failure_kind/message` |

**第三批：安全写入、迁移与统计**

| 迁移 | 引入内容 |
| --- | --- |
| `202608010001_platform_capabilities_safe_writes.sql` | `target_apps`/`config_snapshots` 增加 `platform`，快照增加操作组、来源快照、原文件是否存在、元数据 |
| `202608040001_route_credential_transfer.sql` | `transfer_installation_identity`、`route_credential_transfer_origins`（跨设备迁移溯源） |
| `202608050001_route_credential_archive.sql` | `archived_at`（归档而非删除） |
| `202608060001_route_proxy_key_aliases.sql` | `route_proxy_key_aliases`（密钥别名） |
| `202608060002_route_usage_breakdown.sql` | `usage_events` 增加输入/输出/缓存 token 与美元、人民币计价字段 |

**第四批：更精细的失败与调度**

| 迁移 | 引入内容 |
| --- | --- |
| `202608080001_route_credential_failure_response.sql` | `last_failure_response_json`（保留上游原始错误体） |
| `202608080002_route_credential_priority_concurrency.sql` | `route_priority`（1–5，**默认 3**，带 `CHECK` 约束）、`max_concurrency`（**默认 1**） |
| `202608130001_route_credential_semantic_failure_streak.sql` | `semantic_failure_streak_count`、`semantic_failure_streak_fingerprint`（连续语义失败识别） |

调度行为的用户视角说明见[账号与算力池](/guide/accounts)与[稳定性与自动恢复](/guide/reliability)，统计字段的用法见[用量与请求统计](/guide/usage-stats)。

### 密钥存放位置

路由账号的密钥保存在 SQLite 的 `route_credentials.secret_payload_json` 列中（迁移 `202607130011_route_credentials.sql`），由 `database/repositories/route_credential_repository.rs` 直接读写，**没有**额外的加密层。

`security/mod.rs` 里确实定义了 `SecretStore` trait 和一个基于 `keyring` crate 的 `KeyringSecretStore` 实现，但该文件标着 `#![allow(dead_code)]`，且 `KeyringSecretStore` 在仓库中从未被构造或调用——它是预留接线位，当前**未生效**。`Cargo.toml` 中的 `keyring` 依赖同理。

::: warning
当前版本的 API key 以明文 JSON 存放在本地数据库中，请把整个 `~/.ai-switch` 目录视作凭据目录：注意它的文件权限，备份时按机密数据对待，不要随意拷贝到共享位置。
:::

### 目录布局

`paths.rs` 的 `AppPaths` 定义了数据目录结构，根为 `~/.ai-switch`：

```text
~/.ai-switch/
├── ai-switch.db              # release 数据库（debug 为 ai-switch-dev.db）
├── settings.json             # 应用设置
├── web-service.json          # Web 服务配置
├── route-proxy-https.json    # 路由代理 HTTPS 配置
├── backups/
│   └── config-snapshots/     # 配置写入快照（Unix 下强制 0700）
├── certs/route-proxy/        # 路由代理自签证书
├── imports/
├── logs/
└── tailscale/                # sidecar 状态
```

## sidecar：ai-switch-tsnet

`sidecar/ai-switch-tsnet/` 是一个独立的 Go 程序（`go 1.24.0`，依赖 `tailscale.com v1.82.5`），以 Tauri `externalBin` 的形式随桌面应用分发。它的职责是：

1. 用 OAuth 或 auth key 登录 Tailscale
2. 以 `tsnet` 节点身份加入 tailnet 并对外提供服务
3. 把远程请求反向代理到本机 `127.0.0.1` 上的 AI Switch Web 服务

sidecar 启动时在 stdout 打印 `CONTROL 127.0.0.1:<port>`，Rust 侧的 `tailscale_sidecar` 据此连接其仅监听回环的控制 API：`POST /control/start`、`/control/login-oauth`、`/control/stop`、`/control/logout` 与 `GET /control/status`。

暴露模式分两种：`private` 仅 tailnet 内可达，`public` 通过 Tailscale Funnel 提供公网 HTTPS 入口。**无论哪种模式，AI Switch 自身的访问令牌校验都不会被跳过**——Tailscale 只解决网络可达性，不代替应用鉴权。配置步骤见[远程访问与 HTTPS](/deploy/remote-access)。

## 关键依赖

以下版本取自 `src-tauri/Cargo.toml`：

| 依赖 | 版本 | 用途 |
| --- | --- | --- |
| `tauri` | 2（启用 `tray-icon`） | 桌面外壳与 IPC |
| `axum` | 0.7（启用 `ws`） | Web 服务与路由代理的 HTTP 层 |
| `axum-server` | 0.7（`tls-rustls-no-provider`） | HTTPS 监听 |
| `tokio` | 1（macros、rt-multi-thread、fs、io-util、net、sync、process、time） | 异步运行时 |
| `sqlx` | 0.8（sqlite、migrate、macros、chrono、uuid、json，runtime-tokio-rustls） | 数据库访问与迁移 |
| `reqwest` | 0.12（rustls-tls、json、system-proxy、cookies） | 上游 HTTP 客户端 |
| `rustls` | 0.23（`ring` provider） | TLS 实现 |
| `keyring` | 3 | 已声明但未接线，见上文"密钥存放位置" |
| `portable-pty` | 0.8 | 跨平台 PTY |
| `rcgen` / `x509-parser` | 0.13 / 0.16 | 路由代理自签证书生成与解析 |
| `ed25519-dalek` | 2（`pkcs8`） | 签名校验 |
| `serde` / `serde_json` / `serde_yaml` / `toml` / `toml_edit` | 1 / 1 / 0.9 / 0.8 / 0.20.2 | 各 CLI 配置格式读写 |
| `directories` | 5 | 用户目录解析 |
| `sha2` / `sha1` | 0.10 / 0.10 | 配置文件哈希与校验 |
| `chrono` / `uuid` / `url` / `base64` / `tempfile` | 0.4 / 1 / 2 / 0.22 / 3 | 通用工具 |
| `tower-http` | 0.6（`cors`） | CORS 中间件 |
| `tauri-plugin-{shell,process,updater,dialog,deep-link,single-instance}` | 2 | 系统集成 |

Windows 平台额外依赖 `windows-sys 0.61` 与 `winreg 0.52`（文件系统标志与注册表协议注册）。前端侧的版本见根 `package.json`：React 18.3、TypeScript 5.5、Vite 5.4、Vitest 2.0、UnoCSS 66.7、three 0.185、`@xterm/xterm` 6.0。

## 相关阅读

- [本地开发](/dev/local-setup)——把上面这些模块跑起来所需的命令与工具链
- [发布流程](/dev/release)——CI 如何把它们打包成三平台安装包
- [Web 服务模式](/deploy/web-service)与[独立服务器](/deploy/standalone-server)——两种传输在部署上的差异
- [常见问题](/faq)
