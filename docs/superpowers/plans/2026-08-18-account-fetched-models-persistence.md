# 账号模型列表本地持久化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 API 账号成功获取的模型列表随账号保存，并在再次编辑时直接复用而不自动联网。

**Architecture:** 模型列表作为可选的 `fetched_models` 数组嵌入账号现有 `config_json`。Rust 新增账号服务负责校验并写入该字段；前端用独立纯函数模块解析和序列化缓存，`AccountsScreen` 只负责在新增、编辑和连接配置变化时协调状态。

**Tech Stack:** React 18、TypeScript、Vitest、Testing Library、Tauri 2、Rust、Serde、SQLite（复用现有 `config_json`，无迁移）。

## Global Constraints

- 仅为 API 账号保存模型列表；官方登录账号不增加该能力。
- 点击“获取模型列表”只更新弹窗状态；只有保存账号后才落盘，取消弹窗不得修改账号。
- 打开编辑弹窗不得自动请求上游模型接口。
- 模型列表必须保存在 `config_json.fetched_models`，不得新增数据库表或数据库列。
- API Key、Base URL、接口格式或 Anthropic/Claude 鉴权字段变化时，必须清空当前弹窗模型列表。
- 手动刷新失败时，若连接配置没有变化，必须保留当前有效列表。
- 旧账号缺少字段、字段类型错误或包含无效条目时，编辑界面必须保持可用。
- 新增账号接口收到非法 `fetched_models_json` 时必须拒绝创建账号。
- 账号复制、导入和导出继续复用完整 `config_json`，不得增加第二套传输通道。
- `docs/superpowers/plans` 与 `docs/superpowers/specs` 中的文档使用中文。

---

## 文件结构

- `src-tauri/src/models/route_credential.rs`：扩展新增 API 账号命令输入，声明可选 `fetched_models_json`。
- `src-tauri/src/services/route_credential_service.rs`：校验、规范化模型列表并写入新账号 `config_json`；承载 Rust 单元测试。
- `src-tauri/src/services/deeplink_service.rs`：深链创建账号时显式不提供模型缓存。
- `src-tauri/src/services/route_recovery_service.rs`：测试构造新增账号输入时显式不提供模型缓存。
- `src/lib/api/types.ts`：同步 TypeScript 新增账号输入契约。
- `src/lib/accountFetchedModels.ts`：集中负责缓存解析、规范化和写回，不依赖 UI 状态。
- `tests/accountFetchedModels.test.ts`：覆盖缓存纯函数的兼容性和字段保留行为。
- `src/screens/AccountsScreen.tsx`：把缓存接入新增保存、编辑回填、编辑保存与既有失效逻辑。
- `tests/AccountsScreen.test.tsx`：覆盖用户可见的新增、编辑、刷新失败和配置变更行为。

---

### Task 1: 后端新增账号持久化契约

**Files:**
- Modify: `src-tauri/src/models/route_credential.rs:139`
- Modify: `src-tauri/src/services/route_credential_service.rs:109-153,688-716,1001-1245`
- Modify: `src-tauri/src/services/deeplink_service.rs:151-166`
- Modify: `src-tauri/src/services/route_recovery_service.rs:419-436`
- Test: `src-tauri/src/services/route_credential_service.rs`

**Interfaces:**
- Consumes: `crate::models::route_pool::FetchedRouteModel { id, owned_by, supports_1m }`。
- Produces: `CreateApiRouteCredentialInput.fetched_models_json: Option<String>`。
- Produces: `parse_fetched_models_json(Option<&str>) -> Result<Vec<FetchedRouteModel>, AppError>`。
- Persists: `config_json.fetched_models` 为规范化后的 JSON 数组。

- [x] **Step 1: 写入后端失败测试**

在 `route_credential_service.rs` 的测试模块新增两个测试。第一个要求合法列表被保存，第二个要求空 `id` 被拒绝且数据库中没有半成品账号：

```rust
#[tokio::test]
async fn create_api_credential_persists_fetched_models() {
    let pool = crate::database::create_memory_pool().await.expect("pool");
    crate::database::run_migrations(&pool)
        .await
        .expect("migrations");

    let created = RouteCredentialService::create_api(
        &pool,
        CreateApiRouteCredentialInput {
            platform: "codex".into(),
            display_name: "Cached models".into(),
            api_key: "sk-test".into(),
            base_url: "https://api.example.com/v1".into(),
            interface_format: "openai".into(),
            model_mappings_json: "[]".into(),
            fetched_models_json: Some(
                r#"[{"id":" gpt-5 ","owned_by":" openai ","supports_1m":true}]"#
                    .into(),
            ),
            api_key_field: None,
            preview_json: None,
            batch_id: None,
            responses_custom_tool_compat: None,
            user_agent: None,
        },
    )
    .await
    .expect("create");

    let config: serde_json::Value =
        serde_json::from_str(&created.config_json).expect("config");
    assert_eq!(
        config["fetched_models"],
        serde_json::json!([{
            "id": "gpt-5",
            "owned_by": "openai",
            "supports_1m": true
        }])
    );
}

#[tokio::test]
async fn create_api_credential_rejects_invalid_fetched_models() {
    let pool = crate::database::create_memory_pool().await.expect("pool");
    crate::database::run_migrations(&pool)
        .await
        .expect("migrations");

    let error = RouteCredentialService::create_api(
        &pool,
        CreateApiRouteCredentialInput {
            platform: "codex".into(),
            display_name: "Invalid cache".into(),
            api_key: "sk-test".into(),
            base_url: "https://api.example.com/v1".into(),
            interface_format: "openai".into(),
            model_mappings_json: "[]".into(),
            fetched_models_json: Some(r#"[{"id":"   "}]"#.into()),
            api_key_field: None,
            preview_json: None,
            batch_id: None,
            responses_custom_tool_compat: None,
            user_agent: None,
        },
    )
    .await
    .expect_err("invalid cache must fail");

    assert!(matches!(
        error,
        AppError::Validation {
            code: "validation.fetched_models",
            ..
        }
    ));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM route_credentials")
        .fetch_one(&pool)
        .await
        .expect("credential count");
    assert_eq!(count, 0);
}
```

- [x] **Step 2: 运行测试确认失败**

Run:

```powershell
Set-Location src-tauri
cargo test fetched_models
```

Expected: FAIL，编译器报告 `CreateApiRouteCredentialInput` 尚无 `fetched_models_json` 字段。

- [x] **Step 3: 扩展 Rust 输入结构并补齐所有结构体构造点**

在 `CreateApiRouteCredentialInput` 的 `model_mappings_json` 后增加：

```rust
#[serde(default)]
pub fetched_models_json: Option<String>,
```

在以下现有结构体字面量中加入 `fetched_models_json: None,`，保持深链导入和恢复服务测试的旧行为：

```rust
// src-tauri/src/services/deeplink_service.rs
model_mappings_json: parsed.model_mappings_json.clone(),
fetched_models_json: None,

// src-tauri/src/services/route_recovery_service.rs 及
// route_credential_service.rs 中所有不测试模型缓存的构造点
model_mappings_json: "[]".into(),
fetched_models_json: None,
```

- [x] **Step 4: 实现模型列表校验和规范化**

在 `validate_model_mappings` 附近加入以下辅助函数，并在文件顶部导入 `crate::models::route_pool::FetchedRouteModel`：

```rust
fn parse_fetched_models_json(value: Option<&str>) -> Result<Vec<FetchedRouteModel>, AppError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let mut models = serde_json::from_str::<Vec<FetchedRouteModel>>(value).map_err(|err| {
        AppError::Validation {
            code: "validation.fetched_models",
            message: "Fetched models must be a valid JSON array".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        }
    })?;
    if models.iter().any(|model| model.id.trim().is_empty()) {
        return Err(AppError::Validation {
            code: "validation.fetched_models",
            message: "Fetched models require a non-empty id".to_string(),
            details: Some(value.to_string()),
            recoverable: true,
        });
    }
    for model in &mut models {
        model.id = model.id.trim().to_string();
        model.owned_by = model
            .owned_by
            .take()
            .map(|owned_by| owned_by.trim().to_string())
            .filter(|owned_by| !owned_by.is_empty());
    }
    Ok(models)
}
```

- [x] **Step 5: 在创建服务中写入 `config_json.fetched_models`**

在构造 `config` 前解析模型列表，并把规范化结果加入 JSON：

```rust
let fetched_models = parse_fetched_models_json(input.fetched_models_json.as_deref())?;
let mut config = json!({
    "base_url": input.base_url.trim(),
    "interface_format": input.interface_format,
    "model_mappings": serde_json::from_str::<serde_json::Value>(&input.model_mappings_json)?,
    "fetched_models": fetched_models,
    "responses_custom_tool_compat": input.responses_custom_tool_compat.unwrap_or(false),
});
```

同时把现有 `copy_route_credential_appends_date_to_display_name` 的创建输入改为包含一个合法 `fetched_models_json`，继续用现有 `assert_eq!(copied.config_json, created.config_json)` 验证复制继承缓存。

- [x] **Step 6: 运行后端定向测试**

Run:

```powershell
Set-Location src-tauri
cargo test fetched_models
cargo test copy_route_credential_appends_date_to_display_name
cargo test deeplink_service
```

Expected: PASS。

- [x] **Step 7: 检查 Rust 编译和格式**

Run:

```powershell
Set-Location src-tauri
cargo fmt --check
cargo check
```

Expected: 两条命令均成功且无格式差异。

- [x] **Step 8: 提交后端契约**

```powershell
git add src-tauri/src/models/route_credential.rs src-tauri/src/services/route_credential_service.rs src-tauri/src/services/deeplink_service.rs src-tauri/src/services/route_recovery_service.rs
git commit -m "feat: persist fetched models for API accounts"
```

---

### Task 2: 前端模型缓存纯函数和类型契约

**Files:**
- Create: `src/lib/accountFetchedModels.ts`
- Create: `tests/accountFetchedModels.test.ts`
- Modify: `src/lib/api/types.ts:208-232`

**Interfaces:**
- Consumes: `FetchedRouteModel`。
- Produces: `normalizeFetchedModels(value: unknown): FetchedRouteModel[]`。
- Produces: `parseFetchedModelsFromConfig(configJson: string): FetchedRouteModel[]`。
- Produces: `writeFetchedModelsToConfig(config: Record<string, unknown>, models: FetchedRouteModel[]): Record<string, unknown>`。
- Produces: `CreateApiRouteCredentialInput.fetched_models_json?: string | null`。

- [x] **Step 1: 写入纯函数失败测试**

创建 `tests/accountFetchedModels.test.ts`：

```ts
import { describe, expect, it } from "vitest";
import {
  normalizeFetchedModels,
  parseFetchedModelsFromConfig,
  writeFetchedModelsToConfig,
} from "../src/lib/accountFetchedModels";

describe("accountFetchedModels", () => {
  it("parses valid models and ignores invalid cached entries", () => {
    const models = parseFetchedModelsFromConfig(
      JSON.stringify({
        fetched_models: [
          { id: " gpt-5 ", owned_by: " openai ", supports_1m: true },
          { id: "" },
          null,
        ],
      }),
    );

    expect(models).toEqual([
      { id: "gpt-5", owned_by: "openai", supports_1m: true },
    ]);
  });

  it("returns an empty list for missing or malformed cache data", () => {
    expect(parseFetchedModelsFromConfig("not-json")).toEqual([]);
    expect(parseFetchedModelsFromConfig(JSON.stringify({ fetched_models: {} }))).toEqual([]);
    expect(normalizeFetchedModels(undefined)).toEqual([]);
  });

  it("replaces fetched models while preserving unrelated config fields", () => {
    expect(
      writeFetchedModelsToConfig(
        { base_url: "https://api.example.com/v1", model_mappings: [] },
        [{ id: " gpt-5 ", owned_by: " openai " }],
      ),
    ).toEqual({
      base_url: "https://api.example.com/v1",
      model_mappings: [],
      fetched_models: [{ id: "gpt-5", owned_by: "openai" }],
    });
  });
});
```

- [x] **Step 2: 运行测试确认失败**

Run:

```powershell
pnpm test:run -- tests/accountFetchedModels.test.ts
```

Expected: FAIL，模块 `src/lib/accountFetchedModels.ts` 不存在。

- [x] **Step 3: 实现缓存规范化、解析和写回**

创建 `src/lib/accountFetchedModels.ts`：

```ts
import type { FetchedRouteModel } from "./api/types";

function recordFromUnknown(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

export function normalizeFetchedModels(value: unknown): FetchedRouteModel[] {
  if (!Array.isArray(value)) {
    return [];
  }
  const models: FetchedRouteModel[] = [];
  for (const item of value) {
    const record = recordFromUnknown(item);
    const id = typeof record?.id === "string" ? record.id.trim() : "";
    if (!id) {
      continue;
    }
    const ownedBy =
      typeof record?.owned_by === "string" ? record.owned_by.trim() : "";
    models.push({
      id,
      ...(ownedBy ? { owned_by: ownedBy } : {}),
      ...(typeof record?.supports_1m === "boolean"
        ? { supports_1m: record.supports_1m }
        : {}),
    });
  }
  return models;
}

export function parseFetchedModelsFromConfig(configJson: string): FetchedRouteModel[] {
  try {
    const config = recordFromUnknown(JSON.parse(configJson));
    return normalizeFetchedModels(config?.fetched_models);
  } catch {
    return [];
  }
}

export function writeFetchedModelsToConfig(
  config: Record<string, unknown>,
  models: FetchedRouteModel[],
): Record<string, unknown> {
  return { ...config, fetched_models: normalizeFetchedModels(models) };
}
```

- [x] **Step 4: 同步 TypeScript 新增账号输入**

在 `CreateApiRouteCredentialInput` 的 `model_mappings_json` 后加入：

```ts
fetched_models_json?: string | null;
```

- [x] **Step 5: 运行纯函数测试和类型检查**

Run:

```powershell
pnpm test:run -- tests/accountFetchedModels.test.ts
pnpm typecheck
```

Expected: PASS。

- [x] **Step 6: 提交前端缓存边界**

```powershell
git add src/lib/accountFetchedModels.ts src/lib/api/types.ts tests/accountFetchedModels.test.ts
git commit -m "feat: add account fetched-model cache helpers"
```

---

### Task 3: 接入账号新增和编辑流程

**Files:**
- Modify: `src/screens/AccountsScreen.tsx:36-105,1699-1732,2190-2260,2610-2670,2843-2938`
- Modify: `tests/AccountsScreen.test.tsx:90-155,1446-1480,1690-1840`

**Interfaces:**
- Consumes: `parseFetchedModelsFromConfig(configJson)` from Task 2。
- Consumes: `writeFetchedModelsToConfig(config, models)` from Task 2。
- Consumes: `CreateApiRouteCredentialInput.fetched_models_json` from Tasks 1-2。
- Produces: 新增账号保存时的 `fetched_models_json`。
- Produces: 编辑账号 `config_json.fetched_models` 的回填和更新行为。

- [x] **Step 1: 扩展新增账号交互测试**

修改现有 `fetches upstream models and one-click sets a model mapping`，要求保存调用同时包含模型缓存：

```ts
await waitFor(() =>
  expect(createApiRouteCredential).toHaveBeenCalledWith(
    expect.objectContaining({
      display_name: "Fetched API",
      model_mappings_json: "[{\"from\":\"gpt-5.5\",\"to\":\"gpt-5\"}]",
      fetched_models_json: JSON.stringify([
        { id: "gpt-4o", owned_by: "openai" },
        { id: "gpt-5", owned_by: "openai" },
      ]),
    }),
  ),
);
```

同时在“未获取模型列表即创建账号”的现有测试中断言：

```ts
expect(createApiRouteCredential).toHaveBeenCalledWith(
  expect.objectContaining({ fetched_models_json: "[]" }),
);
```

- [x] **Step 2: 写入编辑回填、刷新失败和失效测试**

在 API 编辑测试附近新增：

```ts
it("hydrates and saves the fetched model list without refetching", async () => {
  const api = {
    ...credentialsFixture[1],
    config_json: JSON.stringify({
      base_url: "https://api.example.com/v1",
      interface_format: "openai",
      model_mappings: [],
      fetched_models: [
        { id: "gpt-4o", owned_by: "openai" },
        { id: "gpt-5", owned_by: "openai" },
      ],
    }),
  };
  vi.mocked(listRouteCredentials).mockResolvedValue([api]);
  vi.mocked(updateRouteCredential).mockResolvedValue(api);

  renderScreen();
  await userEvent.click(await screen.findByRole("button", { name: "编辑 API Account" }));

  expect(screen.getByText(/已获取 2 个模型/)).toBeInTheDocument();
  expect(fetchRouteModels).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

  await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
  const config = JSON.parse(vi.mocked(updateRouteCredential).mock.calls[0][1].config_json);
  expect(config.fetched_models).toEqual([
    { id: "gpt-4o", owned_by: "openai" },
    { id: "gpt-5", owned_by: "openai" },
  ]);
});

it("keeps cached models when a manual refresh fails", async () => {
  const api = {
    ...credentialsFixture[1],
    config_json: JSON.stringify({
      base_url: "https://api.example.com/v1",
      interface_format: "openai",
      model_mappings: [],
      fetched_models: [{ id: "gpt-5", owned_by: "openai" }],
    }),
  };
  vi.mocked(listRouteCredentials).mockResolvedValue([api]);
  vi.mocked(fetchRouteModels).mockRejectedValueOnce(new Error("获取模型列表失败。"));

  renderScreen();
  await userEvent.click(await screen.findByRole("button", { name: "编辑 API Account" }));
  await userEvent.click(screen.getByRole("button", { name: "获取模型列表" }));

  expect(await screen.findByText("获取模型列表失败。")).toBeInTheDocument();
  expect(screen.getByText(/已获取 1 个模型/)).toBeInTheDocument();
});

it("clears cached models when the upstream connection changes", async () => {
  const api = {
    ...credentialsFixture[1],
    config_json: JSON.stringify({
      base_url: "https://api.example.com/v1",
      interface_format: "openai",
      model_mappings: [],
      fetched_models: [{ id: "gpt-5", owned_by: "openai" }],
    }),
  };
  vi.mocked(listRouteCredentials).mockResolvedValue([api]);
  vi.mocked(updateRouteCredential).mockResolvedValue(api);

  renderScreen();
  await userEvent.click(await screen.findByRole("button", { name: "编辑 API Account" }));
  await userEvent.clear(screen.getByLabelText("编辑 Base URL"));
  await userEvent.type(screen.getByLabelText("编辑 Base URL"), "https://new.example.com/v1");

  expect(screen.queryByText(/已获取 1 个模型/)).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "保存修改" }));
  await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
  const config = JSON.parse(vi.mocked(updateRouteCredential).mock.calls[0][1].config_json);
  expect(config.fetched_models).toEqual([]);
});
```

- [x] **Step 3: 运行账号界面测试确认失败**

Run:

```powershell
pnpm test:run -- tests/AccountsScreen.test.tsx
```

Expected: FAIL；新增保存请求缺少 `fetched_models_json`，编辑弹窗显示 0 个模型。

- [x] **Step 4: 接入新增账号保存**

在 `AccountsScreen.tsx` 导入 Task 2 的辅助函数：

```ts
import {
  parseFetchedModelsFromConfig,
  writeFetchedModelsToConfig,
} from "../lib/accountFetchedModels";
```

在 `createApiRouteCredential` 输入中，紧跟 `model_mappings_json` 加入：

```ts
fetched_models_json: JSON.stringify(apiFetchedModels),
```

批量 API Key 循环复用同一个 `apiFetchedModels` 状态，因此每个新账号得到同一份列表。

- [x] **Step 5: 接入编辑回填和保存**

把编辑初始化中的无条件清空：

```ts
setEditFetchedModels([]);
```

替换为：

```ts
setEditFetchedModels(parseFetchedModelsFromConfig(editingCredential.config_json));
```

在 `updateMutation` 中生成 `baseConfig` 后、写入失败策略前增加：

```ts
const configWithFetchedModels =
  editingCredential.kind === "api"
    ? writeFetchedModelsToConfig(baseConfig, editFetchedModels)
    : baseConfig;
const nextConfigJson = JSON.stringify(
  writeFailurePolicyToConfig(configWithFetchedModels, failurePolicy),
  null,
  2,
);
```

删除原先直接把 `baseConfig` 传给 `writeFailurePolicyToConfig` 的代码，确保 API 账号保存当前缓存，官方账号配置不增加该字段。

- [x] **Step 6: 核对已有失效和错误行为**

确认以下现有处理仍保留，且没有在 `editFetchModelsMutation.onMutate` 或 `onError` 中新增清空列表：

```ts
setEditApiKey(event.target.value);
setEditFetchedModels([]);

setEditApiBaseUrl(event.target.value);
setEditFetchedModels([]);

setEditApiInterfaceFormat(event.target.value as InterfaceFormat);
setEditFetchedModels([]);

setEditApiKeyField(event.target.value as AnthropicApiKeyField);
setEditFetchedModels([]);
```

`editFetchModelsMutation.onSuccess` 继续用新列表覆盖状态；`onError` 只设置错误字符串，从而保留未失效的缓存。

- [x] **Step 7: 运行前端测试和类型检查**

Run:

```powershell
pnpm test:run -- tests/accountFetchedModels.test.ts tests/AccountsScreen.test.tsx
pnpm typecheck
```

Expected: PASS。

- [x] **Step 8: 运行完整相关回归检查**

Run:

```powershell
pnpm test:run
pnpm rust:check
pnpm rust:test
```

Expected: 全部成功；现有账号、深链导入、复制、路由配置和模型获取测试无回归。

- [x] **Step 9: 检查差异并提交 UI 集成**

Run:

```powershell
git diff --check
git status --short
```

确认只包含本计划文件后，提交：

```powershell
git add src/screens/AccountsScreen.tsx tests/AccountsScreen.test.tsx
git commit -m "feat: reuse fetched models when editing accounts"
```

---

## 完成判定

- 新增 API 账号获取模型列表后，创建请求携带 `fetched_models_json`，Rust 服务将其写入 `config_json.fetched_models`。
- 再次编辑账号时立即显示已保存模型数量，且 `fetchRouteModels` 调用次数不增加。
- 手动刷新成功覆盖列表，失败保留仍有效的列表。
- 修改连接身份或来源字段后清空列表，保存后写入空数组，取消编辑则不改账号。
- 缺失或异常历史缓存不会让编辑页面崩溃。
- 账号复制继续完整继承 `config_json` 中的模型缓存。
- TypeScript、前端测试、Rust 检查和 Rust 测试全部通过。
