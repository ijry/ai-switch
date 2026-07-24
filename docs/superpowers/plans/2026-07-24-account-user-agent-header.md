# Account User-Agent Header Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let every route credential set a custom `User-Agent` from create/edit UI with presets, persist it in `config.headers`, and make that value override built-in forced UA (including Grok chat-proxy).

**Architecture:** Store UA in existing `config_json.headers["User-Agent"]`. Add a request-builder helper that force-sets non-empty credential UA after platform identity headers. API create accepts optional `user_agent`; edit merges UA into config for both API and official accounts. Frontend exposes preset select + free-text input.

**Tech Stack:** Rust/Tauri, React + TypeScript, Vitest, existing route credential/proxy services.

## Global Constraints

- Work directly on `main`; do not create or switch branches/worktrees.
- Do not revert unrelated dirty worktree changes.
- Scope is all platforms, not only Grok.
- Account custom UA always wins over forced/default UA.
- Empty/whitespace UA omits the key and preserves existing defaults.
- No full custom-headers editor in this plan.
- No DB migration; storage is only `config_json.headers`.
- Key name written as `User-Agent` (read accepts `User-Agent` or `user-agent`).

## File Map

| File | Responsibility |
|---|---|
| `src-tauri/src/services/route_proxy_service.rs` | Final UA override helper; apply on API + official builders |
| `src-tauri/src/models/route_credential.rs` | Optional `user_agent` on create API input |
| `src-tauri/src/services/route_credential_service.rs` | Persist create-time UA into `config.headers` |
| `src-tauri/src/services/deeplink_service.rs` | Compile with new optional field (`None`) |
| `src/lib/api/types.ts` | Frontend create input type |
| `src/lib/accountUserAgent.ts` | Presets + read/write helpers for headers UA |
| `src/screens/AccountsScreen.tsx` | Create/edit UI + save/hydrate |
| `tests/AccountsScreen.test.tsx` | Create/edit UA UI tests |
| `tests/accountUserAgent.test.ts` | Helper unit tests |

---

### Task 1: Proxy Custom UA Override

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs`
- Test: unit tests in the same file

**Interfaces:**
- Consumes: `config.headers` object, case-insensitive `User-Agent` / `user-agent`
- Produces: `apply_credential_user_agent(headers, config) -> Result<(), String>`
- Later tasks rely on: custom non-empty UA is the final outbound `user-agent`

**Locked behavior for Grok:**
- No stored UA => forced workspace UA remains (`xai-grok-workspace/0.2.93`)
- Non-empty stored UA => stored UA wins after forced headers
- Import normalization still upgrades new CPA imports; this task only changes request builder
- Existing test fixture that stores `User-Agent: grok-cli` must be updated so "no intentional custom UA" still asserts forced workspace UA (remove UA from fixture headers)

- [ ] **Step 1: Write failing tests**

Add near existing Grok header tests:

```rust
#[test]
fn build_upstream_request_custom_user_agent_overrides_grok_forced_ua() {
    let credential = SelectedCredential {
        id: "official-grok-custom-ua".to_string(),
        platform: "grok".to_string(),
        kind: "official".to_string(),
        display_name: "Grok Custom UA".to_string(),
        status: "ok".to_string(),
        secret_payload_json: r#"{"access_token":"at-xai"}"#.to_string(),
        config_json: serde_json::json!({
            "base_url": "https://cli-chat-proxy.grok.com/v1",
            "headers": {
                "User-Agent": "MyGrokClient/9.9.9",
                "X-Client-Name": "grok-cli"
            }
        })
        .to_string(),
    };

    let (_, headers, _) = build_upstream_request(
        &credential,
        "grok",
        "/chat/completions",
        None,
        HeaderMap::new(),
        br#"{"model":"grok-4.5"}"#,
    )
    .expect("request");

    assert_eq!(
        headers
            .get("user-agent")
            .and_then(|value| value.to_str().ok()),
        Some("MyGrokClient/9.9.9")
    );
    assert_eq!(
        headers
            .get("x-grok-client-version")
            .and_then(|value| value.to_str().ok()),
        Some("0.2.93")
    );
    assert_eq!(
        headers
            .get("x-xai-token-auth")
            .and_then(|value| value.to_str().ok()),
        Some("xai-grok-cli")
    );
}

#[test]
fn build_upstream_request_empty_user_agent_keeps_grok_forced_ua() {
    let credential = SelectedCredential {
        id: "official-grok-empty-ua".to_string(),
        platform: "grok".to_string(),
        kind: "official".to_string(),
        display_name: "Grok Empty UA".to_string(),
        status: "ok".to_string(),
        secret_payload_json: r#"{"access_token":"at-xai"}"#.to_string(),
        config_json: serde_json::json!({
            "base_url": "https://cli-chat-proxy.grok.com/v1",
            "headers": {
                "User-Agent": "   "
            }
        })
        .to_string(),
    };

    let (_, headers, _) = build_upstream_request(
        &credential,
        "grok",
        "/chat/completions",
        None,
        HeaderMap::new(),
        br#"{"model":"grok-4.5"}"#,
    )
    .expect("request");

    assert_eq!(
        headers
            .get("user-agent")
            .and_then(|value| value.to_str().ok()),
        Some("xai-grok-workspace/0.2.93")
    );
}

#[test]
fn build_upstream_request_custom_user_agent_applies_on_api_accounts() {
    let mut credential = api_credential("relay-ua", "openai");
    credential.config_json = serde_json::json!({
        "base_url": "https://api.example.com/v1",
        "interface_format": "openai",
        "model_mappings": [],
        "headers": {
            "user-agent": "RelayBot/1.0"
        }
    })
    .to_string();

    let (_, headers, _) = build_upstream_request(
        &credential,
        "codex",
        "/chat/completions",
        None,
        HeaderMap::new(),
        br#"{"model":"gpt-5.5"}"#,
    )
    .expect("request");

    assert_eq!(
        headers
            .get("user-agent")
            .and_then(|value| value.to_str().ok()),
        Some("RelayBot/1.0")
    );
}
```

Also update existing test `build_upstream_request_uses_official_cpa_base_url_and_headers` fixture headers to:

```rust
"headers": {
    "X-Client-Name": "grok-cli"
}
```

Keep assertions that final UA is `xai-grok-workspace/0.2.93`.

- [ ] **Step 2: Run tests to verify they fail**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml build_upstream_request_custom_user_agent -- --nocapture
```

Expected: FAIL because helper/call sites do not exist yet, or custom UA does not win.

- [ ] **Step 3: Implement helper and call sites**

In `route_proxy_service.rs`:

```rust
fn credential_user_agent(config: &Value) -> Option<&str> {
    let Some(Value::Object(extra)) = config.get("headers") else {
        return None;
    };
    for (name, value) in extra {
        if name.eq_ignore_ascii_case("user-agent") {
            return value.as_str().map(str::trim).filter(|item| !item.is_empty());
        }
    }
    None
}

fn apply_credential_user_agent(headers: &mut HeaderMap, config: &Value) -> Result<(), String> {
    let Some(user_agent) = credential_user_agent(config) else {
        return Ok(());
    };
    insert_header(headers, "user-agent", user_agent)
}
```

At the end of `build_api_upstream_request`, before returning:

```rust
apply_credential_user_agent(headers, config)?;
Ok((target_url, headers.clone(), rewritten_body))
```

At the end of `build_official_upstream_request`, after Grok forced headers and before return:

```rust
if is_official_grok_platform(platform) && is_grok_cli_chat_proxy_base_url(base_url) {
    apply_official_grok_cli_headers(headers)?;
}
apply_credential_user_agent(headers, config)?;
let target_url = build_target_url(base_url, path, query);
Ok((target_url, headers.clone(), body.to_vec()))
```

- [ ] **Step 4: Run tests to verify they pass**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml build_upstream_request_custom_user_agent -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml build_upstream_request_uses_official_cpa_base_url_and_headers -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml build_upstream_request_empty_user_agent_keeps_grok_forced_ua -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml build_upstream_request_custom_user_agent_applies_on_api_accounts -- --nocapture
```

Expected: PASS

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/services/route_proxy_service.rs
git commit -m "feat: let credential User-Agent override forced proxy headers"
```

---

### Task 2: Persist User-Agent on API Create

**Files:**
- Modify: `src-tauri/src/models/route_credential.rs`
- Modify: `src-tauri/src/services/route_credential_service.rs`
- Modify: `src-tauri/src/services/deeplink_service.rs`
- Modify: `src/lib/api/types.ts`
- Test: unit tests in `route_credential_service.rs`

**Interfaces:**
- Consumes: `CreateApiRouteCredentialInput.user_agent: Option<String>`
- Produces: `config.headers.User-Agent` when non-empty after trim
- Frontend later sends the same field name: `user_agent`

- [ ] **Step 1: Write failing service tests**

Add in `route_credential_service.rs` tests:

```rust
#[tokio::test]
async fn create_api_credential_persists_user_agent_header() {
    let pool = crate::database::create_memory_pool().await.expect("pool");
    crate::database::run_migrations(&pool).await.expect("migrations");

    let created = RouteCredentialService::create_api(
        &pool,
        CreateApiRouteCredentialInput {
            platform: "grok".into(),
            display_name: "Grok UA".into(),
            api_key: "sk-test".into(),
            base_url: "https://api.x.ai/v1".into(),
            interface_format: "openai".into(),
            model_mappings_json: "[]".into(),
            api_key_field: None,
            preview_json: None,
            batch_id: None,
            responses_custom_tool_compat: None,
            user_agent: Some("  MyGrokClient/9.9.9  ".into()),
        },
    )
    .await
    .expect("create");

    let config: serde_json::Value = serde_json::from_str(&created.config_json).expect("config");
    assert_eq!(
        config["headers"]["User-Agent"],
        serde_json::json!("MyGrokClient/9.9.9")
    );
}

#[tokio::test]
async fn create_api_credential_omits_user_agent_when_empty() {
    let pool = crate::database::create_memory_pool().await.expect("pool");
    crate::database::run_migrations(&pool).await.expect("migrations");

    let created = RouteCredentialService::create_api(
        &pool,
        CreateApiRouteCredentialInput {
            platform: "codex".into(),
            display_name: "No UA".into(),
            api_key: "sk-test".into(),
            base_url: "https://api.example.com/v1".into(),
            interface_format: "openai".into(),
            model_mappings_json: "[]".into(),
            api_key_field: None,
            preview_json: None,
            batch_id: None,
            responses_custom_tool_compat: None,
            user_agent: Some("   ".into()),
        },
    )
    .await
    .expect("create");

    let config: serde_json::Value = serde_json::from_str(&created.config_json).expect("config");
    assert!(config.get("headers").is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml create_api_credential_persists_user_agent_header -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml create_api_credential_omits_user_agent_when_empty -- --nocapture
```

Expected: FAIL on unknown field / compile error.

- [ ] **Step 3: Implement input + persistence**

In `src-tauri/src/models/route_credential.rs` add to `CreateApiRouteCredentialInput`:

```rust
#[serde(default)]
pub user_agent: Option<String>,
```

In `route_credential_service.rs` `create_api`:

```rust
let mut config = json!({
    "base_url": input.base_url.trim(),
    "interface_format": input.interface_format,
    "model_mappings": serde_json::from_str::<serde_json::Value>(&input.model_mappings_json)?,
    "responses_custom_tool_compat": input.responses_custom_tool_compat.unwrap_or(false),
});
if let Some(api_key_field) = api_key_field {
    config["api_key_field"] = json!(api_key_field);
}
if let Some(user_agent) = input
    .user_agent
    .as_deref()
    .map(str::trim)
    .filter(|value| !value.is_empty())
{
    config["headers"] = json!({ "User-Agent": user_agent });
}
```

Update every Rust struct literal constructing `CreateApiRouteCredentialInput` (deeplink service + existing tests) with:

```rust
user_agent: None,
```

In `src/lib/api/types.ts`:

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
  user_agent?: string | null;
};
```

- [ ] **Step 4: Run tests to verify they pass**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml create_api_credential_persists_user_agent_header -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml create_api_credential_omits_user_agent_when_empty -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml create_api_credential_defaults_responses_custom_tool_compat_off -- --nocapture
```

Expected: PASS

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/models/route_credential.rs src-tauri/src/services/route_credential_service.rs src-tauri/src/services/deeplink_service.rs src/lib/api/types.ts
git commit -m "feat: persist optional User-Agent on API account create"
```

---

### Task 3: Frontend UA Helpers + Accounts UI

**Files:**
- Create: `src/lib/accountUserAgent.ts`
- Create: `tests/accountUserAgent.test.ts`
- Modify: `src/screens/AccountsScreen.tsx`
- Test: helper unit tests; screen tests in Task 4

**Interfaces:**
- Produces:
  - `USER_AGENT_PRESETS`
  - `BROWSER_USER_AGENT`
  - `readUserAgentFromConfig(config): string`
  - `writeUserAgentToConfig(config, userAgent): Record<string, unknown>`
  - `matchUserAgentPreset(value): UserAgentPresetId`
- Consumes: `config.headers` object

- [ ] **Step 1: Write failing helper tests**

Create `tests/accountUserAgent.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  BROWSER_USER_AGENT,
  matchUserAgentPreset,
  readUserAgentFromConfig,
  writeUserAgentToConfig,
} from "../src/lib/accountUserAgent";

describe("accountUserAgent", () => {
  it("reads User-Agent case-insensitively", () => {
    expect(readUserAgentFromConfig({ headers: { "user-agent": "Bot/1" } })).toBe("Bot/1");
    expect(readUserAgentFromConfig({ headers: { "User-Agent": "Bot/2" } })).toBe("Bot/2");
    expect(readUserAgentFromConfig({})).toBe("");
  });

  it("writes and clears User-Agent while preserving other headers", () => {
    const withUa = writeUserAgentToConfig(
      { headers: { "X-Test": "1" }, base_url: "https://example.com" },
      "  Bot/9  ",
    );
    expect(withUa).toEqual({
      headers: { "X-Test": "1", "User-Agent": "Bot/9" },
      base_url: "https://example.com",
    });

    const cleared = writeUserAgentToConfig(withUa, "   ");
    expect(cleared).toEqual({
      headers: { "X-Test": "1" },
      base_url: "https://example.com",
    });
  });

  it("matches presets and falls back to custom", () => {
    expect(matchUserAgentPreset("")).toBe("default");
    expect(matchUserAgentPreset("xai-grok-workspace/0.2.93")).toBe("grok-workspace");
    expect(matchUserAgentPreset("grok-cli")).toBe("grok-cli");
    expect(matchUserAgentPreset(BROWSER_USER_AGENT)).toBe("browser");
    expect(matchUserAgentPreset("SomethingElse/1.0")).toBe("custom");
  });
});
```

- [ ] **Step 2: Run helper tests to verify they fail**

Run:

```powershell
pnpm exec vitest run tests/accountUserAgent.test.ts
```

Expected: FAIL module not found.

- [ ] **Step 3: Implement helper module**

Create `src/lib/accountUserAgent.ts`:

```ts
export const GROK_WORKSPACE_USER_AGENT = "xai-grok-workspace/0.2.93";
export const GROK_CLI_USER_AGENT = "grok-cli";
export const BROWSER_USER_AGENT =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";

export type UserAgentPresetId =
  | "default"
  | "grok-workspace"
  | "grok-cli"
  | "browser"
  | "custom";

export const USER_AGENT_PRESETS: Array<{
  id: UserAgentPresetId;
  label: string;
  value: string;
}> = [
  { id: "default", label: "默认（空）", value: "" },
  { id: "grok-workspace", label: "Grok Workspace", value: GROK_WORKSPACE_USER_AGENT },
  { id: "grok-cli", label: "Grok CLI (legacy)", value: GROK_CLI_USER_AGENT },
  { id: "browser", label: "Browser", value: BROWSER_USER_AGENT },
  { id: "custom", label: "自定义", value: "" },
];

function headersFromConfig(config: Record<string, unknown>): Record<string, unknown> {
  const headers = config.headers;
  if (!headers || typeof headers !== "object" || Array.isArray(headers)) {
    return {};
  }
  return { ...(headers as Record<string, unknown>) };
}

export function readUserAgentFromConfig(config: Record<string, unknown>): string {
  const headers = headersFromConfig(config);
  for (const [name, value] of Object.entries(headers)) {
    if (name.toLowerCase() === "user-agent" && typeof value === "string") {
      return value.trim();
    }
  }
  return "";
}

export function writeUserAgentToConfig(
  config: Record<string, unknown>,
  userAgent: string,
): Record<string, unknown> {
  const next = { ...config };
  const headers = headersFromConfig(config);
  for (const key of Object.keys(headers)) {
    if (key.toLowerCase() === "user-agent") {
      delete headers[key];
    }
  }
  const trimmed = userAgent.trim();
  if (trimmed) {
    headers["User-Agent"] = trimmed;
  }
  if (Object.keys(headers).length > 0) {
    next.headers = headers;
  } else {
    delete next.headers;
  }
  return next;
}

export function matchUserAgentPreset(value: string): UserAgentPresetId {
  const trimmed = value.trim();
  if (!trimmed) {
    return "default";
  }
  const preset = USER_AGENT_PRESETS.find(
    (item) => item.id !== "custom" && item.id !== "default" && item.value === trimmed,
  );
  return preset?.id ?? "custom";
}
```

- [ ] **Step 4: Wire AccountsScreen state + save/hydrate**

In `src/screens/AccountsScreen.tsx`:

1. Import helpers from `../lib/accountUserAgent`.
2. Add state:
   - `const [apiUserAgent, setApiUserAgent] = useState("");`
   - `const [editUserAgent, setEditUserAgent] = useState("");`
3. Reset `apiUserAgent` when create dialog resets / platform changes.
4. When opening edit (`useEffect` on `editingCredential`):
   - `setEditUserAgent(readUserAgentFromConfig(config));` for both api and official.
5. Extend `apiConfigJsonWithFields(...)` with `userAgent: string` and apply `writeUserAgentToConfig`.
6. API create mutation payload includes:
   - `user_agent: apiUserAgent.trim() || null`
7. API edit save path passes `editUserAgent` into `apiConfigJsonWithFields`.
8. Official edit save path writes UA into config via `writeUserAgentToConfig`.
9. Add shared UI control with aria labels:
   - create: `创建 User-Agent 预设`, `创建 User-Agent`
   - edit: `编辑 User-Agent 预设`, `编辑 User-Agent`
10. Place control in:
   - Create API form (after Base URL)
   - Edit API form (after Base URL)
   - Edit official form (after status)
11. Keep official Config JSON textarea and structured UA field bidirectional.

`apiConfigJsonWithFields` target shape:

```ts
function apiConfigJsonWithFields(
  configJson: string,
  baseUrl: string,
  interfaceFormat: InterfaceFormat,
  mappings: ModelMapping[],
  apiKeyField: AnthropicApiKeyField,
  responsesCustomToolCompat = false,
  userAgent = "",
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
  const withUa = writeUserAgentToConfig(config, userAgent);
  return JSON.stringify(withUa, null, 2);
}
```

- [ ] **Step 5: Run helper tests**

Run:

```powershell
pnpm exec vitest run tests/accountUserAgent.test.ts
```

Expected: PASS

- [ ] **Step 6: Commit**

```powershell
git add src/lib/accountUserAgent.ts tests/accountUserAgent.test.ts src/screens/AccountsScreen.tsx
git commit -m "feat: add account User-Agent presets and form fields"
```

---

### Task 4: AccountsScreen Integration Tests

**Files:**
- Modify: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes: create/edit UI labels from Task 3
- Produces: regression coverage for save/load UA

- [ ] **Step 1: Write failing screen tests**

Add:

```ts
it("creates an API route credential with custom User-Agent", async () => {
  renderScreen();

  await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
  await userEvent.click(screen.getByRole("button", { name: "API 账号" }));
  await userEvent.type(screen.getByLabelText("API 账号名称"), "UA API");
  await userEvent.type(screen.getByLabelText("API Key"), "sk-ua");
  await userEvent.clear(screen.getByLabelText("Base URL"));
  await userEvent.type(screen.getByLabelText("Base URL"), "https://api.upstream.test/v1");
  await userEvent.selectOptions(screen.getByLabelText("创建 User-Agent 预设"), "grok-workspace");
  await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

  await waitFor(() =>
    expect(createApiRouteCredential).toHaveBeenCalledWith(
      expect.objectContaining({
        display_name: "UA API",
        user_agent: "xai-grok-workspace/0.2.93",
      }),
    ),
  );
});

it("hydrates and saves User-Agent when editing an API account", async () => {
  vi.mocked(listRouteCredentials).mockResolvedValue([
    {
      ...credentialsFixture[1],
      config_json: JSON.stringify({
        base_url: "https://api.example.com/v1",
        interface_format: "openai",
        model_mappings: [],
        headers: { "User-Agent": "OldBot/1.0" },
      }),
    },
  ]);
  vi.mocked(updateRouteCredential).mockResolvedValue(credentialsFixture[1]);

  renderScreen();
  await userEvent.click(await screen.findByRole("button", { name: "编辑" }));

  expect(screen.getByLabelText("编辑 User-Agent")).toHaveValue("OldBot/1.0");
  await userEvent.clear(screen.getByLabelText("编辑 User-Agent"));
  await userEvent.type(screen.getByLabelText("编辑 User-Agent"), "NewBot/2.0");
  await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

  await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
  const payload = vi.mocked(updateRouteCredential).mock.calls[0][1];
  const config = JSON.parse(payload.config_json);
  expect(config.headers["User-Agent"]).toBe("NewBot/2.0");
});

it("saves official account User-Agent into config headers", async () => {
  vi.mocked(updateRouteCredential).mockResolvedValue(credentialsFixture[0]);
  renderScreen();

  const editButtons = await screen.findAllByRole("button", { name: "编辑" });
  await userEvent.click(editButtons[0]);
  await userEvent.selectOptions(screen.getByLabelText("编辑 User-Agent 预设"), "browser");
  await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

  await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
  const payload = vi.mocked(updateRouteCredential).mock.calls[0][1];
  const config = JSON.parse(payload.config_json);
  expect(config.headers["User-Agent"]).toContain("Mozilla/5.0");
});
```

If nearby tests use different save button labels, match those exact labels. Also update exact create assertions that need optional `user_agent: null` when implementation always sends the field.

- [ ] **Step 2: Run tests to verify they fail**

Run:

```powershell
pnpm exec vitest run tests/AccountsScreen.test.tsx -t "User-Agent"
```

Expected: FAIL on missing labels/fields.

- [ ] **Step 3: Fix only UI label/save mismatches required by tests**

Do not expand scope.

- [ ] **Step 4: Run focused frontend + rust regression**

Run:

```powershell
pnpm exec vitest run tests/accountUserAgent.test.ts tests/AccountsScreen.test.tsx
cargo test --manifest-path src-tauri/Cargo.toml build_upstream_request_custom_user_agent -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml create_api_credential_persists_user_agent_header -- --nocapture
```

Expected: PASS

- [ ] **Step 5: Commit**

```powershell
git add tests/AccountsScreen.test.tsx src/screens/AccountsScreen.tsx
git commit -m "test: cover account User-Agent create and edit flows"
```

---

## Spec Coverage Check

| Spec requirement | Task |
|---|---|
| All platforms support custom UA | Task 1 + Task 3 |
| Create + edit UI with presets | Task 3 |
| Store in `config.headers.User-Agent` | Task 2 + Task 3 |
| Empty omits key / keeps defaults | Task 1 + Task 2 + Task 3 |
| Custom UA overrides Grok forced UA | Task 1 |
| Non-UA Grok identity headers remain | Task 1 |
| Model test shares builder path | Task 1 (`build_upstream_request`) |
| Import remains compatible; edit can override later | Task 1 + Task 3 |
| Frontend create/edit hydrate + save | Task 3 + Task 4 |

## Placeholder Scan

No TBD/TODO steps. Commands, code, and expected results are concrete.

## Type Consistency

- Field name: `user_agent` on create input (Rust + TS)
- Storage key: `headers["User-Agent"]`
- Helper names: `readUserAgentFromConfig`, `writeUserAgentToConfig`, `matchUserAgentPreset`, `apply_credential_user_agent`
- Aria labels: `创建 User-Agent`, `创建 User-Agent 预设`, `编辑 User-Agent`, `编辑 User-Agent 预设`
