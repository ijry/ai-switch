# Responses Custom Tool Compat Switch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Responses `custom` tool rewrite an account-level on/off switch stored in `config_json`, default off.

**Architecture:** Keep the existing rewrite/restore helpers in `route_proxy_service`. Gate them with `config_json.responses_custom_tool_compat`. Persist the boolean through API account create/edit via existing `config_json` paths, and expose a checkbox in `AccountsScreen`.

**Tech Stack:** Rust/Tauri, React + TypeScript, Vitest, existing route credential/proxy services.

## Global Constraints

- Work directly on `main`; do not create or switch branches/worktrees.
- Do not revert unrelated dirty worktree changes.
- Default is `off`; missing field is treated as `off`.
- Official OAuth credentials are never rewritten.
- No DB migration; store only in `config_json`.
- Field name must be exactly `responses_custom_tool_compat`.

---

### Task 1: Proxy Switch Gate

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs`
- Test: unit tests inside `src-tauri/src/services/route_proxy_service.rs`

**Interfaces:**
- Consumes: `config.responses_custom_tool_compat: bool`
- Produces: rewrite only when switch is true and request is Responses path

- [ ] **Step 1: Write failing tests for switch semantics**

Update/add tests near `build_upstream_request_rewrites_custom_tools_for_responses_api_relays`:

```rust
#[test]
fn build_upstream_request_skips_custom_tool_rewrite_when_switch_off() {
    let mut credential = api_credential("xiaomi", "openai-responses");
    credential.config_json = serde_json::json!({
        "base_url": "https://api.xiaomi.example/v1",
        "interface_format": "openai-responses",
        "model_mappings": [{"from":"gpt-5","to":"mi-model"}],
        "responses_custom_tool_compat": false
    })
    .to_string();

    let body = br#"{
        "model":"gpt-5",
        "tools":[{"type":"custom","name":"apply_patch","description":"patch files"}]
    }"#;
    let (_, _, rewritten) = build_upstream_request(
        &credential,
        "codex",
        "/responses",
        None,
        HeaderMap::new(),
        body,
    )
    .expect("request");
    let value: Value = serde_json::from_slice(&rewritten).expect("json");
    assert_eq!(value.pointer("/model").and_then(Value::as_str), Some("mi-model"));
    assert_eq!(value.pointer("/tools/0/type").and_then(Value::as_str), Some("custom"));
}

#[test]
fn build_upstream_request_rewrites_custom_tools_when_switch_on() {
    let mut credential = api_credential("xiaomi", "openai-responses");
    credential.config_json = serde_json::json!({
        "base_url": "https://api.xiaomi.example/v1",
        "interface_format": "openai-responses",
        "model_mappings": [{"from":"gpt-5","to":"mi-model"}],
        "responses_custom_tool_compat": true
    })
    .to_string();

    let body = br#"{
        "model":"gpt-5",
        "tools":[{"type":"custom","name":"apply_patch","description":"patch files"}]
    }"#;
    let (_, _, rewritten) = build_upstream_request(
        &credential,
        "codex",
        "/responses",
        None,
        HeaderMap::new(),
        body,
    )
    .expect("request");
    let value: Value = serde_json::from_slice(&rewritten).expect("json");
    assert_eq!(value.pointer("/tools/0/type").and_then(Value::as_str), Some("function"));
}

#[test]
fn build_upstream_request_skips_custom_tool_rewrite_when_switch_missing() {
    let mut credential = api_credential("xiaomi", "openai-responses");
    credential.config_json = serde_json::json!({
        "base_url": "https://api.xiaomi.example/v1",
        "interface_format": "openai-responses",
        "model_mappings": []
    })
    .to_string();

    let body = br#"{"tools":[{"type":"custom","name":"apply_patch"}]}"#;
    let (_, _, rewritten) = build_upstream_request(
        &credential,
        "codex",
        "/responses",
        None,
        HeaderMap::new(),
        body,
    )
    .expect("request");
    let value: Value = serde_json::from_slice(&rewritten).expect("json");
    assert_eq!(value.pointer("/tools/0/type").and_then(Value::as_str), Some("custom"));
}

#[test]
fn build_upstream_request_skips_custom_tool_rewrite_on_non_responses_path_even_when_on() {
    let mut credential = api_credential("xiaomi", "openai");
    credential.config_json = serde_json::json!({
        "base_url": "https://api.xiaomi.example/v1",
        "interface_format": "openai",
        "model_mappings": [],
        "responses_custom_tool_compat": true
    })
    .to_string();

    let body = br#"{"tools":[{"type":"custom","name":"apply_patch"}]}"#;
    let (_, _, rewritten) = build_upstream_request(
        &credential,
        "codex",
        "/chat/completions",
        None,
        HeaderMap::new(),
        body,
    )
    .expect("request");
    let value: Value = serde_json::from_slice(&rewritten).expect("json");
    assert_eq!(value.pointer("/tools/0/type").and_then(Value::as_str), Some("custom"));
}
```

Also change the existing always-on test `build_upstream_request_rewrites_custom_tools_for_responses_api_relays` so its fixture includes `"responses_custom_tool_compat": true`.

- [ ] **Step 2: Run tests and confirm switch-on behavior currently mismatches default-off requirement**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml build_upstream_request_rewrites_custom_tools_for_responses_api_relays -- --nocapture
```

Expected: existing always-on path still rewrites even without the switch field until implementation lands. After adding the new off/missing tests, those should fail until the gate is implemented.

- [ ] **Step 3: Implement config-gated rewrite**

In `build_api_upstream_request`, replace:

```rust
if should_rewrite_custom_tools_for_api(interface_format, path) {
    rewritten_body = apply_responses_custom_tool_compat(&rewritten_body);
}
```

with:

```rust
let custom_tool_compat = config
    .get("responses_custom_tool_compat")
    .and_then(Value::as_bool)
    .unwrap_or(false);
if custom_tool_compat && should_rewrite_custom_tools_for_api(interface_format, path) {
    rewritten_body = apply_responses_custom_tool_compat(&rewritten_body);
}
```

Optional helper for clarity:

```rust
fn responses_custom_tool_compat_enabled(config: &Value) -> bool {
    config
        .get("responses_custom_tool_compat")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}
```

Keep `should_rewrite_custom_tools_for_api` as the path/format gate only.

- [ ] **Step 4: Run proxy unit tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml route_proxy_service::tests -- --nocapture
```

Expected: all route_proxy_service tests pass, including on/off/missing/non-responses cases.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/services/route_proxy_service.rs
git commit -m "feat: gate responses custom tool rewrite behind account switch"
```

---

### Task 2: Persist Switch On API Create

**Files:**
- Modify: `src-tauri/src/models/route_credential.rs`
- Modify: `src-tauri/src/services/route_credential_service.rs`
- Modify: `src-tauri/src/services/deeplink_service.rs`
- Modify: `src/lib/api/types.ts`
- Modify: `src/components/deeplink/DeepLinkImportDialog.tsx` only if type construction requires the new optional field explicitly

**Interfaces:**
- Consumes: `CreateApiRouteCredentialInput.responses_custom_tool_compat?: boolean | null`
- Produces: `config_json.responses_custom_tool_compat: boolean`

- [ ] **Step 1: Write failing create-service test**

In `src-tauri/src/services/route_credential_service.rs` tests, add coverage that create writes the boolean into config:

```rust
#[tokio::test]
async fn create_api_credential_persists_responses_custom_tool_compat() {
    // use the existing test pool/helper pattern in this file
    let created = RouteCredentialService::create_api(
        &pool,
        CreateApiRouteCredentialInput {
            platform: "codex".into(),
            display_name: "Xiaomi Relay".into(),
            api_key: "sk-test".into(),
            base_url: "https://api.xiaomi.example/v1".into(),
            interface_format: "openai-responses".into(),
            model_mappings_json: "[]".into(),
            api_key_field: None,
            preview_json: None,
            batch_id: None,
            responses_custom_tool_compat: Some(true),
        },
    )
    .await
    .expect("create");

    let config: serde_json::Value = serde_json::from_str(&created.config_json).expect("config");
    assert_eq!(config["responses_custom_tool_compat"], serde_json::json!(true));
}
```

If the file currently has no async create tests, add a pure unit-style assertion around the config JSON construction path by extracting a small helper, or extend the nearest existing create test. Prefer the smallest change that proves the field is written.

Also update any `CreateApiRouteCredentialInput { ... }` struct literals that break compilation after adding the field, including:

- `src-tauri/src/services/deeplink_service.rs` `to_create_api_input`
- any tests constructing the struct

Use:

```rust
responses_custom_tool_compat: None,
```

for deep-link imports so they keep default off.

- [ ] **Step 2: Extend create input model**

Rust:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateApiRouteCredentialInput {
    pub platform: String,
    pub display_name: String,
    pub api_key: String,
    pub base_url: String,
    pub interface_format: String,
    pub model_mappings_json: String,
    #[serde(default)]
    pub api_key_field: Option<String>,
    pub preview_json: Option<String>,
    pub batch_id: Option<String>,
    #[serde(default)]
    pub responses_custom_tool_compat: Option<bool>,
}
```

TypeScript:

```ts
export type CreateApiRouteCredentialInput = {
  platform: string;
  display_name: string;
  api_key: string;
  base_url: string;
  interface_format: InterfaceFormat;
  model_mappings_json: string;
  api_key_field?: AnthropicApiKeyField | string | null;
  preview_json?: string | null;
  batch_id?: string | null;
  responses_custom_tool_compat?: boolean | null;
};
```

- [ ] **Step 3: Persist field in create service**

In `RouteCredentialService::create_api`, when building config:

```rust
let mut config = json!({
    "base_url": input.base_url.trim(),
    "interface_format": input.interface_format,
    "model_mappings": serde_json::from_str::<serde_json::Value>(&input.model_mappings_json)?,
    "responses_custom_tool_compat": input.responses_custom_tool_compat.unwrap_or(false),
});
```

Always write the boolean so edit form round-trips cleanly.

- [ ] **Step 4: Compile/test create path**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml route_credential_service -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml deeplink_service -- --nocapture
```

Expected: compile succeeds; create persists `true/false`; deep-link create remains default false.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/models/route_credential.rs src-tauri/src/services/route_credential_service.rs src-tauri/src/services/deeplink_service.rs src/lib/api/types.ts
git commit -m "feat: persist responses custom tool compat on API create"
```

---

### Task 3: Accounts UI Switch

**Files:**
- Modify: `src/screens/AccountsScreen.tsx`
- Test: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes: `CreateApiRouteCredentialInput.responses_custom_tool_compat?: boolean | null`
- Produces: create payload boolean and edit `config_json.responses_custom_tool_compat`

- [ ] **Step 1: Write failing UI tests**

Update existing create assertion(s) that fully match the create payload so they include default off:

```ts
expect(createApiRouteCredential).toHaveBeenCalledWith({
  platform: "codex",
  display_name: "Upstream API",
  api_key: "sk-1",
  base_url: "https://api.upstream.test/v1",
  interface_format: "openai-responses",
  model_mappings_json: "[{\"from\":\"gpt-5\",\"to\":\"up-gpt\"}]",
  preview_json: null,
  batch_id: null,
  responses_custom_tool_compat: false,
});
```

Add focused cases:

```ts
it("creates API account with responses custom tool compat enabled when checked", async () => {
  renderScreen();
  await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
  await userEvent.click(screen.getByRole("button", { name: "API 账号" }));
  await userEvent.type(screen.getByLabelText("API 账号名称"), "Compat API");
  await userEvent.type(screen.getByLabelText("API Key"), "sk-compat");
  await userEvent.clear(screen.getByLabelText("Base URL"));
  await userEvent.type(screen.getByLabelText("Base URL"), "https://api.upstream.test/v1");
  await userEvent.selectOptions(screen.getByLabelText("接口格式"), "openai-responses");
  await userEvent.click(screen.getByLabelText("兼容 custom 工具（Responses 中转）"));
  await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

  await waitFor(() =>
    expect(createApiRouteCredential).toHaveBeenCalledWith(
      expect.objectContaining({
        display_name: "Compat API",
        responses_custom_tool_compat: true,
      }),
    ),
  );
});

it("loads and saves responses custom tool compat from API account config", async () => {
  const api = {
    ...credentialsFixture[1],
    config_json: JSON.stringify({
      base_url: "https://api.example.com/v1",
      interface_format: "openai-responses",
      model_mappings: [],
      responses_custom_tool_compat: true,
    }),
  };
  vi.mocked(listRouteCredentials).mockResolvedValue([api]);
  vi.mocked(updateRouteCredential).mockResolvedValue({
    ...api,
    display_name: "API Account Updated",
  });

  renderScreen();
  await userEvent.click(await screen.findByRole("button", { name: "编辑" }));
  const checkbox = await screen.findByLabelText("兼容 custom 工具（Responses 中转）");
  expect(checkbox).toBeChecked();
  await userEvent.click(checkbox); // turn off
  await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

  await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
  const payload = vi.mocked(updateRouteCredential).mock.calls[0][1];
  const config = JSON.parse(payload.config_json);
  expect(config.responses_custom_tool_compat).toBe(false);
});
```

Use the real button labels already present in `AccountsScreen` if they differ slightly; keep aria-label exact for the checkbox.

- [ ] **Step 2: Add helper + state + config writer**

Helper near other config helpers:

```ts
function responsesCustomToolCompatFromConfig(config: Record<string, unknown>): boolean {
  return config.responses_custom_tool_compat === true;
}
```

Extend `apiConfigJsonWithFields`:

```ts
function apiConfigJsonWithFields(
  configJson: string,
  baseUrl: string,
  interfaceFormat: InterfaceFormat,
  mappings: ModelMapping[],
  apiKeyField: AnthropicApiKeyField,
  responsesCustomToolCompat = false,
) {
  const config = parseJsonObject(configJson);
  config.base_url = baseUrl.trim();
  config.interface_format = interfaceFormat;
  config.model_mappings = mappings;
  config.responses_custom_tool_compat = responsesCustomToolCompat;
  if (isAnthropicInterfaceFormat(interfaceFormat)) {
    config.api_key_field = apiKeyField;
  } else {
    delete config.api_key_field;
  }
  return JSON.stringify(config, null, 2);
}
```

Create/edit state:

```ts
const [apiResponsesCustomToolCompat, setApiResponsesCustomToolCompat] = useState(false);
const [editResponsesCustomToolCompat, setEditResponsesCustomToolCompat] = useState(false);
```

Reset create state to `false` when opening create dialog / switching create mode.
When loading edit API account:

```ts
setEditResponsesCustomToolCompat(responsesCustomToolCompatFromConfig(config));
```

- [ ] **Step 3: Wire create/edit payloads and checkbox UI**

Create payload:

```ts
const input = {
  platform: activePlatform,
  display_name: apiKeys.length > 1 ? `${apiName.trim()} ${index + 1}` : apiName.trim(),
  api_key: key,
  base_url: apiBaseUrl,
  interface_format: apiInterfaceFormat,
  model_mappings_json: JSON.stringify(normalizedMappings.mappings),
  preview_json: apiPreviewJson.trim() || null,
  batch_id: batch?.id ?? null,
  responses_custom_tool_compat: apiResponsesCustomToolCompat,
};
```

Edit config:

```ts
apiConfigJsonWithFields(
  editConfigJson.trim() || "{}",
  editApiBaseUrl,
  editApiInterfaceFormat,
  normalizedMappings.mappings,
  editApiKeyField,
  editResponsesCustomToolCompat,
)
```

UI checkbox in both create and edit API forms, under interface format / Claude auth field and above model mappings:

```tsx
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
```

Mirror the same control for edit state.

- [ ] **Step 4: Run frontend tests**

Run:

```powershell
pnpm vitest run tests/AccountsScreen.test.tsx
```

Expected: create default false, checked true, edit load/save round-trip all pass. Update any full payload assertions broken by the new field.

- [ ] **Step 5: Commit**

```powershell
git add src/screens/AccountsScreen.tsx tests/AccountsScreen.test.tsx
git commit -m "feat: add responses custom tool compat switch to account UI"
```

---

### Task 4: Regression Verification

**Files:**
- No new files required.

- [ ] **Step 1: Run backend proxy + credential tests**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml route_proxy_service::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml route_credential_service -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml deeplink_service -- --nocapture
```

Expected: pass.

- [ ] **Step 2: Run frontend account tests / typecheck**

```powershell
pnpm vitest run tests/AccountsScreen.test.tsx
pnpm typecheck
```

Expected: pass.

- [ ] **Step 3: Manual smoke checklist**

1. Create Codex API account mapped to Responses relay, leave switch off.
2. Confirm request keeps `tools[].type = custom`.
3. Enable switch, save, retry.
4. Confirm rewrite to `function` and no `responses_feature_not_supported` for custom tools.
5. Official OAuth account remains unchanged.

- [ ] **Step 4: Final commit only if verification fixes were needed**

If Step 1-2 required small fixes, commit them separately with a focused message. Do not bundle unrelated dirty files.

---

## Spec Coverage Check

- on/off switch with default off -> Task 1 + Task 2 + Task 3
- store in `config_json.responses_custom_tool_compat` -> Task 2 + Task 3
- only API + Responses path -> Task 1
- official OAuth never rewritten -> Task 1 (no official path changes)
- create/edit UI checkbox -> Task 3
- no DB migration -> all tasks
- tests for off/on/missing/non-responses -> Task 1
- frontend create/edit coverage -> Task 3

## Placeholder Scan

- No TBD/TODO left.
- Exact field name, labels, files, commands, and code included.

## Type Consistency

- Field: `responses_custom_tool_compat`
- Create input optional boolean on both TS and Rust
- Config JSON boolean always written on create/edit
- Proxy reads boolean with `unwrap_or(false)`
