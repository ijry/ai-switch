# MCP 设置移植实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**目标：** 将 codeg 的 MCP 本地配置管理和 Official MCP Registry/Smithery 市场能力移植到 AI Switch，并在左侧设置入口上方提供独立 MCP 页面。

**架构：** MCP 以客户端真实配置文件为权威来源。后端拆分为规范化、文件 IO、11 个客户端适配器、跨客户端服务、市场服务和 Tauri command；前端拆分为本地面板、市场面板、编辑器、安装对话框和客户端选择器。所有写操作同时通过 Tauri 和受门禁保护的 Web command 暴露。

**技术栈：** Rust 2021、Tauri 2、`serde_json`、`toml`、`serde_yaml`、`reqwest`、React 18、TypeScript、TanStack Query、`lucide-react`、现有 transport 和 Vitest。

## 全局约束

- 支持 Codex、Claude Code、Gemini CLI、Grok、OpenCode、OpenClaw、Hermes、Cline、Cursor、Kimi Code、CodeBuddy 共 11 个客户端。
- MCP 配置文件是权威来源，不使用 `mcp_servers` 表作为运行时缓存或唯一数据源。
- 左侧系统区顺序必须为 `MCP`、`Skills`、`Settings`；本计划只负责加入 `MCP`，Skills 入口由 Skills 计划加入。
- Codex 不支持 SSE；所有客户端写入前必须执行 transport 能力预检。
- Web 写命令 `mcp_upsert_local_server`、`mcp_install_from_marketplace`、`mcp_set_server_apps`、`mcp_remove_server` 必须受敏感命令门禁保护。
- 写配置时不得记录环境变量值、API key 或 token；配置写入失败不得删除旧文件。
- 从 `codeg` 移植或改写的后端文件保留 Apache-2.0 来源和修改声明；新增 Apache-2.0 许可证文本。
- 不覆盖工作区中已有未提交改动；每次提交只包含当前任务文件。

---

### 任务 1：建立 MCP 类型、适配器接口与第三方归属

**文件：**
- 创建：`src-tauri/src/mcp/mod.rs`
- 创建：`src-tauri/src/mcp/model.rs`
- 创建：`src-tauri/src/mcp/clients/mod.rs`
- 创建：`LICENSES/Apache-2.0.txt`
- 创建：`THIRD_PARTY_NOTICES.md`
- 修改：`src-tauri/Cargo.toml`
- 修改：`src-tauri/src/lib.rs`
- 修改：`src-tauri/src/commands/mod.rs`
- 测试：`src-tauri/src/mcp/model.rs` 内单元测试

**接口：**
- 产出 `McpAppType`、`LocalMcpServer`、`McpMarketplaceProvider`、`McpMarketplaceItem`、`McpMarketplaceInstallParameter`、`McpMarketplaceInstallOption`、`McpMarketplaceServerDetail`。
- 产出 `McpClientAdapter` trait，后续客户端文件必须实现同一接口：

```rust
pub trait McpClientAdapter: Send + Sync {
    fn app(&self) -> McpAppType;
    fn read_servers(&self) -> Result<BTreeMap<String, Value>, AppError>;
    fn upsert_server(&self, id: &str, spec: &Value) -> Result<(), AppError>;
    fn remove_server(&self, id: &str) -> Result<bool, AppError>;
}
```

- `McpAppType` 使用 `#[serde(rename_all = "snake_case")]`，枚举值包含 `claude_code`、`codex`、`gemini`、`open_claw`、`open_code`、`hermes`、`cline`、`cursor`、`kimi_code`、`code_buddy`、`grok`。
- `LocalMcpServer` 至少包含 `id: String`、`spec: serde_json::Value`、`apps: Vec<McpAppType>`。

- [ ] **步骤 1：写失败的模型序列化测试**

```rust
#[test]
fn app_type_serializes_in_snake_case() {
    assert_eq!(serde_json::to_string(&McpAppType::ClaudeCode).unwrap(), "\"claude_code\"");
    assert_eq!(serde_json::to_string(&McpAppType::CodeBuddy).unwrap(), "\"code_buddy\"");
}
```

- [ ] **步骤 2：运行测试确认缺少类型**

运行：`cd src-tauri && cargo test mcp::model::tests::app_type_serializes_in_snake_case`

预期：FAIL，提示 `mcp` 模块或类型尚未定义。

- [ ] **步骤 3：实现模型和模块导出**

在 `src-tauri/src/lib.rs` 增加 `mod mcp;`，在 `commands/mod.rs` 暂时只声明后续 `mcp` command 模块；把 codeg 来源和修改声明写入新增后端文件头部。向 Cargo 增加：

```toml
toml = "0.8"
serde_yaml = "0.9"
```

复制 Apache-2.0 全文到 `LICENSES/Apache-2.0.txt`，在 `THIRD_PARTY_NOTICES.md` 标明来源仓库 `https://github.com/xintaofei/codeg`、移植范围和 AI Switch 修改说明。

- [ ] **步骤 4：运行测试确认通过**

运行：`cd src-tauri && cargo test mcp::model::tests`

预期：PASS。

- [ ] **步骤 5：提交**

```bash
git add src-tauri/Cargo.toml src-tauri/src/lib.rs src-tauri/src/commands/mod.rs src-tauri/src/mcp LICENSES/Apache-2.0.txt THIRD_PARTY_NOTICES.md
git commit -m "feat: scaffold MCP configuration module"
```

### 任务 2：实现 MCP 规范化、通用文件 IO 和 transport 能力校验

**文件：**
- 创建：`src-tauri/src/mcp/clients/common.rs`
- 创建：`src-tauri/src/mcp/normalize.rs`
- 修改：`src-tauri/src/mcp/clients/mod.rs`
- 测试：`src-tauri/src/mcp/normalize.rs` 内单元测试

**接口：**
- `read_json_file(path: &Path) -> Result<Value, AppError>`：不存在返回空对象，存在但 JSON 根不是对象时返回 `mcp.config_invalid`。
- `write_json_file(path: &Path, value: &Value) -> Result<(), AppError>`：创建父目录，写入同目录临时文件后替换目标文件。
- `read_toml_file(path: &Path) -> Result<toml::Value, AppError>`、`write_toml_file(...)`。
- `read_yaml_file(path: &Path) -> Result<serde_yaml::Value, AppError>`、`write_yaml_file(...)`。
- `canonicalize_spec(spec: &Value, source: &str) -> Result<Value, AppError>`：统一输出 `type`、`command`、`args`、`env`、`cwd`、`url`、`headers` 等可跨客户端字段。
- `app_can_host_spec(app: McpAppType, spec: &Value) -> bool`：Codex 与 SSE 返回 false，其余已确认客户端返回 true。

- [ ] **步骤 1：写规范化失败测试**

```rust
#[test]
fn canonicalizes_stdio_from_command_shape() {
    let spec = canonicalize_spec(
        &serde_json::json!({"command":" npx ","args":[" -y ","server"]}),
        "test",
    ).unwrap();
    assert_eq!(spec["type"], "stdio");
    assert_eq!(spec["command"], "npx");
    assert_eq!(spec["args"], serde_json::json!(["-y", "server"]));
}

#[test]
fn rejects_codex_sse() {
    assert!(!app_can_host_spec(McpAppType::Codex, &serde_json::json!({"type":"sse","url":"https://example.test"})));
}
```

- [ ] **步骤 2：运行测试确认失败**

运行：`cd src-tauri && cargo test mcp::normalize::tests`

预期：FAIL，提示规范化函数未实现。

- [ ] **步骤 3：实现规范化和原子写入**

实现 `stdio`、`http`、`sse` 三类解析；对数组、对象、字符串字段做类型校验；写入时使用同目录唯一临时文件、`sync_all` 和替换。错误转换为 `AppError::Validation { code: "mcp.invalid_spec", ... }`、`AppError::Filesystem { code: "mcp.config_io", ... }` 或 `mcp.config_invalid`。

- [ ] **步骤 4：运行测试确认通过**

运行：`cd src-tauri && cargo test mcp::normalize::tests`

预期：PASS，并增加一个临时目录测试验证写入失败时旧文件仍存在。

- [ ] **步骤 5：提交**

```bash
git add src-tauri/src/mcp/clients/common.rs src-tauri/src/mcp/normalize.rs src-tauri/src/mcp/clients/mod.rs
git commit -m "feat: add MCP spec normalization and safe file IO"
```

### 任务 3：实现 JSON MCP 客户端适配器

**文件：**
- 创建：`src-tauri/src/mcp/clients/claude_code.rs`
- 创建：`src-tauri/src/mcp/clients/code_buddy.rs`
- 创建：`src-tauri/src/mcp/clients/gemini.rs`
- 创建：`src-tauri/src/mcp/clients/openclaw.rs`
- 创建：`src-tauri/src/mcp/clients/opencode.rs`
- 创建：`src-tauri/src/mcp/clients/cline.rs`
- 创建：`src-tauri/src/mcp/clients/cursor.rs`
- 创建：`src-tauri/src/mcp/clients/kimi_code.rs`
- 修改：`src-tauri/src/mcp/clients/mod.rs`
- 测试：`src-tauri/src/mcp/clients/json_tests.rs`

**接口：** 每个适配器实现任务 1 的 `McpClientAdapter`，并额外暴露 `config_path()` 供测试注入临时路径。

**配置路径和格式：**

| 客户端 | 默认路径 | MCP 节点/规则 |
| --- | --- | --- |
| Claude Code | `~/.claude.json` | 顶层 `mcpServers`；同时在 `~/.claude/settings.json.enabledPlugins` 维护 `<id>@local: true` |
| CodeBuddy | `~/.codebuddy.json` | 顶层 `mcpServers`；同时在 `~/.codebuddy/settings.json.enabledPlugins` 维护 `<id>@local: true` |
| Gemini CLI | `~/.gemini/settings.json` | 顶层 `mcpServers` |
| OpenClaw | `~/.openclaw/openclaw.json` | 使用客户端实际 `mcpServers` 节点，保留其他字段 |
| OpenCode | `~/.config/opencode/opencode.json` | 优先使用 `mcpServers`，旧格式使用 `mcp` 并转换 transport |
| Cline | `~/.cline/data/settings/cline_mcp_settings.json` | 顶层 `mcpServers` |
| Cursor | `~/.cursor/mcp.json` | 顶层 `mcpServers`，写入只保留 shape-discriminated 字段，不写 `type` |
| Kimi Code | `$KIMI_CODE_HOME/mcp.json` 或 `~/.kimi-code/mcp.json` | 顶层 `mcpServers`，远程条目用 `transport: http/sse` |

- [ ] **步骤 1：写 JSON 适配器测试**

```rust
#[test]
fn cursor_round_trip_drops_type_but_keeps_remote_fields() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mcp.json");
    write_json_file(&path, &serde_json::json!({
        "mcpServers": {"browser": {"type":"http","url":"https://example.test","headers":{"X-Key":"value"}}}
    })).unwrap();
    let servers = CursorAdapter::read_servers_at(&path).unwrap();
    assert_eq!(servers["browser"]["type"], "http");
    CursorAdapter::upsert_server_at(&path, "browser", &servers["browser"]).unwrap();
    let raw = read_json_file(&path).unwrap();
    assert!(raw["mcpServers"]["browser"].get("type").is_none());
}
```

- [ ] **步骤 2：运行测试确认失败**

运行：`cd src-tauri && cargo test mcp::clients::json_tests`

预期：FAIL，提示适配器或注入路径函数未定义。

- [ ] **步骤 3：实现适配器**

每个适配器只修改所属 MCP 节点，保留同文件其他配置。Claude Code 和 CodeBuddy 额外同步 `enabledPlugins`；OpenCode 支持 `mcpServers` 与旧 `mcp`；Cursor 依据 `command`/`url` 形状推断 transport；Kimi Code 依据 `transport` 字段处理远程协议。任何现有无关字段不得被静默清空。

- [ ] **步骤 4：运行测试确认通过**

运行：`cd src-tauri && cargo test mcp::clients::json_tests`

预期：PASS，覆盖每个 JSON 客户端的空文件初始化、读写、删除和保留无关字段。

- [ ] **步骤 5：提交**

```bash
git add src-tauri/src/mcp/clients
git commit -m "feat: support JSON MCP client configurations"
```

### 任务 4：实现 Codex、Grok 和 Hermes MCP 适配器

**文件：**
- 创建：`src-tauri/src/mcp/clients/codex.rs`
- 创建：`src-tauri/src/mcp/clients/grok.rs`
- 创建：`src-tauri/src/mcp/clients/hermes.rs`
- 修改：`src-tauri/src/mcp/clients/mod.rs`
- 测试：`src-tauri/src/mcp/clients/toml_yaml_tests.rs`

**接口和格式：**
- Codex：`$CODEX_HOME/config.toml` 或 `~/.codex/config.toml`，读取新格式 `[mcp_servers.<id>]`，兼容旧 `[mcp.servers.<id>]`；写入新格式并清理同 id 旧格式；SSE 直接返回 `mcp.unsupported_transport`。
- Grok：`$GROK_HOME/config.toml` 或 `~/.grok/config.toml`，使用 `[mcp_servers.<id>]`；无 `type` 时从 `command`/`url` 推断，SSE 写显式 `type = "sse"`。
- Hermes：`~/.hermes/config.yaml` 的 `mcp_servers` 节点；stdio 写 `command/args/env`，远程写 `url` 和 SSE 的 `transport: sse`，保留 mTLS 与 enabled 等已存在字段。

- [ ] **步骤 1：写 TOML/YAML 适配器测试**

```rust
#[test]
fn codex_reads_legacy_servers_without_losing_new_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        "[mcp.servers.legacy]\ncommand = \"legacy\"\n[mcp_servers.current]\ncommand = \"current\"\n",
    )
    .unwrap();
    let servers = CodexAdapter::read_servers_at(&path).unwrap();
    assert_eq!(servers["legacy"]["command"], "legacy");
    assert_eq!(servers["current"]["command"], "current");
}

#[test]
fn hermes_maps_sse_transport_field() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(
        &path,
        "mcp_servers:\n  events:\n    url: https://example.test/events\n    transport: sse\n",
    )
    .unwrap();
    let servers = HermesAdapter::read_servers_at(&path).unwrap();
    assert_eq!(servers["events"]["type"], "sse");
}
```

- [ ] **步骤 2：运行测试确认失败**

运行：`cd src-tauri && cargo test mcp::clients::toml_yaml_tests`

预期：FAIL，提示适配器未实现。

- [ ] **步骤 3：实现转换和安全写入**

使用 `toml` 和 `serde_yaml` 解析整个文档，修改目标节点后写回；Hermes 写入前保留文件权限语义，并拒绝空配置覆盖读取错误。测试辅助函数必须接受 `&Path`，生产路径函数只负责解析 home 和环境变量。

- [ ] **步骤 4：运行测试确认通过**

运行：`cd src-tauri && cargo test mcp::clients::toml_yaml_tests`

预期：PASS，覆盖格式转换、无关顶层字段保留和不兼容 transport。

- [ ] **步骤 5：提交**

```bash
git add src-tauri/src/mcp/clients/codex.rs src-tauri/src/mcp/clients/grok.rs src-tauri/src/mcp/clients/hermes.rs src-tauri/src/mcp/clients/toml_yaml_tests.rs
git commit -m "feat: support Codex Grok and Hermes MCP configs"
```

### 任务 5：实现跨客户端 MCP 服务

**文件：**
- 创建：`src-tauri/src/mcp/service.rs`
- 修改：`src-tauri/src/mcp/mod.rs`
- 测试：`src-tauri/src/mcp/service.rs` 内单元测试

**接口：**

```rust
pub fn scan_local() -> Result<Vec<LocalMcpServer>, AppError>;
pub fn upsert_local_server(id: String, spec: Value, apps: Vec<McpAppType>) -> Result<LocalMcpServer, AppError>;
pub fn set_server_apps(id: String, apps: Vec<McpAppType>) -> Result<Option<LocalMcpServer>, AppError>;
pub fn remove_server(id: String, apps: Option<Vec<McpAppType>>) -> Result<bool, AppError>;
```

- [ ] **步骤 1：写合并和预检测试**

```rust
#[test]
fn scan_groups_same_id_and_sorts_apps() {
    let adapters = vec![
        mock_adapter(McpAppType::Codex, [("demo", json!({"type":"stdio","command":"a"}))]),
        mock_adapter(McpAppType::Gemini, [("demo", json!({"type":"stdio","command":"a"}))]),
    ];
    let result = scan_with_adapters(&adapters).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, "demo");
    assert_eq!(result[0].apps, vec![McpAppType::Codex, McpAppType::Gemini]);
}

#[test]
fn upsert_rejects_when_all_selected_apps_cannot_host_transport() {
    let error = preflight_apps(
        &[McpAppType::Codex],
        &json!({"type":"sse","url":"https://example.test/events"}),
    )
    .unwrap_err();
    assert_eq!(error_code(&error), "mcp.no_compatible_client");
}
```

- [ ] **步骤 2：运行测试确认失败**

运行：`cd src-tauri && cargo test mcp::service::tests`

预期：FAIL，提示 service 函数未定义。

- [ ] **步骤 3：实现跨客户端流程**

扫描每个 adapter，按 id 合并 canonical spec 和 app 集合，忽略单个客户端的无效条目但记录可诊断 warning。为测试提供 `scan_with_adapters(&[Box<dyn McpClientAdapter>])` 和 `preflight_apps`，生产实现通过同一组 adapter 工厂调用它们。写入时先规范化 spec，再过滤不兼容客户端；目标集合为空时在任何写操作前返回错误；删除旧目标、写入新目标后重新扫描并返回。

- [ ] **步骤 4：运行测试确认通过**

运行：`cd src-tauri && cargo test mcp::service::tests`

预期：PASS。

- [ ] **步骤 5：提交**

```bash
git add src-tauri/src/mcp/service.rs src-tauri/src/mcp/mod.rs
git commit -m "feat: add cross-client MCP service"
```

### 任务 6：实现 Official Registry 和 Smithery 市场服务

**文件：**
- 创建：`src-tauri/src/mcp/marketplace.rs`
- 修改：`src-tauri/src/mcp/model.rs`
- 修改：`src-tauri/Cargo.toml`
- 测试：`src-tauri/src/mcp/marketplace.rs` 内 fixture 测试

**接口：**

```rust
pub async fn list_marketplaces() -> Vec<McpMarketplaceProvider>;
pub async fn search(provider_id: String, query: Option<String>, limit: Option<u32>) -> Result<Vec<McpMarketplaceItem>, AppError>;
pub async fn get_detail(provider_id: String, server_id: String) -> Result<McpMarketplaceServerDetail, AppError>;
pub async fn install(provider_id: String, server_id: String, apps: Vec<McpAppType>, option_id: Option<String>, protocol: Option<String>, parameter_values: Option<Value>) -> Result<LocalMcpServer, AppError>;
```

- [ ] **步骤 1：写协议和参数 fixture 测试**

```rust
#[test]
fn official_parameter_kind_maps_json_schema_types() {
    assert_eq!(infer_parameter_kind(Some("boolean")), "boolean");
    assert_eq!(infer_parameter_kind(Some("object")), "json");
    assert_eq!(infer_parameter_kind(None), "string");
}
```

- [ ] **步骤 2：运行测试确认失败**

运行：`cd src-tauri && cargo test mcp::marketplace::tests`

预期：FAIL，提示市场解析函数未定义。

- [ ] **步骤 3：实现 HTTP 客户端和安装解析**

使用 `reqwest::Client`，连接超时 8 秒、总超时 20 秒、User-Agent `ai-switch-mcp-market/1.0`。Official Registry 请求 `https://registry.modelcontextprotocol.io/v0.1/servers?limit=<n>&version=latest&search=<q>`，详情请求 `/servers/<encoded>/versions/latest`；Smithery 按 codeg 的 registry/detail 端点和响应结构实现。将远程、包、协议和变量参数统一映射为 `McpMarketplaceInstallOption`，校验 required、boolean、number、integer、json 和枚举值。市场安装解析出 canonical spec 后调用任务 5 的 `upsert_local_server`，不重复实现写文件逻辑。

- [ ] **步骤 4：运行测试确认通过**

运行：`cd src-tauri && cargo test mcp::marketplace::tests`

预期：PASS；真实网络测试默认不执行，至少覆盖成功 payload、缺失 servers 数组、HTTP 错误和缺少必填参数。

- [ ] **步骤 5：提交**

```bash
git add src-tauri/src/mcp/marketplace.rs src-tauri/src/mcp/model.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat: add MCP marketplace services"
```

### 任务 7：接入 Tauri/Web command、前端 API 类型和敏感命令门禁

**文件：**
- 创建：`src-tauri/src/mcp/command.rs`
- 修改：`src-tauri/src/mcp/mod.rs`
- 修改：`src-tauri/src/lib.rs`
- 修改：`src-tauri/src/web/handlers/mod.rs`
- 修改：`src/lib/api/client.ts`
- 修改：`src/lib/api/types.ts`
- 修改：`src/lib/api/commandSupport.ts`
- 修改：`tests/transport/command-contract.test.ts`
- 测试：`src-tauri/src/web/handlers/mod.rs` 现有 dispatch 测试

**接口：** Tauri command 名称必须与下列字符串完全一致：

```text
mcp_scan_local
mcp_list_marketplaces
mcp_search_marketplace
mcp_get_marketplace_server_detail
mcp_install_from_marketplace
mcp_upsert_local_server
mcp_set_server_apps
mcp_remove_server
```

- [ ] **步骤 1：先扩展前端类型和契约测试**

在 `src/lib/api/types.ts` 增加与 Rust 模型对应的 `McpAppType`、`LocalMcpServer`、市场类型；在 `client.ts` 增加：

```ts
export function mcpScanLocal(): Promise<LocalMcpServer[]> {
  return invoke("mcp_scan_local");
}

export function mcpUpsertLocalServer(input: {
  server_id: string;
  spec: Record<string, unknown>;
  apps: McpAppType[];
}): Promise<LocalMcpServer> {
  return invoke("mcp_upsert_local_server", input);
}
```

为 8 个命令增加 command contract 断言，运行 `pnpm test:run -- tests/transport/command-contract.test.ts`，预期先因 Rust/Web 注册缺失而 FAIL。

- [ ] **步骤 2：实现 Tauri command 薄层**

`command.rs` 的每个函数只调用 `mcp::service` 或 `mcp::marketplace`，不在 command 层读取文件。将函数加入 `generate_handler!`。

- [ ] **步骤 3：实现 Web dispatch 和敏感门禁**

在 `dispatch_command` 中解析 snake_case 参数并调用同一服务；在 `is_sensitive_command` 增加四个 MCP 写命令。列表、详情和扫描命令必须保留 Web 可用性。

- [ ] **步骤 4：运行契约与 Rust 测试**

运行：`pnpm test:run -- tests/transport/command-contract.test.ts` 和 `cd src-tauri && cargo test web::handlers::tests`。

预期：PASS，客户端命令全部出现在 Tauri handler 和非 desktop-only Web dispatch 中，写命令在关闭敏感门禁时返回 404。

- [ ] **步骤 5：提交**

```bash
git add src-tauri/src/mcp/command.rs src-tauri/src/mcp/mod.rs src-tauri/src/lib.rs src-tauri/src/web/handlers/mod.rs src/lib/api/client.ts src/lib/api/types.ts src/lib/api/commandSupport.ts tests/transport/command-contract.test.ts
git commit -m "feat: expose MCP commands over app transports"
```

### 任务 8：接入 MCP 左侧入口、路由和国际化

**文件：**
- 创建：`src/screens/McpScreen.tsx`
- 修改：`src/components/layout/AppLayout.tsx`
- 修改：`src/App.tsx`
- 修改：`src/lib/i18n.tsx`
- 修改：`tests/AppLayout.test.tsx`
- 修改：`tests/SettingsScreen.test.tsx`

**接口：**
- `McpScreen` 无必需 props，使用任务 7 的 API 函数。
- `AppLayout` 的 `settingsFeatureScreens` 增加 `Mcp`，系统区在 `Settings` 之前渲染 MCP 按钮。
- `App` 将 `Mcp` 加入 `implementedScreens`，并在 `screen === "Mcp"` 时渲染 `McpScreen`。

- [ ] **步骤 1：先写导航失败测试**

```tsx
it("renders MCP immediately above Settings", () => {
  render(<AppLayout {...defaultProps} activeScreen="Mcp" />);
  const buttons = screen.getAllByRole("button");
  expect(buttons.findIndex((item) => item.textContent?.includes("MCP")))
    .toBeLessThan(buttons.findIndex((item) => item.textContent?.includes("Settings")));
});
```

运行：`pnpm test:run -- tests/AppLayout.test.tsx`，预期 FAIL。

- [ ] **步骤 2：实现导航、路由和翻译键**

使用 `lucide-react` 的 `Boxes` 图标，不新增 emoji；加入 `nav.mcp`、MCP 页面公共 loading/error/empty 文案的英文和简体中文翻译。更新现有 `layout.settingsHint`，不再宣称 MCP 只存在 Settings 内。

- [ ] **步骤 3：运行导航测试**

运行：`pnpm test:run -- tests/AppLayout.test.tsx tests/SettingsScreen.test.tsx`。

预期：PASS，Settings 页面原有功能不变，MCP 入口可导航。

- [ ] **步骤 4：提交**

```bash
git add src/screens/McpScreen.tsx src/components/layout/AppLayout.tsx src/App.tsx src/lib/i18n.tsx tests/AppLayout.test.tsx tests/SettingsScreen.test.tsx
git commit -m "feat: add MCP settings navigation entry"
```

### 任务 9：实现 MCP 本地管理界面

**文件：**
- 创建：`src/components/mcp/McpLocalPanel.tsx`
- 创建：`src/components/mcp/McpServerEditor.tsx`
- 创建：`src/components/mcp/McpAppSelector.tsx`
- 修改：`src/screens/McpScreen.tsx`
- 测试：`tests/McpScreen.test.tsx`

**接口：**
- `McpLocalPanel` 管理本地列表、搜索、选择和刷新回调。
- `McpServerEditor` 接收 `LocalMcpServer | null`、草稿 JSON 和保存/删除回调。
- `McpAppSelector` 接收 `McpAppType[]`，返回去重后的目标客户端数组。

- [ ] **步骤 1：写本地交互失败测试**

```tsx
it("loads local servers and saves edited JSON with selected apps", async () => {
  vi.mocked(mcpScanLocal).mockResolvedValue([
    { id: "filesystem", spec: { type: "stdio", command: "npx" }, apps: ["codex"] },
  ]);
  render(<McpScreen />);
  expect(await screen.findByText("filesystem")).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: /Save|保存/ }));
  expect(mcpUpsertLocalServer).toHaveBeenCalledWith(expect.objectContaining({ server_id: "filesystem" }));
});
```

运行：`pnpm test:run -- tests/McpScreen.test.tsx`，预期 FAIL。

- [ ] **步骤 2：实现本地列表和编辑器**

页面加载调用 `mcpScanLocal`；列表支持 id、协议和命令摘要搜索；新建使用默认 stdio 草稿；编辑器使用稳定高度的 textarea，解析 JSON 前显示明确校验错误；应用选择器展示全部 11 个客户端，保留后端回读但 UI 未开放的兼容客户端。

- [ ] **步骤 3：实现保存、分配和删除状态**

保存调用 `mcpUpsertLocalServer`，仅在返回成功后替换列表；修改目标客户端调用 `mcpSetServerApps`；删除调用 `mcpRemoveServer` 并重新扫描；每个动作使用独立 running key，避免刷新按钮被其他动作锁死。

- [ ] **步骤 4：运行前端测试**

运行：`pnpm test:run -- tests/McpScreen.test.tsx`。

预期：PASS，覆盖 loading、空列表、无效 JSON、保存成功、删除成功和错误提示。

- [ ] **步骤 5：提交**

```bash
git add src/screens/McpScreen.tsx src/components/mcp tests/McpScreen.test.tsx
git commit -m "feat: add local MCP settings editor"
```

### 任务 10：实现 MCP 市场搜索和安装界面

**文件：**
- 创建：`src/components/mcp/McpMarketplacePanel.tsx`
- 创建：`src/components/mcp/McpInstallDialog.tsx`
- 修改：`src/screens/McpScreen.tsx`
- 修改：`src/lib/i18n.tsx`
- 测试：`tests/McpMarketplace.test.tsx`

**接口：**
- 市场面板调用 `mcpListMarketplaces`、`mcpSearchMarketplace`、`mcpGetMarketplaceServerDetail`。
- 安装对话框把表单值转换为 `Record<string, unknown>`，调用 `mcpInstallFromMarketplace`。

- [ ] **步骤 1：写市场交互失败测试**

```tsx
it("validates required marketplace parameters before install", async () => {
  vi.mocked(mcpGetMarketplaceServerDetail).mockResolvedValue(detailFixture);
  render(<McpScreen />);
  await userEvent.click(await screen.findByText(detailFixture.name));
  await userEvent.click(screen.getByRole("button", { name: /Install|安装/ }));
  expect(mcpInstallFromMarketplace).not.toHaveBeenCalled();
  expect(screen.getByText(/required|必填/)).toBeInTheDocument();
});
```

运行：`pnpm test:run -- tests/McpMarketplace.test.tsx`，预期 FAIL。

- [ ] **步骤 2：实现提供商、搜索和详情状态**

加载提供商后默认选中 Official Registry；搜索按钮带 query 和 limit；连续搜索取消旧结果更新；选择结果加载详情，显示验证状态、协议、主页、下载量和安装选项。

- [ ] **步骤 3：实现动态参数表单和安装**

按 `kind` 渲染 string、boolean、number、integer、json 和 enum 控件；提交前校验 required、JSON 解析和枚举范围；安装对话框选择目标客户端，调用后端安装并切换到新扫描到的本地项。

- [ ] **步骤 4：运行市场测试和类型检查**

运行：`pnpm test:run -- tests/McpMarketplace.test.tsx && pnpm typecheck`。

预期：PASS。

- [ ] **步骤 5：提交**

```bash
git add src/components/mcp/McpMarketplacePanel.tsx src/components/mcp/McpInstallDialog.tsx src/screens/McpScreen.tsx src/lib/i18n.tsx tests/McpMarketplace.test.tsx
git commit -m "feat: add MCP marketplace installer"
```

### 任务 11：完成 MCP 回归验证

**文件：**
- 修改：`tests/transport/command-contract.test.ts`（仅在前序任务遗漏时补充）
- 修改：`src-tauri/src/mcp/*`（仅修复测试发现的问题）

- [ ] **步骤 1：运行前端全量测试**

运行：`pnpm typecheck && pnpm test:run`

预期：现有测试和新增 MCP 测试全部 PASS。

- [ ] **步骤 2：运行 Rust 检查和测试**

运行：`pnpm rust:check && pnpm rust:test`

预期：无编译错误，MCP 适配器、规范化、市场和 Web dispatch 测试全部 PASS。

- [ ] **步骤 3：检查工作区改动范围**

运行：`git status --short`，确认只出现本计划产生的提交之外的用户原有改动，不得出现未预期的大型格式化变更。

- [ ] **步骤 4：提交最终回归修复**

```bash
git add src-tauri/src/mcp src/screens/McpScreen.tsx src/components/mcp tests
git commit -m "test: verify MCP settings migration"
```
