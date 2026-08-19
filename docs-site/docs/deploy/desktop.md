---
title: 桌面端部署
description: AI Switch 桌面端基于 Tauri 2，通过 IPC 直连 Rust 核心。本页说明各平台安装包格式、本机数据目录布局、内置 web 资源位置，以及桌面端与 Web 服务模式共享的数据与代理行为。
---

# 桌面端部署

桌面端是 AI Switch 的默认形态：一个 Tauri 2 原生外壳，里面跑的是与浏览器完全相同的 React 界面，但不经过任何 HTTP 层——界面通过 Tauri IPC 直接调用同一个 Rust 核心（crate `ai_switch_lib`）。账号读写、协议路由、本地代理、终端会话都在这个进程里完成，没有额外的后台服务需要单独启动。

## 安装包格式

发布流水线为三个平台各构建一组安装包，格式由 `.github/workflows/release.yml` 的构建矩阵决定：

| 平台 | 安装包格式 | 说明 |
| --- | --- | --- |
| Windows | NSIS 安装程序（`.exe`） | 常规安装向导，安装后可从开始菜单启动 |
| macOS | `.dmg` + `.app` | dmg 用于分发，`.app` 是应用包本身 |
| Linux | `.deb` + `.AppImage` | deb 面向 Debian/Ubuntu 系；AppImage 免安装直接运行 |

每个平台的产物同时包含 Tauri 更新器所需的签名文件（`.sig`）与 `latest.json` 元数据，应用内更新依赖它们校验签名。发布流程的完整细节见 [发布流程](/dev/release)。

除桌面安装包之外，同一次发布还会附带 `ai-switch-server`（独立服务器二进制）和 `ai-switch-tsnet`（Tailscale sidecar）两个压缩包。前者见 [独立服务器](/deploy/standalone-server)，后者见 [远程访问与 HTTPS](/deploy/remote-access)。

按平台挑选安装包并完成首次配置，请从 [安装](/guide/installation) 和 [快速开始](/guide/quick-start) 开始。

## 首次启动做了什么

首次启动时，桌面端会在用户主目录下创建数据目录 `~/.ai-switch`，然后打开 SQLite 数据库并按顺序执行内置迁移（当前共 23 个，位于仓库 `src-tauri/migrations`）。迁移是幂等的，后续版本升级时只会补跑新增的那几个。

启动后应用会常驻托盘：托盘菜单提供「显示主窗口」和「退出 AI Switch」，关闭窗口不等于退出进程——这一点对本地代理很重要，因为代理需要在窗口关闭后继续为 CLI 提供服务。

## 数据存在哪里

所有状态都落在本机数据目录里，不依赖任何云端账号：

| 路径 | 用途 |
| --- | --- |
| `~/.ai-switch/settings.json` | 应用级设置（语言、主题等） |
| `~/.ai-switch/ai-switch.db` | SQLite 主库：账号、算力池、会话、用量、MCP、技能等 |
| `~/.ai-switch/ai-switch-dev.db` | 开发构建（`pnpm tauri:dev`）使用的独立库，不会污染正式库 |
| `~/.ai-switch/web-service.json` | Web 服务配置（host、port、访问令牌、Tailscale 开关） |
| `~/.ai-switch/route-proxy-https.json` | 本地算力池 HTTPS 的启用与自启状态 |
| `~/.ai-switch/certs/route-proxy/` | 本地算力池 HTTPS 的自签根证书与服务器证书 |
| `~/.ai-switch/tailscale/` | Tailscale sidecar 的状态目录 |
| `~/.ai-switch/backups/` | 备份目录；`backups/config-snapshots` 保存写入原生配置前的快照（Unix 下权限 0700） |
| `~/.ai-switch/imports/` | 导入操作的中转目录 |
| `~/.ai-switch/logs/` | 运行日志 |

::: warning 数据目录包含密钥
API Key、官方账号凭据等敏感信息保存在数据目录下的 SQLite 数据库中。请把 `~/.ai-switch` 当作凭据目录对待：不要放进公开仓库或未加密的同步盘，建议在开启全盘加密的磁盘上使用。需要迁移到另一台机器时，用应用内的导出功能，而不是直接拷贝数据库文件。
:::

## 安装版里的 web 资源

桌面安装包会把构建好的前端资源一起打进去，放在可执行文件旁边的 `web/` 目录下。这份资源不只是给桌面窗口用的——一旦你在同一个应用里开启 [Web 服务模式](/deploy/web-service)，HTTP 服务会直接把它作为浏览器界面提供出去，所以桌面用户开启 Web 服务不需要再单独构建前端。

Rust 侧解析静态资源目录的顺序是：先看环境变量 `AI_SWITCH_STATIC_DIR`，再依次尝试可执行文件同级的 `web/`、`dist/`、`resources/web/` 等候选路径，最后回退到工作目录下的 `web/`、`dist/`。命中的判定条件是该目录里存在 `index.html`。这套解析逻辑对桌面端和独立服务器是同一份，所以在独立服务器模式下也可以放一个同级 `web/` 目录来省掉环境变量。

## 桌面端与 Web 服务是同一份东西

理解这一点可以省掉很多困惑：桌面端和 Web 服务模式不是两个应用，而是同一个 Rust 核心的两种接入方式。

| | 桌面端 | Web 服务 / 独立服务器 |
| --- | --- | --- |
| 界面 | 同一份 React 界面 | 同一份 React 界面 |
| 传输层 | Tauri IPC | `POST /api/:command` + `GET /ws/events` |
| 鉴权 | 由本地进程边界保证 | 访问令牌（Bearer 或 WebSocket 查询参数） |
| 数据库 | `~/.ai-switch` 下的同一个 SQLite 库 | 同一个库（同机运行时就是同一份文件） |
| 本地代理 | 默认监听 `127.0.0.1:19527` | 行为完全一致，端口占用时自动向上找可用端口 |

由此带来两个实际结论：

- **同一台机器上桌面端和 Web 服务读写同一份数据。** 在浏览器里加的账号，桌面端刷新后就能看到，反之亦然；不存在两份需要同步的配置。
- **代理行为不因入口而变。** 无论请求由桌面端界面触发还是由浏览器触发，路由决策、协议桥接、失败重试与自动恢复走的都是同一套代码。相关行为见 [协议路由与桥接](/guide/protocol-routing) 和 [稳定性与自动恢复](/guide/reliability)。

绝大多数命令在两种传输下都可用，只有三个命令是桌面独占的，因为它们依赖原生桌面能力：打开证书目录、把会话拉起到系统终端应用、以及弹出系统保存对话框导出凭据。浏览器里调用它们会直接返回「仅桌面可用」。

## 自动启动

两个后台组件各自记住自己的开关状态，桌面端启动时按需恢复：

- **Web 服务**：`web-service.json` 中的 `autoStart` 为真时，应用启动后自动拉起 HTTP 服务。
- **本地算力池代理**：`route-proxy-https.json` 记录了上次是否处于运行状态，为真时启动应用会自动恢复代理（含 HTTPS 配置）。

此外应用会常驻一个自动恢复调度器，按账号配置的恢复规则定时重新启用被熔断的账号，无需手动干预。

## 下一步

- 想在手机或另一台电脑的浏览器里用同一份配置，看 [Web 服务模式](/deploy/web-service)。
- 想在没有桌面环境的服务器上长期跑一份，看 [独立服务器](/deploy/standalone-server)。
- 想从外网访问或给本地代理配 HTTPS，看 [远程访问与 HTTPS](/deploy/remote-access)。
- 想了解 IPC 与 HTTP 两条传输在代码里怎么共用一套命令，看 [架构总览](/dev/architecture)。
