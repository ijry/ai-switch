---
title: MCP 服务器
description: AI Switch 用一份规范化的 MCP 服务器定义，一次性写入 11 个客户端各自的配置文件，自动处理 stdio/SSE/HTTP 传输差异、键名差异和格式差异，并内置官方注册表与 Smithery 市场。
---

# MCP 服务器

MCP（Model Context Protocol）服务器给 AI CLI 提供额外的工具能力——读文件、查数据库、调 API。问题在于：**同一个 MCP 服务器，在 11 个客户端里要写 11 遍，而且写法各不相同。** 配置文件位置不同，格式有 JSON、TOML、YAML，服务器列表的键名不同，传输类型的表达方式也不同。

AI Switch 的 MCP 管理解决的就是这一件事：你写一份**规范定义**，选中要生效的客户端，它负责翻译成每个客户端认得的形状并写进对应文件。

## 支持的 11 个客户端

以下路径都是各客户端**公开的配置文件位置**——AI Switch 只是按这些公开的文件格式读写。

| 客户端 | 配置文件 | 格式 | 服务器列表键 |
| --- | --- | --- | --- |
| Codex CLI | `$CODEX_HOME/config.toml`（默认 `~/.codex/config.toml`） | TOML | `mcp_servers` |
| Claude Code | `~/.claude.json` | JSON | `mcpServers` |
| Gemini CLI | `~/.gemini/settings.json` | JSON | `mcpServers` |
| Grok | `$GROK_HOME/config.toml`（默认 `~/.grok/config.toml`） | TOML | `mcp_servers` |
| OpenCode | `~/.config/opencode/opencode.json` | JSON | `mcpServers`（兼容旧 `mcp`） |
| OpenClaw | `~/.openclaw/openclaw.json` | JSON | `mcp.servers`（嵌套） |
| Hermes Agent | `$HERMES_HOME/config.yaml`（默认 `~/.hermes/config.yaml`） | YAML | `mcp_servers` |
| Cline | `~/.cline/data/settings/cline_mcp_settings.json` | JSON | `mcpServers` |
| Cursor | `~/.cursor/mcp.json` | JSON | `mcpServers` |
| Kimi Code | `$KIMI_CODE_HOME/mcp.json`（默认 `~/.kimi-code/mcp.json`） | JSON | `mcpServers` |
| CodeBuddy | `~/.codebuddy.json` | JSON | `mcpServers` |

四个客户端支持环境变量覆盖主目录（`CODEX_HOME`、`GROK_HOME`、`HERMES_HOME`、`KIMI_CODE_HOME`），AI Switch 会先读环境变量再回落到默认路径。

Claude Code 和 CodeBuddy 还各有一个辅助文件（`~/.claude/settings.json`、`~/.codebuddy/settings.json`），用于插件启用状态；AI Switch 在需要时一并维护。

## 一份规范定义

界面上你填的是一个 JSON 对象，这是 AI Switch 自己的**规范形状**（canonical spec），不是任何某个客户端的原生格式：

```json
{
  "type": "stdio",
  "command": "npx",
  "args": ["-y", "@modelcontextprotocol/server-filesystem", "."]
}
```

`type` 有三个取值：

| `type` | 需要的字段 | 说明 |
| --- | --- | --- |
| `stdio` | `command`，可选 `args`、`env`、`cwd` | 本地进程，通过标准输入输出通信 |
| `sse` | `url`，可选 `headers` | Server-Sent Events 远程服务器 |
| `http` | `url`，可选 `headers` | 可流式 HTTP 远程服务器 |

完整字段：

| 字段 | 适用传输 | 说明 |
| --- | --- | --- |
| `command` | stdio | 可执行文件 |
| `args` | stdio | 参数数组 |
| `env` | stdio | 环境变量对象 |
| `cwd` | stdio | 工作目录 |
| `url` | sse / http | 服务器地址 |
| `headers` | sse / http | 请求头对象，常用于放 token |

JSON 语法错误会在保存前被拦下（`mcp.invalidJson`），字段不合法则由后端返回 `mcp.invalid_spec`。

服务器 ID 也有校验：不能为空，不能含路径分隔符或其他会破坏配置文件结构的字符，否则返回 `mcp.invalid_server_id`。

## 规范化解决了什么

这是这个功能真正的价值所在。同一份定义写进不同客户端时，会按各客户端**公开的配置格式**做以下调整：

| 差异 | 具体处理 |
| --- | --- |
| **文件格式** | JSON / TOML / YAML 三种序列化器，各自保留原文件里的其他配置段 |
| **服务器列表键名** | `mcpServers` / `mcp_servers` / 嵌套的 `mcp.servers` 三种写法 |
| **传输字段名** | 有的用 `type`，有的用 `transport`，有的干脆不写传输字段 |
| **HTTP 的叫法** | 部分客户端把可流式 HTTP 写成 `streamableHttp` 而不是 `http` |
| **请求头键名** | Codex 的 TOML 里叫 `http_headers`，双向映射到规范里的 `headers` |
| **命令的表达** | OpenCode 的旧式 `mcp` 段把命令写成数组 `[命令, ...参数]`，环境变量键叫 `environment` |
| **多余字段** | Cursor 只保留它认识的键；Grok 的条目里的 `enabled`、`required` 等额外键在改写时**原样保留** |

有一个能力差异必须显式处理：**Codex CLI 不支持 SSE 传输。** 给它写 `type: "sse"` 的服务器会被拒绝，错误码 `mcp.unsupported_transport`。想让 Codex 用同一个远程服务器，改成 `http` 传输（如果对方支持）或用 stdio 代理。

如果选中的客户端里没有一个能接受当前定义，会返回 `mcp.no_compatible_client`。

## 写入是原子的

改配置文件这件事有风险——用户可能正在用那个 CLI，写一半崩了就把配置弄坏了。所以写入流程是：

1. 读现有文件。**文件不存在等同于空对象**，不会报错，也不会因此丢掉本来没有的其他配置。
2. 在内存里的数据结构上只改 MCP 相关的那一段，其他配置段原样保留。
3. 序列化，写进同目录下的临时文件（名字形如 `.config.toml.ai-switch-<uuid>`）。
4. 重命名覆盖目标文件。

同目录内的 rename 在主流文件系统上是原子操作，所以任何时刻读到的都是完整的旧文件或完整的新文件。

读文件失败（权限、磁盘）返回 `mcp.config_io`；文件存在但解析不出来（手工改坏了）返回 `mcp.config_invalid`——这种情况 AI Switch **不会**猜着修复，它宁可报错也不覆盖你可能还想抢救的内容。

::: warning 手工改过的配置文件
如果某个客户端的配置文件里有语法错误（比如 TOML 里少了引号），AI Switch 无法写入该客户端并会报 `mcp.config_invalid`。先用编辑器修好语法，再回来重试。
:::

## 本地服务器管理

MCP 界面分「本地」和「市场」两个视图。本地视图管你已经配置的服务器。

每张服务器卡片显示：服务器 ID、传输类型徽标、`command` 或 `url`、以及一排客户端标签——**标签就是这个服务器当前生效于哪些客户端**。

新建或编辑时需要填三样：

1. **服务器 ID**：在各客户端配置文件里作为键名。
2. **规范 JSON**：上一节那个对象。
3. **目标客户端**：至少选一个。默认勾选 Codex CLI 和 Claude Code。

ID 为空或没勾任何客户端时保存按钮是禁用的。删除会先弹确认框，因为它会同时从所有相关客户端的配置文件里移除该条目。

## 市场

市场视图接了两个来源：

| 来源 | 说明 |
| --- | --- |
| 官方 MCP 注册表 | Model Context Protocol 官方的服务器注册表 |
| Smithery | 第三方 MCP 服务器目录 |

搜索单次返回最多 30 条。点开一条会进详情弹窗，里面有描述、主页链接、目标客户端选择器，以及两块由市场元数据驱动的动态内容：

- **传输选择**：一个服务器可能同时提供 stdio 和远程安装方式，多于一种时给出选择。
- **参数表单**：市场声明的每个参数会渲染成对应控件——枚举变成下拉框，布尔变成复选框，JSON 变成多行文本框，密钥类型变成密码框，数字类型变成数字输入。任何**必填参数没填**，安装按钮就是禁用的。

安装本质上就是"帮你把规范定义填好，然后走和手工新建完全相同的写入流程"，之后它和手工配的服务器没有任何区别。

市场相关错误：网络失败 `mcp.marketplace_network`，返回数据不合法 `mcp.marketplace_invalid`，找不到指定服务器 `mcp.marketplace_not_found`。

::: tip 市场里的服务器不是 AI Switch 审核过的
市场只是把上游目录的内容列出来。装一个 MCP 服务器意味着让你的 AI CLI 能执行它提供的工具，等同于运行第三方代码。装之前看一眼它的主页和源码，尤其是需要填密钥的那些。
:::

## 生效时机

MCP 配置是**客户端启动时读取**的。AI Switch 写完文件之后，已经在运行的 CLI 进程不会自动感知。

所以正确的顺序是：先在 AI Switch 里配好，再启动（或重启）CLI。在 [Vibe 终端](/features/vibe) 里改完配置之后新开一个标签，就自然满足这个顺序。

## 下一步

- [技能管理](/features/skills)：另一套跨客户端的能力配置，思路类似
- [Vibe 终端与皮肤](/features/vibe)：改完配置立刻起一个新终端验证
- [平台支持矩阵](/guide/platform-support)：各平台的配置写入等能力对照
- [架构总览](/dev/architecture)：客户端适配层在代码里的位置
