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
| `AI_SWITCH_HOST` | `127.0.0.1` | 否 | 监听地址。设为非环回地址时默认拒绝明文 HTTP；若由 Nginx/Caddy 等可信 HTTPS 反代保护，可显式设置 `AI_SWITCH_ALLOW_INSECURE_HTTP=1` |
| `AI_SWITCH_PORT` | `19527` | 否 | 监听端口。值无法解析为端口号时静默回退到 `19527` |
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
AI Switch server listening on http://127.0.0.1:19527
```

对外提供服务（非环回地址默认禁止明文；使用 HTTPS 反代时可显式允许明文上游）：

```bash
export AI_SWITCH_HOST=0.0.0.0
export AI_SWITCH_PORT=19527
export AI_SWITCH_TOKEN="$(openssl rand -hex 32)"
export AI_SWITCH_STATIC_DIR=/opt/ai-switch/dist
export AI_SWITCH_TLS_CERT_PATH=/etc/ai-switch/fullchain.pem
export AI_SWITCH_TLS_KEY_PATH=/etc/ai-switch/privkey.pem
/opt/ai-switch/ai-switch-server
```

```powershell
$env:AI_SWITCH_HOST = "0.0.0.0"
$env:AI_SWITCH_PORT = "19527"
$env:AI_SWITCH_TOKEN = "<your-random-token>"
$env:AI_SWITCH_STATIC_DIR = "C:\ai-switch\dist"
$env:AI_SWITCH_TLS_CERT_PATH = "C:\ai-switch\certs\fullchain.pem"
$env:AI_SWITCH_TLS_KEY_PATH  = "C:\ai-switch\certs\privkey.pem"
C:\ai-switch\ai-switch-server.exe
```

如果不想让服务器自己终止 TLS，另一种做法是保持 `AI_SWITCH_HOST=127.0.0.1`，在前面放一个负责 HTTPS 的反向代理。这种情况下服务本身满足环回条件，不需要配置证书路径。

启动后的接口与浏览器行为和桌面端 Web 服务完全一致：`POST /api/:command`、`GET /ws/events`、`GET /health`（不鉴权）。详见 [Web 服务模式](/deploy/web-service)。

## 面板与算力池共用端口

独立服务器只监听一个端口，默认是 `19527`。浏览器面板和算力池 API 可以同时使用它：

- 面板路由（`/api/*`、`/ws/*`、`/health` 和前端页面）继续按面板访问令牌鉴权；
- 模型 API 路由（`/models`、`/v1/*`、`/v1beta/*`、`/messages`、`/responses`）转发到算力池，并单独要求路由代理 API key；
- 面板令牌和算力池 API key 是两套不同的凭据，不能互相替代。

如果服务器绑定 `0.0.0.0` 或其他非环回地址，明文 HTTP 默认会被拒绝。推荐让 Nginx 或 Caddy 在前端终止 HTTPS，并把请求反代到 `127.0.0.1:19527`。如果你明确接受可信网络边界内的裸 HTTP，可设置：

```bash
export AI_SWITCH_ALLOW_INSECURE_HTTP=1
```

这只允许服务启动，不会关闭面板令牌或算力池 API key 鉴权；裸 HTTP 会暴露令牌和请求内容，请勿直接暴露到公网。内置 HTTPS 仍可通过 `AI_SWITCH_TLS_CERT_PATH` 和 `AI_SWITCH_TLS_KEY_PATH` 启用，但不是反代部署的必需项。

## Linux 一键安装

Linux x86_64 服务器可以直接运行下面的命令安装最新 Release：

```bash
AI_SWITCH_PORT=19527 /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/ijry/ai-switch/main/scripts/install-server.sh)"
```

把 `AI_SWITCH_PORT=19527` 换成需要的端口；默认监听 `127.0.0.1`，需要外部访问时可再设置 `AI_SWITCH_HOST`。安装器会创建 `ai-switch` 系统用户，将程序安装到 `/opt/ai-switch`，把令牌和端口持久化到 `/etc/ai-switch/server.env`，并安装、启用、启动 `ai-switch-server.service`。安装完成后会输出面板地址、服务状态和读取访问令牌的命令。升级前会先停止旧服务，避免替换二进制时出现 `Text file busy`。

当前 Linux 服务器二进制仍依赖 WebKitGTK 4.1 运行库。安装器会用 `ldd` 检测缺失库；Debian/Ubuntu 上自动安装 `libwebkit2gtk-4.1-0`，其他发行版需要先自行安装对应运行库。

新安装会默认写入 `AI_SWITCH_ALLOW_INSECURE_HTTP=1`，让非环回地址上的明文 HTTP 可以启动；这只影响一键安装路径，手动运行服务器时仍保持默认拒绝。令牌和算力池 API key 鉴权不会因此关闭。重复执行会保留已有令牌、环境配置和 `~/.ai-switch` 数据，因此已有配置不会自动改成这个默认值。安装器不会修改 Nginx、Certbot、UFW 或其他防火墙配置；HTTPS 反代需要自行配置。

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
- **非环回明文 HTTP 默认被拒绝。** 使用 Nginx/Caddy 等可信 HTTPS 反代时，设置 `AI_SWITCH_ALLOW_INSECURE_HTTP=1` 才会允许上游明文；它不会关闭任一鉴权，并且不适合直接暴露公网。
- **数据目录跟着运行账号走。** 服务始终使用运行账号主目录下的 `~/.ai-switch`，其中的 SQLite 库保存着 API Key 与账号凭据，请按凭据目录对待。
- **多人共享意味着共享一切。** 同一个实例下所有人看到同一份账号、同一份用量、同一批会话，没有按用户隔离的权限模型。
:::

## 下一步

- 想从外网访问这台服务器，见 [远程访问与 HTTPS](/deploy/remote-access)。
- 想了解浏览器端的界面与接口细节，见 [Web 服务模式](/deploy/web-service)。
- 想在本机开发环境跑起来，见 [本地开发](/dev/local-setup)。
- 想了解服务端与桌面端如何共用一套命令，见 [架构总览](/dev/architecture)。
