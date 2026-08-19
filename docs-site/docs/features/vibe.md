---
title: Vibe 终端与皮肤
description: Vibe 是 AI Switch 内置的多标签终端工作区，可直接启动智能体或恢复本地会话，并支持三套内置皮肤与完全自定义的皮肤包（.json / .aiskin / .zip）。
---

# Vibe 终端与皮肤

Vibe 是 AI Switch 里的一个**全屏终端工作区**。它不是外部终端的替代品，而是把"选一个智能体、选一个项目目录、开一个终端"这三件事收进同一个界面里：左侧列出扫描到的本地会话，中间是终端标签，底部是启动栏。每个终端标签背后都是一个真实的 PTY 进程，用 xterm.js 渲染。

从主界面侧边栏顶部的「切换到 Vibe 模式」进入，Vibe 内部也有返回主界面的入口。

## 三种启动方式

Vibe 打开终端时只有三种意图（`TerminalLaunchKind`），后端会据此决定实际执行什么：

| 类型 | 触发方式 | 实际执行的命令 |
| --- | --- | --- |
| `shell` | 新建 Shell 标签 | 系统默认 shell |
| `agent` | 启动栏或"创建会话"对话框 | 按平台映射到可执行文件名 |
| `resume` | 点击左侧会话列表里的恢复按钮 | 把会话的恢复命令交给 shell 执行 |

`agent` 类型的平台映射是固定的七个值，写死在后端：

| 平台参数 | 启动的可执行文件 |
| --- | --- |
| `codex` | `codex` |
| `claude` | `claude` |
| `grok` | `grok` |
| `gemini` | `gemini` |
| `opencode` | `opencode` |
| `openclaw` | `openclaw` |
| `hermes` | `hermes` |

传入其他平台名会被拒绝（`Unsupported terminal platform`）。这也意味着 **Vibe 假设对应的 CLI 已经在 PATH 里**——AI Switch 不负责安装它们。

启动前还有两项校验：工作目录必须非空且确实存在（否则 `Working directory does not exist`）；`resume` 类型必须带恢复命令（否则 `Resume command is required`）。终端默认尺寸是 100 列 × 30 行，之后随面板大小自动 fit 并把新尺寸同步给 PTY。

会话列表的来源、恢复命令怎么拼出来的，见 [会话管理](/features/sessions)。

## 终端行为

- **输出**：后端把 PTY 输出以 `terminal://output` 事件推给前端，`terminal://exit` 与 `terminal://error` 分别对应进程退出与终端错误，退出码会直接打在终端里（`[process exited: 0]`）。
- **输入**：xterm 的 `onData` 直接回写到 PTY，包括控制字符。
- **关闭标签**：如果进程还在 running 状态，关闭标签会先杀掉进程再移除标签。
- **字体**：优先 JetBrains Mono / Cascadia Code / SF Mono / Consolas，字号 13。

Vibe 在桌面端和 [Web 服务模式](/deploy/web-service) 下都可用——终端创建、输入、缩放、关闭都有对应的 HTTP 命令。区别在于**目录选择器**：桌面端可以弹出系统文件夹对话框，浏览器里只能从下拉列表里选已有目录或手工填路径。

## 三种外观模式

外观面板（皮肤任务栏的「外观设置」，或界面上的外观按钮）里有三个主题：

| 主题 | 说明 |
| --- | --- |
| `light` | 浅色 |
| `dark` | Solarized Dark，默认 |
| `skin` | 皮肤模式，套用当前选中的皮肤包 |

选择结果写在浏览器本地存储的 `ai-switch.vibe.appearance` 键里（含主题、皮肤 id、音效开关），导入的自定义皮肤单独存在 `ai-switch.vibe.custom-skin`。

::: tip 皮肤模式下终端是透明的
在皮肤模式下，Vibe 会把 xterm 的 wrapper、viewport、screen、rows、canvas 各层背景都置为透明，好让皮肤定义的 `terminalShell` 背景（图片、渐变、边框）透出来。所以皮肤的可读性取决于 `terminal.foreground` 等前景色配置，而不是 `terminal.background`。
:::

## 三套内置皮肤

内置皮肤和用户导入的皮肤走的是**同一套解析逻辑**——它们就是仓库里三个普通的清单文件，没有任何特殊通道：

- `src/skins/codex-2007-blue/skin.json`
- `src/skins/rescue-pups-adventure-bay/skin.json`
- `src/skins/starship-cockpit/skin.json`

三者的实际差别：

| | Codex 2007 Blue | 汪汪队救援主题 | 星舰驾驶舱 |
| --- | --- | --- | --- |
| 清单 `id` | `codex-2007-blue` | `rescue-pups-adventure-bay` | `starship-cockpit` |
| `decorations.variant` | `codex-2007` | `rescue-pups` | `starship-cockpit` |
| 定制的区域数 | 31 | 49 | 49 |
| `blocks` | titlebar / profile / showcase / statusbar / taskbar | 同左 | 另有 `launch`（启动栏文案） |
| 头像模板 | 无（用 showcase 吉祥物） | `rescue-rider` | `space-ai-core` |
| 展示台模板 | `qq-mascot` | `rescue-hq` | `space-ship` |
| 右侧卡片 | `qq-person` | `rescue-dog-team`、`rescue-civic` | `space-radar`、`space-ship`、`space-starmap`、`space-telemetry` |
| 任务栏开始按钮 | 「开始」 | 「出动」 | 「舰桥」 |
| 音效 | 无 | 无 | **有**，3 个事件音 + 1 条环境音 |
| 附带资源 | 无 | 无 | `assets/sounds/` 下三个 wav |

三套皮肤都用纯 CSS 渐变和应用内置的矢量装饰件构造视觉，**不依赖外部图片**，所以清单本身可以直接当作自定义皮肤的模板抄。

只有星舰驾驶舱带 `assets/` 目录，也只有它定义了 `audio`。

## 皮肤包格式

导入时接受三种文件：

| 扩展名 | 内容 |
| --- | --- |
| `.json` | 纯 JSON 清单，不含资源 |
| `.aiskin` | 既可以是 JSON 清单，也可以是改了扩展名的 zip 包（解析失败会自动按 zip 再试一次） |
| `.zip` | 包内必须有 `skin.json` 或 `vibe-skin.json` |

zip 包里的相对路径资源会在导入时被读出来、转成 data URL 存进本地存储，所以导入之后皮肤不再依赖原始 zip 文件。

限制有两个：

- 导入文件本身不超过 **8 MB**（`8 * 1024 * 1024`）。
- 嵌入资源之后序列化的皮肤不超过约 **4.5 MB**，超了会被本地存储拒绝。

图片资源按扩展名判断 MIME：`.jpg` / `.jpeg`、`.webp`、`.gif`、`.svg`，其余按 PNG 处理。音频只认 `.mp3`、`.ogg`、`.wav`，其他扩展名的音频引用会被直接丢弃。

绝对路径、带协议前缀的 URL、以及含 `..` 的路径一律被当作不安全引用丢掉——只有包内相对路径和白名单内的 data URL 会被接受。

一个典型的包结构：

```text
my-skin.zip
├── skin.json
└── assets/
    ├── background.png
    ├── terminal-shell.png
    ├── avatar.png
    └── sounds/
        └── ping.wav
```

## skin.json 结构

清单是一个 JSON 对象，顶层字段如下：

```json
{
  "id": "my-skin",
  "name": "我的皮肤",
  "author": "someone",
  "version": "1.0.0",
  "ui": {},
  "terminal": {},
  "regions": {},
  "blocks": {},
  "decorations": {},
  "audio": {},
  "showcase": {}
}
```

只有 `id` 和 `name` 是必要的标识信息，其余全部可选；缺失的值回落到内置的 Codex 2007 Blue。

### ui：全局配色

`ui` 是整套皮肤的调色板，每个字段都直接吃 CSS 值，可以写纯色、渐变或 `linear-gradient(...)`：

```json
{
  "ui": {
    "accent": "#1678d8",
    "accentText": "#ffffff",
    "background": "linear-gradient(180deg, #63b9fb 0%, #0e62b8 100%)",
    "backgroundImage": "assets/background.png",
    "backgroundOverlay": "linear-gradient(180deg, rgba(255,255,255,0.24), rgba(8,63,126,0.24))",
    "panel": "rgba(226, 245, 255, 0.88)",
    "panelStrong": "rgba(255, 255, 255, 0.96)",
    "panelSubtle": "rgba(188, 226, 250, 0.8)",
    "border": "rgba(14, 99, 181, 0.42)",
    "text": "#0d315d",
    "mutedText": "#3d6d9f",
    "button": "linear-gradient(180deg, #63c7ff 0%, #0c5cab 100%)",
    "buttonText": "#ffffff",
    "buttonHover": "linear-gradient(180deg, #7bd3ff 0%, #0b539e 100%)",
    "dangerBackground": "linear-gradient(180deg, #ff7e87, #b72434)",
    "dangerText": "#ffffff",
    "tabBar": "rgba(239,250,255,0.94)",
    "tabActive": "#ffffff",
    "tabInactive": "rgba(151,210,247,0.54)",
    "tabHover": "rgba(255, 255, 255, 0.72)",
    "focus": "#44a7ff"
  }
}
```

### terminal：xterm 配色

`terminal` 可以覆盖 xterm 的任意一个颜色键，一共 18 个：

```text
background  foreground
black       red         green        yellow
blue        magenta     cyan         white
brightBlack brightRed   brightGreen  brightYellow
brightBlue  brightMagenta brightCyan brightWhite
```

未指定的键沿用当前明暗主题的默认值。

### regions：分区样式

`regions` 是皮肤能力最强的部分——按界面分区逐块覆盖样式。可用的分区键共 **59** 个：

```text
app                body               titlebar           titlebarControls
windowButton       windowButtonMinimize windowButtonMaximize windowButtonClose
toolbar            sidebar            sidebarHeader      sidebarProfile
avatar             onlineBadge        profileBadge       controlPanel
sessionList        listTrigger        sessionRow         groupPanel
workspace          tabBar             tab                tabActive
tabClose           terminalShell      emptyState         modal
rightRail          rightCard          showcaseStage      showcaseFigure
showcaseFooter     statusBar          button             buttonHover
ghostButton        field              select             danger
showcaseOrb        launchPanel        agentStrip         agentOption
agentOptionActive  composer           composerInput      composerMetaBar
composerControl    composerSendButton composerAddon      taskbar
taskbarStartButton taskbarStartMenu   taskbarMenuItem    taskbarItem
taskbarItemActive  taskbarTray        taskbarClock
```

每个分区支持 **16** 个样式字段：

```text
background  backgroundImage  backgroundOverlay  backgroundSize
backgroundPosition  backgroundRepeat  border  color
shadow  backdropFilter  borderRadius  padding
fontSize  lineHeight  letterSpacing  textTransform
```

例如给终端外壳配一张背景图：

```json
{
  "regions": {
    "terminalShell": {
      "backgroundImage": "assets/terminal-shell.png",
      "backgroundSize": "cover",
      "backgroundPosition": "center",
      "border": "1px solid rgba(255,255,255,0.24)",
      "borderRadius": "12px",
      "shadow": "0 18px 48px rgba(0,0,0,0.42)"
    }
  }
}
```

### blocks：文案与图片

`blocks` 负责替换界面上由应用渲染的**文本和图片引用**，不能注入结构：

| 块 | 可用字段 |
| --- | --- |
| `titlebar` | `title`、`subtitle`、`badge` |
| `profile` | `name`、`status`、`signature`、`badge`、`avatar` |
| `showcase` | `enabled`、`title`、`subtitle`、`body`、`badge`、`figure`、`footer` |
| `statusbar` | `left`、`right` |
| `launch` | `title`、`body`、`placeholder`、`sendLabel`、`folderLabel`、`modelLabel`、`reasoningLabel`、`agentStripLabel`、`agentStripPrefix`、`agentStripSuffix`、`extraLabel`、`extraValue` |
| `taskbar` | `enabled`、`startButton`、`startMenu`、`items`、`tray`、`clockFormat` |

`blocks.showcase` 优先于顶层的旧字段 `showcase`。

任务栏的开始菜单项只允许四个动作，其他值一律被忽略：

| 动作 | 效果 |
| --- | --- |
| `openAppearance` | 打开外观面板 |
| `setTheme` | 切换主题，`theme` 只接受 `dark` / `light` / `skin` |
| `importSkin` | 打开皮肤导入对话框 |
| `clearSkin` | 清除已导入的自定义皮肤 |

菜单里也可以放 `{"type": "separator"}` 分隔线和 `disabled: true` 的装饰性条目。

### decorations：装饰件模板

装饰件不是自由的 HTML，而是从白名单里挑应用内置的矢量图形：

- `variant`：`codex-2007`、`rescue-pups`、`starship-cockpit`
- `titlebarMark`：标题栏角标文字，超过 4 个字符会被截断
- `avatarTemplate` / `showcaseTemplate` / `rightCards[].template` / `rightCards[].items[].template`：从 13 个模板里选——`qq-mascot`、`qq-person`、`rescue-rider`、`rescue-hq`、`rescue-dog-team`、`rescue-civic`、`rescue-mayor`、`rescue-chicken`、`space-ai-core`、`space-ship`、`space-radar`、`space-telemetry`、`space-starmap`
- `items[].tone`：`red`、`blue`、`yellow`、`green`、`pink`、`orange`、`neutral`

不在白名单里的值会被静默忽略，不会报错也不会渲染。

### audio：音效

`audio` 是可选的，星舰驾驶舱皮肤演示了完整用法：

```json
{
  "audio": {
    "enabled": true,
    "volume": 0.48,
    "events": {
      "agentSelect": "assets/sounds/weapon-switch.wav",
      "hologramInteract": "assets/sounds/hologram-tap.wav",
      "radarPulse": "assets/sounds/radar-pulse.wav"
    },
    "ambient": [
      { "id": "radar", "src": "assets/sounds/radar-pulse.wav", "intervalMs": 8500, "volume": 0.18 }
    ]
  }
}
```

- `events` 只认三个事件名：`agentSelect`（切换智能体）、`hologramInteract`（点击全息投影装饰件）、`radarPulse`。
- `ambient` 最多 6 条。带 `loop: true` 就循环播放；带 `intervalMs` 就按毫秒间隔重复触发；两者都没有就只播一次。
- 音量都会被夹到合法区间，事件音默认 0.5，环境音默认 0.35。
- 音效只在**皮肤模式**下、且外观面板里的「皮肤音效」开关打开、且皮肤自己没写 `enabled: false` 时才会响。环境音还需要用户先有一次交互才会启动（浏览器自动播放策略）。

## 安全边界

皮肤是可以随手从网上下载并导入的文件，所以解析层刻意做窄：

- **不执行皮肤里的任何代码**。没有 HTML 片段、没有外挂 CSS 文件、没有脚本，只有字符串、颜色值和图片/音频引用。
- **装饰件、任务栏动作、装饰色调都走白名单**，不认识的值直接丢。
- **启动栏的智能体图标是应用内置的矢量图**，皮肤改不了。
- **标题栏上的最小化/最大化/关闭按钮是装饰性的**，有意不接任何窗口命令。
- **相对路径资源只能指向包内**，`..`、绝对路径和外部 URL 都被拒绝。

换句话说，一个恶意皮肤最多能把界面做得很难看，拿不到执行能力。

## 做一套自己的皮肤

1. 复制 `src/skins/` 下任意一个内置皮肤目录，或者从 `fixtures/vibe-skins/rescue-pups/skin.json` 起步。
2. 改 `id`（必须唯一）和 `name`。
3. 调 `ui` 的调色板，这一步影响最大、收益最快。
4. 需要精修时再逐个分区写 `regions`，需要换文案时写 `blocks`。
5. 有图片或音频就放到 `assets/`，在清单里用相对路径引用。
6. 打包：
   - 无资源 → 直接把 `skin.json` 改名成 `我的皮肤.aiskin`。
   - 有资源 → 把 `skin.json` 和 `assets/` 一起压成 zip（注意 `skin.json` 要在压缩包根目录，不要多套一层文件夹）。
7. 在外观面板里点「导入皮肤」，选中文件；导入成功会自动切到皮肤模式。想撤销就点「清除自定义皮肤」。

## 下一步

- [会话管理](/features/sessions)：Vibe 左侧那份会话列表是怎么来的
- [账号与算力池](/guide/accounts)：让 Vibe 里启动的 CLI 走上算力池
- [平台支持矩阵](/guide/platform-support)：哪些平台支持终端启动与会话恢复
- [架构总览](/dev/architecture)：终端与 PTY 在整体架构里的位置
