---
title: 会话管理
description: AI Switch 通过扫描各个 CLI 自己写在本地的会话记录文件来汇总会话列表，支持跨 7 个平台搜索、查看消息，并用平台原生的恢复命令在 Vibe 终端或系统终端里继续对话。
---

# 会话管理

各家 AI CLI 都会把对话记录写在本地磁盘上，但每家的目录、文件名和 JSON 结构都不一样。AI Switch 的会话管理做的事情很直接：**把这些散落的记录文件扫出来，统一成一份可搜索的列表，再用对应平台自己的命令把某条会话接着聊下去。**

这里有一个前提需要先说清楚：**会话记录不是 AI Switch 产生的，也不由 AI Switch 保管。** 它只读那些文件。删掉 AI Switch 不会影响你的会话记录；反过来，AI Switch 也无法恢复被 CLI 自己清理掉的记录。

## 会话从哪里来

后端为 7 个平台各自定义了扫描根目录和文件扩展名（都相对于用户主目录）：

| 平台 | 扫描目录 | 文件扩展名 |
| --- | --- | --- |
| Codex | `.codex/sessions`、`.codex` | `.jsonl` |
| Claude Code | `.claude/projects`、`.cache/claude/projects` | `.jsonl` |
| Grok | `.grok/sessions`、`.xai/sessions`、`.cache/grok/sessions` | `.json`、`.jsonl` |
| Gemini CLI | `.gemini/tmp`、`.cache/gemini/tmp` | `.json`、`.jsonl` |
| OpenCode | `.local/share/opencode`、`AppData/Local/opencode` | `.json`、`.jsonl` |
| OpenClaw | `.openclaw/agents` | `.jsonl` |
| Hermes Agent | `.hermes/sessions` | `.json`、`.jsonl` |

扫描规则：

- 每个根目录**最多向下递归 6 层**，单个平台**最多收集 1000 个文件**，避免在超大历史目录上卡住。
- 同一路径被多个根目录命中时会去重（比如 Codex 的两个根目录互相嵌套）。
- 目录不存在就跳过，不报错。

因为是按目录扫描，会话列表天然反映"这台机器上装过、用过哪些 CLI"。没装某个 CLI，它就不会出现在列表里。

## 单条会话的字段是怎么推断出来的

记录文件的格式各不相同，所以解析层用的是一组带回退的启发式规则。每个字段都尝试多种常见键名：

| 字段 | 来源 | 回退 |
| --- | --- | --- |
| 会话 ID | 前 20 行里的 `session_id` / `sessionId` / `id` / `payload.id` | 文件名（去扩展名） |
| 项目目录 | `cwd` / `project_dir` / `projectDir` / `payload.cwd` / `payload.project_dir` | 文件所在目录名 |
| 时间戳 | `timestamp` / `created_at` / `createdAt` / `ts`，支持整数纪元秒与 RFC 3339 | 文件修改时间 |
| 标题 | 第一条"真人说的话" | 目录名 |

**标题提取是最费心的一段。** 它会跳过 assistant / developer / system / tool 角色的消息，也会跳过那些看起来是注入上下文而不是用户输入的内容——以 `<permissions instructions>`、`<skills_instructions>`、`<environment_context>`、`<instructions>`、`# agents.md instructions` 之类开头的块都被认为是上下文而非提问。找到第一条真正的用户消息之后截断到 72 个字符，超长补省略号。

为了不把整个大文件读进内存，列表阶段**只读每个文件的前 80 行**来推断标题和时间；点开某条会话查看消息时才读更多，且上限是 **2000 行**。

### 子智能体会话被过滤掉

并行子智能体会给自己也写一份记录文件。这些文件在列表里没有价值（它们不是你发起的对话），所以会被识别并丢弃：

| 平台 | 判定条件 |
| --- | --- |
| Codex | `payload.thread_source == "subagent"`，或存在 `payload.source.subagent.thread_spawn` |
| Claude Code | `isSidechain == true` |
| 其他 5 个平台 | 不过滤（记录格式里没有等价标记） |

### 排序与数量上限

所有平台的结果汇总之后，按**最后活跃时间**降序排列（没有最后活跃时间就用创建时间），然后**截断到 500 条**。所以会话列表始终是"最近 500 条"，不是全量历史。

角色名也会被归一化，方便前端统一显示：`human`、`user_message` → `user`；`assistant_message`、`ai` → `assistant`；`tool`、`tool_result`、`function_call` → `tool`。

## 两个入口

### 会话管理界面

在主界面走 **设置 → 功能入口 → 会话** 打开（它不是顶层导航项）。界面上有：

- **搜索框**：按标题、项目目录、平台过滤。
- **分组 / 平铺** 两种排布：分组按项目目录聚合，平铺就是一条平的时间线。
- **消息面板**：选中一条会话后展示解析出来的消息，带角色标签（用户 / 助手 / 系统 / 工具 / 开发者）和快速导航。
- **复制按钮**：复制目录、复制源文件路径、复制恢复命令。
- **在系统终端中恢复**：直接调起操作系统的终端应用。

### Vibe 终端侧栏

[Vibe 终端](/features/vibe) 左侧就是同一份会话列表。区别是这里恢复会话会在 **Vibe 内部开一个终端标签**，不离开当前界面。

## 恢复会话

每条会话都会被合成一条恢复命令，命令形态取自各平台自己的 CLI 约定：

| 平台 | 恢复命令 |
| --- | --- |
| Codex | `codex resume <session_id>` |
| Claude Code | `claude --resume <session_id>` |
| Grok | `grok resume <session_id>` |
| Gemini CLI | `gemini --resume <session_id>` |
| OpenCode | `opencode session <session_id>` |
| OpenClaw | `openclaw resume <session_id>` |
| Hermes Agent | `hermes resume <session_id>` |

这条命令会在会话的**项目目录**下执行——不是在你启动 AI Switch 的目录下。所有 7 个平台在能力矩阵里都标记为支持 `session_resume`，详见 [平台支持矩阵](/guide/platform-support)。

恢复前有两个硬性前提：**项目目录**和**恢复命令**都必须存在。缺任何一个，按钮会报错而不是启动一个注定失败的终端。

### 两条恢复路径的差别

| | Vibe 内终端标签 | 系统终端 |
| --- | --- | --- |
| 触发位置 | Vibe 左侧列表 | 会话管理界面的「在系统终端中恢复」 |
| 执行方式 | AI Switch 自己起的 PTY | 调起操作系统的终端应用 |
| Web 服务模式 | **可用** | **不可用**（该命令未通过 HTTP 暴露） |
| 适用场景 | 想留在一个界面里管多个任务 | 想用自己配好的终端（配色、快捷键、tmux） |

系统终端的调起方式按操作系统分：

| 系统 | 行为 |
| --- | --- |
| Windows | `cmd.exe /D /K <命令>` |
| macOS | 通过 `osascript` 让 Terminal.app 执行 `cd -- '<目录>' && <命令>` |
| Linux | 依次尝试 `x-terminal-emulator`、`gnome-terminal`、`konsole`、`xfce4-terminal` |

Linux 上四个都找不到时会返回 "No supported terminal emulator was found"——这时候用 Vibe 内的终端标签，或者手工复制恢复命令。

::: tip 恢复失败先看 CLI 本身
恢复命令是拼出来交给 shell 执行的，AI Switch 不解释它的语义。如果终端起来了但 CLI 报"找不到会话"，通常是那个 CLI 自己已经清理了记录，或者它的 resume 子命令在新版本里改了。手工在同一目录下跑一遍同样的命令即可确认。
:::

## 会话与终端的关系

这两个概念容易混，明确一下：

- **会话（session）** 是磁盘上的一份记录文件，由某个 CLI 写入，AI Switch 只读。关掉 AI Switch，会话还在。
- **终端（terminal）** 是 AI Switch 起的一个 PTY 进程，生命周期跟着应用走。关掉标签，进程被杀。

一条会话可以被恢复任意多次，每次都产生一个新的终端。一个终端也不一定对应任何会话——直接开的 Shell 标签就没有。

## 隐私

会话记录里往往包含代码片段、文件路径、有时还有密钥。相关行为如下：

- **只读，不上传。** 解析全部发生在本机（桌面端是本地进程，Web 服务模式下是运行服务的那台机器）。
- **不复制、不缓存。** 会话内容不会被写进 AI Switch 的数据库，每次列表和查看都是重新读文件。
- **Web 服务模式要注意。** 把 Web 服务开出去，等于把那台机器上所有 CLI 的会话记录都开放给能通过鉴权的人。务必配好访问令牌，参考 [Web 服务模式](/deploy/web-service) 和 [远程访问与 HTTPS](/deploy/remote-access)。

## 下一步

- [Vibe 终端与皮肤](/features/vibe)：在应用内的终端标签里恢复会话
- [平台支持矩阵](/guide/platform-support)：各平台的 `session_resume` 与终端启动能力
- [MCP 服务器](/features/mcp)：给这些 CLI 统一配置工具服务器
- [技能管理](/features/skills)：给这些 CLI 统一管理技能
