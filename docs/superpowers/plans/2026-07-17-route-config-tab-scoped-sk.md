# Route Config Tab-Scoped SK Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `写入配置` write only the active agent tab's target config, generate an `sk-...` route proxy key during that write, and auto-clear the write result panel.

**Architecture:** The frontend will pass `activePlatform` through the existing API wrapper to both desktop and web transports. The Rust route config service will normalize that platform, render exactly one target config, generate a per-write route proxy key with `uuid`, and return it in the write outcome. The Accounts screen will clear write outcomes with a 3-second React timer.

**Tech Stack:** React 18, TypeScript, TanStack Query, Vitest, Rust 2021, Tauri 2, Axum web transport, existing `uuid` crate.

## Global Constraints

- Work directly on `main`; do not create or switch branches/worktrees.
- The `写入配置` button in an agent tab must write only that tab's target config.
- The write request must include the active platform, normalized by the backend.
- The backend must generate an `sk-...` route proxy key as part of the write outcome and include it in the rendered target config where applicable.
- Codex config must use `wire_api = "responses"`.
- The success/result panel must disappear automatically after 3 seconds.
- Existing desktop and web transports must keep the same behavior.
- Full route proxy authentication enforcement is out of scope.

---

## File Structure

- Modify `src-tauri/src/services/route_config_service.rs`: single-target route config selection, `sk` generation, renderer signatures, Rust tests.
- Modify `src-tauri/src/commands/route_proxy_commands.rs`: Tauri command accepts `platform`.
- Modify `src-tauri/src/web/handlers/mod.rs`: web command accepts `platform`.
- Modify `src/lib/api/types.ts`: add `route_proxy_key` to `RouteConfigWriteOutcome`.
- Modify `src/lib/api/client.ts`: `writeRouteProxyConfigs(baseUrl, platform)` passes both arguments.
- Modify `src/screens/AccountsScreen.tsx`: pass `activePlatform`, show returned key if useful, auto-clear outcomes after 3 seconds.
- Modify `tests/AccountsScreen.test.tsx`: update mocks and add timer assertion.

---

### Task 1: Backend Single-Target Route Config Writes

**Files:**
- Modify: `src-tauri/src/services/route_config_service.rs`
- Test: `src-tauri/src/services/route_config_service.rs`

**Interfaces:**
- Consumes: `RoutePoolService::normalize_platform(platform: &str) -> Result<String, AppError>`
- Produces: `RouteConfigService::write_configs(paths: &AppPaths, base_url: &str, platform: &str) -> Result<Vec<RouteConfigWriteOutcome>, AppError>`
- Produces: `RouteConfigWriteOutcome { target_key: String, path: String, status: String, route_proxy_key: String }`
- Produces: `render_codex_config(base_url: &str, route_proxy_key: &str) -> String`
- Produces: `render_claude_config(base_url: &str, route_proxy_key: &str) -> String`
- Produces: `render_gemini_config(base_url: &str, route_proxy_key: &str) -> String`

- [ ] **Step 1: Write failing Rust tests**

Append or update tests in `src-tauri/src/services/route_config_service.rs`:

```rust
#[test]
fn generated_route_proxy_key_uses_sk_shape() {
    let key = generate_route_proxy_key();
    assert!(key.starts_with("sk-ai-switch-"));
    assert!(key.len() > "sk-ai-switch-".len() + 20);
}

#[test]
fn render_codex_config_uses_responses_and_route_proxy_key() {
    let rendered = render_codex_config("http://127.0.0.1:43111", "sk-ai-switch-test");
    assert!(rendered.contains("model_provider = \"ai-switch\""));
    assert!(rendered.contains("base_url = \"http://127.0.0.1:43111\""));
    assert!(rendered.contains("wire_api = \"responses\""));
    assert!(rendered.contains("api_key = \"sk-ai-switch-test\""));
    assert!(!rendered.contains("wire_api = \"chat\""));
}

#[test]
fn render_claude_and_gemini_include_route_proxy_key_metadata() {
    let claude = render_claude_config("http://127.0.0.1:43111", "sk-ai-switch-test");
    let gemini = render_gemini_config("http://127.0.0.1:43111", "sk-ai-switch-test");
    assert!(claude.contains("\"apiKey\":\"sk-ai-switch-test\""));
    assert!(claude.contains("AI_SWITCH_ROUTE_PROXY_API_KEY"));
    assert!(gemini.contains("\"apiKey\":\"sk-ai-switch-test\""));
    assert!(gemini.contains("AI_SWITCH_ROUTE_PROXY_API_KEY"));
}

#[tokio::test]
async fn write_configs_rejects_unsupported_platform_without_writing_all_targets() {
    let temp = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths::from_data_dir(temp.path().to_path_buf());
    let error = RouteConfigService::write_configs(&paths, "http://127.0.0.1:43111", "opencode")
        .await
        .expect_err("unsupported target");

    match error {
        AppError::Validation { code, details, .. } => {
            assert_eq!(code, "validation.route_config_target_unsupported");
            assert_eq!(details.as_deref(), Some("opencode"));
        }
        other => panic!("expected validation error, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run Rust tests to verify failure**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml route_config_service
```

Expected: FAIL because `generate_route_proxy_key`, new renderer signatures, `route_proxy_key`, and platform-scoped `write_configs` do not exist yet.

- [ ] **Step 3: Implement single-target service and `sk` generation**

Update `src-tauri/src/services/route_config_service.rs` with these structural changes:

```rust
use crate::config_writer::ConfigWriter;
use crate::error::AppError;
use crate::paths::AppPaths;
use crate::services::route_pool_service::normalize_platform;
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteConfigWriteOutcome {
    pub target_key: String,
    pub path: String,
    pub status: String,
    pub route_proxy_key: String,
}

type TargetRender = fn(&str, &str) -> String;

struct RouteConfigTarget {
    key: &'static str,
    path: PathBuf,
    render: TargetRender,
}

pub struct RouteConfigService;

impl RouteConfigService {
    pub async fn write_configs(
        paths: &AppPaths,
        base_url: &str,
        platform: &str,
    ) -> Result<Vec<RouteConfigWriteOutcome>, AppError> {
        let base_url = base_url.trim().trim_end_matches('/');
        if base_url.is_empty() {
            return Err(AppError::Validation {
                code: "validation.route_proxy_base_url_required",
                message: "Route proxy base URL is required before writing configs".to_string(),
                details: None,
                recoverable: true,
            });
        }

        let home = BaseDirs::new()
            .map(|dirs| dirs.home_dir().to_path_buf())
            .ok_or_else(|| AppError::Filesystem {
                code: "filesystem.home_not_found",
                message: "Could not resolve the current user home directory".to_string(),
                details: None,
                recoverable: false,
            })?;

        let target_key = normalize_platform(platform)?;
        let target = route_config_target(&home, &target_key)?;
        let route_proxy_key = generate_route_proxy_key();
        let content = (target.render)(base_url, &route_proxy_key);

        let _ = paths;
        let write = ConfigWriter::write_atomic(&target.path, &content).await?;
        Ok(vec![RouteConfigWriteOutcome {
            target_key: target.key.to_string(),
            path: write.path,
            status: write.status,
            route_proxy_key,
        }])
    }
}

fn route_config_target(home: &std::path::Path, target_key: &str) -> Result<RouteConfigTarget, AppError> {
    match target_key {
        "codex" => Ok(RouteConfigTarget {
            key: "codex",
            path: home.join(".codex").join("config.toml"),
            render: render_codex_config,
        }),
        "claude" => Ok(RouteConfigTarget {
            key: "claude",
            path: home.join(".claude").join("settings.json"),
            render: render_claude_config,
        }),
        "gemini" => Ok(RouteConfigTarget {
            key: "gemini",
            path: home.join(".gemini").join("settings.json"),
            render: render_gemini_config,
        }),
        other => Err(AppError::Validation {
            code: "validation.route_config_target_unsupported",
            message: "Route config writing is not supported for this target".to_string(),
            details: Some(other.to_string()),
            recoverable: true,
        }),
    }
}

pub fn generate_route_proxy_key() -> String {
    format!("sk-ai-switch-{}", Uuid::new_v4().simple())
}
```

Then update renderers:

```rust
pub fn render_codex_config(base_url: &str, route_proxy_key: &str) -> String {
    format!(
        r#"# Generated by AI Switch route proxy
model_provider = "ai-switch"

[model_providers.ai-switch]
name = "AI Switch Route Proxy"
base_url = "{base_url}"
wire_api = "responses"
api_key = "{route_proxy_key}"
"#
    )
}

pub fn render_claude_config(base_url: &str, route_proxy_key: &str) -> String {
    serde_json::json!({
        "aiSwitch": {
            "routeProxy": {
                "enabled": true,
                "baseUrl": base_url,
                "platform": "claude",
                "apiKey": route_proxy_key
            }
        },
        "env": {
            "ANTHROPIC_BASE_URL": base_url,
            "AI_SWITCH_ROUTE_PROXY": base_url,
            "AI_SWITCH_ROUTE_PROXY_API_KEY": route_proxy_key
        }
    })
    .to_string()
}

pub fn render_gemini_config(base_url: &str, route_proxy_key: &str) -> String {
    serde_json::json!({
        "aiSwitch": {
            "routeProxy": {
                "enabled": true,
                "baseUrl": base_url,
                "platform": "gemini",
                "apiKey": route_proxy_key
            }
        },
        "env": {
            "GEMINI_API_BASE_URL": base_url,
            "GOOGLE_GEMINI_BASE_URL": base_url,
            "AI_SWITCH_ROUTE_PROXY": base_url,
            "AI_SWITCH_ROUTE_PROXY_API_KEY": route_proxy_key
        }
    })
    .to_string()
}
```

- [ ] **Step 4: Run Rust tests to verify backend behavior**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml route_config_service
```

Expected: PASS.

- [ ] **Step 5: Commit backend service changes**

Run:

```powershell
git add -- src-tauri/src/services/route_config_service.rs
git commit -m "fix: scope route config writes by target"
```

Expected: commit succeeds.

---

### Task 2: Transport Command Platform Argument

**Files:**
- Modify: `src-tauri/src/commands/route_proxy_commands.rs`
- Modify: `src-tauri/src/web/handlers/mod.rs`
- Modify: `src/lib/api/types.ts`
- Modify: `src/lib/api/client.ts`

**Interfaces:**
- Consumes: `RouteConfigService::write_configs(&state.paths, &resolved, &platform)`
- Produces: `writeRouteProxyConfigs(baseUrl: string | null | undefined, platform: string): Promise<RouteConfigWriteOutcome[]>`

- [ ] **Step 1: Update frontend API type and wrapper**

In `src/lib/api/types.ts`, update the outcome type:

```ts
export type RouteConfigWriteOutcome = {
  target_key: string;
  path: string;
  status: string;
  route_proxy_key: string;
};
```

In `src/lib/api/client.ts`, update the wrapper:

```ts
export function writeRouteProxyConfigs(
  baseUrl: string | null | undefined,
  platform: string,
): Promise<RouteConfigWriteOutcome[]> {
  return invoke("write_route_proxy_configs", {
    baseUrl: baseUrl ?? null,
    platform,
  });
}
```

- [ ] **Step 2: Update Tauri command**

In `src-tauri/src/commands/route_proxy_commands.rs`, add `platform: String` and pass it through:

```rust
#[tauri::command]
pub async fn write_route_proxy_configs(
    state: State<'_, AppState>,
    base_url: Option<String>,
    platform: String,
) -> Result<Vec<RouteConfigWriteOutcome>, ApiError> {
    let status = RouteProxyService::status(&state.route_proxy).await;
    let resolved = base_url
        .and_then(|value| {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
        .or(status.base_url)
        .ok_or_else(|| {
            ApiError::from(crate::error::AppError::Validation {
                code: "validation.route_proxy_not_running",
                message: "Start the route proxy before writing config files".to_string(),
                details: None,
                recoverable: true,
            })
        })?;

    RouteConfigService::write_configs(&state.paths, &resolved, &platform)
        .await
        .map_err(ApiError::from)
}
```

- [ ] **Step 3: Update web handler**

In `src-tauri/src/web/handlers/mod.rs`, update the `write_route_proxy_configs` match arm:

```rust
"write_route_proxy_configs" => {
    let base_url = optional_string_arg(&args, "baseUrl");
    let platform = optional_string_arg(&args, "platform")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            to_error(AppError::Validation {
                code: "validation.route_config_platform_required",
                message: "Route config platform is required".to_string(),
                details: None,
                recoverable: true,
            })
        })?;
    let status = RouteProxyService::status(&state.route_proxy).await;
    let resolved = base_url
        .filter(|value| !value.is_empty())
        .or(status.base_url)
        .ok_or_else(|| {
            to_error(AppError::Validation {
                code: "validation.route_proxy_not_running",
                message: "Start the route proxy before writing config files".to_string(),
                details: None,
                recoverable: true,
            })
        })?;
    to_value(
        RouteConfigService::write_configs(&state.paths, &resolved, &platform)
            .await
            .map_err(to_error)?,
    )
}
```

- [ ] **Step 4: Run command/API checks**

Run:

```powershell
pnpm vitest run tests/AccountsScreen.test.tsx
cargo test --manifest-path src-tauri/Cargo.toml route_config_service
```

Expected: PASS after Task 1 and the wrapper signature changes are in place. If `AccountsScreen` has not been updated yet, run `pnpm vitest run tests\\transport\\transport.test.ts` instead of the Accounts screen test for this task and leave the Accounts screen test for Task 3.

- [ ] **Step 5: Commit transport changes**

Run:

```powershell
git add -- src-tauri/src/commands/route_proxy_commands.rs src-tauri/src/web/handlers/mod.rs src/lib/api/types.ts src/lib/api/client.ts
git commit -m "fix: pass target platform to route config writer"
```

Expected: commit succeeds with no intentionally failing tests included.

---

### Task 3: Accounts Screen Platform Call and Auto-Clearing Result Panel

**Files:**
- Modify: `src/screens/AccountsScreen.tsx`
- Modify: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes: `writeRouteProxyConfigs(baseUrl, activePlatform)`
- Consumes: `RouteConfigWriteOutcome.route_proxy_key`
- Produces: auto-clearing `configWriteOutcomes` panel after 3000 ms

- [ ] **Step 1: Extend frontend mock outcome and write expectation**

In `tests/AccountsScreen.test.tsx`, update the `writeRouteProxyConfigs` mock outcome:

```ts
vi.mocked(writeRouteProxyConfigs).mockResolvedValue([
  {
    target_key: "codex",
    path: "C:\\Users\\test\\.codex\\config.toml",
    status: "written",
    route_proxy_key: "sk-ai-switch-test",
  },
]);
```

In the existing proxy write test, change the write assertion to:

```ts
await waitFor(() =>
  expect(writeRouteProxyConfigs).toHaveBeenCalledWith("http://127.0.0.1:43111", "codex"),
);
```

- [ ] **Step 2: Add auto-clear timer test**

Change the Vitest import to include `afterEach`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
```

Add cleanup near the `beforeEach`:

```ts
afterEach(() => {
  vi.useRealTimers();
});
```

Add this test after the existing proxy write test:

```ts
it("clears route config write results after a short delay", async () => {
  vi.useFakeTimers();
  const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
  renderScreen();

  await screen.findByText("本地代理：未启动");
  await user.click(screen.getByLabelText("启动本地路由代理"));
  expect(await screen.findByText("本地代理：http://127.0.0.1:43111")).toBeInTheDocument();

  await user.click(screen.getByLabelText("写入路由配置文件"));
  expect(await screen.findByText("配置写入结果")).toBeInTheDocument();

  vi.advanceTimersByTime(2999);
  expect(screen.getByText("配置写入结果")).toBeInTheDocument();

  vi.advanceTimersByTime(1);
  await waitFor(() => expect(screen.queryByText("配置写入结果")).not.toBeInTheDocument());
});
```

- [ ] **Step 3: Run frontend test to verify failure**

Run:

```powershell
pnpm vitest run tests/AccountsScreen.test.tsx
```

Expected: FAIL because the screen does not pass platform and does not clear write outcomes.

- [ ] **Step 4: Update Accounts screen mutation and timer**

In `src/screens/AccountsScreen.tsx`, update the mutation:

```ts
const writeConfigsMutation = useMutation({
  mutationFn: () => writeRouteProxyConfigs(routeProxyQuery.data?.base_url ?? null, activePlatform),
  onSuccess: setConfigWriteOutcomes,
});
```

Add this effect after the existing effects:

```ts
useEffect(() => {
  if (configWriteOutcomes.length === 0) {
    return;
  }

  const timeout = window.setTimeout(() => {
    setConfigWriteOutcomes([]);
  }, 3000);

  return () => window.clearTimeout(timeout);
}, [configWriteOutcomes]);
```

Update the result panel map to display the generated key without exposing extra hierarchy:

```tsx
{configWriteOutcomes.map((outcome) => (
  <p key={`${outcome.target_key}:${outcome.path}`}>
    {outcome.target_key}: {outcome.path} ({outcome.status}) · {outcome.route_proxy_key}
  </p>
))}
```

- [ ] **Step 5: Run frontend test to verify pass**

Run:

```powershell
pnpm vitest run tests/AccountsScreen.test.tsx
```

Expected: PASS.

- [ ] **Step 6: Commit Accounts screen changes**

Run:

```powershell
git add -- src/screens/AccountsScreen.tsx tests/AccountsScreen.test.tsx
git commit -m "fix: write route config for active agent tab"
```

Expected: commit succeeds.

---

### Task 4: Full Verification

**Files:**
- No new files.
- Verify: frontend and Rust test suites relevant to route config writes.

**Interfaces:**
- Consumes: completed Tasks 1-3.
- Produces: verified implementation.

- [ ] **Step 1: Run focused frontend tests**

Run:

```powershell
pnpm vitest run tests/AccountsScreen.test.tsx tests\\transport\\transport.test.ts
```

Expected: PASS.

- [ ] **Step 2: Run focused Rust tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml route_config_service route_proxy
```

Expected: PASS.

- [ ] **Step 3: Run typecheck/build smoke**

Run:

```powershell
pnpm vitest run
```

Expected: PASS.

- [ ] **Step 4: Inspect generated Codex config renderer output in test context**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml render_codex_config_uses_responses_and_route_proxy_key -- --nocapture
```

Expected: PASS and no `wire_api = "chat"` in assertions or output.

- [ ] **Step 5: Commit any verification-only adjustments**

If no files changed, do not commit. If test-only fixes were required, run:

```powershell
git add -- <changed-files>
git commit -m "test: verify tab-scoped route config writes"
```

Expected: working tree has only intentional changes.
