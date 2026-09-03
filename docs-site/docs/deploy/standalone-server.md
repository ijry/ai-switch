---
title: 独立服务器
description: ai-switch-server 是 AI Switch 的无界面服务端二进制，适合团队共享或部署在没有桌面环境的服务器上。本页给出构建命令、完整的环境变量表、PowerShell 与 bash 启动示例，以及静态资源解析规则。
---

# 独立服务器

`ai-switch-server` 是一个不带桌面外壳的服务端二进制：同一个 Rust 核心（`ai_switch_lib`），同一份 React 界面，只保留 HTTP 与 WebSocket 入口。适合两种情况：

- **部署在没有图形环境的服务器上**，比如一台家里的 NAS、一台云主机，长期在后台跑着。
- **团队共享一份配置**，几个人用浏览器访问同一个实例，共用账号池与用量统计。

它与桌面端开启的 Web 服务模式在协议和能力上基本一致，差别见文末的对照表。

## 构建

需要先构建前端，再构建 Rust 二进制。

```bash
pnpm install
pnpm build
pnpm server:build:release
```

```powershell
pnpm install
pnpm build
pnpm server:build:release
```

`pnpm build` 执行 `tsc && vite build`，产物在仓库根目录的 `dist/`。`pnpm server:build:release` 在 `src-tauri` 目录下执行 `cargo build --release --bin ai-switch-server`。

调试构建用 `pnpm server:build`（不带 `--release`，编译快但运行慢，且会使用 `ai-switch-dev.db` 这个独立的开发数据库）。只想做类型与借用检查、不产出二进制时用 `pnpm server:check`。

构建产物路径：

| 构建方式 | 产物 |
| --- | --- |
| `pnpm server:build:release` | `src-tauri/target/release/ai-switch-server`（Windows 为 `ai-switch-server.exe`） |
| `pnpm server:build` | `src-tauri/target/debug/ai-switch-server`（Windows 为 `ai-switch-server.exe`） |

如果不想自己编译，每次正式发布都会在 GitHub Release 里附带按平台打包好的 `ai-switch-server` 压缩包，见 [发布流程](/dev/release)。

发布包解压后的结构就是下面「前端资源怎么找」推荐的布局，`ai-switch-server`、`ai-switch-tsnet` 与 `web/` 已经放在一起，不需要再设 `AI_SWITCH_STATIC_DIR`：

```text
ai-switch-server_v0.7.3_windows-x86_64/
├── ai-switch-server.exe
├── ai-switch-tsnet.exe
└── web/
    ├── index.html
    └── assets/...
```

## 环境变量

服务器的全部运行参数都来自环境变量，没有命令行参数也没有配置文件：

| 变量 | 默认值 | 必填 | 说明 |
| --- | --- | --- | --- |
| `AI_SWITCH_HOST` | `127.0.0.1` | 否 | 监听地址。设为非环回地址时**必须**同时配置 TLS，否则启动失败 |
| `AI_SWITCH_PORT` | `3090` | 否 | 监听端口。值无法解析为端口号时静默回退到 `3090` |
| `AI_SWITCH_TOKEN` | 无 | **是** | 访问令牌，至少 16 个字符。未设置或过短时服务拒绝启动 |
| `AI_SWITCH_STATIC_DIR` | 无 | 否 | 前端 `dist` 目录。仅当该目录下存在 `index.html` 时生效，否则回退到内置候选路径 |
| `AI_SWITCH_TLS_CERT_PATH` | 无 | 与下一项成对 | 证书链 PEM 路径 |
| `AI_SWITCH_TLS_KEY_PATH` | 无 | 与上一项成对 | 私钥 PEM 路径 |
| `AI_SWITCH_TSNET_PATH` | 无 | 否 | Tailscale sidecar 可执行文件路径。默认在当前可执行文件同级目录找 `ai-switch-tsnet` |

关于这张表的几点必要说明：

- **`AI_SWITCH_TOKEN` 是必填项。** 未设置、只有空白字符、或短于 16 个字符时服务直接拒绝启动并打印原因。这是有意的：普通命令里就有能读出账号明文 API Key 的（如 `list_route_credentials`），没有令牌等于把凭据库对所有能访问该端口的人开放。
- **TLS 两个路径必须同时给。** 只提供其中一个会以 `web.tls_paths_incomplete` 报错，服务不会启动。
- **数据目录不可通过环境变量指定。** 服务器始终把数据写在当前用户主目录下的 `~/.ai-switch`。README 里出现过的 `AI_SWITCH_DATA_DIR` 在当前代码中**没有实现**，设置它不会有任何效果。需要换位置的话，请用运行账号的主目录或容器卷挂载来控制。

## 启动

最小可用启动（仅本机访问）：

```bash
export AI_SWITCH_TOKEN="$(openssl rand -hex 32)"
export AI_SWITCH_STATIC_DIR=/opt/ai-switch/dist
/opt/ai-switch/ai-switch-server
```

```powershell
$env:AI_SWITCH_TOKEN = [guid]::NewGuid().ToString()
$env:AI_SWITCH_STATIC_DIR = "C:\ai-switch\dist"
C:\ai-switch\ai-switch-server.exe
```

启动成功后会打印一行监听地址，形如：

```text
AI Switch server listening on http://127.0.0.1:3090
```

对外提供服务（非环回地址，必须带 TLS）：

```bash
export AI_SWITCH_HOST=0.0.0.0
export AI_SWITCH_PORT=3090
export AI_SWITCH_TOKEN="$(openssl rand -hex 32)"
export AI_SWITCH_STATIC_DIR=/opt/ai-switch/dist
export AI_SWITCH_TLS_CERT_PATH=/etc/ai-switch/fullchain.pem
export AI_SWITCH_TLS_KEY_PATH=/etc/ai-switch/privkey.pem
/opt/ai-switch/ai-switch-server
```

```powershell
$env:AI_SWITCH_HOST = "0.0.0.0"
$env:AI_SWITCH_PORT = "3090"
$env:AI_SWITCH_TOKEN = "<your-random-token>"
$env:AI_SWITCH_STATIC_DIR = "C:\ai-switch\dist"
$env:AI_SWITCH_TLS_CERT_PATH = "C:\ai-switch\certs\fullchain.pem"
$env:AI_SWITCH_TLS_KEY_PATH  = "C:\ai-switch\certs\privkey.pem"
C:\ai-switch\ai-switch-server.exe
```

如果不想让服务器自己终止 TLS，另一种做法是保持 `AI_SWITCH_HOST=127.0.0.1`，在前面放一个负责 HTTPS 的反向代理。这种情况下服务本身满足环回条件，不需要配置证书路径。

启动后的接口与浏览器行为和桌面端 Web 服务完全一致：`POST /api/:command`、`GET /ws/events`、`GET /health`（不鉴权）。详见 [Web 服务模式](/deploy/web-service)。

## 前端资源怎么找

`AI_SWITCH_STATIC_DIR` 不是唯一途径。解析顺序如下，任一候选目录中存在 `index.html` 即视为命中：

1. `AI_SWITCH_STATIC_DIR` 指定的目录；
2. 可执行文件同级：`web/`、`dist/`、`resources/web/`；
3. 可执行文件上一级：`../web/`、`../dist/`；
4. 当前工作目录：`web/`、`dist/`。

因此最省事的部署方式是把二进制和前端资源放在一起：

```text
/opt/ai-switch/
├── ai-switch-server
└── web/
    ├── index.html
    └── assets/...
```

这样连 `AI_SWITCH_STATIC_DIR` 都不用设。未匹配到静态文件的路径会回退到 `index.html`，以支持前端路由。

## 与桌面端 Web 服务的差别

| | 桌面端 Web 服务 | 独立服务器 |
| --- | --- | --- |
| 配置来源 | `~/.ai-switch/web-service.json` + 设置界面 | 环境变量 |
| 敏感命令闸门 | 按传输安全性动态判定（HTTPS / 环回 / Tailscale 状态） | 始终开放，因此令牌保护尤为关键 |
| 桌面独占命令 | 桌面窗口内可用 | 不可用（无原生桌面环境） |
| Tailscale | 设置界面里开关与登录 | 需要自行提供 sidecar 可执行文件（`AI_SWITCH_TSNET_PATH` 或同级目录） |
| 托盘与自动更新 | 有 | 无，需要自行做进程守护与升级 |

因为独立服务器的敏感命令闸门不做动态判定，凭据导出、密钥读取、MCP 与技能安装这些命令在令牌校验通过后就都能调用。

## 安全注意事项

::: warning 部署前请确认
- **必须设置 `AI_SWITCH_TOKEN`（缺失时服务不会启动）。** 独立服务器不做敏感命令降级，令牌是唯一的访问控制手段。
- **令牌等价于 shell 权限。** Web API 包含终端会话命令，拿到令牌的人可以在这台服务器上执行命令。
- **非环回监听必须带 TLS**，否则服务直接拒绝启动。这是有意设计，不要试图绕过。
- **数据目录跟着运行账号走。** 服务始终使用运行账号主目录下的 `~/.ai-switch`，其中的 SQLite 库保存着 API Key 与账号凭据，请按凭据目录对待。
- **多人共享意味着共享一切。** 同一个实例下所有人看到同一份账号、同一份用量、同一批会话，没有按用户隔离的权限模型。
:::

## 下一步

- 想从外网访问这台服务器，见 [远程访问与 HTTPS](/deploy/remote-access)。
- 想了解浏览器端的界面与接口细节，见 [Web 服务模式](/deploy/web-service)。
- 想在本机开发环境跑起来，见 [本地开发](/dev/local-setup)。
- 想了解服务端与桌面端如何共用一套命令，见 [架构总览](/dev/architecture)。
