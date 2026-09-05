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
| OpenCode | 部分支持：API 账号必须显式提供 base URL 和接口格式 | 不支持 | 不支持 |
| OpenClaw | 部分支持：API 账号必须显式提供 base URL 和接口格式 | 不支持 | 不支持 |
| Hermes | 部分支持：API 账号必须显式提供 base URL 和接口格式 | 不支持 | 不支持 |

OpenCode、OpenClaw、Hermes 保持可见，用于通用 API 路由、终端启动和会话流程，但 AI Switch 不声称对它们支持原生配置、官方账号导入或额度查询。

Codex、Claude Code、Gemini CLI、Grok 的原生配置写入采用安全直写：AI Switch 在变更前建立快照、原子写入、检测并发修改、支持带守卫的回滚。Phase A 不会解析也不会修改 Hermes 的 `config.yaml`。

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
3. 填写主机、端口和访问令牌
4. 启动服务
5. 可选：启用安全网络（Tailscale），选择访问模式（仅私网 / 公网访问），再点**使用 OAuth 登录**

默认绑定 `127.0.0.1:3090`。绑定到 `0.0.0.0` 必须显式设置。

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
$env:AI_SWITCH_PORT = "3090"
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
- `AI_SWITCH_PORT` 默认 `3090`
- `AI_SWITCH_TOKEN` 访问 API 和 WebSocket 的必填令牌，至少 16 个字符；未设置时服务拒绝启动
- `AI_SWITCH_STATIC_DIR` 浏览器界面用的前端 `dist` 目录（只有你挪动过它才需要设置）

发布包 `ai-switch-server_<tag>_<platform>.zip` 里已经带了二进制、Tailscale sidecar 和同级的 `web/` 目录，所以解压即用，不需要额外配置就能提供浏览器界面。安装版桌面端也会把同一套资源放在可执行文件旁边的 `web/` 下。

### 安全说明

- 每个 `/api/*` 和 `/ws/events` 请求都需要访问令牌
- Tailscale 登录是手动的，应用不会在启动时自动登录
- 即使走 Tailscale，Web 访问同样需要 AI Switch 自己的令牌
- 移动端配对会生成一个独立的移动端令牌；配对码只能用一次且会过期

## 洁净室边界（Clean-Room Boundary）

本项目可能研究相关工具的公开行为、公开文档和公开文件格式。
