# MCP 与 Skills 设置移植设计

## 背景

AI Switch 当前已经有统一的设置入口、Tauri/Web 双传输层和 SQLite 迁移基础，但尚未提供 MCP 或 Skills 管理功能。`codeg` 已经实现了本地 MCP 配置扫描、客户端分配、Official MCP Registry/Smithery 市场安装，以及按客户端管理全局和项目 Skills 的完整设置流程。

本项目将这些用户可见能力移植到 AI Switch，并按模块拆分，避免把 `codeg` 的大文件直接复制到现有 `commands` 或单个前端页面中。

## 目标

- 在左侧设置入口上方增加两个独立入口：`MCP` 和 `Skills`。
- 支持以下 11 个客户端：
  - Codex
  - Claude Code
  - Gemini CLI
  - Grok
  - OpenCode
  - OpenClaw
  - Hermes
  - Cline
  - Cursor
  - Kimi Code
  - CodeBuddy
- MCP 支持本地扫描、查看、创建、编辑、删除和客户端分配。
- MCP 支持 Official MCP Registry 和 Smithery 的搜索、详情查看、协议选择、动态参数填写和安装。
- Skills 支持全局目录和用户选择的项目目录。
- Skills 支持列表、搜索、预览、新建、编辑、保存和删除。
- 识别目录型 `SKILL.md` 和允许单文件布局的 `.md` Skill。
- 保护内置只读 Skill，后端禁止保存和删除。
- 同时支持 Tauri 桌面传输和 Web API 传输。
- 保持现有中英文界面和测试结构。

## 非目标

- 不将 MCP 或 Skill 文件内容迁移成数据库唯一数据源。
- 不在本次工作中增加 codeg 中 AI Switch 当前没有的智能体会话协议、ACP 子代理编排或 Skill 包市场。
- 不改变现有账号、模型路由、代理和配置快照功能。
- 不创建新的全局设置页；MCP 和 Skills 是左侧独立屏幕。

## 方案选择

采用“直接拆分移植 codeg 行为”的方案：

- 以客户端真实配置文件和 Skill 文件为权威来源。
- 复用 codeg 已验证的格式解析、路径规则、市场协议解析和客户端能力判断。
- 在 AI Switch 中拆成小型模型、服务、命令和客户端适配器模块。
- 保留现有 `mcp_servers`、`prompt_assets` 数据库迁移，但不把这些表作为运行时同步缓存。

该方案与“数据库中心化”相比不会产生配置漂移；与“混合缓存”相比不需要额外处理扫描结果与缓存一致性。

## 后端架构

### MCP 模块

新增目录：

```text
src-tauri/src/mcp/
  mod.rs
  model.rs
  command.rs
  service.rs
  marketplace.rs
  clients/
    mod.rs
    claude_code.rs
    codex.rs
    gemini.rs
    openclaw.rs
    opencode.rs
    hermes.rs
    cline.rs
    cursor.rs
    kimi_code.rs
    code_buddy.rs
```

职责边界：

- `model.rs`：MCP app 类型、本地服务器、市场提供商/条目/安装选项等序列化模型。
- `clients/`：每个客户端一个配置适配器，负责默认路径、读取、规范化、写入和删除。
- `service.rs`：跨客户端扫描、按 server id 合并、客户端能力预检和统一写入流程。
- `marketplace.rs`：Official Registry 与 Smithery 的 HTTP 请求、响应解析、协议选项和参数模板解析。
- `command.rs`：Tauri command 薄层，只负责参数接收和调用服务。
- `mod.rs`：模块导出和公共类型重导出。

### Skills 模块

新增目录：

```text
src-tauri/src/skills/
  mod.rs
  model.rs
  command.rs
  service.rs
  frontmatter.rs
  paths.rs
  clients/
    mod.rs
    claude_code.rs
    codex.rs
    gemini.rs
    openclaw.rs
    opencode.rs
    hermes.rs
    cline.rs
    cursor.rs
    kimi_code.rs
    code_buddy.rs
```

职责边界：

- `model.rs`：Skill 客户端、作用域、布局、位置、列表项和内容结果。
- `paths.rs`：每个客户端的全局目录、项目相对目录、路径规范化和安全校验。
- `clients/`：客户端支持的 Skill 布局和目录规则。
- `frontmatter.rs`：读取和解析 YAML front matter，保存正文时保留完整 Markdown。
- `service.rs`：列表、读取、保存、删除、只读判断和重新扫描。
- `command.rs`：Tauri command 薄层。

MCP 与 Skills 共享客户端名称展示，但各自保留配置适配器，因为同一客户端的 MCP 配置路径和 Skill 路径并不相同。

## MCP 命令接口

```text
mcp_scan_local() -> LocalMcpServer[]
mcp_list_marketplaces() -> MarketplaceProvider[]
mcp_search_marketplace(provider_id, query, limit) -> MarketplaceItem[]
mcp_get_marketplace_server_detail(provider_id, server_id) -> MarketplaceServerDetail
mcp_install_from_marketplace(provider_id, server_id, apps, option_id, protocol, parameter_values)
  -> LocalMcpServer
mcp_upsert_local_server(server_id, spec, apps) -> LocalMcpServer
mcp_set_server_apps(server_id, apps) -> LocalMcpServer?
mcp_remove_server(server_id, apps?) -> bool
```

行为约束：

- `spec` 必须是对象，transport 只能规范化为 `stdio`、`sse` 或 `http`/streamable HTTP。
- 所有客户端先去重并按稳定顺序处理。
- Codex 不支持 SSE；被选中的客户端如果无法表达该 transport 会被排除。
- 如果所有选中客户端都不兼容，操作失败且不写入任何文件。
- 更新客户端分配时删除不再选择的客户端条目，保留仍选择的客户端条目。
- 操作完成后重新扫描并返回实际配置结果。

## Skills 命令接口

```text
skills_list_agents() -> SkillAgentInfo[]
skills_list(agent_type, scope, workspace_path?) -> SkillsListResult
skills_read(agent_type, scope, skill_id, workspace_path?) -> SkillContent
skills_save(agent_type, scope, skill_id, content, layout?, workspace_path?) -> SkillItem
skills_delete(agent_type, scope, skill_id, workspace_path?) -> bool
```

行为约束：

- `scope` 为 `global` 或 `project`。
- `project` 必须传已存在的绝对目录。
- Skill ID 只能包含安全文件名字符，拒绝 `..`、路径分隔符和控制字符。
- 客户端只支持目录型 Skill 时，保存路径固定为 `<dir>/<id>/SKILL.md`。
- 客户端允许单文件布局时，可使用 `<dir>/<id>.md`；现有 Skill 的布局优先于默认布局。
- 内置目录中的 Skill 标记为只读，后端拒绝保存和删除。
- 保存或删除后重新列出目录，返回磁盘上的最终状态。

## 数据流与传输层接入

### MCP

1. 页面加载时并行调用本地扫描和市场列表。
2. 本地扫描逐客户端读取配置，按 server id 合并并返回实际客户端列表。
3. 编辑、新建或安装时，后端解析 JSON、规范化 transport，并执行客户端适配器写入。
4. 市场安装先获取详情和动态参数，再生成规范化 spec，复用本地写入流程。
5. 写入完成后重新扫描，前端以返回值更新列表和详情。

### Skills

1. 页面选择客户端和作用域。
2. 后端计算全局目录，或校验用户选择的项目目录。
3. 扫描目录并识别目录型 `SKILL.md` 与单文件 `.md`。
4. 预览/编辑读取原文件；保存保留正文和 front matter。
5. 保存或删除后重新扫描，防止前端状态与磁盘不一致。

命令同时接入：

- `src-tauri/src/lib.rs` 的 Tauri `generate_handler!`。
- `src-tauri/src/web/handlers/mod.rs` 的 Web command dispatch。
- `src/lib/api/client.ts` 的类型化调用。
- `src/lib/api/types.ts` 的共享前后端数据类型。

以下命令加入 Web 敏感命令门禁：

```text
mcp_upsert_local_server
mcp_install_from_marketplace
mcp_set_server_apps
mcp_remove_server
skills_save
skills_delete
```

列表、读取和市场查询命令保持只读可用。

## 前端架构

新增文件结构：

```text
src/
  screens/
    McpScreen.tsx
    SkillsScreen.tsx
  components/
    mcp/
      McpLocalPanel.tsx
      McpMarketplacePanel.tsx
      McpServerEditor.tsx
      McpInstallDialog.tsx
      McpAppSelector.tsx
    skills/
      SkillsToolbar.tsx
      SkillsList.tsx
      SkillEditor.tsx
      SkillPreview.tsx
      SkillScopePicker.tsx
```

### 左侧入口

`AppLayout` 的系统区调整为：

```text
MCP
Skills
Settings
```

新增 `Mcp` 和 `Skills` 屏幕路由，并让它们与 Settings、Sessions、Updates、Log 一样高亮设置区域。按钮使用现有 `lucide-react` 图标，保留折叠侧栏、键盘焦点和移动端布局。

### MCP 页面

- 本地/市场两个标签。
- 本地列表支持名称和协议过滤。
- 详情面板显示 server id、协议摘要、完整 JSON 和目标客户端。
- 新建使用独立草稿状态。
- 市场项显示描述、主页、验证状态、协议选项和动态安装参数。
- 删除、覆盖和安装使用确认/参数对话框。
- 异步操作使用局部 loading，不阻塞另一侧列表。

### Skills 页面

- 工具栏包含客户端选择、`Global`/`Project` 切换、项目目录选择和刷新。
- 左栏显示 Skill 列表、搜索、只读标记和布局标记。
- 中栏编辑 Markdown，支持新建、保存和删除。
- 右栏显示 Markdown 预览和 front matter 摘要。
- 项目作用域未选择目录时显示目录选择空状态。
- 切换客户端或作用域时清空旧选项并重新读取。
- 只读 Skill 禁用保存和删除。
- 删除当前项后自动选择下一个可用项。

### 国际化

在现有 `src/lib/i18n.tsx` 的英文和简体中文字典中增加导航、协议、市场、安装参数、Skill 作用域、布局、只读状态、loading、空状态、确认和错误文案。

## 错误处理

统一转换为现有 `AppError`/`ApiError`，使用稳定错误码：

```text
mcp.invalid_spec
mcp.unsupported_transport
mcp.config_invalid
mcp.marketplace_invalid
mcp.marketplace_network
mcp.server_not_found
mcp.no_compatible_client

skills.invalid_id
skills.path_invalid
skills.directory_missing
skills.config_invalid
skills.read_only
skills.not_found
skills.unsupported_client
```

前端按错误码显示中英文文案；`details` 仅供调试，不包含环境变量值、API key 或 token。

## 文件安全

- MCP JSON/TOML/YAML 在写入前完整解析和规范化。
- 文件写入使用同目录临时文件加替换，避免中断留下半个配置文件。
- 写入失败不删除旧文件。
- 项目目录必须存在，最终 Skill 路径必须位于项目目录或客户端明确的全局目录下。
- 只读路径由后端再次判断，不能只依赖前端按钮禁用。
- 市场请求设置有限连接和总超时，非 2xx 或无效 JSON 返回结构化错误。
- 日志和错误中不输出敏感参数。

## 测试与验收

### Rust

- 11 个客户端 MCP 适配器的读取、写入、删除和客户端分配。
- JSON、TOML、YAML 的解析、序列化和无文件初始化。
- Codex SSE 等不兼容 transport 的拒绝逻辑。
- MCP 规范化、市场协议选项和安装参数解析。
- Skill 全局/项目路径解析、ID 校验和 front matter 解析。
- 只读 Skill 保存/删除拒绝。
- Web command dispatch 和敏感命令门禁。

### TypeScript

- 左侧 MCP/Skills/Settings 顺序和导航。
- MCP 本地扫描、编辑、新建、删除。
- 市场搜索、协议选择、参数校验和安装。
- Skills 全局/项目切换、目录选择、预览、保存和删除。
- 只读、错误、loading、空状态和双语文案。

### 本地检查

```text
pnpm typecheck
pnpm test:run
pnpm rust:check
pnpm rust:test
```

## 第三方归属

`codeg` 采用 Apache-2.0。移植或改写其实现时：

- 在新增的衍生后端文件中保留 Apache-2.0 许可证头和“基于 xintaofei/codeg，已由 AI Switch 修改”的声明。
- 新增 Apache-2.0 许可证文本和来源说明文件。
- 不移除 codeg 的版权、专利和许可证通知。
- AI Switch 自有代码继续遵循仓库现有 MIT 许可；第三方衍生文件同时受 Apache-2.0 条款约束。

## 成功标准

1. 左侧能分别进入 MCP 和 Skills 页面，Settings 入口仍然可用。
2. 在未安装配置的机器上打开页面不会报错，能创建首个 MCP 或 Skill。
3. 本地 MCP 修改能在对应 11 个客户端的真实配置文件中生效，并在重新扫描后保持分配状态。
4. Official Registry 和 Smithery 的市场搜索、详情和参数化安装可用，网络失败有明确错误。
5. Skills 可以管理全局目录和用户选择的项目目录，内置只读项不可修改。
6. Tauri 和 Web 两种传输都能调用同一套命令，Web 写操作受敏感命令门禁保护。
7. 既有工作区改动不被覆盖，现有测试和发布检查保持通过。
