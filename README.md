# ai-switch

[English](README-EN.md) | 简体中文

AI Switch 是一个用于切换 AI 供应商与官方账号的应用，有桌面端和自托管 Web 服务两种形态。

<img width="2360" height="1520" alt="ai-switch" src="https://github.com/user-attachments/assets/fbd3932e-29a7-4e3f-a980-e93fb093b643" />

当前已有的基础能力：

- Tauri 2 + React + TypeScript 桌面外壳
- 桌面端与 Web 端共用同一个 Rust 核心，只在传输层不同
- 独立二进制 `ai-switch-server`，供浏览器和移动端访问
- SQLite 基础表结构
- 账号、会话、终端与路由代理的完整流程
- 设置保存在 `~/.ai-switch/settings.json`
- Web 服务设置，HTTP 访问由访问令牌保护
- Tailscale 登录入口，用于私网远程访问，支持 MagicDNS HTTPS 与移动端配对

## 平台支持

| 平台 | 路由账号与 API 路由 | 原生配置写入 | 官方导入与额度 |
| --- | --- | --- | --- |
| Codex | 支持 | 支持 | 支持 |
| Claude Code | 支持 | 支持 | 上游账号流程允许的范围内支持 |
| Gemini CLI | 支持 | 支持 | 支持导入；不声称支持官方额度 |
| Grok | 支持 | 支持 | 上游账号流程允许的范围内支持 |
| OpenCode | 部分支持：API 账号必须显式提供 base URL 和接口格式 | 支持 | 不支持 |
| OpenClaw | 部分支持：API 账号必须显式提供 base URL 和接口格式 | 支持 | 不支持 |
| Hermes | 部分支持：API 账号必须显式提供 base URL 和接口格式 | 支持 | 不支持 |

后三个是 agent harness 而不是模型厂商，没有自己的官方登录态，所以官方账号导入、官方账号路由、deeplink 和额度查询对它们不存在 —— 这就是「部分支持」的全部含义。

原生配置写入采用安全直写：AI Switch 在变更前建立快照、原子写入、检测并发修改、支持带守卫的回滚。7 个平台各写自己的文件：Codex 的 `~/.codex/config.toml`、Claude Code / Gemini CLI / Grok 的 `settings.json`，以及 `~/.config/opencode/opencode.json`、`~/.openclaw/openclaw.json`、`~/.hermes/config.yaml` 里的 `ai-switch` 自定义 provider。

### 协议路由

Codex 和 Claude 的 API 路由账号可以选择 `openai`、`openai-responses`、`anthropic`、`gemini` 四种上游协议。Codex 本地入口仍使用 OpenAI Responses；Claude 本地入口仍使用 Anthropic Messages。AI Switch 会在本地入口协议和上游账号协议不一致时进行桥接转换。Gemini CLI 本地入口目前保持 Gemini native，只路由到 Gemini 协议账号。

## 开发

安装依赖：

```powershell
corepack enable
pnpm install
```

运行前端检查：

```powershell
pnpm typecheck
pnpm test:run
```

运行 Rust 检查：

```powershell
pnpm rust:check
pnpm rust:test
pnpm server:check
```

以开发模式运行桌面应用：

```powershell
pnpm tauri:dev
```

构建桌面前端和安装包：

```powershell
pnpm build
pnpm tauri:build
```

## 发布自动化

推送版本 tag 后，GitHub Actions 会自动构建并发布跨平台的 Release 资产。

必需的仓库 secret：

- `TAURI_SIGNING_PRIVATE_KEY`

可选的仓库 secret：

- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

创建并推送版本 tag：

```bash
git tag v0.4.2
git push origin v0.4.2
```

带 `-rc`、`-beta`、`-alpha` 的 tag 会作为预发布版发布，例如：

```bash
git tag v0.4.2-rc.1
git push origin v0.4.2-rc.1
```

tag 去掉 `v` 前缀后的版本号，必须与 `package.json` 和 `src-tauri/tauri.conf.json` 完全一致，包括预发布后缀。打 tag 的提交必须属于仓库的默认分支。

工作流会构建带签名的 Tauri 桌面安装包、`ai-switch-server`、`ai-switch-tsnet`，以及供 GitHub Releases 使用的 `latest.json` 更新器清单。

### 包管理器

另一个工作流 `.github/workflows/package-managers.yml` 负责把**已经发布**的 Release 推给 Homebrew 和 WinGet。它由 `release: published` 触发，`workflow_dispatch` 接受一个 `tag` 参数，因此任何一个历史 Release 都能重新提交而不必重新构建。草稿和预发布版会被跳过。

两条链路都要往别的仓库里写东西，因此各需要一个 secret。缺 secret 不会让工作流失败，只会记一条 warning 并跳过对应的那条链路：

- `HOMEBREW_TAP_TOKEN` —— 对 tap 仓库（`HOMEBREW_TAP_REPO`，默认 `ijry/homebrew-ai-switch`）有 `contents: write` 权限的 PAT
- `WINGET_TOKEN` —— 带 `public_repo` scope 的 classic PAT，另外还需要在 `WINGET_FORK_USER` 下有一份 `microsoft/winget-pkgs` 的 fork

有两步是一次性的、无法自动化的：建好公开的 `homebrew-` 前缀 tap 仓库，以及手工把第一个 `Lingyun.AISwitch` 版本提交到 winget-pkgs —— 这个 action 只会给已经存在的包升版本。完整的准备步骤见[发布流程](https://ijry.github.io/ai-switch/dev/release)。

## Web 服务与服务器模式

桌面端和浏览器共用同一套 React 界面。桌面端走 Tauri IPC，浏览器模式走：

- `POST /api/:command`
- `GET /ws/events`
- 两个端点都需要令牌鉴权

### 从桌面端配置

1. 打开设置
2. 选择 **Web 服务**
3. 填写主机、服务端口和访问令牌
4. 点击**保存**，再按需要启动**共享服务端口**
5. 可选：启用安全网络（Tailscale），选择访问模式（仅私网 / 公网访问），再点**使用 OAuth 登录**

GUI 版和独立 server 版一样，Web 页面、SaaS（启用时）与算力池模型 API 共用一个监听端口。设置页把它拆成两块：**共享服务端口**只负责监听地址、端口、TLS 和启停；**算力池路由接入**只决定模型 API 是否接受池路由。开启路由接入时会在需要时自动启动共享端口；关闭路由接入不会停止端口，Web 页面仍可访问。桌面 dev 模式默认使用 `10086`，并使用独立的 `web-service-dev.json`，不会改动正式版配置；安装版和独立 server 的新配置默认使用服务端口 `19527`。旧独立算力池监听会在共享服务成功启动后收拢，不会并行保留第二套端口。

桌面端会记住路由接入开关；应用启动时如果该开关为开，会自动恢复共享端口，因此不再需要单独的 Web 服务自动启动选项。独立 server 与 Docker 的监听地址、端口、TLS 和进程启停始终由环境变量与进程管理器控制，浏览器端不能修改或启停；路由接入开关仍可在浏览器端切换。

共享服务的 HTTPS 在 **Web 服务 → TLS** 中配置，不使用旧的独立算力池 HTTPS 端口。修改地址、服务端口或 TLS 后请重启共享服务；若客户端仍指向旧地址，请重新执行“写入路由配置文件”。管理令牌、移动端令牌、算力池 Key 与 SaaS Key 的权限不会因端口共享而合并。路由接入关闭时，模型 API 返回 `route_proxy.access_disabled`，面板与健康检查不受影响。

默认绑定 `127.0.0.1:19527`。未启用 TLS 时，`0.0.0.0` 等非环回地址会拒绝启动；需要绑定所有网卡时，请先启用 Web 服务 TLS。

私网访问时，桌面端通过 Tailscale `ListenTLS` 发布 `https://<magicdns-名称>:<端口>`。请先在 Tailscale 管理后台启用 MagicDNS 和 HTTPS 证书；不要把 `100.x.y.z` 这个 IP 填成移动端 URL，因为证书是按 MagicDNS 名称签发的。手机上必须已经用官方 Tailscale App 登录同一个 tailnet。uni-app 客户端本身不内嵌 Tailscale SDK。

H5 和小程序客户端建议把公网 HTTPS 地址作为默认的跨端入口。H5 需要 CORS，小程序需要把域名加入合法请求域名列表。安全网络面板可以显示一个短期、一次性的移动端配对二维码：它只包含 URL 和配对码，**不包含** Web 服务的长期令牌。扫码只是回填表单，移动端用户仍然可以手动输入或修改 URL 和令牌。

### 独立服务器

构建：

```powershell
pnpm build
pnpm server:build
```

运行：

```powershell
$env:AI_SWITCH_HOST = "127.0.0.1"
$env:AI_SWITCH_PORT = "19527"
$env:AI_SWITCH_TOKEN = [guid]::NewGuid().ToString()
$env:AI_SWITCH_STATIC_DIR = "$PWD\dist"
.\src-tauri\target\debug\ai-switch-server.exe
```

release 二进制路径：

```text
src-tauri/target/release/ai-switch-server.exe
```

可选的环境变量：

- `AI_SWITCH_HOST` 默认 `127.0.0.1`
- `AI_SWITCH_PORT` 默认 `19527`
- `AI_SWITCH_TOKEN` 访问 API 和 WebSocket 的必填令牌，至少 16 个字符；未设置时服务拒绝启动
- `AI_SWITCH_STATIC_DIR` 浏览器界面用的前端 `dist` 目录（只有你挪动过它才需要设置）

发布包 `ai-switch-server_<tag>_<platform>.zip` 里已经带了二进制、Tailscale sidecar 和同级的 `web/` 目录，所以解压即用，不需要额外配置就能提供浏览器界面。安装版桌面端也会把同一套资源放在可执行文件旁边的 `web/` 下。

### 共享端口与 Linux 一键安装

独立服务器默认只监听 `19527`，面板与算力池共用此端口。`/api/*`、`/ws/*` 和面板页面使用 `AI_SWITCH_TOKEN`；`/models`、`/v1/*`、`/v1beta/*`、`/messages`、`/responses` 转发到算力池并使用独立的路由代理 API key。两套凭据都不会互相替代。

非环回地址默认禁止明文 HTTP。使用 Nginx 或 Caddy 在前端终止 HTTPS、反代到 `127.0.0.1:19527` 时，无需启用内置 TLS。确实需要可信网络内裸 HTTP 时才设置 `AI_SWITCH_ALLOW_INSECURE_HTTP=1`，这不会关闭任何 API 鉴权，也不适合直接暴露公网。

Linux x86_64 可以一键安装：

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/ijry/ai-switch/main/scripts/install-server.sh)"
```

安装器会创建 `ai-switch` 系统用户、安装到 `/opt/ai-switch`，持久化 `/etc/ai-switch/server.env`，并启用 systemd 服务；重复运行保留现有令牌和数据。它不会自动配置 Nginx、Certbot 或防火墙。

### Docker 一键启动 server 与 SaaS

Docker 镜像直接复用 GitHub Release 里已打包的 standalone server，不在本机编译 Rust，也不需要桌面 WebKitGTK。正式版本发布后，CI 会同步推送 `ijry/ai-switch` 的 `linux/amd64` 与 `linux/arm64` 镜像。默认会启动 Redis 作为日志队列、PostgreSQL 作为日志存储，并自动开启 SaaS：

```bash
docker compose -f deploy/docker-compose.yml up -d
```

容器内日志队列与存储默认使用 `redis://redis:6379` 和 `postgresql://ai_switch:change-me@postgres:5432/ai_switch_logs?sslmode=disable`，对应环境变量是 `SAAS_LOGS_REDIS_URL` 和 `SAAS_LOGS_POSTGRES_URL`。SaaS 设置只保存环境名引用，不在数据库里落库连接串。

常用覆盖参数：

- `AI_SWITCH_PORT`：宿主映射端口，默认 `19527`。
- `AI_SWITCH_DOCKER_IMAGE`：镜像地址，默认 `ijry/ai-switch:latest`；固定版本可用 `ijry/ai-switch:0.9.0` 或 `ijry/ai-switch:0.9`。
- `AI_SWITCH_TOKEN`：不设置时 entrypoint 会生成并打印一个容器本地令牌；跨重启请显式设置。
- `AI_SWITCH_SAAS_ENABLE`：默认 `1`，设为 `0` 只启动独立 server。
- `AI_SWITCH_SAAS_ACTIVATION_CODE`、`AI_SWITCH_SAAS_INSTANCE_ID`、`AI_SWITCH_SAAS_SITE_NAME`、`AI_SWITCH_SAAS_PUBLIC_BASE_URL`。
- `AI_SWITCH_SAAS_LOGS_QUEUE`、`AI_SWITCH_SAAS_LOGS_STORE`，默认分别为 `redis` 和 `postgres`。
- `AI_SWITCH_SAAS_LOGS_REDIS_URL_ENV`、`AI_SWITCH_SAAS_LOGS_POSTGRES_URL_ENV`，默认分别引用 `SAAS_LOGS_REDIS_URL` 和 `SAAS_LOGS_POSTGRES_URL`。

数据保存在 named volumes：`ai-switch-data`、`redis-data`、`postgres-data`。生产环境请替换默认 PostgreSQL 密码，并在前端反向代理处终止 HTTPS。

如需本地构造镜像，Dockerfile 也会下载并校验 Release 包，而不是编译源码：

```bash
docker build --build-arg AI_SWITCH_VERSION=v0.9.0 .
```

### 安全说明

- 每个 `/api/*` 和 `/ws/events` 请求都需要访问令牌
- Tailscale 登录是手动的，应用不会在启动时自动登录
- 即使走 Tailscale，Web 访问同样需要 AI Switch 自己的令牌
- 移动端配对会生成一个独立的移动端令牌；配对码只能用一次且会过期

## 洁净室边界（Clean-Room Boundary）

本项目可能研究相关工具的公开行为、公开文档和公开文件格式。

## 许可证

仓库默认采用根目录 LICENSE 中的 MIT License，但 SaaS 插件是独立许可范围：src/saas/ 与 src-tauri/src/saas/ 采用 GNU General Public License v3.0 only（GPL-3.0-only），分别以目录内的 LICENSE 为准。范围、分发注意事项及中英文说明见 docs/saas-license.md。
