# Claude Desktop 配置写入支持

日期：2026-09-03
状态：**已撤销**（2026-09-03 审计后移除实现，保留本文档说明原因）

## 撤销原因

这个方案不可实现，下面记录的设计和实现都已从代码中移除。两个层面的问题：

1. **实现写错了文件。** `JsonAgentAdapter::claude_desktop()` 与 `claude()` 逐字段相同，`config_dir` 同为 `.claude`，于是 `resolve_path` 得到 `~/.claude/settings.json`——那是 **Claude Code** 的配置文件。本文档下面「配置文件」一节写的"Claude Desktop 使用与 Claude Code 相同的配置文件路径"与开头「背景」一节写的"官方不共用配置"直接矛盾，代码跟着后者走了。
   叠加影响：`JsonAgentAdapter::native()` 硬编码 `true`，而写入对话框默认勾选所有 native 客户端，因此 Claude 平台默认把同一个文件写两遍（两条快照、两份备份、两个 target 状态行），并且只要 Claude Code 写过，Claude Desktop 就报"已接管"。

2. **改成正确路径也不会生效。** Claude Desktop 没有 bring-your-own base URL 的机制：它用登录账号鉴权，而它自己的配置文件（macOS `~/Library/Application Support/Claude/claude_desktop_config.json`、Windows `%APPDATA%\Claude\`）**只配置 MCP 服务器**，没有 API endpoint 或 auth token 的概念。所以无论适配器把路径改得多对，都不可能把 Claude Desktop 路由到本机代理。

与 Cursor BYOK 是同一类问题：GUI 客户端的 endpoint 不在本地文件里。**新增 GUI 客户端适配器之前，先确认该客户端真的从本地文件读 API base URL**——CLI 客户端通常会，桌面应用通常不会。

移除范围：`adapters/route_config/json_agent.rs` 的构造器、`adapters/route_config/mod.rs` 的注册、`database/repositories/target_repository.rs` 的种子行，以及三处把它和真 CLI 一起断言的测试。老库里残留的 `target_apps` 行是惰性的——客户端列表由注册表驱动（`target_service.rs::list_config_write_clients_for_home`），不读那张表。

---

以下为原始设计，仅作记录。

## 背景

Claude Desktop 和 Claude Code 官方不共用配置。用户希望能够为 Claude Desktop 写入算力池配置，就像为 Claude Code 写入一样。

## 实现

### 适配器

在 `JsonAgentAdapter` 中添加了 `claude_desktop()` 构造函数，配置如下：

- **client_key**: `claude_desktop`
- **target_key**: `claude_desktop`
- **display_name**: "Claude Desktop"
- **platform**: `PlatformId::Claude`
- **config_dir**: `.claude`（与 Claude Code 相同）
- **platform_base_url_keys**: `["ANTHROPIC_BASE_URL"]`
- **platform_auth_token_keys**: `["ANTHROPIC_AUTH_TOKEN"]`
- **writes_claude_model_env**: `true`
- **native**: `true`
- **restart_required**: `false`

### 配置文件

Claude Desktop 使用与 Claude Code 相同的配置文件路径：`~/.claude/settings.json`

这意味着：
- Claude Desktop 和 Claude Code **共享同一个配置文件**
- 写入配置时，两者的设置会互相影响
- 用户如果同时使用两者，只需选择其中一个写入即可

### 客户端列表

Claude 平台现在支持以下客户端（按顺序）：

1. **Claude Code** (claude_code) - 原生 CLI
2. **Claude Desktop** (claude_desktop) - 原生桌面应用
3. **ZCode** (zcode) - 第三方开发环境
4. **DeepSeek Harness** (deepseek_harness) - 第三方工具

### 测试覆盖

更新了以下测试以验证 Claude Desktop 适配器：

- `registry_contains_only_verified_native_config_adapters` - 验证注册表包含 Claude Desktop
- `native_cli_adapters_resolve_by_client_and_platform` - 验证按客户端和平台解析
- `clients_for_platform_lists_the_native_cli_first_then_zcode` - 验证客户端列表顺序

所有测试通过。

## 用户体验

用户在配置写入对话框中会看到：
- 平台名称：**Claude**
- 可选客户端：
  - ☑ Claude Code（原生 CLI）
  - ☑ Claude Desktop（原生桌面应用）
  - ☐ ZCode
  - ☐ DeepSeek Harness

默认勾选两个原生客户端（Claude Code 和 Claude Desktop），用户可以根据实际使用情况选择。

## 注意事项

由于 Claude Desktop 和 Claude Code 共享配置文件，用户：
- 可以同时勾选两者（写入一次配置，两个客户端都能用）
- 也可以只勾选其中一个（推荐）
- 写入后两个客户端都会使用相同的算力池配置
