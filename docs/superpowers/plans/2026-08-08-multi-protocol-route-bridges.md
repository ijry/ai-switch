# 多协议路由桥接实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 Codex Responses 和 Claude Messages 本地入口都能按配置桥接到 `openai`、`openai-responses`、`anthropic`、`gemini` 四种上游协议，同时移除旧的 `anthropic-messages`。

**Architecture:** 在现有 `services::route_protocol_bridge` 边界内扩展 dispatcher 和协议对 transformer。代理继续负责凭据选择、鉴权、重试、日志、用量记录；bridge 只负责路径、query、请求体、响应体和 SSE 格式转换。UI 和导入导出只保留规范协议值，并按平台控制协议下拉框与 Codex custom tool 兼容开关。

**Tech Stack:** Rust/Tauri、Axum、Reqwest、Serde JSON、Vitest、React、TypeScript。

## Global Constraints

- 直接在 `main` 工作，不创建或切换 branch/worktree，除非用户明确要求。
- `docs/superpowers/specs` 和 `docs/superpowers/plans` 文档默认使用中文。
- 未经用户明确要求不执行 `git commit`；每个任务末尾用测试和 `git diff --check` 作为 checkpoint。
- 规范账号协议只有 `openai`、`openai-responses`、`anthropic`、`gemini`。
- 删除旧的 `anthropic-messages` 支持，不保留解析兼容。
- Claude 本地入口使用 `/v1/messages`，不支持 `/v1/complete`。
- Codex 本地 Base URL 保持 `http://127.0.0.1:<port>/v1`。
- Claude 本地 Base URL 保持 `http://127.0.0.1:<port>`。
- Gemini 本地入口保持 Gemini-only，不增加协议下拉框。
- 已缓冲 SSE 转换可以接受；不做实时逐 chunk 流式重构。

---

## Scope Check

这个 spec 涉及 UI、协议类型、路由代理和模型测试，但它们服务同一个可验证目标：本地入口协议与上游账号协议解耦。无需拆分为多个 spec。任务按依赖关系拆分，每个任务结束时仓库可编译或至少能运行该任务对应的窄测试。

## 文件结构

- Modify: `AGENTS.md`，已加入以后 spec/plan 默认中文规则。
- Modify: `src/lib/api/types.ts`，移除 `anthropic-messages`，放宽模型测试 override 类型到四协议字符串。
- Modify: `src/screens/AccountsScreen.tsx`，按平台控制协议选项、Gemini 隐藏下拉框、Codex-only custom tool 兼容开关。
- Modify: `src/components/accounts/RouteCredentialImportDialog.tsx`，导入选择只显示四个规范协议。
- Modify: `src/components/deeplink/DeepLinkImportDialog.tsx`，保持 deeplink payload 使用规范协议类型。
- Modify: `src/lib/codexModelTestEndpoint.ts`，如果继续保留 Codex endpoint helper，确保它不限制四协议模型测试。
- Modify: `tests/AccountsScreen.test.tsx`、`tests/lib/codexModelTestEndpoint.test.ts`、`tests/TauriConfig.test.ts`，覆盖 UI 和配置写入行为。
- Modify: `src-tauri/src/models/platform.rs`，`ApiDialect::parse` 不再接受 `anthropic_messages`。
- Modify: `src-tauri/src/models/route_credential_transfer.rs`，测试夹具改为 `anthropic`。
- Modify: `src-tauri/src/services/route_credential_service.rs`，创建 API 凭据时拒绝 `anthropic-messages`。
- Modify: `src-tauri/src/services/route_model_fetch_service.rs`，模型获取只识别 `anthropic`。
- Modify: `src-tauri/src/services/route_model_test_service.rs`，模型测试生成本地入口请求，并通过同一 bridge 路径构造上游请求和响应。
- Modify: `src-tauri/src/services/route_credential_transfer_codec.rs`，转移编码和测试只接受规范协议。
- Modify: `src-tauri/src/services/route_credential_transfer_import_service.rs`，导入平台选择和冲突测试只接受规范协议。
- Modify: `src-tauri/src/services/deeplink_service.rs`，deeplink app 映射与测试只输出规范协议。
- Modify: `src-tauri/src/services/cpa_export_service.rs`，导出校验复用规范 `ApiDialect`。
- Modify: `src-tauri/src/services/route_proxy_service.rs`，接收 bridge query、暴露模型测试可用的 upstream request 结果、调用 response bridge。
- Modify: `src-tauri/src/services/route_protocol_bridge/mod.rs`，扩展 dispatcher、bridge kind、request/response 公共结构。
- Keep/Modify: `src-tauri/src/services/route_protocol_bridge/responses_chat.rs`，保留已有 Responses <-> Chat 转换并接入 shared helper。
- Create: `src-tauri/src/services/route_protocol_bridge/common.rs`，共享 JSON、文本、ID、用量、路径和 query helper。
- Create: `src-tauri/src/services/route_protocol_bridge/sse.rs`，共享 buffered SSE 解析和事件输出 helper。
- Create: `src-tauri/src/services/route_protocol_bridge/responses_claude.rs`，Codex Responses <-> Anthropic Messages。
- Create: `src-tauri/src/services/route_protocol_bridge/responses_gemini.rs`，Codex Responses <-> Gemini native。
- Create: `src-tauri/src/services/route_protocol_bridge/claude_chat.rs`，Anthropic Messages <-> OpenAI Chat Completions。
- Create: `src-tauri/src/services/route_protocol_bridge/claude_responses.rs`，Anthropic Messages <-> OpenAI Responses。
- Create: `src-tauri/src/services/route_protocol_bridge/claude_gemini.rs`，Anthropic Messages <-> Gemini native。

## 公共接口约定

`route_protocol_bridge::prepare_request` 保持代理侧唯一入口，但返回 query：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolBridgeKind {
    ResponsesToChat,
    ResponsesToAnthropic,
    ResponsesToGemini,
    ClaudeToChat,
    ClaudeToResponses,
    ClaudeToGemini,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedBridgeRequest {
    pub kind: Option<ProtocolBridgeKind>,
    pub upstream_path: String,
    pub upstream_query: Option<String>,
    pub body: Vec<u8>,
    pub streaming: bool,
}
```

`route_proxy_service` 内部 request 结果也携带 bridge 信息：

```rust
#[derive(Debug)]
pub(crate) struct BuiltUpstreamRequest {
    pub target_url: String,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
    pub bridge_kind: Option<ProtocolBridgeKind>,
}
```

Gemini streaming 使用 `upstream_path = "/v1beta/models/{model}:streamGenerateContent"` 和 `upstream_query = Some("alt=sse".to_string())`，然后由 proxy 将它与原始 query 和 `key` 参数合并。

---

### Task 1: 清理协议值和 UI 表单规则

**Files:**
- Modify: `src/lib/api/types.ts`
- Modify: `src/screens/AccountsScreen.tsx`
- Modify: `src/components/accounts/RouteCredentialImportDialog.tsx`
- Modify: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Produces: `InterfaceFormat = "openai" | "openai-responses" | "anthropic" | "gemini"`
- Produces: `interfaceFormatsForPlatform(platform: PlatformKey): InterfaceFormat[]`
- Produces: `shouldShowInterfaceFormatSelect(platform: PlatformKey): boolean`
- Produces: `shouldShowResponsesCustomToolCompat(platform: PlatformKey): boolean`

- [ ] **Step 1: 写前端失败测试**

在 `tests/AccountsScreen.test.tsx` 中更新现有接口格式测试，加入这些断言：

```ts
expect(screen.queryByText("Claude Messages（兼容）")).not.toBeInTheDocument();

const codexOptions = within(screen.getByLabelText("接口格式")).getAllByRole("option");
expect(codexOptions.map((option) => option.getAttribute("value"))).toEqual([
  "openai",
  "openai-responses",
  "anthropic",
  "gemini",
]);
```

为 Claude tab 增加同样的四协议断言。为 Gemini tab 增加断言：

```ts
expect(screen.queryByLabelText("接口格式")).not.toBeInTheDocument();
expect(screen.queryByLabelText("兼容 custom 工具（Responses 中转）")).not.toBeInTheDocument();
```

为 Claude tab 增加断言：

```ts
expect(screen.queryByLabelText("兼容 custom 工具（Responses 中转）")).not.toBeInTheDocument();
```

- [ ] **Step 2: 运行前端窄测试并确认失败**

Run: `pnpm test:run tests/AccountsScreen.test.tsx`

Expected: FAIL，失败点包含旧 `anthropic-messages` option 或 Gemini 仍显示接口格式。

- [ ] **Step 3: 修改 TypeScript 协议类型**

在 `src/lib/api/types.ts` 中删除 `anthropic-messages`：

```ts
export type InterfaceFormat =
  | "openai"
  | "openai-responses"
  | "anthropic"
  | "gemini";
```

将 `RoutePoolModelTestRequest.interface_format` 从 Codex-only union 改成四协议：

```ts
interface_format?: InterfaceFormat | null;
```

- [ ] **Step 4: 修改账号页 helper**

在 `src/screens/AccountsScreen.tsx` 中替换协议列表：

```ts
const routeInterfaceFormats: InterfaceFormat[] = ["openai", "openai-responses", "anthropic", "gemini"];

const interfaceFormatLabels: Record<InterfaceFormat, string> = {
  openai: "OpenAI Chat Completions",
  "openai-responses": "OpenAI Responses",
  anthropic: "Claude Messages",
  gemini: "Gemini",
};

function interfaceFormatsForPlatform(platform: PlatformKey): InterfaceFormat[] {
  if (platform === "gemini") {
    return ["gemini"];
  }
  if (platform === "codex" || platform === "claude") {
    return routeInterfaceFormats;
  }
  return [defaultInterfaceFormat(platform)];
}

function shouldShowInterfaceFormatSelect(platform: PlatformKey) {
  return interfaceFormatsForPlatform(platform).length > 1;
}

function isAnthropicInterfaceFormat(value: InterfaceFormat | string) {
  return value === "anthropic";
}

function shouldShowResponsesCustomToolCompat(platform: PlatformKey) {
  return platform === "codex";
}
```

渲染 create/edit 下拉框时使用 `interfaceFormatsForPlatform(activePlatform)`。当平台切换到 Gemini 时，状态同步到 `gemini`；当平台切换到 Grok/OpenCode/OpenClaw/Hermes 时，状态同步到 `defaultInterfaceFormat(activePlatform)`。

- [ ] **Step 5: 修改 custom tool 兼容开关显示条件**

create 表单和 edit 表单都用同一条件：

```tsx
{shouldShowResponsesCustomToolCompat(activePlatform) ? (
  <label className="flex items-start gap-2 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[12px] font-medium text-stone-700">
    <input
      aria-label="兼容 custom 工具（Responses 中转）"
      checked={apiResponsesCustomToolCompat}
      className="mt-0.5"
      onChange={(event) => setApiResponsesCustomToolCompat(event.target.checked)}
      type="checkbox"
    />
    <span className="grid gap-1">
      <span>兼容 custom 工具（Responses 中转）</span>
      <span className="text-[11px] font-medium text-stone-500">
        把 custom 工具改写成 function，给不支持 custom 的中转站用。默认关闭。
      </span>
    </span>
  </label>
) : null}
```

edit 表单使用 `editResponsesCustomToolCompat` 和 `setEditResponsesCustomToolCompat`。

- [ ] **Step 6: 修改导入弹窗选项**

在 `src/components/accounts/RouteCredentialImportDialog.tsx` 中保留四个选项：

```ts
const interfaceFormatOptions = [
  { value: "openai", label: "OpenAI Chat Completions" },
  { value: "openai-responses", label: "OpenAI Responses" },
  { value: "anthropic", label: "Anthropic Messages" },
  { value: "gemini", label: "Gemini" },
];
```

- [ ] **Step 7: 运行前端窄测试并确认通过**

Run: `pnpm test:run tests/AccountsScreen.test.tsx tests/lib/codexModelTestEndpoint.test.ts`

Expected: PASS。

- [ ] **Step 8: 前端 checkpoint**

Run: `pnpm typecheck`

Expected: PASS。

---

### Task 2: 清理 Rust 协议解析和导入导出

**Files:**
- Modify: `src-tauri/src/models/platform.rs`
- Modify: `src-tauri/src/services/route_credential_service.rs`
- Modify: `src-tauri/src/services/route_model_fetch_service.rs`
- Modify: `src-tauri/src/services/route_model_test_service.rs`
- Modify: `src-tauri/src/models/route_credential_transfer.rs`
- Modify: `src-tauri/src/services/route_credential_transfer_codec.rs`
- Modify: `src-tauri/src/services/route_credential_transfer_import_service.rs`
- Modify: `src-tauri/src/services/deeplink_service.rs`
- Modify: `src-tauri/src/services/cpa_export_service.rs`

**Interfaces:**
- Consumes: canonical protocol values from Task 1.
- Produces: `ApiDialect::parse("anthropic-messages")` returns validation error.
- Produces: credential creation/import/model fetch/model test code only treats `anthropic` as Anthropic.

- [ ] **Step 1: 写 Rust 失败测试**

在 `src-tauri/src/models/platform.rs` 的 `parses_supported_api_dialect_aliases` 测试中替换旧断言：

```rust
assert_eq!(
    ApiDialect::parse("openai-responses").unwrap(),
    ApiDialect::OpenAiResponses
);
assert_eq!(ApiDialect::parse("anthropic").unwrap(), ApiDialect::Anthropic);
assert!(ApiDialect::parse("anthropic-messages").is_err());
assert!(ApiDialect::parse("anthropic_messages").is_err());
assert!(ApiDialect::parse("automatic").is_err());
```

在 `route_credential_service` 测试模块增加：

```rust
#[test]
fn validate_interface_format_rejects_legacy_anthropic_messages() {
    assert!(validate_interface_format("anthropic").is_ok());
    assert!(validate_interface_format("anthropic-messages").is_err());
}
```

将 transfer/import/deeplink 测试 fixture 里的 `"anthropic-messages"` 改成 `"anthropic"`，并加一个失败输入测试验证 legacy 值被拒绝。

- [ ] **Step 2: 运行 Rust 窄测试并确认失败**

Run: `cd src-tauri && cargo test platform::tests::parses_supported_api_dialect_aliases route_credential_service::tests::validate_interface_format_rejects_legacy_anthropic_messages route_credential_transfer_codec::tests --lib`

Expected: FAIL，失败点包含 parser 仍接受 legacy 值或 fixture 仍期待 legacy 值。

- [ ] **Step 3: 修改 `ApiDialect::parse`**

在 `src-tauri/src/models/platform.rs` 中只保留：

```rust
match normalize_identifier(value).as_str() {
    "openai" => Ok(Self::OpenAi),
    "openai_responses" => Ok(Self::OpenAiResponses),
    "anthropic" => Ok(Self::Anthropic),
    "gemini" => Ok(Self::Gemini),
    _ => Err(AppError::Validation {
        code: "validation.api_dialect",
        message: "API dialect is not recognized".to_string(),
        details: Some(value.trim().to_string()),
        recoverable: true,
    }),
}
```

- [ ] **Step 4: 修改凭据校验和 Anthropic helper**

在 `route_credential_service.rs` 中：

```rust
fn validate_interface_format(value: &str) -> Result<(), AppError> {
    match value {
        "openai" | "openai-responses" | "anthropic" | "gemini" => Ok(()),
        _ => Err(AppError::Validation {
            code: "validation.interface_format",
            message: "Interface format is not supported".to_string(),
            details: Some(value.to_string()),
            recoverable: true,
        }),
    }
}

fn is_anthropic_interface_format(value: &str) -> bool {
    value == "anthropic"
}
```

- [ ] **Step 5: 修改模型获取和模型测试字符串分支**

把 `route_model_fetch_service.rs`、`route_model_test_service.rs` 中所有 `matches!(..., "anthropic" | "anthropic-messages")` 改为只匹配 `"anthropic"`。`default_model_for` 保持：

```rust
"anthropic" => "claude-sonnet-4-20250514",
```

- [ ] **Step 6: 修改 transfer/deeplink/CPA 测试 fixture**

用 `rg -n "anthropic-messages|anthropic_messages" src-tauri/src` 找到每个残留点。所有序列化 fixture 和断言改为 `anthropic`。legacy 输入测试的期望是 validation error，不是自动归一化。

- [ ] **Step 7: 运行 Rust 清理测试**

Run: `cd src-tauri && cargo test platform::tests route_credential_service::tests route_credential_transfer_codec::tests route_credential_transfer_import_service::tests deeplink_service::tests route_model_fetch_service::tests route_model_test_service::tests --lib`

Expected: PASS。

- [ ] **Step 8: 全仓库 legacy 字符串扫描**

Run: `rg -n "anthropic-messages|anthropic_messages" src-tauri/src src tests`

Expected: no matches。

---

### Task 3: 扩展 bridge dispatcher、路径和 query 规则

**Files:**
- Modify: `src-tauri/src/services/route_protocol_bridge/mod.rs`
- Create: `src-tauri/src/services/route_protocol_bridge/common.rs`
- Create: `src-tauri/src/services/route_protocol_bridge/sse.rs`
- Modify: `src-tauri/src/services/route_proxy_service.rs`

**Interfaces:**
- Produces: `ProtocolBridgeKind` 六个变体。
- Produces: `PreparedBridgeRequest.upstream_query: Option<String>`。
- Produces: `common::gemini_model_from_request(body: &Value) -> Result<String, String>`。
- Produces: `common::merge_query(existing: Option<&str>, extra: Option<&str>) -> Option<String>`。

- [ ] **Step 1: 写 dispatcher 失败测试**

在 `route_protocol_bridge::tests` 中新增矩阵测试：

```rust
#[test]
fn selects_codex_responses_bridge_matrix() {
    let body = br#"{"model":"gpt-5","input":"hello"}"#;
    let cases = [
        (ApiDialect::OpenAi, Some(ProtocolBridgeKind::ResponsesToChat), "/chat/completions", None),
        (ApiDialect::OpenAiResponses, None, "/responses", None),
        (ApiDialect::Anthropic, Some(ProtocolBridgeKind::ResponsesToAnthropic), "/v1/messages", None),
        (
            ApiDialect::Gemini,
            Some(ProtocolBridgeKind::ResponsesToGemini),
            "/v1beta/models/gpt-5:generateContent",
            None,
        ),
    ];

    for (dialect, expected_kind, expected_path, expected_query) in cases {
        let prepared = prepare_request(PlatformId::Codex, dialect, "/v1/responses", body).unwrap();
        assert_eq!(prepared.kind, expected_kind);
        assert_eq!(prepared.upstream_path, expected_path);
        assert_eq!(prepared.upstream_query.as_deref(), expected_query);
    }
}
```

新增 Claude 矩阵测试：

```rust
#[test]
fn selects_claude_messages_bridge_matrix() {
    let body = br#"{"model":"claude-sonnet-4-20250514","messages":[{"role":"user","content":"hello"}],"max_tokens":16}"#;
    let cases = [
        (ApiDialect::OpenAi, Some(ProtocolBridgeKind::ClaudeToChat), "/chat/completions"),
        (ApiDialect::OpenAiResponses, Some(ProtocolBridgeKind::ClaudeToResponses), "/responses"),
        (ApiDialect::Anthropic, None, "/v1/messages"),
        (ApiDialect::Gemini, Some(ProtocolBridgeKind::ClaudeToGemini), "/v1beta/models/claude-sonnet-4-20250514:generateContent"),
    ];

    for (dialect, expected_kind, expected_path) in cases {
        let prepared = prepare_request(PlatformId::Claude, dialect, "/v1/messages", body).unwrap();
        assert_eq!(prepared.kind, expected_kind);
        assert_eq!(prepared.upstream_path, expected_path);
    }
}
```

新增 Gemini streaming 测试：

```rust
#[test]
fn gemini_bridge_streaming_uses_alt_sse_query() {
    let prepared = prepare_request(
        PlatformId::Claude,
        ApiDialect::Gemini,
        "/v1/messages",
        br#"{"model":"gemini-2.5-flash","stream":true,"messages":[{"role":"user","content":"hello"}],"max_tokens":16}"#,
    )
    .unwrap();

    assert_eq!(prepared.upstream_path, "/v1beta/models/gemini-2.5-flash:streamGenerateContent");
    assert_eq!(prepared.upstream_query.as_deref(), Some("alt=sse"));
}
```

- [ ] **Step 2: 运行 dispatcher 测试并确认失败**

Run: `cd src-tauri && cargo test route_protocol_bridge::tests::selects_codex_responses_bridge_matrix route_protocol_bridge::tests::selects_claude_messages_bridge_matrix route_protocol_bridge::tests::gemini_bridge_streaming_uses_alt_sse_query --lib`

Expected: FAIL，失败点包含缺少 enum variant 或未选择新 bridge。

- [ ] **Step 3: 扩展公共结构**

在 `mod.rs` 中加入模块：

```rust
mod common;
mod sse;
mod responses_chat;
mod responses_claude;
mod responses_gemini;
mod claude_chat;
mod claude_responses;
mod claude_gemini;
```

扩展 `ProtocolBridgeKind` 和 `PreparedBridgeRequest`。所有 passthrough 返回都设置 `upstream_query: None`。

- [ ] **Step 4: 实现 path classifier**

在 `common.rs` 中放入：

```rust
pub(super) fn normalize_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        String::new()
    } else if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

pub(super) fn is_create_path(path: &str, expected: &str) -> bool {
    let normalized = normalize_path(path);
    let mut remaining = normalized.trim_start_matches('/');
    while let Some(first) = remaining.split('/').next() {
        if !is_version_segment(first) {
            break;
        }
        remaining = remaining[first.len()..].trim_start_matches('/');
    }
    remaining.trim_end_matches('/') == expected.trim_start_matches('/')
}

fn is_version_segment(segment: &str) -> bool {
    let lower = segment.to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix('v') else {
        return false;
    };
    !rest.is_empty() && rest.chars().next().is_some_and(|ch| ch.is_ascii_digit())
}
```

- [ ] **Step 5: 实现 Gemini endpoint helper**

在 `common.rs` 中加入：

```rust
pub(super) fn request_streaming(body: &[u8]) -> bool {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| value.get("stream").and_then(Value::as_bool))
        .unwrap_or(false)
}

pub(super) fn gemini_model_from_body(body: &[u8]) -> Result<String, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Request JSON is invalid: {error}"))?;
    value
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "Gemini bridge request is missing model".to_string())
}

pub(super) fn gemini_endpoint(model: &str, streaming: bool) -> (String, Option<String>) {
    if streaming {
        (
            format!("/v1beta/models/{model}:streamGenerateContent"),
            Some("alt=sse".to_string()),
        )
    } else {
        (format!("/v1beta/models/{model}:generateContent"), None)
    }
}
```

- [ ] **Step 6: 修改 `prepare_request` 矩阵**

在 `mod.rs` 中按矩阵分支。示例结构：

```rust
let streaming = common::request_streaming(body);
let normalized_path = common::normalize_path(path);
let is_responses = common::is_create_path(&normalized_path, "responses");
let is_messages = common::is_create_path(&normalized_path, "messages");

match (platform, upstream_dialect, is_responses, is_messages) {
    (PlatformId::Codex, ApiDialect::OpenAi, true, _) => prepare_responses_to_chat(body, streaming),
    (PlatformId::Codex, ApiDialect::OpenAiResponses, true, _) => passthrough("/responses", body, streaming),
    (PlatformId::Codex, ApiDialect::Anthropic, true, _) => prepare_responses_to_anthropic(body, streaming),
    (PlatformId::Codex, ApiDialect::Gemini, true, _) => prepare_responses_to_gemini(body, streaming),
    (PlatformId::Claude, ApiDialect::OpenAi, _, true) => prepare_claude_to_chat(body, streaming),
    (PlatformId::Claude, ApiDialect::OpenAiResponses, _, true) => prepare_claude_to_responses(body, streaming),
    (PlatformId::Claude, ApiDialect::Anthropic, _, true) => passthrough("/v1/messages", body, streaming),
    (PlatformId::Claude, ApiDialect::Gemini, _, true) => prepare_claude_to_gemini(body, streaming),
    _ => passthrough(&normalized_path, body, streaming),
}
```

在本任务中，新增 pair module 可先返回明确 request conversion error；Task 4 和 Task 5 会替换为真实转换。已存在的 `responses_chat` 继续真实转换。

- [ ] **Step 7: 修改 proxy query 合并**

在 `route_proxy_service.rs` 中给 `BuiltUpstreamRequest` 加 `bridge_kind` 仍保留，并让内部 URL 构建接收 bridge query：

```rust
fn merge_query_parts(original: Option<&str>, bridge: Option<&str>) -> Option<String> {
    match (original.filter(|value| !value.is_empty()), bridge.filter(|value| !value.is_empty())) {
        (Some(left), Some(right)) => Some(format!("{left}&{right}")),
        (Some(left), None) => Some(left.to_string()),
        (None, Some(right)) => Some(right.to_string()),
        (None, None) => None,
    }
}
```

`build_api_upstream_request` 使用：

```rust
let PreparedBridgeRequest {
    kind: bridge_kind,
    upstream_path,
    upstream_query,
    body: rewritten_body,
    ..
} = prepare_protocol_bridge_request(platform, dialect, &upstream_path, &rewritten_body)?;
let merged_query = merge_query_parts(query, upstream_query.as_deref());
let mut target_url = build_target_url(base_url, &upstream_path, merged_query.as_deref());
```

Gemini 鉴权继续用 `append_query_param(&target_url, "key", api_key)`，此时 `alt=sse` 已在 URL query 中。

- [ ] **Step 8: 运行 dispatcher 和 URL 测试**

Run: `cd src-tauri && cargo test route_protocol_bridge::tests route_proxy_service::tests::build_target_url_joins_base_path_and_query --lib`

Expected: PASS。

---

### Task 4: 实现 Codex Responses 本地入口的三类上游桥接

**Files:**
- Modify: `src-tauri/src/services/route_protocol_bridge/responses_chat.rs`
- Create/Modify: `src-tauri/src/services/route_protocol_bridge/responses_claude.rs`
- Create/Modify: `src-tauri/src/services/route_protocol_bridge/responses_gemini.rs`
- Modify: `src-tauri/src/services/route_protocol_bridge/common.rs`
- Modify: `src-tauri/src/services/route_protocol_bridge/sse.rs`

**Interfaces:**
- Produces: `responses_claude::responses_request_to_anthropic(body: &[u8]) -> Result<Vec<u8>, String>`
- Produces: `responses_claude::anthropic_response_to_responses(status: u16, content_type: Option<&str>, body: &[u8]) -> Result<TransformedBridgeResponse, String>`
- Produces: `responses_gemini::responses_request_to_gemini(body: &[u8]) -> Result<Vec<u8>, String>`
- Produces: `responses_gemini::gemini_response_to_responses(status: u16, content_type: Option<&str>, body: &[u8]) -> Result<TransformedBridgeResponse, String>`

- [ ] **Step 1: 写 Responses->Anthropic 请求失败测试**

在 `responses_claude.rs` 测试模块中加入：

```rust
#[test]
fn converts_responses_request_to_anthropic_messages() {
    let body = json!({
        "model": "claude-sonnet-4-20250514",
        "instructions": "Be concise",
        "input": [
            {"role": "user", "content": [{"type": "input_text", "text": "Find x"}]},
            {"type": "function_call", "call_id": "call_1", "name": "lookup", "arguments": "{\"key\":\"x\"}"},
            {"type": "function_call_output", "call_id": "call_1", "output": "42"}
        ],
        "max_output_tokens": 64,
        "temperature": 0.2,
        "tools": [{
            "type": "function",
            "name": "lookup",
            "description": "Lookup value",
            "parameters": {"type":"object","properties":{"key":{"type":"string"}}}
        }]
    });

    let converted: Value = serde_json::from_slice(
        &responses_request_to_anthropic(&serde_json::to_vec(&body).unwrap()).unwrap(),
    )
    .unwrap();

    assert_eq!(converted["system"], "Be concise");
    assert_eq!(converted["messages"][0]["role"], "user");
    assert_eq!(converted["messages"][0]["content"][0]["type"], "text");
    assert_eq!(converted["messages"][1]["content"][0]["type"], "tool_use");
    assert_eq!(converted["messages"][2]["content"][0]["type"], "tool_result");
    assert_eq!(converted["max_tokens"], 64);
    assert_eq!(converted["tools"][0]["input_schema"]["type"], "object");
}
```

- [ ] **Step 2: 写 Anthropic->Responses 响应失败测试**

```rust
#[test]
fn converts_anthropic_response_to_responses_json() {
    let upstream = json!({
        "id": "msg_1",
        "type": "message",
        "role": "assistant",
        "model": "claude-sonnet-4-20250514",
        "content": [
            {"type": "text", "text": "hello"},
            {"type": "tool_use", "id": "toolu_1", "name": "lookup", "input": {"key":"x"}}
        ],
        "stop_reason": "tool_use",
        "usage": {"input_tokens": 3, "output_tokens": 5}
    });

    let converted = anthropic_response_to_responses(
        200,
        Some("application/json"),
        serde_json::to_vec(&upstream).unwrap().as_slice(),
    )
    .unwrap();
    let output: Value = serde_json::from_slice(&converted.body).unwrap();

    assert_eq!(output["object"], "response");
    assert_eq!(output["id"], "msg_1");
    assert_eq!(output["output_text"], "hello");
    assert_eq!(output["output"][1]["type"], "function_call");
    assert_eq!(output["output"][1]["call_id"], "toolu_1");
    assert_eq!(output["usage"]["input_tokens"], 3);
    assert_eq!(output["usage"]["output_tokens"], 5);
}
```

- [ ] **Step 3: 写 Responses->Gemini 请求失败测试**

```rust
#[test]
fn converts_responses_request_to_gemini_generate_content() {
    let body = json!({
        "model": "gemini-2.5-flash",
        "instructions": "Be concise",
        "input": [{"role":"user","content":[{"type":"input_text","text":"hello"}]}],
        "max_output_tokens": 32,
        "temperature": 0,
        "tools": [{"type":"function","name":"lookup","parameters":{"type":"object","properties":{}}}]
    });

    let converted: Value = serde_json::from_slice(
        &responses_request_to_gemini(&serde_json::to_vec(&body).unwrap()).unwrap(),
    )
    .unwrap();

    assert_eq!(converted["systemInstruction"]["parts"][0]["text"], "Be concise");
    assert_eq!(converted["contents"][0]["role"], "user");
    assert_eq!(converted["contents"][0]["parts"][0]["text"], "hello");
    assert_eq!(converted["generationConfig"]["maxOutputTokens"], 32);
    assert_eq!(converted["tools"][0]["functionDeclarations"][0]["name"], "lookup");
}
```

- [ ] **Step 4: 写 Gemini->Responses 响应失败测试**

```rust
#[test]
fn converts_gemini_response_to_responses_json() {
    let upstream = json!({
        "candidates": [{
            "content": {"role": "model", "parts": [{"text": "hello"}]},
            "finishReason": "STOP"
        }],
        "usageMetadata": {
            "promptTokenCount": 3,
            "candidatesTokenCount": 5,
            "totalTokenCount": 8
        }
    });

    let converted = gemini_response_to_responses(
        200,
        Some("application/json"),
        serde_json::to_vec(&upstream).unwrap().as_slice(),
    )
    .unwrap();
    let output: Value = serde_json::from_slice(&converted.body).unwrap();

    assert_eq!(output["object"], "response");
    assert_eq!(output["output_text"], "hello");
    assert_eq!(output["usage"]["input_tokens"], 3);
    assert_eq!(output["usage"]["output_tokens"], 5);
    assert_eq!(output["usage"]["total_tokens"], 8);
}
```

- [ ] **Step 5: 运行新模块测试并确认失败**

Run: `cd src-tauri && cargo test route_protocol_bridge::responses_claude::tests route_protocol_bridge::responses_gemini::tests --lib`

Expected: FAIL，失败点包含缺少函数或转换字段。

- [ ] **Step 6: 实现 Responses->Anthropic 请求转换**

请求字段映射必须固定为：

```rust
model -> model
instructions -> system
input string -> messages[0] user text block
input message content input_text/output_text/text -> text block
input_image image_url data URL -> image block source base64 when data URL; URL image returns conversion error
function_call -> assistant content tool_use
function_call_output -> user content tool_result
max_output_tokens -> max_tokens
temperature/top_p/stop/stream -> same name where Anthropic supports it
tools function parameters -> tools[].input_schema
```

URL 图片无法转成 Anthropic base64 block 时返回：

```rust
Err("Anthropic bridge only supports base64 data URL images".to_string())
```

- [ ] **Step 7: 实现 Anthropic->Responses JSON 和 SSE**

非流式映射：

```rust
content text -> output message output_text
content tool_use -> output function_call
stop_reason "max_tokens" -> status "incomplete"
stop_reason "end_turn" | "tool_use" | "stop_sequence" -> status "completed"
usage.input_tokens -> usage.input_tokens
usage.output_tokens -> usage.output_tokens
```

SSE 解析使用 `sse.rs` 的 `parse_sse_data_records(body)`，映射这些事件：

```text
message_start -> response.created + response.in_progress
content_block_start text -> response.output_item.added + response.content_part.added
content_block_delta text_delta -> response.output_text.delta
content_block_start tool_use -> response.output_item.added(function_call)
content_block_delta input_json_delta -> response.function_call_arguments.delta
content_block_stop -> done event
message_delta usage/stop_reason -> update final response usage/status
message_stop -> response.completed/response.incomplete/response.failed
```

- [ ] **Step 8: 实现 Responses->Gemini 请求转换**

请求字段映射必须固定为：

```rust
instructions -> systemInstruction.parts[].text
input user -> contents role user
input assistant -> contents role model
input text part -> parts[].text
input_image data URL -> inlineData { mimeType, data }
function_call -> parts[].functionCall { name, args }
function_call_output -> parts[].functionResponse { name, response }
max_output_tokens -> generationConfig.maxOutputTokens
temperature/top_p/stop -> generationConfig.temperature/topP/stopSequences
tools function -> tools[].functionDeclarations[]
```

Gemini function response 需要 name。若 Responses `function_call_output` 只有 `call_id` 没有 name，先用当前请求中同一 `call_id` 的 `function_call.name` 建立 map；找不到时返回：

```rust
Err(format!("Gemini bridge cannot resolve function name for call_id `{call_id}`"))
```

- [ ] **Step 9: 实现 Gemini->Responses JSON 和 SSE**

非流式映射：

```rust
candidates[0].content.parts[].text -> output_text
candidates[0].content.parts[].functionCall -> function_call
finishReason MAX_TOKENS -> incomplete
finishReason STOP | FUNCTION_CALL -> completed
usageMetadata.promptTokenCount -> input_tokens
usageMetadata.candidatesTokenCount -> output_tokens
usageMetadata.totalTokenCount -> total_tokens
```

SSE record 是 Gemini JSON chunk，逐条解析 `data:`，按文本和 functionCall delta 生成 Responses SSE。buffered SSE 末尾必须输出一个 `response.completed` 或 `response.incomplete`。

- [ ] **Step 10: 运行 Codex bridge 测试**

Run: `cd src-tauri && cargo test route_protocol_bridge::tests route_protocol_bridge::responses_chat::tests route_protocol_bridge::responses_claude::tests route_protocol_bridge::responses_gemini::tests --lib`

Expected: PASS。

---

### Task 5: 实现 Claude Messages 本地入口的三类上游桥接

**Files:**
- Create/Modify: `src-tauri/src/services/route_protocol_bridge/claude_chat.rs`
- Create/Modify: `src-tauri/src/services/route_protocol_bridge/claude_responses.rs`
- Create/Modify: `src-tauri/src/services/route_protocol_bridge/claude_gemini.rs`
- Modify: `src-tauri/src/services/route_protocol_bridge/common.rs`
- Modify: `src-tauri/src/services/route_protocol_bridge/sse.rs`
- Modify: `src-tauri/src/services/route_protocol_bridge/mod.rs`

**Interfaces:**
- Produces: `claude_chat::anthropic_request_to_chat(body: &[u8]) -> Result<Vec<u8>, String>`
- Produces: `claude_chat::chat_response_to_anthropic(status: u16, content_type: Option<&str>, body: &[u8]) -> Result<TransformedBridgeResponse, String>`
- Produces: `claude_responses::anthropic_request_to_responses(body: &[u8]) -> Result<Vec<u8>, String>`
- Produces: `claude_responses::responses_response_to_anthropic(status: u16, content_type: Option<&str>, body: &[u8]) -> Result<TransformedBridgeResponse, String>`
- Produces: `claude_gemini::anthropic_request_to_gemini(body: &[u8]) -> Result<Vec<u8>, String>`
- Produces: `claude_gemini::gemini_response_to_anthropic(status: u16, content_type: Option<&str>, body: &[u8]) -> Result<TransformedBridgeResponse, String>`

- [ ] **Step 1: 写 Claude->Chat 请求和响应失败测试**

在 `claude_chat.rs` 中加入：

```rust
#[test]
fn converts_anthropic_request_to_chat() {
    let body = json!({
        "model": "gpt-5.5",
        "system": "Be concise",
        "messages": [
            {"role":"user","content":[{"type":"text","text":"Find x"}]},
            {"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"lookup","input":{"key":"x"}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"42"}]}
        ],
        "max_tokens": 64,
        "tools": [{"name":"lookup","input_schema":{"type":"object","properties":{}}}]
    });

    let converted: Value = serde_json::from_slice(
        &anthropic_request_to_chat(&serde_json::to_vec(&body).unwrap()).unwrap(),
    )
    .unwrap();

    assert_eq!(converted["messages"][0]["role"], "system");
    assert_eq!(converted["messages"][1]["role"], "user");
    assert_eq!(converted["messages"][2]["tool_calls"][0]["id"], "toolu_1");
    assert_eq!(converted["messages"][3]["role"], "tool");
    assert_eq!(converted["max_tokens"], 64);
    assert_eq!(converted["tools"][0]["function"]["name"], "lookup");
}
```

响应测试：

```rust
#[test]
fn converts_chat_response_to_anthropic_json() {
    let upstream = json!({
        "id": "chatcmpl_1",
        "model": "gpt-5.5",
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "hello",
                "tool_calls": [{"id":"call_1","type":"function","function":{"name":"lookup","arguments":"{\"key\":\"x\"}"}}]
            },
            "finish_reason": "tool_calls"
        }],
        "usage": {"prompt_tokens":3,"completion_tokens":5}
    });

    let converted = chat_response_to_anthropic(200, Some("application/json"), serde_json::to_vec(&upstream).unwrap().as_slice()).unwrap();
    let output: Value = serde_json::from_slice(&converted.body).unwrap();

    assert_eq!(output["type"], "message");
    assert_eq!(output["content"][0]["type"], "text");
    assert_eq!(output["content"][1]["type"], "tool_use");
    assert_eq!(output["stop_reason"], "tool_use");
    assert_eq!(output["usage"]["input_tokens"], 3);
    assert_eq!(output["usage"]["output_tokens"], 5);
}
```

- [ ] **Step 2: 写 Claude->Responses 请求和响应失败测试**

请求测试断言：

```rust
assert_eq!(converted["input"][0]["role"], "user");
assert_eq!(converted["instructions"], "Be concise");
assert_eq!(converted["max_output_tokens"], 64);
assert_eq!(converted["tools"][0]["type"], "function");
```

响应测试使用 Responses JSON：

```rust
let upstream = json!({
    "id": "resp_1",
    "model": "gpt-5.5",
    "status": "completed",
    "output": [
        {"type":"message","role":"assistant","content":[{"type":"output_text","text":"hello"}]},
        {"type":"function_call","call_id":"call_1","name":"lookup","arguments":"{\"key\":\"x\"}"}
    ],
    "usage": {"input_tokens":3,"output_tokens":5}
});
```

断言 Anthropic 输出：

```rust
assert_eq!(output["content"][0]["type"], "text");
assert_eq!(output["content"][1]["type"], "tool_use");
assert_eq!(output["stop_reason"], "tool_use");
```

- [ ] **Step 3: 写 Claude->Gemini 请求和响应失败测试**

请求测试断言：

```rust
assert_eq!(converted["systemInstruction"]["parts"][0]["text"], "Be concise");
assert_eq!(converted["contents"][0]["role"], "user");
assert_eq!(converted["contents"][0]["parts"][0]["text"], "Find x");
assert_eq!(converted["tools"][0]["functionDeclarations"][0]["name"], "lookup");
```

响应测试使用 Gemini JSON 并断言 Anthropic：

```rust
assert_eq!(output["type"], "message");
assert_eq!(output["content"][0]["type"], "text");
assert_eq!(output["usage"]["input_tokens"], 3);
assert_eq!(output["usage"]["output_tokens"], 5);
```

- [ ] **Step 4: 运行 Claude bridge 测试并确认失败**

Run: `cd src-tauri && cargo test route_protocol_bridge::claude_chat::tests route_protocol_bridge::claude_responses::tests route_protocol_bridge::claude_gemini::tests --lib`

Expected: FAIL，失败点包含缺少函数或转换字段。

- [ ] **Step 5: 实现 Claude->Chat**

字段映射固定为：

```rust
system string -> Chat system message
system array text -> Chat system message with joined text
messages user text/image -> Chat user content
messages assistant text -> Chat assistant content
messages assistant tool_use -> Chat assistant tool_calls
messages user tool_result -> Chat tool message
max_tokens -> max_tokens
temperature/top_p/stop_sequences/stream -> temperature/top_p/stop/stream
tools[].input_schema -> tools[].function.parameters
tool_choice auto/any/tool -> Chat compatible tool_choice
```

Chat 响应映射回 Anthropic：

```rust
message.content -> content text
message.tool_calls[].function -> content tool_use
finish_reason "tool_calls" -> stop_reason "tool_use"
finish_reason "length" -> stop_reason "max_tokens"
finish_reason "stop" | null -> stop_reason "end_turn"
usage.prompt_tokens -> input_tokens
usage.completion_tokens -> output_tokens
```

- [ ] **Step 6: 实现 Claude->Responses**

Anthropic request 转 Responses：

```rust
system -> instructions
messages -> input message/function_call/function_call_output
max_tokens -> max_output_tokens
tools -> function tools
tool_choice -> compatible Responses tool_choice
```

Responses response 转 Anthropic：

```rust
output message output_text -> content text
output function_call -> content tool_use
status incomplete with max_output_tokens -> stop_reason max_tokens
status failed -> content text with error message and stop_reason error
usage.input_tokens/output_tokens -> Anthropic usage
```

SSE 映射：

```text
response.created -> message_start
response.output_text.delta -> content_block_delta text_delta
response.function_call_arguments.delta -> content_block_delta input_json_delta
response.output_item.done -> content_block_stop
response.completed/incomplete/failed -> message_delta + message_stop
```

- [ ] **Step 7: 实现 Claude->Gemini**

采用 Task 4 的 Gemini helper，但本地输入为 Anthropic。字段映射：

```rust
system -> systemInstruction
messages user -> contents role user
messages assistant -> contents role model
text -> parts text
image base64 -> inlineData
tool_use -> functionCall
tool_result -> functionResponse
max_tokens -> generationConfig.maxOutputTokens
tools -> functionDeclarations
```

Gemini 响应转 Anthropic：

```rust
parts text -> content text
parts functionCall -> content tool_use
finishReason MAX_TOKENS -> stop_reason max_tokens
finishReason STOP -> stop_reason end_turn
finishReason FUNCTION_CALL -> stop_reason tool_use
usageMetadata -> usage
```

- [ ] **Step 8: 接入 `transform_response` match**

在 `mod.rs` 中完整匹配：

```rust
match kind {
    ProtocolBridgeKind::ResponsesToChat => responses_chat::chat_response_to_responses(status, content_type, body),
    ProtocolBridgeKind::ResponsesToAnthropic => responses_claude::anthropic_response_to_responses(status, content_type, body),
    ProtocolBridgeKind::ResponsesToGemini => responses_gemini::gemini_response_to_responses(status, content_type, body),
    ProtocolBridgeKind::ClaudeToChat => claude_chat::chat_response_to_anthropic(status, content_type, body),
    ProtocolBridgeKind::ClaudeToResponses => claude_responses::responses_response_to_anthropic(status, content_type, body),
    ProtocolBridgeKind::ClaudeToGemini => claude_gemini::gemini_response_to_anthropic(status, content_type, body),
}
```

- [ ] **Step 9: 运行全部 bridge 测试**

Run: `cd src-tauri && cargo test route_protocol_bridge:: --lib`

Expected: PASS。

---

### Task 6: 接入代理 URL、响应转换和模型测试

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs`
- Modify: `src-tauri/src/services/route_model_test_service.rs`
- Modify: `src-tauri/src/models/route_pool.rs`
- Modify: `src/lib/api/types.ts`
- Modify: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Produces: `pub fn build_upstream_request_with_bridge(...) -> Result<BuiltUpstreamRequest, String>`
- Produces: direct model test applies `transform_protocol_bridge_response` before success detection and response text extraction.
- Produces: model test override accepts four canonical protocols for Codex and Claude, and only `gemini` for Gemini.

- [ ] **Step 1: 写代理 URL 失败测试**

在 `route_proxy_service.rs` 测试模块中加入：

```rust
#[test]
fn gemini_bridge_query_merges_with_original_query_and_key() {
    let credential = api_credential("gemini-upstream", "gemini");
    let (url, _, _) = build_upstream_request(
        &credential,
        "codex",
        "/v1/responses",
        Some("trace=1"),
        HeaderMap::new(),
        br#"{"model":"gemini-2.5-flash","stream":true,"input":"hello"}"#,
    )
    .unwrap();

    assert!(url.contains("/v1beta/models/gemini-2.5-flash:streamGenerateContent?"));
    assert!(url.contains("trace=1"));
    assert!(url.contains("alt=sse"));
    assert!(url.contains("key="));
}
```

- [ ] **Step 2: 写模型测试失败测试**

在 `route_model_test_service.rs` 测试模块中加入：

```rust
#[test]
fn codex_model_test_builds_local_responses_body_for_anthropic_upstream() {
    let credential = api_credential("anthropic");
    let request = build_model_test_request(&credential, "codex", Some("claude-sonnet-4-20250514"), None).unwrap();

    assert_eq!(request.interface_format, "anthropic");
    assert_eq!(request.request_path, "/responses");
    let body: Value = serde_json::from_str(&request.request_body_json).unwrap();
    assert_eq!(body["input"], MODEL_TEST_PROMPT);
    assert_eq!(body["max_output_tokens"], 16);
}
```

为 Claude 加：

```rust
#[test]
fn claude_model_test_builds_local_messages_body_for_openai_upstream() {
    let credential = api_credential("openai");
    let request = build_model_test_request(&credential, "claude", Some("gpt-5.5"), None).unwrap();

    assert_eq!(request.interface_format, "openai");
    assert_eq!(request.request_path, "/v1/messages");
    let body: Value = serde_json::from_str(&request.request_body_json).unwrap();
    assert_eq!(body["messages"][0]["role"], "user");
    assert_eq!(body["max_tokens"], 16);
}
```

- [ ] **Step 3: 运行代理和模型测试并确认失败**

Run: `cd src-tauri && cargo test route_proxy_service::tests::gemini_bridge_query_merges_with_original_query_and_key route_model_test_service::tests::codex_model_test_builds_local_responses_body_for_anthropic_upstream route_model_test_service::tests::claude_model_test_builds_local_messages_body_for_openai_upstream --lib`

Expected: FAIL，失败点包含 URL query 不完整或模型测试仍按上游协议构造 body。

- [ ] **Step 4: 暴露内部 request 结果给模型测试**

在 `route_proxy_service.rs` 中增加：

```rust
pub(crate) fn build_upstream_request_with_bridge(
    credential: &SelectedCredential,
    platform: &str,
    path: &str,
    query: Option<&str>,
    headers: HeaderMap,
    body: &[u8],
) -> Result<BuiltUpstreamRequest, String> {
    build_upstream_request_internal(credential, platform, path, query, headers, body)
}
```

保留原 `build_upstream_request` 的三元组返回，避免影响现有调用：

```rust
pub fn build_upstream_request(...) -> Result<(String, HeaderMap, Vec<u8>), String> {
    let request = build_upstream_request_internal(...)?;
    Ok((request.target_url, request.headers, request.body))
}
```

- [ ] **Step 5: 修改直接模型测试 response transform**

`route_model_test_service.rs` 改用 `build_upstream_request_with_bridge`。收到上游 body 后：

```rust
let mut body = body;
if let Some(bridge_kind) = upstream_request.bridge_kind {
    let transformed = transform_protocol_bridge_response(
        bridge_kind,
        status,
        Some("application/json"),
        &body,
    )
    .map_err(|error| format!("could not transform upstream response: {error}"));
    match transformed {
        Ok(response) => body = response.body,
        Err(error) => {
            return finish_outcome(
                pool,
                &platform,
                credential,
                parts,
                next_index,
                Some(status),
                truncate_response_body(&body),
                None,
                Some(error),
                false,
                duration_ms,
                RouteUsageBreakdown::default(),
            )
            .await;
        }
    }
}
```

实际代码需要在已有 `finish_outcome` 参数顺序中保持 credential ownership 正确，不能 clone secret payload 到日志。

- [ ] **Step 6: 修改模型测试请求构造为本地入口协议**

`build_model_test_request` 先解析上游 `interface_format`，但 `request_path` 和 `request_body_json` 按本地平台生成：

```rust
match platform {
    "codex" => ("/responses", json!({
        "model": model,
        "input": MODEL_TEST_PROMPT,
        "temperature": 0,
        "max_output_tokens": 16
    })),
    "claude" => ("/v1/messages", json!({
        "model": model,
        "messages": [{"role": "user", "content": MODEL_TEST_PROMPT}],
        "max_tokens": 16
    })),
    "gemini" => (format!("/v1beta/models/{}:generateContent", gemini_path_model(&mappings, requested_model)), json!({
        "contents": [{"role": "user", "parts": [{"text": MODEL_TEST_PROMPT}]}],
        "generationConfig": {"temperature": 0, "maxOutputTokens": 16}
    })),
    _ => ("/chat/completions".to_string(), json!({
        "model": model,
        "messages": [{"role": "user", "content": MODEL_TEST_PROMPT}],
        "temperature": 0,
        "max_tokens": 16
    })),
}
```

`interface_format` 字段继续记录选中上游协议，便于 UI 展示。

- [ ] **Step 7: 放宽模型测试 override 校验**

`validate_model_test_interface_override` 改为：

```rust
let allowed = match platform {
    "codex" | "claude" => matches!(requested, "openai" | "openai-responses" | "anthropic" | "gemini"),
    "gemini" => requested == "gemini",
    "grok" | "opencode" | "openclaw" | "hermes" => requested == "openai",
    _ => false,
};
```

错误码保持 `validation.route_model_test_interface_format`。

- [ ] **Step 8: 通过代理模型测试路径保持本地 entry path**

在 `test_model_through_proxy_with_root_certificate` 中不要用 `normalize_api_upstream_path(&parts.interface_format, &parts.request_path)` 作为 local entry path。改为：

```rust
let entry_path = normalize_local_model_test_entry_path(&platform, &parts.request_path);
```

helper 行为：

```rust
codex -> strip leading /v1 and keep /responses for route proxy base /v1
claude -> keep /v1/messages
gemini -> keep Gemini native path
other -> existing normalize_api_upstream_path behavior
```

- [ ] **Step 9: 运行模型测试与代理测试**

Run: `cd src-tauri && cargo test route_proxy_service::tests route_model_test_service::tests --lib`

Expected: PASS。

---

### Task 7: 补齐导入导出、deeplink 和配置写入回归测试

**Files:**
- Modify: `src-tauri/src/adapters/route_config/codex.rs`
- Modify: `src-tauri/src/adapters/route_config/json_agent.rs`
- Modify: `src-tauri/src/adapters/route_config/mod.rs`
- Modify: `src-tauri/src/services/route_config_service.rs`
- Modify: `src-tauri/src/services/target_service.rs`
- Modify: `tests/TauriConfig.test.ts`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `README.md`

**Interfaces:**
- Consumes: Task 1 and Task 2 canonical protocol values.
- Produces: Codex generated config has `/v1` Base URL, `wire_api = "responses"`, and no inline `api_key`.
- Produces: Claude generated config has root Base URL and `/v1/messages` client endpoint semantics.
- Produces: README documents four upstream protocols for Codex and Claude.

- [ ] **Step 1: 写配置写入回归断言**

在 Rust adapter 测试和 `tests/TauriConfig.test.ts` 中确认：

```rust
assert!(rendered.contains("base_url = \"http://127.0.0.1:43111/v1\""));
assert!(rendered.contains("wire_api = \"responses\""));
assert!(!rendered.contains("api_key = \""));
```

Claude JSON adapter 断言：

```rust
assert_eq!(route_proxy["baseUrl"], "http://127.0.0.1:43111");
assert_eq!(env["ANTHROPIC_BASE_URL"], "http://127.0.0.1:43111");
```

- [ ] **Step 2: 运行配置测试并确认当前状态**

Run: `cd src-tauri && cargo test route_config --lib`

Run: `pnpm test:run tests/TauriConfig.test.ts`

Expected: PASS；若失败，只修复与 Base URL、wire_api、api_key 写入相关的断言和实现。

- [ ] **Step 3: 更新 README**

在 README 的路由代理或账号配置说明中加入中文说明：

```markdown
Codex 和 Claude 的 API 路由账号可以选择 `openai`、`openai-responses`、`anthropic`、`gemini` 四种上游协议。Codex 本地入口仍使用 OpenAI Responses；Claude 本地入口仍使用 Anthropic Messages。AI Switch 会在本地入口协议和上游账号协议不一致时进行桥接转换。
```

- [ ] **Step 4: 运行导入导出和 deeplink 测试**

Run: `cd src-tauri && cargo test route_credential_transfer route_credential_transfer_import deeplink cpa_export --lib`

Expected: PASS。

- [ ] **Step 5: 文档和配置 checkpoint**

Run: `git diff --check`

Expected: PASS。

---

### Task 8: 全量验证和残留扫描

**Files:**
- No create.
- Modify only files already touched by earlier tasks if validation exposes scoped failures.

**Interfaces:**
- Consumes: all previous task outputs.
- Produces: full Rust and frontend validation pass, or a precise list of unrelated failures.

- [ ] **Step 1: 扫描旧协议残留**

Run: `rg -n "anthropic-messages|anthropic_messages" src-tauri/src src tests docs/superpowers/specs docs/superpowers/plans`

Expected: no matches except this plan only if the command target includes the plan file. If this plan appears in results, confirm no production code or test fixture contains the legacy value.

- [ ] **Step 2: 扫描 bridge variant 覆盖**

Run: `rg -n "ProtocolBridgeKind|ResponsesToAnthropic|ResponsesToGemini|ClaudeToChat|ClaudeToResponses|ClaudeToGemini" src-tauri/src/services/route_protocol_bridge src-tauri/src/services/route_proxy_service.rs src-tauri/src/services/route_model_test_service.rs`

Expected: each variant appears in selection tests, request preparation, response transform match, and at least one converter module test.

- [ ] **Step 3: 运行 Rust 全量测试**

Run: `pnpm rust:test`

Expected: PASS。

- [ ] **Step 4: 运行前端测试**

Run: `pnpm test:run`

Expected: PASS。

- [ ] **Step 5: 运行类型检查**

Run: `pnpm typecheck`

Expected: PASS。

- [ ] **Step 6: 检查未提交 diff**

Run: `git status --short`

Expected: 只包含本任务相关文件和进入本轮前已有的未提交文件。不要回滚用户已有改动。

## Self-Review

- Spec coverage: Task 1 和 Task 2 覆盖旧协议删除、UI 选项和 Codex-only custom tool 兼容开关；Task 3 覆盖路由矩阵、端点和 query；Task 4 覆盖 Codex Responses 到三类上游；Task 5 覆盖 Claude Messages 到三类上游；Task 6 覆盖代理和模型测试同路径；Task 7 覆盖配置写入、导入导出和文档；Task 8 覆盖全量验证。
- Placeholder scan: 已扫描常见英文占位符模式，计划正文没有空白实现点。
- Type consistency: `ProtocolBridgeKind`、`PreparedBridgeRequest.upstream_query`、`BuiltUpstreamRequest.bridge_kind`、`InterfaceFormat` 在任务之间名称一致。

## 执行交接

Plan complete and saved to `docs/superpowers/plans/2026-08-08-multi-protocol-route-bridges.md`. Two execution options:

1. Subagent-Driven (recommended) - dispatch a fresh subagent per task, review between tasks, fast iteration.
2. Inline Execution - execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
