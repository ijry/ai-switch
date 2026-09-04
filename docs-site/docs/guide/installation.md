---
title: 安装
description: 从 GitHub Releases 下载 AI Switch 桌面端安装包，了解 Windows、macOS、Linux 各平台的安装方式，解决 macOS 上「已损坏」「无法验证开发者」的 Gatekeeper 拦截，以及本地数据目录结构、内置自动更新机制和从源码构建的入口。
---

# 安装

AI Switch 桌面端从 GitHub Releases 获取，三个平台都有预构建的安装包。装完第一次启动会在你的用户目录下建好数据目录，不需要额外配置。

## 下载

打开 [Releases 页面](https://github.com/ijry/ai-switch/releases/latest)，正文顶部有一张下载表格，三个平台的安装包各占一行，点对应你系统的链接就行。

想直接翻资产列表也可以：桌面端安装包命名为 `ai-switch-<版本>-<平台>`（如 `ai-switch-0.8.0-windows-x86_64-setup.exe`），排在列表最前面。往后是独立服务器、Tailscale sidecar，以及自动更新才会用到的 `ai-switch-updater-*` 和 `latest.json`。

带 `-rc`、`-beta`、`-alpha` 的版本是预发布版，正常使用请挑不带这些后缀的。

### Windows

下载 `ai-switch-<版本>-windows-x86_64-setup.exe`（NSIS 安装包），双击安装。当前发布的是 **x86_64** 架构。

安装完成后可执行文件所在目录会同时带一份 `web/` 目录，里面是 Web 界面的静态资源，Web 服务模式会用到。

### macOS

装机用 **`ai-switch-<版本>-darwin-aarch64.dmg`**：挂载后把 AI Switch 拖进「应用程序」。同名的 `ai-switch-updater-*.app.tar.gz` 是内置自动更新下载的 `.app` 归档，手动安装不需要它。

当前发布的是 **aarch64（Apple Silicon）** 架构，因为 CI 的 macOS 构建跑在 Apple Silicon runner 上。Intel Mac 需要自己从源码构建。

首次打开会被 Gatekeeper 拦住，这是预期行为，处理方式见 [macOS 打不开：「已损坏」「无法验证开发者」](#macos-打不开-「已损坏」「无法验证开发者」)。

### Linux

有两种资产，都是 **x86_64**：

- **`ai-switch-<版本>-linux-x86_64.deb`** —— Debian / Ubuntu 系用 `sudo apt install ./<文件名>` 安装
- **`ai-switch-<版本>-linux-x86_64.AppImage`** —— 不用安装，`chmod +x` 之后直接运行

```bash
# .deb
sudo apt install ./ai-switch-0.8.0-linux-x86_64.deb

# AppImage
chmod +x ./ai-switch-0.8.0-linux-x86_64.AppImage
./ai-switch-0.8.0-linux-x86_64.AppImage
```

AI Switch 是 Tauri 应用，依赖系统的 WebKitGTK。发行版没预装的话需要自己补上（Debian / Ubuntu 上对应 `libwebkit2gtk-4.1-0`、`libgtk-3-0`、`librsvg2-2`，托盘图标还需要 ayatana appindicator）。`.deb` 会声明依赖，`apt` 会自动处理；用 AppImage 的话要手动确认。

## macOS 打不开：「已损坏」「无法验证开发者」

在 macOS 上第一次打开 AI Switch，你大概会遇到下面这类提示之一：

- 「**"AI Switch" 已损坏，无法打开。你应该把它移到废纸篓。**」
- 「**无法打开 "AI Switch"，因为无法验证开发者。**」
- 「**Apple 无法验证 "AI Switch" 是否包含恶意软件。**」

::: warning 先说清楚原因，别被「已损坏」这个词误导
**这不是下载损坏，也不是文件出错**，重新下载没有用。

原因是 AI Switch 的 macOS 包**没有经过 Apple 代码签名和公证（notarization）**。公证需要付费的 Apple Developer Program 账号，本项目目前没有配置。Gatekeeper 对未签名应用一律拦下，而「已损坏」是它在这种情况下会给出的措辞之一。

这也意味着：**Apple 没有替你扫描过这个包**。是否绕过这道拦截，是你自己的信任判断，不只是一个技术步骤。要降低风险，只从 [GitHub Releases 官方页面](https://github.com/ijry/ai-switch/releases/latest)下载，别用第三方转载的包。想彻底避开这个问题，就[从源码构建](/dev/local-setup)。
:::

### 方法一：系统设置里「仍要打开」（推荐）

这是 Apple 官方文档给出的路径，新旧系统通用；Sequoia 之后它也是唯一的放行入口。

1. 先**双击一次** AI Switch，让它被拦下来。这一步必须做，否则下一步不会出现按钮。
2. 打开**系统设置 → 隐私与安全性**，向下滚动到「安全性」区域。
3. 会看到一行说明 AI Switch 已被阻止打开，点右边的「**仍要打开**」。
4. 在再次弹出的警告里确认，需要时输入密码或用 Touch ID。

放行一次之后，AI Switch 会被记为例外，以后正常双击即可。

### 方法二：移除隔离属性

如果方法一里那行提示没出现（从压缩包解压出来的 `.app` 上比较常见），可以直接去掉 macOS 给下载文件打的隔离标记：

```bash
xattr -dr com.apple.quarantine "/Applications/AI Switch.app"
```

如果 `.app` 不在「应用程序」里，把路径换成实际位置。不确定路径就先输入 `xattr -dr com.apple.quarantine`，在后面留一个空格，然后从 Finder 把 `AI Switch.app` 拖进终端窗口，路径会自动补上，再回车。

::: tip 这条命令只作用于你指定的那一个路径
`-d` 是删除属性，`-r` 是递归处理 app 包内部，`com.apple.quarantine` 就是「这个文件来自网络」的标记。它不改动任何系统级安全设置，影响范围仅限你写在命令后面的那个路径。

通常不需要 `sudo`。app 在你自己的用户目录下时肯定不需要；装在 `/Applications` 里如果报权限不足，再加 `sudo` 并输入密码。
:::

### 两个别照做的老办法

网上的 macOS 解锁教程大多写于几年前，其中两条在现在的系统上已经不适用：

**「右键 → 打开」**：Apple 从 **macOS Sequoia（15）起移除了这个绕过途径**，Control-点按不再能覆盖 Gatekeeper，必须走系统设置。在 Sonoma（14）及更早的系统上它仍然有效。

**`sudo spctl --master-disable`**：这条命令**全局关闭**整台机器的 Gatekeeper 校验，此后任何来源的任何程序都不再被检查 —— 代价远超装一个应用所需。较新的 macOS 在「隐私与安全性」里也已经不再显示它对应的「任何来源」选项（现在只有「App Store」和「App Store 和已知开发者」两项）。装 AI Switch 不需要它，请用上面两个方法。

### 放行之前想自己验一遍

Release 页面目前**不公布 SHA-256 校验值**，所以没法拿官方哈希来核对。能做的是这两件事：

- **确认下载来源**。地址必须是 `github.com/ijry/ai-switch/releases/…`，文件名符合 `ai-switch_<版本>_darwin-aarch64_…` 的格式。
- **`.app.tar.gz` 带 minisign 签名可验**。资产列表里 `ai-switch_<版本>_darwin-aarch64_AI-Switch.app.tar.gz` 有配套的 `.sig` 文件，公钥在仓库的 `src-tauri/tauri.conf.json` 里（`plugins.updater.pubkey`）。注意 **`.dmg` 没有 `.sig`** —— 这个签名是给自动更新用的，不覆盖 dmg 安装包。

这些都替代不了 Apple 公证：公证的价值在于 Apple 扫描过内容，而签名只能证明文件出自持有该私钥的一方、传输途中没被换掉。

::: info 装好之后的自动更新不会再被拦
应用内置的 updater 走自己的签名链路（minisign 公钥编译进应用，见下面的[自动更新](#自动更新)），和 Apple 公证是两套独立机制。所以上面这套操作只在首次安装时做一次，后续更新不会再遇到。
:::

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

**签名校验**：更新元数据从 GitHub Releases 的 `latest.json` 读取，updater 走的每个资产都带 minisign 签名（`.exe`、`.deb`、`.AppImage` 和 macOS 的 `.app.tar.gz` 各有配套的 `.sig`；**`.dmg` 没有** —— 它是给人手工安装用的，不在更新链路上）。安装前会用应用内置的公钥校验签名，校验不过就不会安装。发布流程里还有一道额外的检查，确认每个签名的 key id 和公钥匹配，不匹配则构建失败。

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
