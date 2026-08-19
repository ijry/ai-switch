# 账号预设（服务商预设） Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在「新增账号」表单顶部加一个服务商预设下拉，选中 AgentRouter 即自动填好 Base URL、接口格式、模型映射和账号名称，用户通常只需再填 API Key。

**Architecture:** 预设数据与判定逻辑放进新的纯函数模块 `src/lib/accountPresets.ts`（不依赖 React，可独立单测）；`AccountsScreen.tsx` 只新增一个约 30 行的 `PresetFields` 渲染组件，套用逻辑写在下拉的 `onChange` 里直接调用现有 setter。不新增任何 state——下拉选中项由 `apiBaseUrl` 派生。后端零改动。

**Tech Stack:** React 18、TypeScript、Vitest、Testing Library、UnoCSS（Tailwind 风格 class 字符串）。

## Global Constraints

- 后端零改动：不修改 `src-tauri/` 下任何文件，不改 `CreateApiRouteCredentialInput`，无数据库迁移。
- 不新增 React state。下拉选中项必须由 `matchPresetByBaseUrl(activePlatform, apiBaseUrl)` 在渲染时派生。
- 不提供 `applyPreset` 纯函数；套用逻辑留在 UI 层的 `onChange` 中。
- 预设只作用于新增表单；编辑抽屉（`AccountsScreen.tsx:5579-6090`）不加预设下拉。
- 预设不修改 `platform`。
- 不引入 i18n：`AccountsScreen.tsx` 全文硬编码中文，新增字符串同样硬编码中文。
- 预设下拉的 `aria-label` 恰为 `创建 账号预设`（跟随 `UserAgentFields` 的 `${idPrefix} User-Agent 预设` 命名法）。
- 提示条文案恰为 `已套用 AgentRouter 预设，通常只需填写 API Key。`（含句末句号，"通常"二字必须保留）。
- 下拉首项恒为 `自定义`，其后才是该平台的预设条目。
- 两条预设的 `defaultName` 必须不相等。
- Base URL 匹配规则：两侧同做 trim、去尾部斜杠、转小写后要求**完全相等**，不做前缀匹配。
- 文档使用中文（`docs/superpowers/plans` 与 `docs/superpowers/specs`）。

---

## 文件结构

- `src/lib/accountPresets.ts`（新建）：预设数据数组与两个判定纯函数。唯一的服务商扩展点。
- `tests/accountPresets.test.ts`（新建）：覆盖纯函数的数据正确性与匹配边界。
- `src/screens/AccountsScreen.tsx`（修改）：新增 `PresetFields` 组件、导入纯函数、在 API 表单顶部渲染下拉。
- `tests/AccountsScreen.test.tsx`（修改）：覆盖用户可见的套用、不覆盖名称、覆盖映射、回到自定义、端到端保存、非 codex 不渲染。

---

### Task 1: 预设数据与判定纯函数

**Files:**
- Create: `src/lib/accountPresets.ts`
- Test: `tests/accountPresets.test.ts`

**Interfaces:**
- Consumes: `InterfaceFormat`、`ModelMapping`、`PlatformId` from `src/lib/api/types.ts`。
- Produces: `type AccountPreset = { id: string; platform: PlatformId; label: string; defaultName: string; baseUrl: string; interfaceFormat: InterfaceFormat; modelMappings: ModelMapping[] }`。
- Produces: `ACCOUNT_PRESETS: AccountPreset[]`。
- Produces: `presetsForPlatform(platform: PlatformId): AccountPreset[]`。
- Produces: `matchPresetByBaseUrl(platform: PlatformId, baseUrl: string): AccountPreset | null`。

- [x] **Step 1: 写入失败测试**

创建 `tests/accountPresets.test.ts`：

```ts
import { describe, expect, it } from "vitest";
import {
  ACCOUNT_PRESETS,
  matchPresetByBaseUrl,
  presetsForPlatform,
} from "../src/lib/accountPresets";

describe("accountPresets", () => {
  it("exposes both AgentRouter lines for codex", () => {
    const presets = presetsForPlatform("codex");

    expect(presets).toHaveLength(2);
    expect(presets.map((preset) => preset.baseUrl)).toEqual([
      "https://agentrouter.org/v1",
      "https://ps.air-outer.com/v1",
    ]);
  });

  it("returns no presets for platforms without any", () => {
    expect(presetsForPlatform("claude")).toEqual([]);
    expect(presetsForPlatform("gemini")).toEqual([]);
    expect(presetsForPlatform("grok")).toEqual([]);
  });

  it("describes the AgentRouter primary line completely", () => {
    const preset = presetsForPlatform("codex")[0];

    expect(preset.platform).toBe("codex");
    expect(preset.label).toBe("AgentRouter (agentrouter.org)");
    expect(preset.defaultName).toBe("AgentRouter");
    expect(preset.baseUrl).toBe("https://agentrouter.org/v1");
    expect(preset.interfaceFormat).toBe("openai");
    expect(preset.modelMappings).toEqual([
      { from: "gpt-5.6-sol", to: "gpt-5.6-sol" },
    ]);
  });

  it("describes the AgentRouter backup line completely", () => {
    const preset = presetsForPlatform("codex")[1];

    expect(preset.label).toBe("AgentRouter (ps.air-outer.com)");
    expect(preset.defaultName).toBe("AgentRouter 备用");
    expect(preset.baseUrl).toBe("https://ps.air-outer.com/v1");
    expect(preset.interfaceFormat).toBe("openai");
    expect(preset.modelMappings).toEqual([
      { from: "gpt-5.6-sol", to: "gpt-5.6-sol" },
    ]);
  });

  it("keeps every preset id and default name unique", () => {
    const ids = ACCOUNT_PRESETS.map((preset) => preset.id);
    const names = ACCOUNT_PRESETS.map((preset) => preset.defaultName);

    expect(new Set(ids).size).toBe(ids.length);
    expect(new Set(names).size).toBe(names.length);
  });

  it("matches a base url regardless of case, spacing or trailing slash", () => {
    for (const value of [
      "https://agentrouter.org/v1",
      "https://agentrouter.org/v1/",
      "https://AgentRouter.org/v1",
      "  https://agentrouter.org/v1  ",
    ]) {
      expect(matchPresetByBaseUrl("codex", value)?.id).toBe(
        presetsForPlatform("codex")[0].id,
      );
    }
  });

  it("matches the backup line independently", () => {
    expect(matchPresetByBaseUrl("codex", "https://ps.air-outer.com/v1")?.id).toBe(
      presetsForPlatform("codex")[1].id,
    );
  });

  it("returns null for unknown, empty or near-miss base urls", () => {
    expect(matchPresetByBaseUrl("codex", "https://api.example.com/v1")).toBeNull();
    expect(matchPresetByBaseUrl("codex", "")).toBeNull();
    expect(matchPresetByBaseUrl("codex", "   ")).toBeNull();
    // Different endpoints must not be mistaken for the primary line.
    expect(matchPresetByBaseUrl("codex", "https://agentrouter.org/v2")).toBeNull();
    expect(matchPresetByBaseUrl("codex", "https://agentrouter.org")).toBeNull();
  });

  it("scopes matching to the requested platform", () => {
    expect(matchPresetByBaseUrl("claude", "https://agentrouter.org/v1")).toBeNull();
  });
});
```

- [x] **Step 2: 运行测试确认失败**

Run:

```powershell
pnpm test:run -- tests/accountPresets.test.ts
```

Expected: FAIL，模块 `src/lib/accountPresets.ts` 不存在。

- [x] **Step 3: 实现预设数据与判定函数**

创建 `src/lib/accountPresets.ts`：

```ts
import type { InterfaceFormat, ModelMapping, PlatformId } from "./api/types";

export type AccountPreset = {
  id: string;
  platform: PlatformId;
  label: string;
  defaultName: string;
  baseUrl: string;
  interfaceFormat: InterfaceFormat;
  modelMappings: ModelMapping[];
};

export const ACCOUNT_PRESETS: AccountPreset[] = [
  {
    id: "agentrouter-primary",
    platform: "codex",
    label: "AgentRouter (agentrouter.org)",
    defaultName: "AgentRouter",
    baseUrl: "https://agentrouter.org/v1",
    interfaceFormat: "openai",
    modelMappings: [{ from: "gpt-5.6-sol", to: "gpt-5.6-sol" }],
  },
  {
    id: "agentrouter-backup",
    platform: "codex",
    label: "AgentRouter (ps.air-outer.com)",
    defaultName: "AgentRouter 备用",
    baseUrl: "https://ps.air-outer.com/v1",
    interfaceFormat: "openai",
    modelMappings: [{ from: "gpt-5.6-sol", to: "gpt-5.6-sol" }],
  },
];

export function presetsForPlatform(platform: PlatformId): AccountPreset[] {
  return ACCOUNT_PRESETS.filter((preset) => preset.platform === platform);
}

function normalizeBaseUrl(value: string) {
  return value.trim().replace(/\/+$/, "").toLowerCase();
}

export function matchPresetByBaseUrl(
  platform: PlatformId,
  baseUrl: string,
): AccountPreset | null {
  const normalized = normalizeBaseUrl(baseUrl);
  if (!normalized) {
    return null;
  }
  return (
    presetsForPlatform(platform).find(
      (preset) => normalizeBaseUrl(preset.baseUrl) === normalized,
    ) ?? null
  );
}
```

注意 `modelMappings` 数组是模块级共享引用。UI 层套用时必须复制（Task 2 Step 4 用 `preset.modelMappings.map((mapping) => ({ ...mapping }))`），否则用户编辑映射行会改坏预设常量本身。

- [x] **Step 4: 运行测试确认通过**

Run:

```powershell
pnpm test:run -- tests/accountPresets.test.ts
pnpm typecheck
```

Expected: PASS，9 个测试全绿。

- [x] **Step 5: 提交纯函数模块**

```powershell
git add src/lib/accountPresets.ts tests/accountPresets.test.ts
git commit -m "feat: add account preset data and matching helpers"
```

---

### Task 2: 新增表单接入预设下拉

**Files:**
- Modify: `src/screens/AccountsScreen.tsx:103-115`（导入区）
- Modify: `src/screens/AccountsScreen.tsx:1556`（在 `UserAgentFields` 前插入 `PresetFields`）
- Modify: `src/screens/AccountsScreen.tsx:5281-5291`（在「账号名称」前渲染下拉）
- Test: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes: `presetsForPlatform(platform)` from Task 1。
- Consumes: `matchPresetByBaseUrl(platform, baseUrl)` from Task 1。
- Consumes: `AccountPreset` 类型 from Task 1。
- Consumes 现有 setter：`setApiBaseUrl`、`setApiInterfaceFormat`、`setApiMappings`、`setApiName`、`setApiFetchedModels`、`setApiFetchModelsError`、`setApiMappingsError`（声明于 `:1685-1705`）。
- Consumes 现有样式常量：`fieldClass`、`labelClass`（声明于 `:3541-3544`）。
- Produces: `PresetFields` 组件（仅本文件内使用，不导出）。

- [x] **Step 1: 写入界面失败测试**

在 `tests/AccountsScreen.test.tsx` 中，紧跟现有 `it("fetches upstream models and one-click sets a model mapping", ...)`（`:1448-1482`）之后插入六个测试：

```ts
  it("applies the AgentRouter preset to the create form", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    // Move the interface format off "openai" first, so the assertion below
    // proves the preset set it rather than passively matching the codex default.
    await userEvent.selectOptions(screen.getByLabelText("接口格式"), "anthropic");
    await userEvent.selectOptions(
      screen.getByLabelText("创建 账号预设"),
      "agentrouter-primary",
    );

    expect(screen.getByLabelText("Base URL")).toHaveValue("https://agentrouter.org/v1");
    expect(screen.getByLabelText("接口格式")).toHaveValue("openai");
    expect(screen.getByLabelText("API 账号名称")).toHaveValue("AgentRouter");
    expect(screen.getByLabelText("请求模型 1")).toHaveValue("gpt-5.6-sol");
    expect(screen.getByLabelText("上游模型 1")).toHaveValue("gpt-5.6-sol");
    expect(
      screen.getByText("已套用 AgentRouter 预设，通常只需填写 API Key。"),
    ).toBeInTheDocument();
  });

  it("keeps a name the user already typed when applying a preset", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.type(screen.getByLabelText("API 账号名称"), "我的账号");
    await userEvent.selectOptions(
      screen.getByLabelText("创建 账号预设"),
      "agentrouter-primary",
    );

    expect(screen.getByLabelText("API 账号名称")).toHaveValue("我的账号");
    expect(screen.getByLabelText("Base URL")).toHaveValue("https://agentrouter.org/v1");
  });

  it("replaces existing model mappings when applying a preset", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    // The create form starts with zero mapping rows, so add one to overwrite.
    await userEvent.click(screen.getByRole("button", { name: "新增映射" }));
    await userEvent.type(screen.getByLabelText("请求模型 1"), "foo");
    await userEvent.type(screen.getByLabelText("上游模型 1"), "bar");
    await userEvent.selectOptions(
      screen.getByLabelText("创建 账号预设"),
      "agentrouter-primary",
    );

    expect(screen.getByLabelText("请求模型 1")).toHaveValue("gpt-5.6-sol");
    expect(screen.getByLabelText("上游模型 1")).toHaveValue("gpt-5.6-sol");
    expect(screen.queryByLabelText("请求模型 2")).not.toBeInTheDocument();
  });

  it("falls back to the custom option after the base url changes", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.selectOptions(
      screen.getByLabelText("创建 账号预设"),
      "agentrouter-primary",
    );
    expect(screen.getByLabelText("创建 账号预设")).toHaveValue("agentrouter-primary");

    await userEvent.clear(screen.getByLabelText("Base URL"));
    await userEvent.type(screen.getByLabelText("Base URL"), "https://other.example.com/v1");

    expect(screen.getByLabelText("创建 账号预设")).toHaveValue("");
    expect(
      screen.queryByText("已套用 AgentRouter 预设，通常只需填写 API Key。"),
    ).not.toBeInTheDocument();
  });

  it("creates an AgentRouter account from a preset and an api key alone", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.selectOptions(
      screen.getByLabelText("创建 账号预设"),
      "agentrouter-primary",
    );
    await userEvent.type(screen.getByLabelText("API Key"), "sk-agentrouter");
    await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

    await waitFor(() =>
      expect(createApiRouteCredential).toHaveBeenCalledWith(
        expect.objectContaining({
          platform: "codex",
          display_name: "AgentRouter",
          api_key: "sk-agentrouter",
          base_url: "https://agentrouter.org/v1",
          interface_format: "openai",
          model_mappings_json: "[{\"from\":\"gpt-5.6-sol\",\"to\":\"gpt-5.6-sol\"}]",
        }),
      ),
    );
  });

  it("hides the preset select on platforms without presets", async () => {
    renderScreen("claude");

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));

    expect(screen.queryByLabelText("创建 账号预设")).not.toBeInTheDocument();
  });
```

- [x] **Step 2: 运行测试确认失败**

Run:

```powershell
pnpm test:run -- tests/AccountsScreen.test.tsx
```

Expected: FAIL，前五个测试报找不到 `创建 账号预设` 这个可访问名称；最后一个（claude 隐藏）此时会误通过，因为下拉根本还不存在。

- [x] **Step 3: 导入 Task 1 的纯函数**

在 `AccountsScreen.tsx` 现有的 `accountUserAgent` 导入块（`:110-115`）之前插入：

```ts
import {
  type AccountPreset,
  matchPresetByBaseUrl,
  presetsForPlatform,
} from "../lib/accountPresets";
```

- [x] **Step 4: 新增 `PresetFields` 组件**

在 `function UserAgentFields({`（`:1556`）之前插入。组件只负责渲染，套用哪些字段由父级传入的 `onApply` 决定：

```tsx
function PresetFields({
  baseUrl,
  fieldClass,
  idPrefix,
  labelClass,
  onApply,
  platform,
}: {
  baseUrl: string;
  fieldClass: string;
  idPrefix: string;
  labelClass: string;
  onApply: (preset: AccountPreset) => void;
  platform: PlatformKey;
}) {
  const presets = presetsForPlatform(platform);
  if (presets.length === 0) {
    return null;
  }
  const matched = matchPresetByBaseUrl(platform, baseUrl);
  return (
    <label className={labelClass}>
      账号预设
      <select
        aria-label={`${idPrefix} 账号预设`}
        className={fieldClass}
        onChange={(event) => {
          const selected = presets.find((preset) => preset.id === event.target.value);
          if (!selected) {
            return;
          }
          onApply(selected);
        }}
        value={matched?.id ?? ""}
      >
        <option value="">自定义</option>
        {presets.map((preset) => (
          <option key={preset.id} value={preset.id}>
            {preset.label}
          </option>
        ))}
      </select>
      {matched ? (
        <span className="text-[11px] font-medium text-stone-500">
          已套用 AgentRouter 预设，通常只需填写 API Key。
        </span>
      ) : null}
    </label>
  );
}
```

选中"自定义"（`value=""`）时 `presets.find` 返回 `undefined`，函数直接 `return`，不改任何字段——与 `UserAgentFields:1579-1581` 的先例一致。

- [x] **Step 5: 在 API 表单顶部渲染下拉**

在 `{createMode === "api" && (` 分支内（`:5281-5282`）的 `<div className="mt-4 grid gap-3">` 之后、「账号名称」`<label>`（`:5283`）之前插入。必须放在这个分支内部——「批量导入」模式没有 Base URL 与模型映射字段，预设在那里无意义：

```tsx
                <PresetFields
                  baseUrl={apiBaseUrl}
                  fieldClass={fieldClass}
                  idPrefix="创建"
                  labelClass={labelClass}
                  onApply={(preset) => {
                    setApiBaseUrl(preset.baseUrl);
                    setApiInterfaceFormat(preset.interfaceFormat);
                    setApiMappings(preset.modelMappings.map((mapping) => ({ ...mapping })));
                    setApiName((current) => (current.trim() ? current : preset.defaultName));
                    setApiFetchedModels([]);
                    setApiFetchModelsError(null);
                    setApiMappingsError(null);
                  }}
                  platform={activePlatform}
                />
```

`setApiName` 用函数式更新读当前值，避免把 `apiName` 加进依赖；`modelMappings` 必须逐项浅拷贝，否则用户编辑映射行会改坏 Task 1 的模块级常量。

- [x] **Step 6: 运行测试确认通过**

Run:

```powershell
pnpm test:run -- tests/AccountsScreen.test.tsx
pnpm typecheck
```

Expected: PASS，六个新测试全绿且既有账号测试无回归。

- [x] **Step 7: 运行完整回归检查**

Run:

```powershell
pnpm test:run
pnpm rust:check
```

Expected: 全部成功。`pnpm rust:check` 用于确认本次改动确实没牵连后端（本计划不应修改 `src-tauri/` 下任何文件）。

- [x] **Step 8: 检查差异并提交**

Run:

```powershell
git diff --check
git status --short
```

确认只改动了 `src/screens/AccountsScreen.tsx` 和 `tests/AccountsScreen.test.tsx` 后提交：

```powershell
git add src/screens/AccountsScreen.tsx tests/AccountsScreen.test.tsx
git commit -m "feat: add provider preset select to the account create form"
```

---

## 完成判定

- codex 新增账号表单顶部出现「账号预设」下拉，首项为「自定义」，其后是两条 AgentRouter 线路。
- 选中预设后 Base URL、接口格式、模型映射自动填好，账号名称在原为空时填入预设默认名，并显示提示条 `已套用 AgentRouter 预设，通常只需填写 API Key。`
- 已手填的账号名称不被预设覆盖。
- 选中预设会覆盖已有模型映射，并清空已获取的模型列表与相关错误。
- 改写 Base URL 后下拉回到「自定义」，提示条消失。
- claude、gemini、grok 平台不渲染预设下拉。
- 仅选预设加填一个 API Key 即可创建账号，`createApiRouteCredential` 收到预设的 `base_url`、`interface_format` 与 `model_mappings_json`。
- `src-tauri/` 无改动；`pnpm test:run`、`pnpm typecheck`、`pnpm rust:check` 全部通过。
