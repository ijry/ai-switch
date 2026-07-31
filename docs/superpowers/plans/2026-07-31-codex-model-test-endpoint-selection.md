# Codex Model Test Endpoint Selection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let Codex real-generation tests choose `/responses` or `/chat/completions`, remember the last selection globally, and make the backend honor that choice for direct and proxy-routed tests.

**Architecture:** Add an optional request-level `interface_format` override to the shared frontend/Rust request contract. Validate it at the model-test service boundary, pass it into existing request construction, and keep the current credential-derived behavior when it is absent. Store the Codex-only UI preference in `localStorage` through a small focused helper and render a two-option segmented control in the existing dialog.

**Tech Stack:** Rust 2021, Serde, Tauri 2/web command serialization, React 18, TypeScript, TanStack Query, browser `localStorage`, Vitest, Testing Library.

## Global Constraints

- Work directly on `main`; do not create a branch or worktree.
- Show the endpoint control only for Codex.
- Use `/responses` as the initial and invalid-storage fallback.
- Share one persisted selection across pool tests, single-account tests, screen remounts, and application restarts.
- Do not modify credential `config_json`, saved `interface_format`, or application settings.
- Accept backend overrides only for Codex and only `openai-responses` or `openai`.
- Preserve existing behavior for requests that omit `interface_format` and for all non-Codex platforms.
- Preserve the untracked `tauri-dev.err` and `tauri-dev.log.err` files.

---

## File Structure

- Create `src/lib/codexModelTestEndpoint.ts`: owns the endpoint type, storage key, validation, loading, persistence, and interface-format mapping.
- Create `tests/lib/codexModelTestEndpoint.test.ts`: verifies default, valid stored values, invalid fallback, storage failure handling, and mapping.
- Modify `src/lib/api/types.ts`: adds optional `interface_format` to `RoutePoolModelTestRequest`.
- Modify `src/screens/AccountsScreen.tsx`: initializes the global preference, renders the Codex-only segmented control, persists changes, and submits the override.
- Modify `tests/AccountsScreen.test.tsx`: verifies UI visibility, default submission, switching, persistence, remount restoration, account-test sharing, and non-Codex absence.
- Modify `src-tauri/src/models/route_pool.rs`: adds the optional serialized request field.
- Modify `src-tauri/src/services/route_model_test_service.rs`: validates the override and threads it through direct/proxy request construction without changing fallback behavior.

### Task 1: Frontend Preference Helper

**Files:**
- Create: `src/lib/codexModelTestEndpoint.ts`
- Create: `tests/lib/codexModelTestEndpoint.test.ts`

**Interfaces:**
- Produces: `CodexModelTestEndpoint = "/responses" | "/chat/completions"`.
- Produces: `CODEX_MODEL_TEST_ENDPOINT_STORAGE_KEY`.
- Produces: `loadCodexModelTestEndpoint(storage?: Pick<Storage, "getItem">): CodexModelTestEndpoint`.
- Produces: `saveCodexModelTestEndpoint(endpoint, storage?: Pick<Storage, "setItem">): void`.
- Produces: `codexModelTestInterfaceFormat(endpoint): "openai-responses" | "openai"`.

- [ ] **Step 1: Write failing helper tests**

Create `tests/lib/codexModelTestEndpoint.test.ts`:

```ts
import { beforeEach, describe, expect, it } from "vitest";
import {
  CODEX_MODEL_TEST_ENDPOINT_STORAGE_KEY,
  codexModelTestInterfaceFormat,
  loadCodexModelTestEndpoint,
  saveCodexModelTestEndpoint,
} from "../../src/lib/codexModelTestEndpoint";

describe("codexModelTestEndpoint", () => {
  beforeEach(() => window.localStorage.clear());

  it("defaults to responses and maps both endpoints", () => {
    expect(loadCodexModelTestEndpoint()).toBe("/responses");
    expect(codexModelTestInterfaceFormat("/responses")).toBe("openai-responses");
    expect(codexModelTestInterfaceFormat("/chat/completions")).toBe("openai");
  });

  it("loads valid values and falls back for invalid values", () => {
    window.localStorage.setItem(CODEX_MODEL_TEST_ENDPOINT_STORAGE_KEY, "/chat/completions");
    expect(loadCodexModelTestEndpoint()).toBe("/chat/completions");

    window.localStorage.setItem(CODEX_MODEL_TEST_ENDPOINT_STORAGE_KEY, "/v1/responses");
    expect(loadCodexModelTestEndpoint()).toBe("/responses");
  });

  it("persists selections and tolerates unavailable storage", () => {
    saveCodexModelTestEndpoint("/chat/completions");
    expect(window.localStorage.getItem(CODEX_MODEL_TEST_ENDPOINT_STORAGE_KEY)).toBe(
      "/chat/completions",
    );

    expect(() =>
      loadCodexModelTestEndpoint({ getItem: () => { throw new Error("blocked"); } }),
    ).not.toThrow();
    expect(() =>
      saveCodexModelTestEndpoint("/responses", { setItem: () => { throw new Error("blocked"); } }),
    ).not.toThrow();
  });
});
```

- [ ] **Step 2: Run the helper test and verify failure**

Run: `pnpm vitest run tests/lib/codexModelTestEndpoint.test.ts`

Expected: FAIL because `src/lib/codexModelTestEndpoint.ts` does not exist.

- [ ] **Step 3: Implement the helper**

Create `src/lib/codexModelTestEndpoint.ts`:

```ts
export type CodexModelTestEndpoint = "/responses" | "/chat/completions";

export const CODEX_MODEL_TEST_ENDPOINT_STORAGE_KEY =
  "ai-switch.codex-model-test-endpoint";

const DEFAULT_CODEX_MODEL_TEST_ENDPOINT: CodexModelTestEndpoint = "/responses";

function isCodexModelTestEndpoint(value: string | null): value is CodexModelTestEndpoint {
  return value === "/responses" || value === "/chat/completions";
}

export function loadCodexModelTestEndpoint(
  storage: Pick<Storage, "getItem"> = window.localStorage,
): CodexModelTestEndpoint {
  try {
    const stored = storage.getItem(CODEX_MODEL_TEST_ENDPOINT_STORAGE_KEY);
    return isCodexModelTestEndpoint(stored) ? stored : DEFAULT_CODEX_MODEL_TEST_ENDPOINT;
  } catch {
    return DEFAULT_CODEX_MODEL_TEST_ENDPOINT;
  }
}

export function saveCodexModelTestEndpoint(
  endpoint: CodexModelTestEndpoint,
  storage: Pick<Storage, "setItem"> = window.localStorage,
): void {
  try {
    storage.setItem(CODEX_MODEL_TEST_ENDPOINT_STORAGE_KEY, endpoint);
  } catch {
    // Storage can be unavailable in restricted browser contexts.
  }
}

export function codexModelTestInterfaceFormat(
  endpoint: CodexModelTestEndpoint,
): "openai-responses" | "openai" {
  return endpoint === "/responses" ? "openai-responses" : "openai";
}
```

- [ ] **Step 4: Run the helper test and typecheck**

Run: `pnpm vitest run tests/lib/codexModelTestEndpoint.test.ts`

Expected: PASS with 3 tests.

Run: `pnpm typecheck`

Expected: PASS.

- [ ] **Step 5: Commit the preference helper**

```bash
git add src/lib/codexModelTestEndpoint.ts tests/lib/codexModelTestEndpoint.test.ts
git commit -m "feat: persist Codex model test endpoint"
```

### Task 2: Backend Request-Level Interface Override

**Files:**
- Modify: `src-tauri/src/models/route_pool.rs:65`
- Modify: `src-tauri/src/services/route_model_test_service.rs:39`

**Interfaces:**
- Consumes: optional `RoutePoolModelTestRequest.interface_format: Option<String>`.
- Produces: `validate_model_test_interface_override(platform, requested) -> Result<Option<String>, AppError>`.
- Produces: `build_model_test_request(credential, platform, requested_model, interface_override)`.
- Preserves: absent override uses `interface_format_for(credential, platform, config)`.

- [ ] **Step 1: Add failing Rust tests for override construction and validation**

In `src-tauri/src/services/route_model_test_service.rs`, add tests equivalent to:

```rust
#[test]
fn codex_override_selects_responses_request_shape() {
    let request = build_model_test_request(
        &api_credential("openai"),
        "codex",
        Some("gpt-5.5"),
        Some("openai-responses"),
    )
    .expect("request");
    let body: Value = serde_json::from_str(&request.request_body_json).expect("json");

    assert_eq!(request.interface_format, "openai-responses");
    assert_eq!(request.request_path, "/responses");
    assert_eq!(body.pointer("/input").and_then(Value::as_str), Some(MODEL_TEST_PROMPT));
}

#[test]
fn codex_override_selects_chat_completions_request_shape() {
    let request = build_model_test_request(
        &api_credential("openai-responses"),
        "codex",
        Some("gpt-5.5"),
        Some("openai"),
    )
    .expect("request");
    let body: Value = serde_json::from_str(&request.request_body_json).expect("json");

    assert_eq!(request.interface_format, "openai");
    assert_eq!(request.request_path, "/chat/completions");
    assert_eq!(
        body.pointer("/messages/0/content").and_then(Value::as_str),
        Some(MODEL_TEST_PROMPT),
    );
}

#[test]
fn validates_model_test_interface_override_scope_and_values() {
    assert_eq!(
        validate_model_test_interface_override("codex", Some("openai"))
            .expect("valid override")
            .as_deref(),
        Some("openai"),
    );
    assert!(validate_model_test_interface_override("codex", Some("gemini")).is_err());
    assert!(validate_model_test_interface_override("claude", Some("openai")).is_err());
    assert_eq!(
        validate_model_test_interface_override("claude", None).expect("missing override"),
        None,
    );
}
```

Update one existing request-construction test to call the new fourth argument with `None`, proving existing derivation remains available.

- [ ] **Step 2: Run the focused Rust tests and verify failure**

Run: `cargo test model_test_interface_override --lib`

Expected: FAIL because the request field, validator, and fourth function argument do not exist.

- [ ] **Step 3: Extend the serialized request contract**

Modify `src-tauri/src/models/route_pool.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutePoolModelTestRequest {
    pub platform: String,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub interface_format: Option<String>,
}
```

Add `interface_format: None` to existing Rust request literals.

- [ ] **Step 4: Validate and thread the override through both service paths**

In `src-tauri/src/services/route_model_test_service.rs`, add:

```rust
fn validate_model_test_interface_override(
    platform: &str,
    requested: Option<&str>,
) -> Result<Option<String>, AppError> {
    let Some(requested) = requested
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    if platform != "codex" || !matches!(requested, "openai" | "openai-responses") {
        return Err(AppError::Validation {
            code: "validation.route_model_test_interface_format",
            message: "Unsupported model test interface format".to_string(),
            details: Some(format!("{platform}:{requested}")),
            recoverable: true,
        });
    }

    Ok(Some(requested.to_string()))
}
```

Immediately after normalizing `platform` in both `test_model` and `test_model_through_proxy`, compute:

```rust
let interface_override = validate_model_test_interface_override(
    &platform,
    request.interface_format.as_deref(),
)?;
```

Pass `interface_override.as_deref()` to request construction. Change the function signature and interface selection to:

```rust
pub fn build_model_test_request(
    credential: &SelectedCredential,
    platform: &str,
    requested_model: Option<&str>,
    interface_override: Option<&str>,
) -> Result<ModelTestRequestParts, String> {
    let config = parse_json_object(&credential.config_json, "config")?;
    let interface_format = interface_override
        .map(str::to_string)
        .unwrap_or_else(|| interface_format_for(credential, platform, &config));
    // Existing request construction remains unchanged.
}
```

When `test_model_through_proxy` delegates an account-specific request to `test_model`, leave the request intact so validation occurs once in the direct path.

- [ ] **Step 5: Run focused and full Rust tests**

Run: `cargo test model_test_interface_override --lib`

Expected: PASS.

Run: `cargo test route_model_test_service::tests --lib`

Expected: PASS.

Run: `cargo test --lib`

Expected: PASS; the two sidecar-binary tests may remain ignored.

- [ ] **Step 6: Commit the backend contract and behavior**

```bash
git add src-tauri/src/models/route_pool.rs src-tauri/src/services/route_model_test_service.rs
git commit -m "feat: override Codex model test interface"
```

### Task 3: Codex Dialog Selector and Submitted Override

**Files:**
- Modify: `src/lib/api/types.ts:228`
- Modify: `src/screens/AccountsScreen.tsx:1`
- Modify: `tests/AccountsScreen.test.tsx:150`

**Interfaces:**
- Consumes: preference helper from Task 1.
- Consumes: backend request field from Task 2.
- Produces: Codex-only control with accessible labels `测试接口 /responses` and `测试接口 /chat/completions`.

- [ ] **Step 1: Add failing Accounts screen tests**

In `tests/AccountsScreen.test.tsx`, clear `window.localStorage` in `beforeEach`, import the storage key, and add tests equivalent to:

```tsx
it("defaults Codex model tests to responses", async () => {
  renderScreen();
  await userEvent.click(await screen.findByLabelText("测试 API Account"));

  expect(screen.getByLabelText("测试接口 /responses")).toHaveAttribute("aria-pressed", "true");
  expect(screen.getByLabelText("测试接口 /chat/completions")).toHaveAttribute(
    "aria-pressed",
    "false",
  );

  await userEvent.click(screen.getByLabelText("开始真实生成测试"));
  await waitFor(() =>
    expect(routePoolTestModel).toHaveBeenCalledWith({
      platform: "codex",
      account_id: "cred-api-1",
      model: null,
      interface_format: "openai-responses",
    }),
  );
});

it("persists Chat Completions globally for pool and account tests", async () => {
  const first = renderScreen();
  await userEvent.click(await screen.findByLabelText("测试 API Account"));
  await userEvent.click(screen.getByLabelText("测试接口 /chat/completions"));
  expect(window.localStorage.getItem(CODEX_MODEL_TEST_ENDPOINT_STORAGE_KEY)).toBe(
    "/chat/completions",
  );
  await userEvent.click(screen.getByLabelText("关闭真实生成测试弹窗"));
  first.unmount();

  renderScreen();
  await userEvent.click(await screen.findByLabelText("测试 API Account"));
  expect(screen.getByLabelText("测试接口 /chat/completions")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
});

it("does not show the endpoint selector for Claude", async () => {
  renderScreen("claude");
  await userEvent.click(await screen.findByLabelText("测试 API Account"));
  expect(screen.queryByLabelText("测试接口 /responses")).not.toBeInTheDocument();
  expect(screen.queryByLabelText("测试接口 /chat/completions")).not.toBeInTheDocument();
});
```

Update existing Codex `routePoolTestModel` expectations to include the default `interface_format: "openai-responses"`. Keep non-Codex expectations unchanged.

- [ ] **Step 2: Run the focused frontend test and verify failure**

Run: `pnpm vitest run tests/AccountsScreen.test.tsx -t "Codex model tests|persists Chat Completions|endpoint selector"`

Expected: FAIL because the selector and submitted request field do not exist.

- [ ] **Step 3: Extend the TypeScript request type**

Modify `src/lib/api/types.ts`:

```ts
export type RoutePoolModelTestRequest = {
  platform: string;
  account_id?: string | null;
  model?: string | null;
  interface_format?: "openai" | "openai-responses" | null;
};
```

- [ ] **Step 4: Add state, persistence, and request mapping to AccountsScreen**

Import the Task 1 helper and initialize one component state:

```ts
const [codexModelTestEndpoint, setCodexModelTestEndpoint] =
  useState<CodexModelTestEndpoint>(() => loadCodexModelTestEndpoint());
```

Add a selection handler:

```ts
const selectCodexModelTestEndpoint = (endpoint: CodexModelTestEndpoint) => {
  setCodexModelTestEndpoint(endpoint);
  saveCodexModelTestEndpoint(endpoint);
};
```

Extend `submitModelTest`:

```ts
modelTestMutation.mutate({
  platform: activePlatform,
  ...(accountId ? { account_id: accountId } : {}),
  model: routeTestModel.trim() || null,
  ...(activePlatform === "codex"
    ? { interface_format: codexModelTestInterfaceFormat(codexModelTestEndpoint) }
    : {}),
});
```

- [ ] **Step 5: Render the Codex-only segmented control**

Place it above the model input in the dialog:

```tsx
{activePlatform === "codex" && (
  <fieldset className="mt-4">
    <legend className={labelClass}>测试接口</legend>
    <div className="mt-1 grid grid-cols-2 gap-1 rounded-lg bg-stone-100 p-1">
      {(["/responses", "/chat/completions"] as const).map((endpoint) => {
        const selected = codexModelTestEndpoint === endpoint;
        return (
          <button
            aria-label={`测试接口 ${endpoint}`}
            aria-pressed={selected}
            className={`min-h-9 rounded-md px-2 text-[12px] font-semibold transition-colors ${
              selected
                ? "bg-white text-stone-950 shadow-sm"
                : "text-stone-600 hover:text-stone-900"
            }`}
            key={endpoint}
            onClick={() => selectCodexModelTestEndpoint(endpoint)}
            type="button"
          >
            {endpoint}
          </button>
        );
      })}
    </div>
  </fieldset>
)}
```

Keep stable two-column sizing so the longer `/chat/completions` label does not resize the dialog.

- [ ] **Step 6: Run frontend tests and typecheck**

Run: `pnpm vitest run tests/AccountsScreen.test.tsx`

Expected: PASS.

Run: `pnpm typecheck`

Expected: PASS.

- [ ] **Step 7: Commit the dialog behavior**

```bash
git add src/lib/api/types.ts src/screens/AccountsScreen.tsx tests/AccountsScreen.test.tsx
git commit -m "feat: choose Codex model test endpoint"
```

### Task 4: End-to-End Verification

**Files:**
- Verify only; no planned code changes.

**Interfaces:**
- Verifies the frontend value `/responses | /chat/completions` maps to the backend value `openai-responses | openai` and reaches existing direct/proxy request construction.

- [ ] **Step 1: Run all frontend tests**

Run: `pnpm test:run`

Expected: PASS.

- [ ] **Step 2: Run the full backend library suite**

Run: `cargo test --lib`

Expected: PASS with only explicitly ignored environment-dependent tests.

- [ ] **Step 3: Run type and formatting checks**

Run: `pnpm typecheck`

Expected: PASS.

Run: `cargo fmt --all -- --check`

Expected: PASS. If pre-existing mixed line endings cause unrelated diff noise, format only edited Rust regions/files while preserving repository line endings, then rerun `git diff --check`.

Run: `git diff --check`

Expected: no whitespace errors.

- [ ] **Step 4: Review the final worktree scope**

Run: `git status --short`

Expected: feature files are committed; `tauri-dev.err` and `tauri-dev.log.err` remain untracked and untouched.

- [ ] **Step 5: Record verification if a final follow-up commit is needed**

Only if verification required code/test adjustments:

```bash
git add <only files changed by verification fixes>
git commit -m "test: verify Codex model test endpoint selection"
```
