---
title: 安装
description: 从 GitHub Releases 下载 AI Switch 桌面端安装包，了解 Windows、macOS、Linux 各平台的安装方式、本地数据目录结构、内置自动更新机制以及从源码构建的入口。
---

# 安装

AI Switch 桌面端从 GitHub Releases 获取，三个平台都有预构建的安装包。装完第一次启动会在你的用户目录下建好数据目录，不需要额外配置。

## 下载

打开 [Releases 页面](https://github.com/ijry/ai-switch/releases/latest)，在资产列表里找对应你系统的文件。

资产文件名统一是 `ai-switch_<版本>_<平台>_<原始文件名>` 的形式，`<平台>` 部分是 `windows-x86_64`、`darwin-aarch64` 或 `linux-x86_64`，照这个前缀找就行。

带 `-rc`、`-beta`、`-alpha` 的版本是预发布版，正常使用请挑不带这些后缀的。

### Windows

下载 NSIS 安装包（`.exe`），双击安装。当前发布的是 **x86_64** 架构。

安装完成后可执行文件所在目录会同时带一份 `web/` 目录，里面是 Web 界面的静态资源，Web 服务模式会用到。

### macOS

有两种资产：

- **`.dmg`** —— 挂载后把 AI Switch 拖进「应用程序」，常规装法
- **`.app`**（打包在压缩包里）—— 直接解压使用

当前发布的是 **aarch64（Apple Silicon）** 架构，因为 CI 的 macOS 构建跑在 Apple Silicon runner 上。Intel Mac 需要自己从源码构建。

首次打开如果被 Gatekeeper 拦住，在「系统设置 → 隐私与安全性」里允许一次。

### Linux

有两种资产，都是 **x86_64**：

- **`.deb`** —— Debian / Ubuntu 系用 `sudo apt install ./<文件名>.deb` 安装
- **`.AppImage`** —— 不用安装，`chmod +x` 之后直接运行

```bash
# .deb
sudo apt install ./ai-switch_0.6.7_linux-x86_64_ai-switch_0.6.7_amd64.deb

# AppImage
chmod +x ./ai-switch_*_linux-x86_64_*.AppImage
./ai-switch_*_linux-x86_64_*.AppImage
```

AI Switch 是 Tauri 应用，依赖系统的 WebKitGTK。发行版没预装的话需要自己补上（Debian / Ubuntu 上对应 `libwebkit2gtk-4.1-0`、`libgtk-3-0`、`librsvg2-2`，托盘图标还需要 ayatana appindicator）。`.deb` 会声明依赖，`apt` 会自动处理；用 AppImage 的话要手动确认。

## 数据存在哪里

AI Switch 的全部本地状态都在用户主目录下的 `~/.ai-switch/`（Windows 上是 `%USERPROFILE%\.ai-switch\`）。这个路径在启动时解析，**不可配置**。

```text
~/.ai-switch/
├── settings.json                    # 应用设置
├── ai-switch.db                     # SQLite 数据库（正式版）
├── ai-switch-dev.db                 # SQLite 数据库（开发版，与正式版隔离）
├── web-service.json                 # Web 服务配置
├── route-proxy-https.json           # 本地代理 HTTPS 配置
├── backups/
│   └── config-snapshots/            # 写入 CLI 配置前的快照（Unix 上权限 0700）
├── imports/                         # 账号导入的中间文件
├── logs/                            # 日志
├── tailscale/                       # Tailscale sidecar 状态
└── certs/route-proxy/               # 本地代理的 HTTPS 证书
```

几点说明：

**`settings.json`** 存应用级设置。数据库路径等信息会回写到这个文件里，但它是只读展示用的，改它不会让程序换目录。

**SQLite 数据库**是主要的数据载体：路由账号、算力池成员与游标、用量事件、会话、MCP 服务器、技能等等都在里面。schema 由 `src-tauri/migrations` 下的 **23 个迁移**管理，启动时自动执行。正式版用 `ai-switch.db`，开发版（`tauri dev` / debug 构建）用 `ai-switch-dev.db`，两者互不干扰，所以本地开发不会碰坏你日常使用的数据。

如果迁移出现冲突（比如从新版降级回旧版），程序会把数据库文件移到 `backups/` 下并加 `.migration-conflict-<时间戳>` 后缀，而不是直接损坏它。

**`backups/config-snapshots/`** 是安全直写机制的一部分。AI Switch 每次改写 CLI 配置文件之前都会先在这里存一份快照，用于回滚。这个目录在 Unix 上会被设为 `0700`。

::: warning 把 `~/.ai-switch` 当作凭据目录对待
路由账号的密钥（API key、令牌）保存在 `~/.ai-switch` 下的 **SQLite 数据库**里，不是系统钥匙串。

所以：

- **不要**把这个目录或数据库文件提交到 Git、丢进共享盘、放进公开的备份
- 备份的时候按凭据级别处理，最好加密
- 多用户机器上确认目录权限，只有你自己能读
- 迁移到新机器时，复制整个 `~/.ai-switch` 目录就能带走全部状态，包括密钥
:::

### AI Switch 写到别处的文件

除了自己的数据目录，AI Switch 在你点「写入路由配置文件」时会改动 CLI 自己的配置：

| 平台 | 文件 |
| --- | --- |
| Codex | `~/.codex/config.toml`，以及 `~/.codex/ai-switch-model-catalog.json` |
| Claude Code | `~/.claude/settings.json` |
| Gemini CLI | `~/.gemini/settings.json` |
| Grok | `~/.grok/settings.json` |

这些写入是**安全直写**：变更前建立快照、原子写入、检测并发修改、支持带守卫的回滚。你在这些文件里的其他配置项会被保留，AI Switch 只增改自己管理的字段。

OpenCode、OpenClaw、Hermes 不在此列 —— AI Switch 不写它们的原生配置。

## 自动更新

桌面端内置了 updater，不需要你手动去看有没有新版。

**手动检查**：应用内有 Updates 界面，可以主动检查、下载并安装，装完提示重启。

**自动检查**：应用会在启动后检查一次，之后每小时检查一次。有新版本时弹出提示，由你决定是否安装。这个间隔是固定的，目前没有开关或者更新通道的设置项。

**签名校验**：更新元数据从 GitHub Releases 的 `latest.json` 读取，每个平台的资产都带 minisign 签名。安装前会用应用内置的公钥校验签名，校验不过就不会安装。发布流程里还有一道额外的检查，确认每个签名的 key id 和公钥匹配，不匹配则构建失败。

所以自动更新链路上的信任锚点是编译进应用的那个公钥，中途被替换的包过不了校验。

## 从源码构建

不想用预构建包，或者需要 Intel Mac、Linux ARM 之类 Releases 里没有的目标，可以自己构建。

大致需要 Node（配 pnpm）、Rust 工具链，以及构建 Tailscale sidecar 用的 Go：

```powershell
corepack enable
pnpm install
pnpm build
pnpm tauri:build
```

完整的环境要求、各平台系统依赖、开发模式运行和检查命令，见 [本地开发](/dev/local-setup)。发布流程和 CI 细节见 [发布流程](/dev/release)。

## 下一步

- [快速开始](/guide/quick-start) —— 添加账号、启动代理、跑通第一条请求
- [平台支持矩阵](/guide/platform-support) —— 你的 CLI 支持到什么程度
- [桌面端](/deploy/desktop) —— 桌面端的部署细节
- [Web 服务模式](/deploy/web-service) —— 在浏览器或手机上访问
