# 多客户端配置写入（ZCode 接入算力池）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把「写入路由配置文件」从 platform→单适配器的一对一改为按 `(client_key, platform)` 索引，使 ZCode 能一键接入 ai-switch 算力池。

**Architecture:** `TargetAdapter` 新增 `client_key()` / `client_display_name()` / `native()` / `restart_required()`；一个适配器实例仍只服务一个平台，ZCode 出 codex 与 claude 两个实例共享 `client_key = "zcode"`、各占一行 `target_apps`。写入按客户端逐个走独立的 `write_group`，模型清单从算力池映射生成后经 `RouteConfigInput` 传入适配器。

**Tech Stack:** Rust（Tauri 2、sqlx、serde_json、toml_edit）、TypeScript + React 18 + TanStack Query、vitest、cargo test。

**Spec:** `docs/superpowers/specs/2026-09-02-multi-client-config-write-zcode-design.md`

## Global Constraints

- 构建目录：AI 调试与验证只能用 `src-tauri/target-codex/`，在 `src-tauri` 下执行时设 `CARGO_TARGET_DIR=target-codex`。禁止新建其他 target 目录。
- **构建前置**：`cargo test` 会在构建脚本阶段失败，除非以下两个路径存在（均被 gitignore，全新环境必须先造）：
  ```bash
  cd sidecar/ai-switch-tsnet && go build -o ../../src-tauri/binaries/ai-switch-tsnet-x86_64-pc-windows-msvc.exe
  mkdir -p dist
  ```
  这两项不进提交，保留在工作区即可。
- 验证命令：`pnpm typecheck`、`pnpm test:run`、`cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test`、`cd src-tauri && cargo fmt --check`。
- **`RouteConfigInput` 的构造点有 7 处**：`route_config_service.rs` 四处（约 :85 / :184 / :270 / :302）、`config_write_service.rs` 测试两处、`commands/target_commands.rs` 测试一处，外加各 adapter 测试模块里的 `fn input()` 辅助。给该结构体加字段时用 `grep -rn "RouteConfigInput {" src-tauri/src` 找齐，别照抄计划里的数量。
- **`ModelMapping` 有四个字段**：`from`、`to`、`label: Option<String>`、`supports_1m: Option<bool>`（`models/route_credential.rs:305`）。构造字面量时 `label` 不能省。
- **`TargetAdapter` 有三个 impl**：两个真适配器模块外，`config_write_service.rs` 的测试替身 `ConflictOnCommitAdapter` 也 impl 了它。给 trait 加方法时必须同步转发给 `self.inner`，否则编译失败。用 `grep "impl TargetAdapter for"` 找齐。
- **`route_model_capability.rs` 的 `mod tests` 用显式 `use super::{...}` 列表**（不是 `use super::*`），新引用的符号必须加进该列表。
- 不新增 `PlatformId` 变体。平台枚举的既有硬编码（`PlatformId::ALL`、能力矩阵、`AgentIcon`、`AppLayout`、`VibeScreen`、`platformLabels`）一处都不改。
- ZCode 配置路径固定为 `~/.zcode/v2/config.json`，不支持 `ZCODE_DATA_BASE_DIR` / `dataBaseDir` 覆盖。
- ZCode 条目**不写** `apiFormat`（会被 `kind` 重算覆盖）、**不写** `enabled`、模型项**不写** `name` 与 `zcode` 子对象。
- ZCode 记录键不得以 `builtin:` 或 `default-` 开头。新建时用 `ai-switch-codex` / `ai-switch-claude`。
- `kind` 与 baseURL 的配对：`openai` 写 `{base}/v1`；`anthropic` 写 `{base}`（不带 `/v1`）。
- 秘密不得出现在 outcome、错误详情或快照 `metadata_json` 中。
- Rust 中不放中文 UI 文案；提示语留在前端。
- 新增 Tauri 命令必须同时注册进 `src-tauri/src/lib.rs` 的 `generate_handler!`、`src-tauri/src/web/handlers/mod.rs` 的 match、`src/lib/api/client.ts`。

## File Structure

**新建**
- `src-tauri/src/adapters/route_config/zcode.rs` — ZCode 适配器：路径解析、provider 条目渲染与接管、按平台过滤的 inspect。
- `src/components/accounts/ConfigWriteTargetsDialog.tsx` — 客户端多选弹窗。

**修改**
- `src-tauri/src/adapters/route_config/mod.rs` — trait 扩展、`ClientTargetDescriptor`、注册表新 API、`RouteConfigInput.client_models`。
- `src-tauri/src/adapters/route_config/codex.rs`、`json_agent.rs` — 实现新增的 trait 方法。
- `src-tauri/src/database/repositories/target_repository.rs` — 两行 ZCode 种子。
- `src-tauri/src/models/settings.rs` — `config_write_clients_json`。
- `src-tauri/src/services/route_config_service.rs` — 客户端解析、逐客户端写入、模型清单、stale 作用域。
- `src-tauri/src/services/target_service.rs` — 新增 `list_config_write_clients_for_home`。
- `src-tauri/src/commands/route_proxy_commands.rs`、`target_commands.rs`、`lib.rs`、`web/handlers/mod.rs` — 命令签名与注册。
- `src/lib/api/client.ts`、`types.ts`、`src/screens/AccountsScreen.tsx` — 前端接线。

---

### Task 1: 扩展 TargetAdapter trait 与注册表

把适配器从「platform → 一个」改为「(client_key, platform) → 一个」。本任务不引入 ZCode，只让现有四个适配器声明客户端身份，并替换注册表 API。

**Files:**
- Modify: `src-tauri/src/adapters/route_config/mod.rs`（trait 80-91、注册表 93-129、测试 158-529）
- Modify: `src-tauri/src/adapters/route_config/codex.rs:17-28`
- Modify: `src-tauri/src/adapters/route_config/json_agent.rs:12-63, 92-104`
- Modify: `src-tauri/src/services/route_config_service.rs:66, 151, 251, 293, 503-512`
- Modify: `src-tauri/src/services/config_write_service.rs:1025, 1042`
- Modify: `src-tauri/src/commands/target_commands.rs:119`

**Interfaces:**
- Produces: `TargetAdapter::client_key() -> &'static str`、`client_display_name() -> &'static str`、`native() -> bool`、`restart_required() -> bool`、`requires_client_models() -> bool`；`ClientTargetDescriptor`（字段见下）；`TargetAdapterRegistry::by_client_and_platform(&self, client_key: &str, platform: PlatformId) -> Option<Arc<dyn TargetAdapter>>`；`TargetAdapterRegistry::clients_for_platform(&self, platform: PlatformId) -> Vec<ClientTargetDescriptor>`。`for_platform` 删除。

四个适配器的取值（本任务内均为原生 CLI）：

| 适配器 | client_key | client_display_name | native | restart_required | requires_client_models |
|---|---|---|---|---|---|
| `CodexAdapter` | `codex` | `Codex CLI` | true | false | false |
| `JsonAgentAdapter::claude()` | `claude_code` | `Claude Code` | true | false | false |
| `JsonAgentAdapter::gemini()` | `gemini_cli` | `Gemini CLI` | true | false | false |
| `JsonAgentAdapter::grok()` | `grok` | `Grok` | true | false | false |

- [ ] **Step 1: 写失败测试**

追加到 `src-tauri/src/adapters/route_config/mod.rs` 的 `mod tests`：

```rust
    #[test]
    fn registry_keys_are_unique_across_target_and_client_platform_pairs() {
        let registry = TargetAdapterRegistry::new();

        let mut target_keys = std::collections::HashSet::new();
        let mut client_platform_pairs = std::collections::HashSet::new();
        for adapter in &registry.adapters {
            assert!(
                target_keys.insert(adapter.target_key()),
                "duplicate target_key: {}",
                adapter.target_key()
            );
            assert!(
                client_platform_pairs.insert((adapter.client_key(), adapter.platform())),
                "duplicate (client_key, platform): {} {:?}",
                adapter.client_key(),
                adapter.platform()
            );
            assert!(
                !adapter.client_display_name().is_empty(),
                "empty display name: {}",
                adapter.target_key()
            );
        }
    }

    #[test]
    fn native_cli_adapters_resolve_by_client_and_platform() {
        let registry = TargetAdapterRegistry::new();

        for (client_key, platform, target_key) in [
            ("codex", PlatformId::Codex, "codex"),
            ("claude_code", PlatformId::Claude, "claude_code"),
            ("gemini_cli", PlatformId::Gemini, "gemini_cli"),
            ("grok", PlatformId::Grok, "grok"),
        ] {
            let adapter = registry
                .by_client_and_platform(client_key, platform)
                .unwrap_or_else(|| panic!("adapter for {client_key}"));
            assert_eq!(adapter.target_key(), target_key);
            assert!(adapter.native(), "{client_key} is a first-party CLI");
            // CLIs read config on next invocation, so nothing needs restarting.
            assert!(!adapter.restart_required(), "{client_key}");
        }

        // Wrong platform for a real client key resolves to nothing rather than
        // silently writing the wrong file.
        assert!(registry
            .by_client_and_platform("codex", PlatformId::Claude)
            .is_none());
        assert!(registry
            .by_client_and_platform("unknown", PlatformId::Codex)
            .is_none());
    }

    #[test]
    fn clients_for_platform_lists_native_cli_only_before_zcode_exists() {
        let registry = TargetAdapterRegistry::new();

        let codex = registry.clients_for_platform(PlatformId::Codex);
        assert_eq!(
            codex
                .iter()
                .map(|client| client.client_key.as_str())
                .collect::<Vec<_>>(),
            vec!["codex"]
        );
        assert_eq!(codex[0].display_name, "Codex CLI");
        assert_eq!(codex[0].target_key, "codex");
        assert_eq!(codex[0].platform, PlatformId::Codex);

        // Platforms with no adapter list nothing rather than erroring.
        assert!(registry
            .clients_for_platform(PlatformId::Hermes)
            .is_empty());
    }
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib adapters::route_config
```

预期：编译失败，`no method named 'client_key' found`、`no method named 'by_client_and_platform'`、`no method named 'clients_for_platform'`。

- [ ] **Step 3: 扩展 trait 与注册表**

`mod.rs` 中 `RouteConfigInput` 之后加入描述符（`Serialize` 供后续命令直接返回）：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientTargetDescriptor {
    pub client_key: String,
    pub display_name: String,
    /// This client is the platform's first-party CLI. Drives the dialog's
    /// default selection when the user has never chosen.
    pub native: bool,
    /// Long-running app that reads config at startup, so a write does not take
    /// effect until it restarts.
    pub restart_required: bool,
    /// Client cannot discover models on its own; the write must carry the pool's
    /// advertised model list.
    pub requires_client_models: bool,
    pub target_key: String,
    pub platform: PlatformId,
}
```

`TargetAdapter` trait 增加五个方法（`target_key` 之后）：

```rust
    /// Client this adapter writes for. Distinct from `target_key`: one client
    /// can serve several platforms and then owns one target row per platform.
    fn client_key(&self) -> &'static str;
    fn client_display_name(&self) -> &'static str;
    /// Whether this client is the platform's first-party CLI.
    fn native(&self) -> bool;
    /// Whether the client must restart before a write takes effect.
    fn restart_required(&self) -> bool;
    /// Whether the client needs the pool's advertised model list written into
    /// its config because it cannot discover models itself.
    fn requires_client_models(&self) -> bool;
```

注册表把 `for_platform` 换成两个新方法（`adapters` 字段无需改可见性：`mod tests` 是 `mod.rs` 的子模块，可直接访问私有字段）：

```rust
    pub fn by_client_and_platform(
        &self,
        client_key: &str,
        platform: PlatformId,
    ) -> Option<Arc<dyn TargetAdapter>> {
        self.adapters
            .iter()
            .find(|adapter| adapter.client_key() == client_key && adapter.platform() == platform)
            .cloned()
    }

    pub fn clients_for_platform(&self, platform: PlatformId) -> Vec<ClientTargetDescriptor> {
        self.adapters
            .iter()
            .filter(|adapter| adapter.platform() == platform)
            .map(|adapter| ClientTargetDescriptor {
                client_key: adapter.client_key().to_string(),
                display_name: adapter.client_display_name().to_string(),
                native: adapter.native(),
                restart_required: adapter.restart_required(),
                requires_client_models: adapter.requires_client_models(),
                target_key: adapter.target_key().to_string(),
                platform: adapter.platform(),
            })
            .collect()
    }
```

`codex.rs` 在 `target_key` 之后加入：

```rust
    fn client_key(&self) -> &'static str {
        "codex"
    }

    fn client_display_name(&self) -> &'static str {
        "Codex CLI"
    }

    fn native(&self) -> bool {
        true
    }

    fn restart_required(&self) -> bool {
        false
    }

    fn requires_client_models(&self) -> bool {
        false
    }
```

`json_agent.rs` 的 `JsonAgentAdapter` 结构体增加两个字段：

```rust
    client_key: &'static str,
    client_display_name: &'static str,
```

三个构造函数分别补 `client_key: "claude_code", client_display_name: "Claude Code"`、`client_key: "gemini_cli", client_display_name: "Gemini CLI"`、`client_key: "grok", client_display_name: "Grok"`，并实现方法：

```rust
    fn client_key(&self) -> &'static str {
        self.client_key
    }

    fn client_display_name(&self) -> &'static str {
        self.client_display_name
    }

    fn native(&self) -> bool {
        true
    }

    fn restart_required(&self) -> bool {
        false
    }

    fn requires_client_models(&self) -> bool {
        false
    }
```

- [ ] **Step 4: 修掉 for_platform 的 6 处调用**

`route_config_service.rs:503-512` 的 `route_config_adapter` 改为按客户端解析，并新增错误码：

```rust
fn route_config_adapter(
    client_key: &str,
    platform: PlatformId,
) -> Result<Arc<dyn TargetAdapter>, AppError> {
    TargetAdapterRegistry::new()
        .by_client_and_platform(client_key, platform)
        .ok_or_else(|| AppError::Validation {
            code: "config.client_unavailable",
            message: "No verified configuration adapter is available for this client".to_string(),
            details: Some(format!("{client_key}:{}", platform.as_str())),
            recoverable: true,
        })
}

/// The platform's first-party CLI client key. Used by call sites that predate
/// explicit client selection so their behavior is unchanged.
fn native_client_key(platform: PlatformId) -> Result<String, AppError> {
    TargetAdapterRegistry::new()
        .clients_for_platform(platform)
        .into_iter()
        .find(|client| client.native)
        .map(|client| client.client_key)
        .ok_or_else(|| AppError::Validation {
            code: "config.adapter_unavailable",
            message: "No verified native configuration adapter is available".to_string(),
            details: Some(platform.as_str().to_string()),
            recoverable: true,
        })
}
```

四处调用点改为先取 native 客户端键再解析（Task 4/5 会把其中两处换成显式客户端列表）：
- `:66` → `let client_key = native_client_key(platform)?; let adapter = route_config_adapter(&client_key, platform)?;`
- `:251` → 同上
- `:293` → `adapter: route_config_adapter(&native_client_key(platform)?, platform)?,`
- `:151`（`write_existing_configs_for_home` 内的 `registry.for_platform(parsed)`）→
  ```rust
            let Some(adapter) = TargetAdapterRegistry::new()
                .clients_for_platform(parsed)
                .into_iter()
                .find(|client| client.native)
                .and_then(|client| {
                    TargetAdapterRegistry::new().by_client_and_platform(&client.client_key, parsed)
                })
            else {
  ```

`config_write_service.rs:1025` 与 `:1042`（测试 fixture）→ `.by_client_and_platform("codex", PlatformId::Codex)` / `.by_client_and_platform("claude_code", PlatformId::Claude)`。

`commands/target_commands.rs:119`（测试）→ `.by_client_and_platform("codex", PlatformId::Codex)`。

`mod.rs` 测试里 `for_platform(...)` 的 9 处调用同样替换；`registry_contains_only_verified_native_config_adapters` 中三处 `for_platform(OpenCode/OpenClaw/Hermes).is_none()` 改为 `clients_for_platform(...).is_empty()`。

- [ ] **Step 5: 运行测试确认通过**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib && cargo fmt
```

预期：全部 PASS。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/adapters/route_config src-tauri/src/services/route_config_service.rs src-tauri/src/services/config_write_service.rs src-tauri/src/commands/target_commands.rs
git commit -m "refactor: 配置写入适配器按 (客户端, 平台) 索引"
```

---

### Task 2: RouteConfigInput 携带模型清单

ZCode 需要把算力池宣告的模型写进配置（它不会自己去拉 `/v1/models`）。本任务只把通道打通：`RouteConfigInput` 增加 `client_models`，并把生成模型清单的函数暴露出来。现有四个适配器忽略这个字段。

**Files:**
- Modify: `src-tauri/src/adapters/route_config/mod.rs`（`RouteConfigInput` 21-30、测试 167-173）
- Modify: `src-tauri/src/services/route_model_capability.rs:209`
- Modify: `src-tauri/src/services/route_config_service.rs`

**Interfaces:**
- Consumes: Task 1 的 `requires_client_models()`。
- Produces: `RouteConfigInput.client_models: Vec<ClientModel>`；`ClientModel { id: String, context_window: u32, max_output_tokens: u32 }`；`route_model_capability::advertised_model_catalog_entries` 改为 `pub(crate)`；`RouteConfigService::resolve_client_models(pool, platform) -> Result<Vec<ClientModel>, AppError>`。

- [ ] **Step 1: 写失败测试**

追加到 `src-tauri/src/adapters/route_config/mod.rs` 的 `mod tests`：

```rust
    #[test]
    fn client_models_carry_context_limits_and_are_ignored_by_native_adapters() {
        let registry = TargetAdapterRegistry::new();
        let with_models = RouteConfigInput {
            client_models: vec![
                ClientModel {
                    id: "gpt-5.6-sol".to_string(),
                    context_window: 200_000,
                    max_output_tokens: 128_000,
                },
                ClientModel {
                    id: "claude-sonnet-alias[1m]".to_string(),
                    context_window: 1_000_000,
                    max_output_tokens: 128_000,
                },
            ],
            ..input()
        };

        // The four native CLIs discover models themselves, so the list must not
        // leak into their files.
        let codex = registry
            .by_client_and_platform("codex", PlatformId::Codex)
            .unwrap();
        let rendered = codex
            .render(Path::new("config.toml"), None, &with_models)
            .unwrap();
        let rendered = String::from_utf8(rendered).unwrap();
        assert!(!rendered.contains("gpt-5.6-sol"));

        let claude = registry
            .by_client_and_platform("claude_code", PlatformId::Claude)
            .unwrap();
        let rendered = claude
            .render(Path::new("settings.json"), None, &with_models)
            .unwrap();
        assert!(!String::from_utf8(rendered).unwrap().contains("gpt-5.6-sol"));
    }
```

追加到 `src-tauri/src/services/route_model_capability.rs` 的 `mod tests`：

```rust
    #[test]
    fn catalog_entries_are_shared_with_client_config_writers() {
        let mapping = ModelMapping {
            from: "gpt-5.6-sol".to_string(),
            to: "gpt-5.6-sol".to_string(),
            supports_1m: None,
        };
        let capability = ModelCapability {
            mappings: vec![mapping],
        };

        // Same source of truth the Codex catalog uses, so a client config and
        // the catalog can never advertise different models.
        let entries = advertised_model_catalog_entries("codex", &[capability]);

        assert_eq!(
            entries.iter().map(|entry| entry.id.as_str()).collect::<Vec<_>>(),
            vec!["gpt-5.6-sol"]
        );
    }
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib
```

预期：编译失败，`struct 'RouteConfigInput' has no field named 'client_models'`、`cannot find struct 'ClientModel'`。

- [ ] **Step 3: 实现**

`mod.rs` 中 `RouteConfigInput` 增加字段并新增结构体：

```rust
pub struct RouteConfigInput {
    pub base_url: String,
    pub route_proxy_key: String,
    pub claude_env: ClaudeEnvPlan,
    /// Models the pool advertises, for clients that cannot discover models on
    /// their own. Empty for clients that can — the four native CLIs ignore it.
    pub client_models: Vec<ClientModel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientModel {
    pub id: String,
    pub context_window: u32,
    pub max_output_tokens: u32,
}
```

`mod.rs` 测试里的 `fn input()` 补 `client_models: Vec::new()`。

`route_model_capability.rs:209` 的 `fn advertised_model_catalog_entries` 改为 `pub(crate) fn`，并把 `AdvertisedModel`（第 11 行）与其 `id` 字段改为 `pub(crate)`。

`route_config_service.rs` 增加（`resolve_claude_env_plan` 附近，复用 `agent_launch_service::platform_models` 同样的池读取路径）：

```rust
    /// Models this platform's pool advertises, shaped for client configs that
    /// must carry the list themselves.
    pub(crate) async fn resolve_client_models(
        pool: &SqlitePool,
        platform: PlatformId,
    ) -> Result<Vec<ClientModel>, AppError> {
        let ids = RoutePoolRepository::list_member_ids(pool, platform.as_str()).await?;
        let credentials = RouteCredentialRepository::list_by_ids(
            pool,
            &ids,
            &RouteCredentialSelectionContext {
                platform: platform.as_str().to_string(),
                pool_scope: RouteCredentialPoolScope::InPool,
            },
        )
        .await?;
        let capabilities = credentials
            .iter()
            .map(|credential| parse_model_capability(&credential.config_json))
            .collect::<Vec<_>>();

        Ok(
            advertised_model_catalog_entries(platform.as_str(), &capabilities)
                .into_iter()
                .map(|model| ClientModel {
                    context_window: client_model_context_window(&model.id),
                    max_output_tokens: CLIENT_MODEL_MAX_OUTPUT_TOKENS,
                    id: model.id,
                })
                .collect(),
        )
    }
```

同文件顶层加入（`generate_route_proxy_key` 附近）：

```rust
const CLIENT_MODEL_MAX_OUTPUT_TOKENS: u32 = 128_000;
const CLIENT_MODEL_CONTEXT_WINDOW: u32 = 200_000;
const CLIENT_MODEL_ONE_M_CONTEXT_WINDOW: u32 = 1_000_000;

/// The `[1m]` suffix is how the pool advertises a 1M-context variant of a
/// model, so the written limit has to follow it rather than the base id.
fn client_model_context_window(model_id: &str) -> u32 {
    if model_id.trim().to_ascii_lowercase().ends_with("[1m]") {
        CLIENT_MODEL_ONE_M_CONTEXT_WINDOW
    } else {
        CLIENT_MODEL_CONTEXT_WINDOW
    }
}
```

导入补 `ClientModel` 与 `advertised_model_catalog_entries`。全仓库构造 `RouteConfigInput` 的位置（`route_config_service.rs` 三处、`config_write_service.rs` 测试两处、`mod.rs` 测试）补 `client_models: Vec::new()`。

- [ ] **Step 4: 为上下文窗口推导写测试并运行**

追加到 `route_config_service.rs` 的 `mod tests`：

```rust
    #[test]
    fn one_m_suffixed_models_get_the_larger_context_window() {
        assert_eq!(client_model_context_window("gpt-5.6-sol"), 200_000);
        assert_eq!(
            client_model_context_window("claude-sonnet-alias[1m]"),
            1_000_000
        );
        assert_eq!(
            client_model_context_window("Claude-Sonnet-Alias[1M]"),
            1_000_000
        );
    }
```

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib && cargo fmt
```

预期：全部 PASS。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/adapters/route_config/mod.rs src-tauri/src/services/route_model_capability.rs src-tauri/src/services/route_config_service.rs src-tauri/src/services/config_write_service.rs
git commit -m "feat: 配置写入输入携带算力池模型清单"
```

---

### Task 3: ZCode 适配器

**Files:**
- Create: `src-tauri/src/adapters/route_config/zcode.rs`
- Modify: `src-tauri/src/adapters/route_config/mod.rs`（模块声明 1-2、注册表 99-108）
- Modify: `src-tauri/src/database/repositories/target_repository.rs:11-20`

**Interfaces:**
- Consumes: Task 1 的 trait 与注册表；Task 2 的 `RouteConfigInput.client_models` 与 `ClientModel`。
- Produces: `ZCodeAdapter::codex()`、`ZCodeAdapter::claude()`（`pub(super)`）；`target_key` 为 `zcode_codex` / `zcode_claude`，`client_key` 均为 `zcode`；`RouteConfigInput.route_proxy_key_aliases: Vec<String>`（Step 3 加入）。

写入的条目形状（codex 实例，`models` 由 `client_models` 生成）：

```json
{
  "name": "AI Switch (Codex)",
  "kind": "openai",
  "source": "custom",
  "options": {
    "apiKey": "sk-ai-switch-...",
    "baseURL": "http://127.0.0.1:19527/v1",
    "apiKeyRequired": true
  },
  "models": {
    "gpt-5.6-sol": {
      "limit": { "context": 200000, "output": 128000 },
      "modalities": { "input": ["text"], "output": ["text"] }
    }
  },
  "aiSwitch": { "managed": true, "platform": "codex" }
}
```

- [ ] **Step 1: 写失败测试**

新建 `src-tauri/src/adapters/route_config/zcode.rs`，先只放测试模块（实现留空，下一步补）：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::route_config::{ClientModel, TargetAdapterRegistry};
    use serde_json::json;

    const BASE_URL: &str = "http://127.0.0.1:19527";
    const CODEX_KEY: &str = "sk-ai-switch-codexkey";

    fn input(models: &[&str]) -> RouteConfigInput {
        RouteConfigInput {
            base_url: BASE_URL.to_string(),
            route_proxy_key: CODEX_KEY.to_string(),
            // Field added in Step 3 of this task.
            route_proxy_key_aliases: Vec::new(),
            claude_env: crate::adapters::route_config::ClaudeEnvPlan::default(),
            client_models: models
                .iter()
                .map(|id| ClientModel {
                    id: (*id).to_string(),
                    context_window: 200_000,
                    max_output_tokens: 128_000,
                })
                .collect(),
        }
    }

    fn codex_adapter() -> std::sync::Arc<dyn TargetAdapter> {
        TargetAdapterRegistry::new()
            .by_client_and_platform("zcode", PlatformId::Codex)
            .expect("zcode codex adapter")
    }

    fn claude_adapter() -> std::sync::Arc<dyn TargetAdapter> {
        TargetAdapterRegistry::new()
            .by_client_and_platform("zcode", PlatformId::Claude)
            .expect("zcode claude adapter")
    }

    fn render(adapter: &dyn TargetAdapter, existing: Option<&[u8]>, models: &[&str]) -> Value {
        let bytes = adapter
            .render(Path::new("config.json"), existing, &input(models))
            .expect("render");
        serde_json::from_slice(&bytes).expect("valid JSON")
    }

    #[test]
    fn adapter_identity_declares_zcode_as_a_restart_required_non_native_client() {
        for adapter in [codex_adapter(), claude_adapter()] {
            assert_eq!(adapter.client_key(), "zcode");
            assert!(!adapter.native(), "ZCode is not a platform's first-party CLI");
            // ZCode reads config at startup and has no file watcher.
            assert!(adapter.restart_required());
            // ZCode never probes /v1/models for custom providers.
            assert!(adapter.requires_client_models());
        }
        assert_eq!(codex_adapter().target_key(), "zcode_codex");
        assert_eq!(claude_adapter().target_key(), "zcode_claude");
    }

    #[test]
    fn resolved_path_is_the_desktop_provider_store() {
        let home = Path::new("/home/user");
        assert_eq!(
            codex_adapter().resolve_path(home),
            home.join(".zcode").join("v2").join("config.json")
        );
    }

    #[test]
    fn codex_writes_a_v1_suffixed_base_url_and_claude_does_not() {
        // kind=openai hits {baseURL}/responses, so the /v1 has to be in baseURL.
        let json = render(codex_adapter().as_ref(), None, &["gpt-5.6-sol"]);
        let entry = &json["provider"]["ai-switch-codex"];
        assert_eq!(entry["kind"], "openai");
        assert_eq!(entry["options"]["baseURL"], "http://127.0.0.1:19527/v1");

        // kind=anthropic appends /v1/messages itself; a /v1 here would produce
        // /v1/v1/messages.
        let json = render(claude_adapter().as_ref(), None, &["claude-sonnet-alias"]);
        let entry = &json["provider"]["ai-switch-claude"];
        assert_eq!(entry["kind"], "anthropic");
        assert_eq!(entry["options"]["baseURL"], "http://127.0.0.1:19527");
    }

    #[test]
    fn managed_entry_carries_credentials_models_and_the_managed_marker() {
        let json = render(codex_adapter().as_ref(), None, &["gpt-5.6-sol"]);
        let entry = &json["provider"]["ai-switch-codex"];

        assert_eq!(entry["options"]["apiKey"], CODEX_KEY);
        assert_eq!(entry["options"]["apiKeyRequired"], true);
        assert_eq!(entry["source"], "custom");
        assert_eq!(entry["aiSwitch"]["managed"], true);
        assert_eq!(entry["aiSwitch"]["platform"], "codex");
        assert_eq!(
            entry["models"]["gpt-5.6-sol"]["limit"],
            json!({ "context": 200000, "output": 128000 })
        );
        assert_eq!(
            entry["models"]["gpt-5.6-sol"]["modalities"],
            json!({ "input": ["text"], "output": ["text"] })
        );

        // apiFormat is recomputed from kind, so writing it would be misleading.
        assert!(entry.get("apiFormat").is_none());
        // Leaving enabled unset keeps the entry "not explicitly disabled".
        assert!(entry.get("enabled").is_none());
        // ZCode owns the per-model zcode sidecar and rewrites whatever we put there.
        assert!(entry["models"]["gpt-5.6-sol"].get("zcode").is_none());
        // Omitting name makes ZCode fall back to the record key.
        assert!(entry["models"]["gpt-5.6-sol"].get("name").is_none());
    }

    #[test]
    fn render_preserves_other_providers_and_the_sibling_platform_entry() {
        let existing = br#"{
  "$schema": "https://example.invalid/schema.json",
  "provider": {
    "builtin:bigmodel": {
      "name": "Bigmodel - API Key",
      "kind": "anthropic",
      "options": { "apiKey": "", "baseURL": "https://open.bigmodel.cn/api/anthropic" }
    },
    "ai-switch-claude": {
      "name": "AI Switch (Claude)",
      "kind": "anthropic",
      "options": { "apiKey": "sk-ai-switch-claudekey", "baseURL": "http://127.0.0.1:19527" },
      "aiSwitch": { "managed": true, "platform": "claude" }
    }
  }
}"#;

        let json = render(codex_adapter().as_ref(), Some(existing), &["gpt-5.6-sol"]);

        // A corrupted write here empties every provider the user has, so the
        // untouched entries matter as much as the one we write.
        assert_eq!(json["provider"]["builtin:bigmodel"]["name"], "Bigmodel - API Key");
        assert_eq!(
            json["provider"]["ai-switch-claude"]["options"]["apiKey"],
            "sk-ai-switch-claudekey"
        );
        assert_eq!(json["$schema"], "https://example.invalid/schema.json");
        assert_eq!(json["provider"]["ai-switch-codex"]["kind"], "openai");
    }

    #[test]
    fn adoption_claims_the_entry_marked_managed_for_this_platform() {
        let existing = br#"{
  "provider": {
    "3c109843-30ed-4307-a74e-ac537218d8be": {
      "name": "My Renamed Pool",
      "kind": "openai",
      "options": { "apiKey": "sk-ai-switch-stale", "baseURL": "http://127.0.0.1:1/v1" },
      "aiSwitch": { "managed": true, "platform": "codex" }
    }
  }
}"#;

        let json = render(codex_adapter().as_ref(), Some(existing), &["gpt-5.6-sol"]);
        let entry = &json["provider"]["3c109843-30ed-4307-a74e-ac537218d8be"];

        // Adopt in place: a second entry pointing at the same proxy would show up
        // twice in ZCode's picker.
        assert!(json["provider"].get("ai-switch-codex").is_none());
        // The user may have renamed it; renaming it back is a surprise.
        assert_eq!(entry["name"], "My Renamed Pool");
        assert_eq!(entry["options"]["apiKey"], CODEX_KEY);
        assert_eq!(entry["options"]["baseURL"], "http://127.0.0.1:19527/v1");
    }

    #[test]
    fn adoption_claims_a_hand_made_entry_by_base_url_and_key() {
        // What a user who wired this up by hand actually has: no managed marker,
        // but the platform's own sk and a local base URL.
        let existing = br#"{
  "provider": {
    "3c109843-30ed-4307-a74e-ac537218d8be": {
      "name": "Ai-Switch",
      "kind": "openai",
      "options": { "apiKey": "sk-ai-switch-codexkey", "baseURL": "http://127.0.0.1:19527/v1" },
      "models": { "glm-5.3": { "name": "GLM-5.3" } }
    }
  }
}"#;

        let json = render(codex_adapter().as_ref(), Some(existing), &["gpt-5.6-sol"]);
        let entry = &json["provider"]["3c109843-30ed-4307-a74e-ac537218d8be"];

        assert!(json["provider"].get("ai-switch-codex").is_none());
        assert_eq!(entry["aiSwitch"]["managed"], true);
        // Confirmed with the user: models is replaced wholesale, so a hand-added
        // model is removed rather than merged.
        assert!(entry["models"].get("glm-5.3").is_none());
        assert_eq!(entry["models"]["gpt-5.6-sol"]["limit"]["context"], 200000);
    }

    #[test]
    fn adoption_recognizes_a_rotated_key_via_the_alias_list() {
        let existing = br#"{
  "provider": {
    "hand-made": {
      "name": "Ai-Switch",
      "kind": "openai",
      "options": { "apiKey": "sk-ai-switch-previous", "baseURL": "http://127.0.0.1:19527/v1" }
    }
  }
}"#;

        let mut with_alias = input(&["gpt-5.6-sol"]);
        with_alias.route_proxy_key_aliases = vec!["sk-ai-switch-previous".to_string()];
        let bytes = codex_adapter()
            .render(Path::new("config.json"), Some(existing), &with_alias)
            .expect("render");
        let json: Value = serde_json::from_slice(&bytes).expect("valid JSON");

        // The user rotated their sk; the stale entry is still theirs.
        assert!(json["provider"].get("ai-switch-codex").is_none());
        assert_eq!(json["provider"]["hand-made"]["options"]["apiKey"], CODEX_KEY);
    }

    #[test]
    fn unrelated_local_provider_is_not_adopted() {
        // Same host, different port and a foreign key: not ours.
        let existing = br#"{
  "provider": {
    "someone-elses-proxy": {
      "name": "Other Proxy",
      "kind": "openai",
      "options": { "apiKey": "sk-not-ours", "baseURL": "http://127.0.0.1:8080/v1" }
    }
  }
}"#;

        let json = render(codex_adapter().as_ref(), Some(existing), &["gpt-5.6-sol"]);

        assert_eq!(json["provider"]["someone-elses-proxy"]["options"]["apiKey"], "sk-not-ours");
        assert_eq!(json["provider"]["ai-switch-codex"]["options"]["apiKey"], CODEX_KEY);
    }

    #[test]
    fn inspection_filters_by_platform_so_the_two_targets_do_not_impersonate_each_other() {
        let path = Path::new("config.json");
        assert_eq!(codex_adapter().inspect(path, None).file_status, "missing");

        let claude_only = br#"{
  "provider": {
    "ai-switch-claude": {
      "kind": "anthropic",
      "options": { "apiKey": "k", "baseURL": "http://127.0.0.1:19527" },
      "aiSwitch": { "managed": true, "platform": "claude" }
    }
  }
}"#;
        // Both adapters read the same file, so an unfiltered check would report
        // the Codex target as managed here.
        assert_eq!(
            codex_adapter().inspect(path, Some(claude_only)).file_status,
            "unmanaged"
        );
        assert_eq!(
            claude_adapter().inspect(path, Some(claude_only)).file_status,
            "managed"
        );

        let managed = codex_adapter()
            .render(path, None, &input(&["gpt-5.6-sol"]))
            .unwrap();
        let inspection = codex_adapter().inspect(path, Some(&managed));
        assert_eq!(inspection.file_status, "managed");
        assert!(inspection.managed);
    }

    #[test]
    fn corrupt_config_is_refused_rather_than_overwritten() {
        let path = Path::new("config.json");
        // A failed parse makes ZCode fall back to legacy files and end up with an
        // empty provider list, so overwriting would destroy every provider.
        let error = codex_adapter()
            .render(path, Some(b"{not json"), &input(&["gpt-5.6-sol"]))
            .expect_err("must refuse");
        assert!(matches!(
            error,
            AppError::Validation {
                code: "validation.route_config_existing_invalid",
                ..
            }
        ));

        assert_eq!(codex_adapter().inspect(path, Some(b"{not json")).file_status, "invalid");
        // A JSON array parses but has no place for `provider`.
        assert_eq!(codex_adapter().inspect(path, Some(b"[]")).file_status, "invalid");
        assert!(codex_adapter()
            .render(path, Some(b"[]"), &input(&["gpt-5.6-sol"]))
            .is_err());
    }

    #[test]
    fn empty_config_file_renders_a_fresh_provider_map() {
        let json = render(codex_adapter().as_ref(), Some(b"   "), &["gpt-5.6-sol"]);
        assert_eq!(json["provider"]["ai-switch-codex"]["kind"], "openai");
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

先在 `mod.rs` 加 `mod zcode;`，然后：

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib adapters::route_config::zcode
```

预期：编译失败，`cannot find type 'ZCodeAdapter'`、`no field 'route_proxy_key_aliases'`。

- [ ] **Step 3: 在 RouteConfigInput 上加别名字段**

上面的别名测试要求 `RouteConfigInput` 携带历史 key。在 `mod.rs` 的 `RouteConfigInput` 增加：

```rust
    /// Proxy keys this platform used before rotation. A hand-made client entry
    /// still carrying an old key is the same user's entry, so adoption has to
    /// recognize it instead of adding a duplicate.
    pub route_proxy_key_aliases: Vec<String>,
```

全仓库构造 `RouteConfigInput` 的位置补 `route_proxy_key_aliases: Vec::new()`（Task 5 会为 ZCode 填真值）。

- [ ] **Step 4: 实现适配器**

在 `zcode.rs` 的测试模块之前写入实现：

```rust
use super::{
    existing_text, generated_invalid, invalid_existing_config, ClientModel, RouteConfigInput,
    TargetAdapter, TargetInspection,
};
use crate::{error::AppError, models::platform::PlatformId};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub(super) struct ZCodeAdapter {
    target_key: &'static str,
    platform: PlatformId,
    /// Selects ZCode's wire protocol. `openai` hits `{baseURL}/responses`;
    /// `anthropic` hits `{baseURL}/v1/messages`.
    kind: &'static str,
    /// Appended to the proxy base URL. Differs per kind because ZCode adds its
    /// own suffix: anthropic would otherwise produce `/v1/v1/messages`.
    base_url_suffix: &'static str,
    /// Record key used when no existing entry can be adopted. Never prefixed
    /// with `builtin:` (coerced to a builtin and dropped from ZCode's registry)
    /// or `default-` (filtered when the key is blank).
    fallback_provider_id: &'static str,
    display_name: &'static str,
}

impl ZCodeAdapter {
    pub(super) const fn codex() -> Self {
        Self {
            target_key: "zcode_codex",
            platform: PlatformId::Codex,
            kind: "openai",
            base_url_suffix: "/v1",
            fallback_provider_id: "ai-switch-codex",
            display_name: "AI Switch (Codex)",
        }
    }

    pub(super) const fn claude() -> Self {
        Self {
            target_key: "zcode_claude",
            platform: PlatformId::Claude,
            kind: "anthropic",
            base_url_suffix: "",
            fallback_provider_id: "ai-switch-claude",
            display_name: "AI Switch (Claude)",
        }
    }

    fn base_url(&self, base_url: &str) -> String {
        let trimmed = base_url.trim().trim_end_matches('/');
        if self.base_url_suffix.is_empty() {
            return trimmed.to_string();
        }
        if trimmed
            .rsplit('/')
            .next()
            .is_some_and(|segment| segment.eq_ignore_ascii_case("v1"))
        {
            return trimmed.to_string();
        }
        format!("{trimmed}{}", self.base_url_suffix)
    }

    /// Record key of the entry we should write into, in priority order: our own
    /// marker, then a hand-made entry recognizable by base URL plus current or
    /// rotated key.
    fn adoption_target(&self, providers: &Map<String, Value>, input: &RouteConfigInput) -> Option<String> {
        if let Some(key) = providers.iter().find_map(|(key, entry)| {
            let managed = entry.pointer("/aiSwitch/managed").and_then(Value::as_bool) == Some(true);
            let platform = entry
                .pointer("/aiSwitch/platform")
                .and_then(Value::as_str)
                .is_some_and(|value| value == self.platform.as_str());
            (managed && platform).then(|| key.clone())
        }) {
            return Some(key);
        }

        let expected_base = self.base_url(&input.base_url);
        providers.iter().find_map(|(key, entry)| {
            let base_matches = entry
                .pointer("/options/baseURL")
                .and_then(Value::as_str)
                .map(|value| value.trim().trim_end_matches('/'))
                .is_some_and(|value| value == expected_base);
            let api_key = entry
                .pointer("/options/apiKey")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            let key_matches = !api_key.is_empty()
                && (api_key == input.route_proxy_key
                    || input
                        .route_proxy_key_aliases
                        .iter()
                        .any(|alias| alias == api_key));
            (base_matches && key_matches).then(|| key.clone())
        })
    }

    fn model_entries(&self, models: &[ClientModel]) -> Map<String, Value> {
        models
            .iter()
            .map(|model| {
                (
                    model.id.clone(),
                    json!({
                        "limit": {
                            "context": model.context_window,
                            "output": model.max_output_tokens,
                        },
                        "modalities": { "input": ["text"], "output": ["text"] },
                    }),
                )
            })
            .collect()
    }
}

impl TargetAdapter for ZCodeAdapter {
    fn target_key(&self) -> &'static str {
        self.target_key
    }

    fn client_key(&self) -> &'static str {
        "zcode"
    }

    fn client_display_name(&self) -> &'static str {
        "ZCode"
    }

    fn native(&self) -> bool {
        false
    }

    fn restart_required(&self) -> bool {
        true
    }

    fn requires_client_models(&self) -> bool {
        true
    }

    fn platform(&self) -> PlatformId {
        self.platform
    }

    fn resolve_path(&self, home: &Path) -> PathBuf {
        home.join(".zcode").join("v2").join("config.json")
    }

    fn render(
        &self,
        path: &Path,
        existing: Option<&[u8]>,
        input: &RouteConfigInput,
    ) -> Result<Vec<u8>, AppError> {
        let mut config = match existing {
            Some(bytes) => {
                let content = existing_text(path, "JSON", bytes)?;
                if content.trim().is_empty() {
                    Value::Object(Map::new())
                } else {
                    serde_json::from_str(content)
                        .map_err(|_| invalid_existing_config(path, "JSON", "syntax is invalid"))?
                }
            }
            None => Value::Object(Map::new()),
        };

        let root = config
            .as_object_mut()
            .ok_or_else(|| invalid_existing_config(path, "JSON", "root value must be an object"))?;
        let providers = root
            .entry("provider".to_string())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| invalid_existing_config(path, "JSON", "provider must be an object"))?;

        let provider_id = self
            .adoption_target(providers, input)
            .unwrap_or_else(|| self.fallback_provider_id.to_string());
        let existing_entry = providers.get(&provider_id).cloned();
        let entry = providers
            .entry(provider_id)
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| invalid_existing_config(path, "JSON", "provider entry must be an object"))?;

        // Keep a name the user may have edited; only fill one in when adopting an
        // entry that has none.
        let name = existing_entry
            .as_ref()
            .and_then(|entry| entry.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(self.display_name)
            .to_string();
        entry.insert("name".to_string(), Value::String(name));
        entry.insert("kind".to_string(), Value::String(self.kind.to_string()));
        entry.insert("source".to_string(), Value::String("custom".to_string()));

        let mut options = existing_entry
            .as_ref()
            .and_then(|entry| entry.get("options"))
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        options.insert(
            "apiKey".to_string(),
            Value::String(input.route_proxy_key.clone()),
        );
        options.insert(
            "baseURL".to_string(),
            Value::String(self.base_url(&input.base_url)),
        );
        options.insert("apiKeyRequired".to_string(), Value::Bool(true));
        entry.insert("options".to_string(), Value::Object(options));

        entry.insert(
            "models".to_string(),
            Value::Object(self.model_entries(&input.client_models)),
        );
        entry.insert(
            "aiSwitch".to_string(),
            json!({ "managed": true, "platform": self.platform.as_str() }),
        );

        let rendered = serde_json::to_vec_pretty(&config).map_err(|_| generated_invalid(path, "JSON"))?;
        let generated: Value =
            serde_json::from_slice(&rendered).map_err(|_| generated_invalid(path, "JSON"))?;
        if !generated.is_object() {
            return Err(generated_invalid(path, "JSON"));
        }
        Ok(rendered)
    }

    fn inspect(&self, _path: &Path, existing: Option<&[u8]>) -> TargetInspection {
        let Some(bytes) = existing else {
            return TargetInspection::missing();
        };
        let Ok(config) = serde_json::from_slice::<Value>(bytes) else {
            return TargetInspection::invalid();
        };
        let Some(root) = config.as_object() else {
            return TargetInspection::invalid();
        };

        // Both ZCode adapters read this one file, so the marker has to be matched
        // per platform or each target would report the other's entry as its own.
        let managed = root
            .get("provider")
            .and_then(Value::as_object)
            .is_some_and(|providers| {
                providers.values().any(|entry| {
                    entry.pointer("/aiSwitch/managed").and_then(Value::as_bool) == Some(true)
                        && entry
                            .pointer("/aiSwitch/platform")
                            .and_then(Value::as_str)
                            .is_some_and(|value| value == self.platform.as_str())
                })
            });

        TargetInspection::valid(managed)
    }
}
```

`mod.rs` 顶部加 `mod zcode;`、`use zcode::ZCodeAdapter;`，注册表 `adapters` 末尾追加：

```rust
                Arc::new(ZCodeAdapter::codex()),
                Arc::new(ZCodeAdapter::claude()),
```

`target_repository.rs:11-20` 的种子表追加两行（放在 `grok` 之后、`opencode` 之前，让 ZCode 紧随原生客户端）：

```rust
            ("zcode_codex", "codex", "ZCode (Codex)"),
            ("zcode_claude", "claude", "ZCode (Claude)"),
```

**每行的 platform 必须与对应适配器的 `platform()` 完全一致。** `config_write_service.rs:685` 的 `validate_adapter_target` 会在每次 prepare 时校验这一点，不一致则报 `config.adapter_target_mismatch` 并拒绝写入——这是防止把配置写到错误文件的保险。**不要修改 `validate_adapter_target`**：本方案之所以选择「一个适配器实例只服务一个平台」，正是为了让这条不变量原封不动地继续成立。

- [ ] **Step 5: 运行测试确认通过**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib && cargo fmt
```

预期：全部 PASS。`clients_for_platform(Codex)` 现在返回两项，Task 1 中断言只有 `codex` 的那个测试会失败——把它改为断言 `vec!["codex", "zcode"]` 并补 `assert!(codex[1].restart_required)`。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/adapters/route_config src-tauri/src/database/repositories/target_repository.rs
git commit -m "feat: 新增 ZCode 配置写入适配器"
```

---

### Task 4: 客户端选择的持久化与解析

写入需要知道「这个平台要写哪些客户端」。存进 settings，并给出「settings 记录 → 无记录则只写 native」的回退。

**Files:**
- Modify: `src-tauri/src/models/settings.rs`
- Modify: `src-tauri/src/services/route_config_service.rs`
- Modify: `src/lib/api/types.ts:729` 附近（`AppSettings`）

**Interfaces:**
- Consumes: Task 1 的 `clients_for_platform`。
- Produces: `AppSettings.config_write_clients_json: Option<String>`；`RouteConfigService::resolve_write_clients(paths, platform, requested: Option<&[String]>) -> Result<Vec<Arc<dyn TargetAdapter>>, AppError>`。

`config_write_clients_json` 的形状：`{"codex":["codex","zcode"],"claude":["claude_code"]}`。按平台分别记录，因为弹窗总在某个平台的上下文中打开。

- [ ] **Step 1: 写失败测试**

追加到 `src-tauri/src/services/route_config_service.rs` 的 `mod tests`：

```rust
    #[tokio::test]
    async fn write_clients_default_to_the_native_cli_when_never_chosen() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::from_data_dir(dir.path().to_path_buf());
        paths.ensure().await.expect("paths");

        let clients = RouteConfigService::resolve_write_clients(&paths, PlatformId::Codex, None)
            .await
            .expect("clients");

        // Unchanged behavior for users who never open the dialog.
        assert_eq!(
            clients
                .iter()
                .map(|adapter| adapter.client_key())
                .collect::<Vec<_>>(),
            vec!["codex"]
        );
    }

    #[tokio::test]
    async fn stored_selection_is_honored_per_platform() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::from_data_dir(dir.path().to_path_buf());
        paths.ensure().await.expect("paths");
        let mut settings =
            AppSettings::defaults_for_data_dir(paths.data_dir.display().to_string());
        settings.config_write_clients_json =
            Some(r#"{"codex":["codex","zcode"],"claude":["claude_code"]}"#.to_string());
        SettingsService::save(&paths, &settings).await.expect("save");

        let codex = RouteConfigService::resolve_write_clients(&paths, PlatformId::Codex, None)
            .await
            .expect("codex clients");
        assert_eq!(
            codex
                .iter()
                .map(|adapter| adapter.target_key())
                .collect::<Vec<_>>(),
            vec!["codex", "zcode_codex"]
        );

        // The stored codex selection must not leak into another platform.
        let claude = RouteConfigService::resolve_write_clients(&paths, PlatformId::Claude, None)
            .await
            .expect("claude clients");
        assert_eq!(
            claude
                .iter()
                .map(|adapter| adapter.target_key())
                .collect::<Vec<_>>(),
            vec!["claude_code"]
        );
    }

    #[tokio::test]
    async fn explicit_request_overrides_storage_and_rejects_unknown_clients() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::from_data_dir(dir.path().to_path_buf());
        paths.ensure().await.expect("paths");

        let requested = vec!["zcode".to_string()];
        let clients =
            RouteConfigService::resolve_write_clients(&paths, PlatformId::Codex, Some(&requested))
                .await
                .expect("clients");
        assert_eq!(
            clients
                .iter()
                .map(|adapter| adapter.target_key())
                .collect::<Vec<_>>(),
            vec!["zcode_codex"]
        );

        // Fail loudly: silently skipping an unknown key would look like a
        // successful write that did nothing.
        let unknown = vec!["not-a-client".to_string()];
        let error =
            RouteConfigService::resolve_write_clients(&paths, PlatformId::Codex, Some(&unknown))
                .await
                .expect_err("must reject");
        assert!(matches!(
            error,
            AppError::Validation {
                code: "config.client_unavailable",
                ..
            }
        ));

        // A client that exists but not for this platform is equally a mismatch.
        let wrong_platform = vec!["claude_code".to_string()];
        assert!(RouteConfigService::resolve_write_clients(
            &paths,
            PlatformId::Codex,
            Some(&wrong_platform)
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn malformed_selection_json_falls_back_to_the_native_cli() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::from_data_dir(dir.path().to_path_buf());
        paths.ensure().await.expect("paths");
        let mut settings =
            AppSettings::defaults_for_data_dir(paths.data_dir.display().to_string());
        settings.config_write_clients_json = Some("{not json".to_string());
        SettingsService::save(&paths, &settings).await.expect("save");

        // A corrupt preference must never be the thing that blocks a write.
        let clients = RouteConfigService::resolve_write_clients(&paths, PlatformId::Codex, None)
            .await
            .expect("clients");
        assert_eq!(
            clients
                .iter()
                .map(|adapter| adapter.client_key())
                .collect::<Vec<_>>(),
            vec!["codex"]
        );
    }

    #[tokio::test]
    async fn empty_stored_selection_falls_back_to_the_native_cli() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::from_data_dir(dir.path().to_path_buf());
        paths.ensure().await.expect("paths");
        let mut settings =
            AppSettings::defaults_for_data_dir(paths.data_dir.display().to_string());
        settings.config_write_clients_json = Some(r#"{"codex":[]}"#.to_string());
        SettingsService::save(&paths, &settings).await.expect("save");

        // An empty list would otherwise write nothing and report success.
        let clients = RouteConfigService::resolve_write_clients(&paths, PlatformId::Codex, None)
            .await
            .expect("clients");
        assert_eq!(
            clients
                .iter()
                .map(|adapter| adapter.client_key())
                .collect::<Vec<_>>(),
            vec!["codex"]
        );
    }
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib route_config_service
```

预期：编译失败，`no field 'config_write_clients_json'`、`no function 'resolve_write_clients'`。

- [ ] **Step 3: 实现**

`models/settings.rs` 的 `AppSettings` 增加字段（`claude_client_config_json` 之后）：

```rust
    /// Which clients each platform writes config for, as
    /// `{"codex":["codex","zcode"]}`. Recorded per platform because the dialog
    /// always opens in one platform's context. Absent or empty means the
    /// platform's native CLI only.
    #[serde(default)]
    pub config_write_clients_json: Option<String>,
```

`AppSettingsView` 同步加 `pub config_write_clients_json: Option<String>`，`from_settings` 与 `defaults_for_data_dir` 各补一行（默认 `None`）。

`route_config_service.rs` 增加：

```rust
    /// Adapters to write for this platform. Explicit `requested` wins; otherwise
    /// the stored per-platform selection; otherwise the platform's native CLI so
    /// callers that predate client selection behave exactly as before.
    pub(crate) async fn resolve_write_clients(
        paths: &AppPaths,
        platform: PlatformId,
        requested: Option<&[String]>,
    ) -> Result<Vec<Arc<dyn TargetAdapter>>, AppError> {
        let keys = match requested {
            Some(keys) if !keys.is_empty() => keys.to_vec(),
            _ => Self::stored_write_client_keys(paths, platform).await,
        };
        if keys.is_empty() {
            return Ok(vec![route_config_adapter(
                &native_client_key(platform)?,
                platform,
            )?]);
        }

        keys.iter()
            .map(|key| route_config_adapter(key, platform))
            .collect()
    }

    /// Stored selection for this platform, or empty when absent, malformed, or
    /// empty. A corrupt preference must never block a write.
    async fn stored_write_client_keys(paths: &AppPaths, platform: PlatformId) -> Vec<String> {
        let Ok(settings) = SettingsService::load(paths).await else {
            return Vec::new();
        };
        let Some(raw) = settings.config_write_clients_json else {
            return Vec::new();
        };
        serde_json::from_str::<Map<String, Value>>(&raw)
            .ok()
            .and_then(|map| map.get(platform.as_str()).cloned())
            .and_then(|value| serde_json::from_value::<Vec<String>>(value).ok())
            .unwrap_or_default()
    }
```

`src/lib/api/types.ts` 的 `AppSettings` 加 `config_write_clients_json?: string | null;`。

- [ ] **Step 4: 运行测试确认通过**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib && cargo fmt
cd .. && pnpm typecheck
```

预期：全部 PASS。若 `settings_service.rs` 的既有测试构造 `AppSettings` 字面量，补 `config_write_clients_json: None`。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/models/settings.rs src-tauri/src/services/route_config_service.rs src/lib/api/types.ts
git commit -m "feat: 按平台记录配置写入的客户端选择"
```

---

### Task 5: 逐客户端写入编排

把 `write_configs_for_home` 从「单适配器一次 write_group」改为「每客户端各一次 write_group，汇总 outcome」。

**Files:**
- Modify: `src-tauri/src/services/route_config_service.rs:55-104`（`write_configs*`）、`208-277`（stale）、`444-485`（Codex 目录）
- Modify: `src-tauri/src/database/repositories/route_proxy_key_repository.rs`
- Modify: `src-tauri/src/error.rs`（新增 `AppError::code()`）

**Interfaces:**
- Consumes: Task 2 的 `resolve_client_models`、Task 3 的 ZCode 适配器与 `route_proxy_key_aliases`、Task 4 的 `resolve_write_clients`。
- Produces: `RouteConfigService::write_configs(paths, pool, runtime, base_url, platform, client_keys: Option<&[String]>)`；`config_write_is_stale(paths, pool, base_url, platform, client_keys: Option<&[String]>)`；`RouteProxyKeyRepository::list_aliases_for_platform(pool, platform) -> Result<Vec<String>, AppError>`；`AppError::code() -> &'static str`。

三条关键行为：
1. **每客户端一次 `write_group`**。`write_group` 全组原子，若同组则 ZCode 配置损坏会连带让 Codex 写不了——对最常见场景的可用性倒退。
2. **新建的 sk 只在全部客户端都失败时回删**。否则 codex 成功而 zcode 失败会打断已生效的 codex。
3. **Codex 模型目录只在选中 `codex` 客户端时才写**，只勾 ZCode 时不碰 `~/.codex/`。

- [ ] **Step 1: 写失败测试**

追加到 `src-tauri/src/services/route_config_service.rs` 的 `mod tests`（沿用文件内既有的 `seed_claude_pool_member` 风格辅助函数；codex 侧需要一个在池的 api 账号才有模型可写）：

```rust
    #[tokio::test]
    async fn zcode_only_write_leaves_the_codex_cli_files_untouched() {
        let fixture = ServiceFixture::new().await;
        seed_codex_pool_member(&fixture.pool, "gpt-5.6-sol").await;

        let outcomes = RouteConfigService::write_configs_for_home(
            &fixture.paths,
            &fixture.pool,
            &fixture.runtime,
            BASE_URL,
            "codex",
            &fixture.home,
            Some(&["zcode".to_string()]),
        )
        .await
        .expect("write");

        assert_eq!(
            outcomes.iter().map(|o| o.target_key.as_str()).collect::<Vec<_>>(),
            vec!["zcode_codex"]
        );
        assert!(fixture.home.join(".zcode/v2/config.json").exists());
        // Neither the CLI config nor the catalog it references belongs to this write.
        assert!(!fixture.home.join(".codex/config.toml").exists());
        assert!(!fixture
            .home
            .join(".codex/ai-switch-model-catalog.json")
            .exists());
    }

    #[tokio::test]
    async fn a_corrupt_zcode_config_does_not_block_the_codex_cli_write() {
        let fixture = ServiceFixture::new().await;
        seed_codex_pool_member(&fixture.pool, "gpt-5.6-sol").await;
        let zcode_path = fixture.home.join(".zcode/v2/config.json");
        tokio::fs::create_dir_all(zcode_path.parent().unwrap())
            .await
            .expect("dir");
        tokio::fs::write(&zcode_path, b"{not json")
            .await
            .expect("corrupt");

        let outcomes = RouteConfigService::write_configs_for_home(
            &fixture.paths,
            &fixture.pool,
            &fixture.runtime,
            BASE_URL,
            "codex",
            &fixture.home,
            Some(&["codex".to_string(), "zcode".to_string()]),
        )
        .await
        .expect("partial success is not an error");

        // Independent client configs are not one transaction: one broken file
        // must not cost the user their working Codex setup.
        let codex = outcomes.iter().find(|o| o.target_key == "codex").expect("codex");
        assert_eq!(codex.status, "succeeded");
        let zcode = outcomes
            .iter()
            .find(|o| o.target_key == "zcode_codex")
            .expect("zcode");
        assert_ne!(zcode.status, "succeeded");
        assert!(fixture.home.join(".codex/config.toml").exists());
        // Refused, not overwritten.
        assert_eq!(
            tokio::fs::read(&zcode_path).await.expect("read"),
            b"{not json"
        );

        // The key backs a write that did land, so it must survive.
        assert_eq!(
            RouteProxyKeyRepository::get_existing_platform_key(&fixture.pool, "codex")
                .await
                .expect("key"),
            Some(codex_written_key(&fixture.home).await)
        );
    }

    #[tokio::test]
    async fn a_new_key_is_removed_only_when_every_client_fails() {
        let fixture = ServiceFixture::new().await;
        seed_codex_pool_member(&fixture.pool, "gpt-5.6-sol").await;
        let zcode_path = fixture.home.join(".zcode/v2/config.json");
        tokio::fs::create_dir_all(zcode_path.parent().unwrap())
            .await
            .expect("dir");
        tokio::fs::write(&zcode_path, b"{not json")
            .await
            .expect("corrupt");

        let error = RouteConfigService::write_configs_for_home(
            &fixture.paths,
            &fixture.pool,
            &fixture.runtime,
            BASE_URL,
            "codex",
            &fixture.home,
            Some(&["zcode".to_string()]),
        )
        .await
        .expect_err("only client failed");

        assert!(matches!(error, AppError::Validation { .. } | AppError::Filesystem { .. }));
        // Nothing was written, so the key we minted has no reason to exist.
        assert!(
            RouteProxyKeyRepository::get_existing_platform_key(&fixture.pool, "codex")
                .await
                .expect("key")
                .is_none()
        );
    }

    #[tokio::test]
    async fn writing_zcode_without_pool_models_is_refused() {
        let fixture = ServiceFixture::new().await;
        // No pool member: nothing to advertise.

        let error = RouteConfigService::write_configs_for_home(
            &fixture.paths,
            &fixture.pool,
            &fixture.runtime,
            BASE_URL,
            "codex",
            &fixture.home,
            Some(&["zcode".to_string()]),
        )
        .await
        .expect_err("must refuse");

        // A models-less provider is unselectable in ZCode, so writing one would
        // look like success and behave like a dead entry.
        assert!(matches!(
            error,
            AppError::Validation {
                code: "config.pool_models_empty",
                ..
            }
        ));
        assert!(!fixture.home.join(".zcode/v2/config.json").exists());
    }

    #[tokio::test]
    async fn zcode_write_carries_the_pool_models_and_rotated_key_aliases() {
        let fixture = ServiceFixture::new().await;
        seed_codex_pool_member(&fixture.pool, "gpt-5.6-sol").await;

        RouteConfigService::write_configs_for_home(
            &fixture.paths,
            &fixture.pool,
            &fixture.runtime,
            BASE_URL,
            "codex",
            &fixture.home,
            Some(&["zcode".to_string()]),
        )
        .await
        .expect("write");

        let raw = tokio::fs::read(fixture.home.join(".zcode/v2/config.json"))
            .await
            .expect("read");
        let json: Value = serde_json::from_slice(&raw).expect("json");
        let entry = &json["provider"]["ai-switch-codex"];
        assert_eq!(entry["models"]["gpt-5.6-sol"]["limit"]["output"], 128000);
        assert_eq!(entry["aiSwitch"]["platform"], "codex");
    }

    #[tokio::test]
    async fn stale_check_covers_every_selected_client() {
        let fixture = ServiceFixture::new().await;
        seed_codex_pool_member(&fixture.pool, "gpt-5.6-sol").await;
        let clients = vec!["codex".to_string(), "zcode".to_string()];

        RouteConfigService::write_configs_for_home(
            &fixture.paths,
            &fixture.pool,
            &fixture.runtime,
            BASE_URL,
            "codex",
            &fixture.home,
            Some(&clients),
        )
        .await
        .expect("write");

        assert!(
            !RouteConfigService::config_write_is_stale_for_home(
                &fixture.paths,
                &fixture.pool,
                BASE_URL,
                "codex",
                &fixture.home,
                Some(&clients),
            )
            .await
        );

        // A ZCode-only drift still has to surface on the shared nudge.
        tokio::fs::remove_file(fixture.home.join(".zcode/v2/config.json"))
            .await
            .expect("remove");
        assert!(
            RouteConfigService::config_write_is_stale_for_home(
                &fixture.paths,
                &fixture.pool,
                BASE_URL,
                "codex",
                &fixture.home,
                Some(&clients),
            )
            .await
        );

        // Narrowing the selection back to the intact client clears the nudge.
        assert!(
            !RouteConfigService::config_write_is_stale_for_home(
                &fixture.paths,
                &fixture.pool,
                BASE_URL,
                "codex",
                &fixture.home,
                Some(&["codex".to_string()]),
            )
            .await
        );
    }

    #[tokio::test]
    async fn a_pool_mapping_change_marks_the_zcode_config_stale() {
        let fixture = ServiceFixture::new().await;
        seed_codex_pool_member(&fixture.pool, "gpt-5.6-sol").await;
        let clients = vec!["zcode".to_string()];

        RouteConfigService::write_configs_for_home(
            &fixture.paths,
            &fixture.pool,
            &fixture.runtime,
            BASE_URL,
            "codex",
            &fixture.home,
            Some(&clients),
        )
        .await
        .expect("write");

        assert!(
            !RouteConfigService::config_write_is_stale_for_home(
                &fixture.paths,
                &fixture.pool,
                BASE_URL,
                "codex",
                &fixture.home,
                Some(&clients),
            )
            .await
        );

        // ZCode's model list lives in the config file, so a pool mapping change
        // leaves the file advertising a stale set until the user writes again.
        seed_codex_pool_member(&fixture.pool, "gpt-5.6-terra").await;
        assert!(
            RouteConfigService::config_write_is_stale_for_home(
                &fixture.paths,
                &fixture.pool,
                BASE_URL,
                "codex",
                &fixture.home,
                Some(&clients),
            )
            .await
        );
    }
```

辅助函数与 fixture（放在 `mod tests` 内。`route_config_service.rs` 的既有测试是逐个内联 setup 的，没有共享 fixture，所以这里新建一个——本任务新增六个测试都需要同一套 paths/pool/runtime/home）：

```rust
    struct ServiceFixture {
        _temp: tempfile::TempDir,
        paths: AppPaths,
        pool: SqlitePool,
        runtime: ConfigWriteRuntimeState,
        home: PathBuf,
    }

    impl ServiceFixture {
        async fn new() -> Self {
            let temp = tempfile::tempdir().expect("temp dir");
            let paths = AppPaths::from_data_dir(temp.path().join("app-data"));
            paths.ensure().await.expect("paths");
            let pool = create_memory_pool().await.expect("pool");
            run_migrations(&pool).await.expect("migrations");
            let home = temp.path().join("home");
            tokio::fs::create_dir_all(&home).await.expect("home");

            Self {
                _temp: temp,
                paths,
                pool,
                runtime: ConfigWriteRuntimeState::default(),
                home,
            }
        }
    }

    /// An in-pool api credential mapping one model to itself, which is the
    /// minimum for the pool to advertise anything.
    async fn seed_codex_pool_member(pool: &SqlitePool, model: &str) {
        let credential_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let config_json = serde_json::json!({
            "model_mappings": [{ "from": model, "to": model }]
        })
        .to_string();
        sqlx::query(
            "INSERT INTO route_credentials (id, platform, kind, display_name, secret_payload_json, config_json, preview_json, created_at, updated_at)
             VALUES (?, 'codex', 'api', 'seed', '{}', ?, '{}', ?, ?)",
        )
        .bind(&credential_id)
        .bind(&config_json)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .expect("insert credential");
        sqlx::query(
            "INSERT INTO route_pool_members (id, platform, route_credential_id, created_at, updated_at)
             VALUES (?, 'codex', ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&credential_id)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .expect("insert pool member");
    }

    /// The sk the Codex CLI config was written with, read back from disk.
    async fn codex_written_key(home: &Path) -> String {
        let raw = tokio::fs::read_to_string(home.join(".codex/config.toml"))
            .await
            .expect("read config.toml");
        raw.lines()
            .find_map(|line| line.trim().strip_prefix("experimental_bearer_token = "))
            .map(|value| value.trim().trim_matches('"').to_string())
            .expect("bearer token")
    }
```

`mod tests` 的 `use` 需补 `chrono::Utc`、`uuid::Uuid`、`std::path::PathBuf`（若尚未导入）。本任务新增测试中出现的 `BASE_URL` 常量若文件内没有，加 `const BASE_URL: &str = "http://127.0.0.1:43111";`。

本任务还要更新既有测试的调用点：文件内所有 `write_configs_for_home(...)` 与 `config_write_is_stale_for_home(...)` 调用末尾补 `None`（保持只写 native 客户端的既有行为）。

- [ ] **Step 2: 运行测试确认失败**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib route_config_service
```

预期：编译失败，`write_configs_for_home` 参数数量不匹配。

- [ ] **Step 3: 重写 write_configs_for_home**

```rust
    pub async fn write_configs(
        paths: &AppPaths,
        pool: &SqlitePool,
        runtime: &ConfigWriteRuntimeState,
        base_url: &str,
        platform: &str,
        client_keys: Option<&[String]>,
    ) -> Result<Vec<ConfigWriteOutcome>, AppError> {
        let home = resolve_home_dir()?;
        Self::write_configs_for_home(paths, pool, runtime, base_url, platform, &home, client_keys)
            .await
    }

    pub(crate) async fn write_configs_for_home(
        paths: &AppPaths,
        pool: &SqlitePool,
        runtime: &ConfigWriteRuntimeState,
        base_url: &str,
        platform: &str,
        home: &Path,
        client_keys: Option<&[String]>,
    ) -> Result<Vec<ConfigWriteOutcome>, AppError> {
        let base_url = normalize_base_url(base_url)?;
        let platform = PlatformId::parse(platform)?;
        PlatformCapabilityService::require(platform, PlatformOperation::ConfigWrite)?;
        let adapters = Self::resolve_write_clients(paths, platform, client_keys).await?;
        let platform_key = platform.as_str();

        let existing_route_proxy_key =
            RouteProxyKeyRepository::get_existing_platform_key(pool, platform_key).await?;
        let route_proxy_key = RouteProxyKeyRepository::ensure_platform_key(
            pool,
            platform_key,
            &generate_route_proxy_key(),
        )
        .await?;

        let claude_env = Self::resolve_claude_env_plan(paths, pool, platform).await?;
        let needs_models = adapters
            .iter()
            .any(|adapter| adapter.requires_client_models());
        let client_models = if needs_models {
            let models = Self::resolve_client_models(pool, platform).await?;
            if models.is_empty() {
                // A models-less provider is unselectable in ZCode, so writing one
                // would report success and leave a dead entry.
                if existing_route_proxy_key.is_none() {
                    let _ = RouteProxyKeyRepository::delete_if_matches(
                        pool,
                        platform_key,
                        &route_proxy_key,
                    )
                    .await;
                }
                return Err(AppError::Validation {
                    code: "config.pool_models_empty",
                    message: "The pool advertises no models for this platform".to_string(),
                    details: Some(platform_key.to_string()),
                    recoverable: true,
                });
            }
            models
        } else {
            Vec::new()
        };

        // The Codex CLI's config.toml references this catalog, so it belongs to
        // that client and not to the platform.
        if adapters.iter().any(|adapter| adapter.client_key() == "codex") {
            Self::write_codex_model_catalog(pool, home).await?;
        }

        let aliases = RouteProxyKeyRepository::list_aliases_for_platform(pool, platform_key).await?;

        // One group per client: `write_group` aborts the whole group when any
        // prepare fails, so grouping them would let a corrupt ZCode config cost
        // the user their working CLI write.
        let mut outcomes = Vec::new();
        let mut last_error = None;
        let mut any_succeeded = false;
        for adapter in adapters {
            let target_key = adapter.target_key();
            let request = ConfigWriteRequest {
                adapter,
                home: home.to_path_buf(),
                input: RouteConfigInput {
                    base_url: base_url.to_string(),
                    route_proxy_key: route_proxy_key.clone(),
                    route_proxy_key_aliases: aliases.clone(),
                    claude_env: claude_env.clone(),
                    client_models: client_models.clone(),
                },
            };
            match ConfigWriteCoordinator::write_group(paths, pool, runtime, vec![request]).await {
                Ok(group) => {
                    any_succeeded |= group.iter().any(|outcome| outcome.status == "succeeded");
                    outcomes.extend(group);
                }
                Err(error) => {
                    // A failed group produces no outcome row, so the result panel
                    // would silently omit this client.
                    outcomes.push(failed_client_outcome(target_key, platform_key, &error));
                    last_error = Some(error);
                }
            }
        }

        if !any_succeeded {
            if existing_route_proxy_key.is_none() {
                let _ =
                    RouteProxyKeyRepository::delete_if_matches(pool, platform_key, &route_proxy_key)
                        .await;
            }
            if let Some(error) = last_error {
                return Err(error);
            }
        }

        Ok(outcomes)
    }
```

`ClaudeEnvPlan` 需要 `Clone`（已有 `#[derive(Debug, Clone, Default, PartialEq, Eq)]`，无需改动）。

同文件顶层加入 `failed_client_outcome`（`skipped_outcome` 旁）：

```rust
/// A client whose group write errored has no outcome row of its own, so the
/// result panel would silently omit it.
fn failed_client_outcome(target_key: &str, platform: &str, error: &AppError) -> ConfigWriteOutcome {
    ConfigWriteOutcome {
        operation_id: String::new(),
        snapshot_id: None,
        target_app_id: None,
        target_key: target_key.to_string(),
        platform: platform.to_string(),
        path: String::new(),
        status: "failed".to_string(),
        before_hash: None,
        after_hash: None,
        error_code: Some(error.code().to_string()),
    }
}
```

`AppError` 目前没有 `code()` 访问器——错误码只在 `From<AppError> for ApiError` 的 match 里被取出。在 `src-tauri/src/error.rs` 补一个；四个变体的 `code` 字段类型一致，所以是纯读取：

```rust
impl AppError {
    /// The stable error code. Mirrors what `ApiError` surfaces, for call sites
    /// that need the code without converting the whole error.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation { code, .. }
            | Self::Filesystem { code, .. }
            | Self::Database { code, .. }
            | Self::Secret { code, .. } => code,
        }
    }
}
```

- [ ] **Step 4: 新增别名查询**

`src-tauri/src/database/repositories/route_proxy_key_repository.rs` 增加：

```rust
    /// Keys this platform used before rotation. Adoption uses them to recognize
    /// a client entry the user wired up with an older key.
    pub async fn list_aliases_for_platform(
        pool: &SqlitePool,
        platform: &str,
    ) -> Result<Vec<String>, AppError> {
        let rows = sqlx::query(
            "SELECT proxy_key FROM route_proxy_key_aliases WHERE platform = ? ORDER BY created_at DESC",
        )
        .bind(platform)
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_proxy_key_alias_list",
            message: "Could not load route proxy key aliases".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(rows
            .into_iter()
            .map(|row| row.get::<String, _>("proxy_key"))
            .collect())
    }
```

- [ ] **Step 5: 改造 stale 检查**

`config_write_is_stale` / `config_write_is_stale_for_home` 各加 `client_keys: Option<&[String]>`，`rendered_config_differs` 改为遍历客户端：

```rust
    async fn rendered_config_differs(
        paths: &AppPaths,
        pool: &SqlitePool,
        base_url: &str,
        platform: &str,
        home: &Path,
        client_keys: Option<&[String]>,
    ) -> Result<bool, AppError> {
        let base_url = normalize_base_url(base_url)?;
        let platform = PlatformId::parse(platform)?;
        PlatformCapabilityService::require(platform, PlatformOperation::ConfigWrite)?;

        let Some(route_proxy_key) =
            RouteProxyKeyRepository::get_existing_platform_key(pool, platform.as_str()).await?
        else {
            return Ok(false);
        };

        let adapters = Self::resolve_write_clients(paths, platform, client_keys).await?;
        let claude_env = Self::resolve_claude_env_plan(paths, pool, platform).await?;
        let needs_models = adapters
            .iter()
            .any(|adapter| adapter.requires_client_models());
        let client_models = if needs_models {
            Self::resolve_client_models(pool, platform).await?
        } else {
            Vec::new()
        };
        let aliases =
            RouteProxyKeyRepository::list_aliases_for_platform(pool, platform.as_str()).await?;

        for adapter in adapters {
            let path = adapter.resolve_path(home);
            let Some(existing) = tokio::fs::read(&path).await.ok() else {
                // A file we manage is gone; writing would recreate it.
                return Ok(true);
            };
            let input = RouteConfigInput {
                base_url: base_url.to_string(),
                route_proxy_key: route_proxy_key.clone(),
                route_proxy_key_aliases: aliases.clone(),
                claude_env: claude_env.clone(),
                client_models: client_models.clone(),
            };
            // One client's render error must not hide another client's real drift.
            let Ok(rendered) = adapter.render(&path, Some(&existing), &input) else {
                continue;
            };
            if config_content_differs(&existing, &rendered) {
                return Ok(true);
            }
        }

        Ok(false)
    }
```

- [ ] **Step 6: 运行测试确认通过**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib && cargo fmt
```

预期：全部 PASS。`write_existing_config_for_home`（`:279`）与 `write_existing_configs_for_home`（`:117`）保持只写 native 客户端，其 `RouteConfigInput` 补 `route_proxy_key_aliases: Vec::new()` 与 `client_models: Vec::new()`。

- [ ] **Step 7: 提交**

```bash
git add src-tauri/src/services/route_config_service.rs src-tauri/src/database/repositories/route_proxy_key_repository.rs src-tauri/src/error.rs
git commit -m "feat: 配置写入按客户端逐个执行并汇总结果"
```

---

### Task 6: 命令层与两个传输层

**Files:**
- Modify: `src-tauri/src/services/target_service.rs`
- Modify: `src-tauri/src/commands/target_commands.rs`、`route_proxy_commands.rs:44-106`
- Modify: `src-tauri/src/lib.rs:471-489`
- Modify: `src-tauri/src/web/handlers/mod.rs:291-300, 740-800`
- Modify: `src/lib/api/client.ts:254-269`、`src/lib/api/types.ts`

**Interfaces:**
- Consumes: Task 1 的 `clients_for_platform`、Task 5 的新签名。
- Produces: 命令 `list_config_write_clients(platform) -> Vec<ConfigWriteClientStatus>`；`ConfigWriteClientStatus { client_key, display_name, native, restart_required, target_key, platform, config_path: Option<String>, file_status: String, error_code: Option<String> }`；TS `listConfigWriteClients(platform)`、`writeRouteProxyConfigs(baseUrl, platform, clientKeys)`、`routeConfigWriteIsStale(baseUrl, platform, clientKeys)`。

不复用 `listTargetConfigStatuses`：它跨全平台枚举、要跑 reconcile 与快照统计，且不带 `client_key` 与 `native`。

- [ ] **Step 1: 写失败测试**

追加到 `src-tauri/src/services/target_service.rs` 的 `mod tests`：

```rust
    #[tokio::test]
    async fn config_write_clients_report_per_client_file_status() {
        let fixture = TargetFixture::new().await;
        let zcode_path = fixture.home.join(".zcode/v2/config.json");
        tokio::fs::create_dir_all(zcode_path.parent().unwrap())
            .await
            .expect("dir");
        tokio::fs::write(
            &zcode_path,
            br#"{"provider":{"builtin:bigmodel":{"kind":"anthropic"}}}"#,
        )
        .await
        .expect("write");

        let clients = TargetService::list_config_write_clients_for_home(
            &fixture.pool,
            PlatformId::Codex,
            &fixture.home,
        )
        .await
        .expect("clients");

        assert_eq!(
            clients
                .iter()
                .map(|client| client.client_key.as_str())
                .collect::<Vec<_>>(),
            vec!["codex", "zcode"]
        );

        let codex = &clients[0];
        assert!(codex.native);
        assert!(!codex.restart_required);
        assert_eq!(codex.file_status, "missing");
        assert!(codex
            .config_path
            .as_deref()
            .expect("path")
            .ends_with("config.toml"));

        let zcode = &clients[1];
        assert!(!zcode.native);
        // ZCode has no file watcher, so the UI has to tell the user to restart.
        assert!(zcode.restart_required);
        // The file exists but carries no ai-switch entry for this platform.
        assert_eq!(zcode.file_status, "unmanaged");
        assert_eq!(zcode.target_key, "zcode_codex");
    }

    #[tokio::test]
    async fn config_write_clients_surface_a_corrupt_file_without_erroring() {
        let fixture = TargetFixture::new().await;
        let zcode_path = fixture.home.join(".zcode/v2/config.json");
        tokio::fs::create_dir_all(zcode_path.parent().unwrap())
            .await
            .expect("dir");
        tokio::fs::write(&zcode_path, b"{not json").await.expect("write");

        let clients = TargetService::list_config_write_clients_for_home(
            &fixture.pool,
            PlatformId::Codex,
            &fixture.home,
        )
        .await
        .expect("listing must not fail on a bad file");

        let zcode = clients
            .iter()
            .find(|client| client.client_key == "zcode")
            .expect("zcode");
        assert_eq!(zcode.file_status, "invalid");
        assert_eq!(
            zcode.error_code.as_deref(),
            Some("validation.route_config_existing_invalid")
        );
    }
```

`TargetFixture`（放在 `mod tests` 内。`target_service.rs` 的既有测试逐个内联 setup，无共享 fixture，故新建）：

```rust
    struct TargetFixture {
        _temp: tempfile::TempDir,
        pool: SqlitePool,
        home: PathBuf,
    }

    impl TargetFixture {
        async fn new() -> Self {
            let temp = tempfile::tempdir().expect("temp dir");
            let pool = create_memory_pool().await.expect("pool");
            run_migrations(&pool).await.expect("migrations");
            TargetRepository::ensure_defaults(&pool)
                .await
                .expect("targets");
            let home = temp.path().join("home");
            tokio::fs::create_dir_all(&home).await.expect("home");

            Self {
                _temp: temp,
                pool,
                home,
            }
        }
    }
```

`mod tests` 的 `use` 需补 `crate::database::{create_memory_pool, run_migrations}`、`std::path::PathBuf`（若尚未导入）。

- [ ] **Step 2: 运行测试确认失败**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib target_service
```

预期：编译失败，`no function 'list_config_write_clients_for_home'`。

- [ ] **Step 3: 实现服务与模型**

`src-tauri/src/models/target_app.rs` 追加：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfigWriteClientStatus {
    pub client_key: String,
    pub display_name: String,
    pub native: bool,
    pub restart_required: bool,
    pub target_key: String,
    pub platform: String,
    pub config_path: Option<String>,
    pub file_status: String,
    pub error_code: Option<String>,
}
```

`target_service.rs` 追加：

```rust
    pub async fn list_config_write_clients(
        pool: &SqlitePool,
        platform: PlatformId,
    ) -> Result<Vec<ConfigWriteClientStatus>, AppError> {
        let home = BaseDirs::new()
            .map(|dirs| dirs.home_dir().to_path_buf())
            .ok_or_else(|| AppError::Filesystem {
                code: "filesystem.home_not_found",
                message: "Could not resolve the current user home directory".to_string(),
                details: None,
                recoverable: false,
            })?;
        Self::list_config_write_clients_for_home(pool, platform, &home).await
    }

    /// Clients this platform can write config for, with each one's current file
    /// state. Deliberately narrower than `list_config_statuses`: no reconcile, no
    /// snapshot counts, and it carries the client identity the dialog needs.
    pub(crate) async fn list_config_write_clients_for_home(
        pool: &SqlitePool,
        platform: PlatformId,
        home: &Path,
    ) -> Result<Vec<ConfigWriteClientStatus>, AppError> {
        TargetRepository::ensure_defaults(pool).await?;
        let registry = TargetAdapterRegistry::new();
        let mut statuses = Vec::new();

        for client in registry.clients_for_platform(platform) {
            let Some(adapter) = registry.by_client_and_platform(&client.client_key, platform) else {
                continue;
            };
            let path = adapter.resolve_path(home);
            let inspection = match ConfigWriter::inspect(&path).await {
                Ok(file) => adapter.inspect(&path, file.bytes.as_deref()),
                Err(_) => crate::adapters::route_config::TargetInspection {
                    file_status: "error".to_string(),
                    managed: false,
                    error_code: None,
                },
            };
            statuses.push(ConfigWriteClientStatus {
                client_key: client.client_key,
                display_name: client.display_name,
                native: client.native,
                restart_required: client.restart_required,
                target_key: client.target_key,
                platform: platform.as_str().to_string(),
                config_path: Some(path.display().to_string()),
                file_status: inspection.file_status,
                error_code: inspection.error_code,
            });
        }

        Ok(statuses)
    }
```

- [ ] **Step 4: 接线命令与两个传输层**

`commands/target_commands.rs` 追加：

```rust
#[tauri::command]
pub async fn list_config_write_clients(
    state: State<'_, AppState>,
    platform: String,
) -> Result<Vec<ConfigWriteClientStatus>, ApiError> {
    let platform = PlatformId::parse(&platform).map_err(ApiError::from)?;
    TargetService::list_config_write_clients(&state.pool, platform)
        .await
        .map_err(ApiError::from)
}
```

`commands/route_proxy_commands.rs` 的两个命令各加参数并下传：

```rust
pub async fn write_route_proxy_configs(
    state: State<'_, AppState>,
    base_url: Option<String>,
    platform: String,
    client_keys: Option<Vec<String>>,
) -> Result<Vec<ConfigWriteOutcome>, ApiError> {
```
调用处改为 `RouteConfigService::write_configs(&state.paths, &state.pool, &state.config_writes, &resolved, &platform, client_keys.as_deref())`。`route_config_write_is_stale` 同样加 `client_keys: Option<Vec<String>>` 并下传。

`lib.rs` 在 `list_target_config_statuses` 之后加一行 `list_config_write_clients,`，并在文件顶部的 `use crate::commands::target_commands::{...}` 中补上该名字。

`web/handlers/mod.rs` 在 `"list_target_config_statuses"` 之后加：

```rust
        "list_config_write_clients" => {
            let platform = required_string_arg(&args, "platform")?;
            let platform = PlatformId::parse(&platform).map_err(to_error)?;
            to_value(
                TargetService::list_config_write_clients(&state.pool, platform)
                    .await
                    .map_err(to_error)?,
            )
        }
```

`"write_route_proxy_configs"`（:740）与 `"route_config_write_is_stale"`（:774）两个分支各解析可选字符串数组参数：

```rust
            let client_keys = optional_string_array_arg(&args, "clientKeys")?;
```
若 `optional_string_array_arg` 不存在（当前确实没有），在 `optional_string_arg`（`:904`）旁新增，沿用同文件的 `invalid_argument` 构造：

```rust
fn optional_string_array_arg(args: &Value, key: &str) -> Result<Option<Vec<String>>, ApiError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(|text| text.trim().to_string())
                    .filter(|text| !text.is_empty())
                    .ok_or_else(|| {
                        invalid_argument(key, Some("expected array of strings".to_string()))
                    })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        Some(_) => Err(invalid_argument(
            key,
            Some("expected array of strings".to_string()),
        )),
    }
}
```

`src/lib/api/client.ts`：

```ts
export function writeRouteProxyConfigs(
  baseUrl: string | null | undefined,
  platform: string,
  clientKeys?: string[] | null,
): Promise<ConfigWriteOutcome[]> {
  return invoke("write_route_proxy_configs", {
    baseUrl: baseUrl ?? null,
    platform,
    clientKeys: clientKeys ?? null,
  });
}

export function routeConfigWriteIsStale(
  baseUrl: string | null | undefined,
  platform: string,
  clientKeys?: string[] | null,
): Promise<boolean> {
  return invoke("route_config_write_is_stale", {
    baseUrl: baseUrl ?? null,
    platform,
    clientKeys: clientKeys ?? null,
  });
}

export function listConfigWriteClients(platform: string): Promise<ConfigWriteClientStatus[]> {
  return invoke("list_config_write_clients", { platform });
}
```

`src/lib/api/types.ts` 追加：

```ts
export type ConfigWriteClientStatus = {
  client_key: string;
  display_name: string;
  native: boolean;
  restart_required: boolean;
  target_key: string;
  platform: string;
  config_path?: string | null;
  file_status: string;
  error_code?: string | null;
};
```

- [ ] **Step 5: 运行测试确认通过**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib && cargo fmt
cd .. && pnpm typecheck && pnpm test:run tests/transport/command-contract.test.ts
```

预期：全部 PASS。契约测试验证 `list_config_write_clients` 在三处均已注册。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/models/target_app.rs src-tauri/src/services/target_service.rs src-tauri/src/commands src-tauri/src/lib.rs src-tauri/src/web/handlers/mod.rs src/lib/api
git commit -m "feat: 新增可写客户端列表命令并支持按客户端写入"
```

---

### Task 7: 客户端多选弹窗

**Files:**
- Create: `src/components/accounts/ConfigWriteTargetsDialog.tsx`
- Create: `tests/ConfigWriteTargetsDialog.test.tsx`

**Interfaces:**
- Consumes: Task 6 的 `ConfigWriteClientStatus`。
- Produces: `ConfigWriteTargetsDialog` 组件，props 为 `{ platform, clients, initialSelection, capabilityDisabledReason, loading, error, onClose, onSubmit }`；`onSubmit(clientKeys: string[])`。

`initialSelection` 为 `null` 表示用户从未选过 → 只勾 `native`。文件状态的中文映射：`missing` → 未建立、`managed` → 已接管、`unmanaged` → 未接管、`invalid` → 无法解析、`error` → 无法读取。

- [ ] **Step 1: 写失败测试**

新建 `tests/ConfigWriteTargetsDialog.test.tsx`：

```tsx
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ConfigWriteTargetsDialog } from "../src/components/accounts/ConfigWriteTargetsDialog";
import type { ConfigWriteClientStatus } from "../src/lib/api/types";

const clients: ConfigWriteClientStatus[] = [
  {
    client_key: "codex",
    display_name: "Codex CLI",
    native: true,
    restart_required: false,
    target_key: "codex",
    platform: "codex",
    config_path: "/home/u/.codex/config.toml",
    file_status: "managed",
    error_code: null,
  },
  {
    client_key: "zcode",
    display_name: "ZCode",
    native: false,
    restart_required: true,
    target_key: "zcode_codex",
    platform: "codex",
    config_path: "/home/u/.zcode/v2/config.json",
    file_status: "unmanaged",
    error_code: null,
  },
];

function setup(overrides: Partial<React.ComponentProps<typeof ConfigWriteTargetsDialog>> = {}) {
  const onSubmit = vi.fn();
  const onClose = vi.fn();
  render(
    <ConfigWriteTargetsDialog
      clients={clients}
      error={null}
      initialSelection={null}
      loading={false}
      onClose={onClose}
      onSubmit={onSubmit}
      platform="codex"
      {...overrides}
    />,
  );
  return { onSubmit, onClose };
}

describe("ConfigWriteTargetsDialog", () => {
  it("lists every client with its file status", () => {
    setup();

    expect(screen.getByRole("checkbox", { name: /Codex CLI/ })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: /ZCode/ })).toBeInTheDocument();
    expect(screen.getByText("已接管")).toBeInTheDocument();
    expect(screen.getByText("未接管")).toBeInTheDocument();
    expect(screen.getByText("/home/u/.zcode/v2/config.json")).toBeInTheDocument();
  });

  it("checks only the native client when the user has never chosen", () => {
    setup();

    // Preserves today's behavior for users who never open the dialog.
    expect(screen.getByRole("checkbox", { name: /Codex CLI/ })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: /ZCode/ })).not.toBeChecked();
  });

  it("restores a stored selection", () => {
    setup({ initialSelection: ["zcode"] });

    expect(screen.getByRole("checkbox", { name: /Codex CLI/ })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: /ZCode/ })).toBeChecked();
  });

  it("shows the restart notice only while a restart-required client is checked", async () => {
    const user = userEvent.setup();
    setup();

    expect(screen.queryByText(/需重启 ZCode/)).not.toBeInTheDocument();

    await user.click(screen.getByRole("checkbox", { name: /ZCode/ }));
    expect(screen.getByText(/需重启 ZCode/)).toBeInTheDocument();

    await user.click(screen.getByRole("checkbox", { name: /ZCode/ }));
    expect(screen.queryByText(/需重启 ZCode/)).not.toBeInTheDocument();
  });

  it("submits the checked client keys", async () => {
    const user = userEvent.setup();
    const { onSubmit } = setup();

    await user.click(screen.getByRole("checkbox", { name: /ZCode/ }));
    await user.click(screen.getByRole("button", { name: "写入" }));

    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith(["codex", "zcode"]));
  });

  it("refuses to submit with nothing checked", async () => {
    const user = userEvent.setup();
    const { onSubmit } = setup();

    await user.click(screen.getByRole("checkbox", { name: /Codex CLI/ }));
    // An empty write would report success and do nothing.
    expect(screen.getByRole("button", { name: "写入" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "写入" }));
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("disables every row and the submit button when the platform cannot write config", () => {
    setup({ capabilityDisabledReason: "该平台的原生配置写入尚未实现。" });

    expect(screen.getByRole("checkbox", { name: /Codex CLI/ })).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: /ZCode/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: "写入" })).toBeDisabled();
    expect(screen.getByText("该平台的原生配置写入尚未实现。")).toBeInTheDocument();
  });

  it("closes on Escape when not writing", async () => {
    const user = userEvent.setup();
    const { onClose } = setup();

    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalled();
  });

  it("stays open on Escape while a write is in flight", async () => {
    const user = userEvent.setup();
    const { onClose } = setup({ loading: true });

    await user.keyboard("{Escape}");
    expect(onClose).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: 运行测试确认失败**

```bash
pnpm test:run tests/ConfigWriteTargetsDialog.test.tsx
```

预期：FAIL，`Failed to resolve import ".../ConfigWriteTargetsDialog"`。

- [ ] **Step 3: 实现组件**

新建 `src/components/accounts/ConfigWriteTargetsDialog.tsx`：

```tsx
import { X } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent } from "react";
import type { ConfigWriteClientStatus } from "../../lib/api/types";

const fileStatusLabels: Record<string, string> = {
  missing: "未建立",
  managed: "已接管",
  unmanaged: "未接管",
  invalid: "无法解析",
  error: "无法读取",
};

type ConfigWriteTargetsDialogProps = {
  platform: string;
  clients: ConfigWriteClientStatus[];
  /** `null` means the user has never chosen, so only the native client is checked. */
  initialSelection: string[] | null;
  capabilityDisabledReason?: string;
  loading: boolean;
  error: string | null;
  onClose: () => void;
  onSubmit: (clientKeys: string[]) => void;
};

export function ConfigWriteTargetsDialog({
  clients,
  initialSelection,
  capabilityDisabledReason,
  loading,
  error,
  onClose,
  onSubmit,
}: ConfigWriteTargetsDialogProps) {
  const [selected, setSelected] = useState<string[]>(() =>
    initialSelection ??
    clients.filter((client) => client.native).map((client) => client.client_key),
  );
  const dialogRef = useRef<HTMLDivElement>(null);
  const closeRef = useRef(onClose);
  const loadingRef = useRef(loading);

  closeRef.current = onClose;
  loadingRef.current = loading;

  useEffect(() => {
    const previousFocus =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    dialogRef.current?.querySelector<HTMLInputElement>("input[type=checkbox]")?.focus();

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !loadingRef.current) {
        event.preventDefault();
        closeRef.current();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      previousFocus?.focus();
    };
  }, []);

  const disabled = Boolean(capabilityDisabledReason);
  const restartNeeded = clients.some(
    (client) => client.restart_required && selected.includes(client.client_key),
  );
  const restartNames = clients
    .filter((client) => client.restart_required && selected.includes(client.client_key))
    .map((client) => client.display_name)
    .join("、");

  const toggle = (clientKey: string) => {
    setSelected((current) =>
      current.includes(clientKey)
        ? current.filter((key) => key !== clientKey)
        : [...current, clientKey],
    );
  };

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault();
    if (disabled || loading || selected.length === 0) {
      return;
    }
    // Submit in registry order so the result panel lists clients predictably.
    onSubmit(
      clients
        .map((client) => client.client_key)
        .filter((clientKey) => selected.includes(clientKey)),
    );
  };

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/40 p-4">
      <div
        aria-labelledby="config-write-targets-title"
        aria-modal="true"
        className="w-full max-w-md border border-stone-300 bg-white p-4 shadow-xl"
        ref={dialogRef}
        role="dialog"
      >
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-sm font-semibold text-stone-950" id="config-write-targets-title">
            选择要写入的客户端
          </h2>
          <button
            aria-label="关闭"
            className="grid h-6 w-6 place-items-center border border-stone-300 text-stone-700 disabled:opacity-50"
            disabled={loading}
            onClick={onClose}
            type="button"
          >
            <X aria-hidden="true" className="h-3.5 w-3.5" />
          </button>
        </div>

        {capabilityDisabledReason ? (
          <p className="mb-3 border border-amber-200 bg-amber-50 px-2.5 py-2 text-[12px] text-amber-800">
            {capabilityDisabledReason}
          </p>
        ) : null}

        <form onSubmit={handleSubmit}>
          <ul className="space-y-2">
            {clients.map((client) => (
              <li className="border border-stone-200 px-2.5 py-2" key={client.client_key}>
                <label className="flex items-start gap-2 text-[12px] text-stone-700">
                  <input
                    checked={selected.includes(client.client_key)}
                    className="mt-0.5"
                    disabled={disabled || loading}
                    onChange={() => toggle(client.client_key)}
                    type="checkbox"
                  />
                  <span className="min-w-0">
                    <span className="font-semibold text-stone-950">{client.display_name}</span>
                    <span className="ml-2 text-stone-500">
                      {fileStatusLabels[client.file_status] ?? client.file_status}
                    </span>
                    {client.config_path ? (
                      <span className="mt-0.5 block truncate font-mono text-[11px] text-stone-500">
                        {client.config_path}
                      </span>
                    ) : null}
                    {client.error_code ? (
                      <span className="mt-0.5 block font-mono text-[11px] text-red-600">
                        {client.error_code}
                      </span>
                    ) : null}
                  </span>
                </label>
              </li>
            ))}
          </ul>

          {restartNeeded ? (
            <p className="mt-3 border border-stone-200 bg-stone-50 px-2.5 py-2 text-[12px] text-stone-600">
              写入后需重启 {restartNames} 才生效（它不监听配置文件变化）。
            </p>
          ) : null}

          {error ? (
            <p className="mt-3 border border-red-200 bg-red-50 px-2.5 py-2 text-[12px] font-semibold text-red-700" role="alert">
              {error}
            </p>
          ) : null}

          <div className="mt-4 flex justify-end gap-2">
            <button
              className="border border-stone-300 px-3 py-1.5 text-[12px] text-stone-700 disabled:opacity-50"
              disabled={loading}
              onClick={onClose}
              type="button"
            >
              取消
            </button>
            <button
              className="border border-stone-900 bg-stone-900 px-3 py-1.5 text-[12px] font-semibold text-white disabled:opacity-50"
              disabled={disabled || loading || selected.length === 0}
              type="submit"
            >
              写入
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: 运行测试确认通过**

```bash
pnpm test:run tests/ConfigWriteTargetsDialog.test.tsx && pnpm typecheck
```

预期：全部 PASS。

- [ ] **Step 5: 提交**

```bash
git add src/components/accounts/ConfigWriteTargetsDialog.tsx tests/ConfigWriteTargetsDialog.test.tsx
git commit -m "feat: 新增配置写入客户端选择弹窗"
```

---

### Task 8: AccountsScreen 接线

按钮从"直接写"改为"打开弹窗"，写入结果显示客户端名而非 `target_key`。

**Files:**
- Modify: `src/screens/AccountsScreen.tsx`（写入按钮 4215-4245、mutation 3254-3290、结果面板 4399-4422）
- Modify: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes: Task 6 的 `listConfigWriteClients` 与新签名、Task 7 的 `ConfigWriteTargetsDialog`。

- [ ] **Step 1: 改造既有测试并新增**

`tests/AccountsScreen.test.tsx` 的既有两处需要跟随（现在点按钮只开弹窗，不再直接写）：

- `nudges to re-write config when pending edits would change the file`：点击「写入路由配置文件」之后补一步 `await userEvent.click(screen.getByRole("button", { name: "写入" }))`。
- `clears route config write results after a short delay`：同样在 `fireEvent.click(按钮)` 之后补一次点弹窗内「写入」，再断言结果面板。

在 mock 区补 `listConfigWriteClients`（返回 codex + zcode 两项，形状同 Task 7 的 fixture），并新增：

```tsx
  it("opens the client dialog instead of writing immediately", async () => {
    renderScreen();

    await screen.findByText("本地代理：未启动");
    await userEvent.click(screen.getByLabelText("启动本地路由代理"));
    expect(await screen.findByText("本地代理：http://127.0.0.1:43111")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("写入路由配置文件"));

    expect(await screen.findByText("选择要写入的客户端")).toBeInTheDocument();
    // The dialog is the confirmation step, so nothing is written yet.
    expect(writeRouteProxyConfigs).not.toHaveBeenCalled();
  });

  it("writes the selected clients and persists the choice", async () => {
    renderScreen();

    await screen.findByText("本地代理：未启动");
    await userEvent.click(screen.getByLabelText("启动本地路由代理"));
    expect(await screen.findByText("本地代理：http://127.0.0.1:43111")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("写入路由配置文件"));
    await screen.findByText("选择要写入的客户端");
    await userEvent.click(screen.getByRole("checkbox", { name: /ZCode/ }));
    await userEvent.click(screen.getByRole("button", { name: "写入" }));

    await waitFor(() =>
      expect(writeRouteProxyConfigs).toHaveBeenCalledWith(
        "http://127.0.0.1:43111",
        "codex",
        ["codex", "zcode"],
      ),
    );
    // The choice is remembered so the next write does not need re-picking.
    await waitFor(() =>
      expect(saveSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          config_write_clients_json: JSON.stringify({ codex: ["codex", "zcode"] }),
        }),
      ),
    );
  });

  it("labels write results by client name rather than target key", async () => {
    vi.mocked(writeRouteProxyConfigs).mockResolvedValue([
      {
        operation_id: "operation-1",
        snapshot_id: "snapshot-1",
        target_app_id: "target-zcode",
        target_key: "zcode_codex",
        platform: "codex",
        path: "/home/u/.zcode/v2/config.json",
        status: "succeeded",
        before_hash: null,
        after_hash: "after-hash",
        error_code: null,
      },
    ]);
    renderScreen();

    await screen.findByText("本地代理：未启动");
    await userEvent.click(screen.getByLabelText("启动本地路由代理"));
    expect(await screen.findByText("本地代理：http://127.0.0.1:43111")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("写入路由配置文件"));
    await screen.findByText("选择要写入的客户端");
    await userEvent.click(screen.getByRole("button", { name: "写入" }));

    expect(await screen.findByText("配置写入结果")).toBeInTheDocument();
    // `zcode_codex` is an internal key and means nothing to the user.
    expect(screen.queryByText(/zcode_codex/)).not.toBeInTheDocument();
    expect(screen.getByText(/ZCode/)).toBeInTheDocument();
    // The user very likely switched windows already, so repeat the notice here.
    expect(screen.getByText(/需重启 ZCode/)).toBeInTheDocument();
    // Same guarantee the existing outcome test makes: no credential in the panel.
    expect(screen.queryByText(/sk-ai-switch/)).not.toBeInTheDocument();
  });

  it("explains that a corrupt ZCode config was refused rather than overwritten", async () => {
    vi.mocked(writeRouteProxyConfigs).mockRejectedValue({
      code: "validation.route_config_existing_invalid",
      message: "Existing CLI configuration is invalid; refusing to overwrite it",
      details: "/home/u/.zcode/v2/config.json (JSON): syntax is invalid",
      recoverable: true,
    });
    renderScreen();

    await screen.findByText("本地代理：未启动");
    await userEvent.click(screen.getByLabelText("启动本地路由代理"));
    expect(await screen.findByText("本地代理：http://127.0.0.1:43111")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("写入路由配置文件"));
    await screen.findByText("选择要写入的客户端");
    await userEvent.click(screen.getByRole("button", { name: "写入" }));

    // The stakes are specific here: a bad parse makes ZCode fall back and lose
    // every provider, so the message has to say we did not touch the file.
    expect(
      await screen.findByText(/现有配置文件无法解析，已拒绝覆盖以免丢失你的 provider 配置/),
    ).toBeInTheDocument();
  });
```

- [ ] **Step 2: 运行测试确认失败**

```bash
pnpm test:run tests/AccountsScreen.test.tsx
```

预期：FAIL，找不到「选择要写入的客户端」。

- [ ] **Step 3: 实现**

`AccountsScreen.tsx` 的改动：

导入 `ConfigWriteTargetsDialog`、`listConfigWriteClients`、`ConfigWriteClientStatus`。

新增状态与查询：

```tsx
  const [configWriteDialogOpen, setConfigWriteDialogOpen] = useState(false);
  const configWriteClientsQuery = useQuery({
    queryKey: ["config-write-clients", activePlatform],
    queryFn: () => listConfigWriteClients(activePlatform),
    enabled: configWriteDialogOpen,
  });
  /** `null` until the user picks, so the dialog defaults to the native client. */
  const storedClientSelection = useMemo(() => {
    const raw = settingsQuery.data?.config_write_clients_json;
    if (!raw) {
      return null;
    }
    try {
      const parsed = JSON.parse(raw) as Record<string, string[]>;
      const stored = parsed[activePlatform];
      return Array.isArray(stored) && stored.length > 0 ? stored : null;
    } catch {
      // A corrupt preference must not block writing.
      return null;
    }
  }, [settingsQuery.data?.config_write_clients_json, activePlatform]);
```

`writeConfigsMutation` 改为接受客户端列表，并在成功后持久化选择：

```tsx
  const writeConfigsMutation = useMutation({
    mutationFn: async (clientKeys: string[]) => {
      if (!configWriteEnabled) {
        throw new Error(configWriteReason);
      }
      const outcomes = await writeRouteProxyConfigs(
        routeProxyQuery.data?.base_url ?? null,
        activePlatform,
        clientKeys,
      );
      const settings = settingsQuery.data;
      if (settings) {
        let existing: Record<string, string[]> = {};
        try {
          existing = settings.config_write_clients_json
            ? (JSON.parse(settings.config_write_clients_json) as Record<string, string[]>)
            : {};
        } catch {
          existing = {};
        }
        const updated = await saveSettings({
          ...settings,
          config_write_clients_json: JSON.stringify({
            ...existing,
            [activePlatform]: clientKeys,
          }),
        });
        queryClient.setQueryData(["settings"], updated);
      }
      return outcomes;
    },
    onMutate: () => setConfigWriteError(null),
    onSuccess: (outcomes) => {
      setConfigWriteOutcomes(outcomes);
      setConfigWriteDialogOpen(false);
      void queryClient.invalidateQueries({ queryKey: ["route-config-stale"] });
    },
    onError: (error) => setConfigWriteError(formatApiError(error, "配置写入失败。")),
  });
```

写入按钮（`:4215`）的 `onClick` 改为 `() => setConfigWriteDialogOpen(true)`，`disabled` 改为 `!routeProxyQuery.data?.running`（能力门禁下移到弹窗）。

stale 查询把选择带上，让提示只覆盖用户选中的客户端：

```tsx
      storedClientSelection,
```
加进 `queryKey`，`queryFn` 改为
```tsx
    queryFn: () =>
      routeConfigWriteIsStale(
        routeProxyQuery.data?.base_url ?? null,
        activePlatform,
        storedClientSelection,
      ),
```

结果面板（`:4399`）把 `outcome.target_key` 换成客户端名，并在含 `restart_required` 目标时重复提示。为使面板在弹窗关闭后仍能解析名称，把查询结果缓存进一个 map：

```tsx
  const clientLabelByTargetKey = useMemo(() => {
    const map = new Map<string, ConfigWriteClientStatus>();
    for (const client of configWriteClientsQuery.data ?? []) {
      map.set(client.target_key, client);
    }
    return map;
  }, [configWriteClientsQuery.data]);
```
面板内 `{outcome.target_key} · {outcome.platform}` 改为
```tsx
                  {clientLabelByTargetKey.get(outcome.target_key)?.display_name ?? outcome.target_key} · {outcome.platform}
```
并在 `configWriteOutcomes.map(...)` 之后追加：
```tsx
            {configWriteOutcomes.some(
              (outcome) => clientLabelByTargetKey.get(outcome.target_key)?.restart_required,
            ) ? (
              <p className="text-[11px] text-stone-500">
                写入后需重启 ZCode 才生效（它不监听配置文件变化）。
              </p>
            ) : null}
```

**注意**：`configWriteClientsQuery` 的 `enabled` 依赖弹窗开启，写入成功后弹窗关闭会让查询失活但 TanStack Query 保留缓存数据，所以 map 仍可用。为稳妥起见把 `enabled` 改为 `configWriteDialogOpen || configWriteOutcomes.length > 0`。

弹窗挂载（放在其他弹窗附近）：

```tsx
      {configWriteDialogOpen ? (
        <ConfigWriteTargetsDialog
          capabilityDisabledReason={configWriteEnabled ? undefined : configWriteReason}
          clients={configWriteClientsQuery.data ?? []}
          error={configWriteError}
          initialSelection={storedClientSelection}
          loading={writeConfigsMutation.isPending}
          onClose={() => setConfigWriteDialogOpen(false)}
          onSubmit={(clientKeys) => writeConfigsMutation.mutate(clientKeys)}
          platform={activePlatform}
        />
      ) : null}
```

`formatApiError`（`:203`）会把后端消息原样呈现，而 `validation.route_config_existing_invalid` 的英文 message 说不清后果。在 `writeConfigsMutation` 的 `onError` 前加一层按错误码的中文映射：

```tsx
const configWriteErrorMessages: Record<string, string> = {
  // A failed parse makes ZCode fall back to legacy files and end up with an
  // empty provider list, so "we did not touch it" is the load-bearing part.
  "validation.route_config_existing_invalid":
    "现有配置文件无法解析，已拒绝覆盖以免丢失你的 provider 配置。请先修复该文件再重试。",
  "config.concurrent_modification":
    "配置文件在写入期间被其他程序修改，未做改动。请重试。",
  "config.pool_models_empty": "算力池中没有可用模型，请先向池中加入账号。",
  "config.client_unavailable": "所选客户端不支持当前平台。",
};

/** Prefers a code-specific Chinese message over the backend's raw text. */
function formatConfigWriteError(error: unknown): string {
  const code =
    error && typeof error === "object" && typeof (error as { code?: unknown }).code === "string"
      ? (error as { code: string }).code
      : "";
  return configWriteErrorMessages[code] ?? formatApiError(error, "配置写入失败。");
}
```
`onError` 改为 `(error) => setConfigWriteError(formatConfigWriteError(error))`。

- [ ] **Step 4: 运行测试确认通过**

```bash
pnpm test:run tests/AccountsScreen.test.tsx && pnpm typecheck
```

预期：全部 PASS。

- [ ] **Step 5: 提交**

```bash
git add src/screens/AccountsScreen.tsx tests/AccountsScreen.test.tsx
git commit -m "feat: 写入配置前弹窗选择目标客户端"
```

---

### Task 9: 全量验证与真机确认

前面每个任务只跑了局部测试。本任务跑全套，并在真实 ZCode 上确认端到端可用——自动化测试无法覆盖「ZCode 真的能用这条 provider 发出请求」。

**Files:** 仅在验证暴露缺陷时修改相应文件。

- [ ] **Step 1: 全套自动化验证**

```bash
pnpm typecheck
pnpm test:run
cd src-tauri && cargo fmt --check && CARGO_TARGET_DIR=target-codex cargo test
```

预期：全部 PASS。`TargetsScreen.test.tsx` 可能因新增两行 ZCode 目标而失败（它断言目标列表）——按新的目标集合更新断言，并确认 ZCode 行的回滚按钮可用（`adapter_available && operation === "write" && status === "succeeded"`）。

- [ ] **Step 2: 备份真实 ZCode 配置**

```bash
cp ~/.zcode/v2/config.json ~/.zcode/v2/config.json.pre-ai-switch-backup
node -e "JSON.parse(require('fs').readFileSync(process.env.USERPROFILE + '/.zcode/v2/config.json.pre-ai-switch-backup','utf8')); console.log('backup parses OK')"
```

必须先确认备份可解析再往下走：一个写坏的 `config.json` 会让 ZCode 静默回落并丢掉用户所有 provider。

- [ ] **Step 3: 启动应用并写入**

```bash
pnpm tauri:dev
```

在界面上：启动本地路由代理 → 切到 Codex 平台 → 点「写入路由配置文件」→ 弹窗中勾选 ZCode → 写入。确认结果面板显示 `ZCode · codex` 且状态为 `succeeded`，并出现重启提示。

- [ ] **Step 4: 校验落盘内容**

```bash
node -e "
const fs=require('fs');
const p=process.env.USERPROFILE+'/.zcode/v2/config.json';
const j=JSON.parse(fs.readFileSync(p,'utf8'));
const managed=Object.entries(j.provider).filter(([,e])=>e.aiSwitch&&e.aiSwitch.managed);
for(const [id,e] of managed){
  console.log(id, '| kind=', e.kind, '| baseURL=', e.options.baseURL,
    '| platform=', e.aiSwitch.platform, '| models=', Object.keys(e.models).join(','),
    '| apiKey len=', (e.options.apiKey||'').length);
}
console.log('total providers:', Object.keys(j.provider).length);
"
```

预期：恰好一条托管条目，`kind=openai`、`baseURL` 以 `/v1` 结尾、`models` 非空；provider 总数不少于备份中的数量（未托管的条目都还在）。

- [ ] **Step 5: 重启 ZCode 并发一次真实请求**

关闭并重新打开 ZCode → 在 provider 设置中确认 `AI Switch (Codex)` 可选且模型可选 → 发一条消息 → 回到 ai-switch 的请求日志确认这次请求落到了算力池上。

若请求失败，记录 ZCode 报的错与 ai-switch 请求日志中的实际路径和状态码，**不要把本任务标记为完成**。最可能的偏差点是 `kind`/baseURL 后缀配对（`/v1/responses` vs `/v1/v1/responses`）。

- [ ] **Step 6: 幂等与接管复验**

再点一次写入（同样勾 ZCode），确认：provider 条目数不变（就地接管而非新增一条）、`aiSwitch` 标记仍在、stale 提示在写入后消失。

- [ ] **Step 7: 恢复或保留**

真机验证通过则保留写入结果，删除备份：

```bash
rm ~/.zcode/v2/config.json.pre-ai-switch-backup
```

未通过则恢复备份后再排查：

```bash
cp ~/.zcode/v2/config.json.pre-ai-switch-backup ~/.zcode/v2/config.json
```

- [ ] **Step 8: 提交验证期间的修正**

若前面步骤改动了代码：

```bash
git add -A
git commit -m "fix: 修正 ZCode 配置写入在真机验证中暴露的问题"
```
