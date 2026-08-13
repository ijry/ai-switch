# MCP/Skills 多语言与 Skills 布局修复实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 MCP 和 Skills 页面补齐 `en`/`zh-CN` 本地化，并修复 Skills 双栏页面在长内容、窄窗口和大量列表下的布局溢出。

**Architecture:** 保留现有 MCP/Skills API 和页面行为，只把用户可见文案迁移到 `src/lib/i18n.tsx`，用错误码映射替代直接展示后端英文。Skills 页面采用可收缩的双栏网格，左侧列表和右侧编辑/预览分别建立滚动边界；本计划不引入新的 UI 依赖或新的后端数据模型。

**Tech Stack:** React 18、TypeScript、TanStack Query、`lucide-react`、Tailwind CSS、Vitest、Testing Library、现有 Tauri/Web transport。

## Global Constraints

- 仅支持现有 `en` 和 `zh-CN`，不新增第三种语言。
- 不修改 MCP/Skills 命令名称、请求字段或后端文件格式。
- MCP、Skills 页面中的用户可见英文不得继续硬编码；Skill ID、路径、版本、哈希和 MCP JSON 字段保持原样。
- 前端错误主提示按 `ApiClientError.code` 本地化，未知错误使用模块级通用回退文案。
- Skills 左侧列表必须独立滚动；页面根容器不得出现非预期横向滚动。
- 不覆盖工作区中已有的更新检查改动和未提交文档。
- 本计划只生成实施步骤；执行阶段未获用户明确要求时不执行 `git commit`。

---

### Task 1: 建立页面文案与错误码映射

**Files:**
- Modify: `src/lib/i18n.tsx`
- Modify: `src/lib/api/errors.ts`
- Create: `src/lib/api/errorMessages.ts`
- Test: `tests/api/errorMessages.test.ts`

**Interfaces:**
- `src/lib/api/errorMessages.ts` 导出 `apiErrorMessageKey(code: string)`，返回 `TranslationKey`。
- `src/lib/i18n.tsx` 导出 `TranslationKey` 类型，继续由英文词典键推导。
- `ApiClientError` 的 `code`、`details` 和 `recoverable` 字段保持不变。

- [ ] **Step 1: 为错误码映射写失败测试**

```ts
import { describe, expect, it } from "vitest";
import { apiErrorMessageKey } from "../../src/lib/api/errorMessages";

describe("apiErrorMessageKey", () => {
  it("maps MCP and Skills codes to stable translation keys", () => {
    expect(apiErrorMessageKey("mcp.config_invalid")).toBe("errors.mcp.configInvalid");
    expect(apiErrorMessageKey("skills.read_only")).toBe("errors.skills.readOnly");
  });

  it("falls back for unknown codes", () => {
    expect(apiErrorMessageKey("future.unknown")).toBe("errors.operationFailed");
  });
});
```

- [ ] **Step 2: 运行测试确认失败**

运行：`pnpm test:run -- tests/api/errorMessages.test.ts`

预期：FAIL，因为 `TranslationKey`、错误键和 `apiErrorMessageKey` 尚未存在。

- [ ] **Step 3: 添加中英文翻译键**

在 `src/lib/i18n.tsx` 的英文和简体中文字典中增加以下键组：

```text
errors.operationFailed
errors.mcp.configInvalid
errors.mcp.configIo
errors.mcp.invalidSpec
errors.mcp.marketplaceNetwork
errors.mcp.serverNotFound
errors.skills.configInvalid
errors.skills.configIo
errors.skills.invalidId
errors.skills.directoryMissing
errors.skills.pathInvalid
errors.skills.readOnly
errors.skills.notFound
errors.skills.manifestInvalid
errors.skills.packageScanFailed
```

同时增加 `mcp.*`、`skills.*`、`skills.package.*` 的页面标题、按钮、标签、加载、空状态、确认、客户端、来源和状态文案。导出由英文词典推导出的 `TranslationKey`，保证调用方只能使用已登记的键。

- [ ] **Step 4: 实现错误码到翻译键的纯函数**

在 `src/lib/api/errorMessages.ts` 中建立完整映射：

```ts
const ERROR_KEYS = {
  "mcp.config_invalid": "errors.mcp.configInvalid",
  "mcp.config_io": "errors.mcp.configIo",
  "skills.config_invalid": "errors.skills.configInvalid",
  "skills.config_io": "errors.skills.configIo",
  "skills.invalid_id": "errors.skills.invalidId",
  "skills.directory_missing": "errors.skills.directoryMissing",
  "skills.path_invalid": "errors.skills.pathInvalid",
  "skills.read_only": "errors.skills.readOnly",
  "skills.not_found": "errors.skills.notFound",
  "skills.manifest_invalid": "errors.skills.manifestInvalid",
  "skills.package_scan_failed": "errors.skills.packageScanFailed",
} as const;
```

未知 code 返回 `errors.operationFailed`；不要把 `details` 拼入主翻译。

- [ ] **Step 5: 运行测试确认通过**

运行：`pnpm test:run -- tests/api/errorMessages.test.ts`

预期：PASS，覆盖 MCP、Skills 和未知错误码。

### Task 2: 本地化 MCP 页面和客户端选择器

**Files:**
- Modify: `src/screens/McpScreen.tsx`
- Modify: `src/components/mcp/McpAppSelector.tsx`
- Modify: `src/components/mcp/catalog.ts`
- Create: `tests/McpScreen.test.tsx`
- Create: `tests/McpAppSelector.test.tsx`

**Interfaces:**
- `McpScreen` 使用 `useI18n()` 的 `t` 渲染页面文案。
- `McpAppSelector` 的 `legend` 仍由调用方传入，但客户端名称通过 `mcp.client.*` 翻译键生成。
- MCP API 调用方式和 `McpAppType` 类型不变。

- [ ] **Step 1: 为 MCP 中文渲染和错误提示写失败测试**

```tsx
it("renders MCP controls in Simplified Chinese", async () => {
  vi.mocked(mcpScanLocal).mockResolvedValue([]);
  render(
    <I18nProvider initialLanguage="zh-CN">
      <McpScreen />
    </I18nProvider>,
  );
  expect(await screen.findByText("MCP 服务器")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "添加服务器" })).toBeInTheDocument();
});

it("uses the localized message for a structured MCP error", async () => {
  vi.mocked(mcpScanLocal).mockRejectedValue(
    new ApiClientError("raw backend text", "mcp.config_invalid", null, true, null),
  );
  render(
    <I18nProvider initialLanguage="zh-CN">
      <McpScreen />
    </I18nProvider>,
  );
  expect(await screen.findByRole("alert")).toHaveTextContent("MCP 配置无效");
  expect(screen.getByRole("alert")).not.toHaveTextContent("raw backend text");
});
```

- [ ] **Step 2: 运行测试确认失败**

运行：`pnpm test:run -- tests/McpScreen.test.tsx`

预期：FAIL，因为页面仍使用硬编码英文并直接显示 `error.message`。

- [ ] **Step 3: 接入 `useI18n` 和错误码显示**

在 `McpScreen` 中：

1. 使用 `const { t } = useI18n()`。
2. 将 `Integrations`、`MCP servers`、`Local configuration`、`Marketplace`、`Refresh`、`Add server`、编辑器、市场参数、确认和空状态全部替换为 `t(...)`。
3. 用 `apiErrorMessageKey(error.code)` 生成错误主提示；仅在需要诊断时显示 `details`。
4. 将 `window.confirm` 的英文字符串替换为翻译文案。

在 `McpAppSelector` 中接收 `t` 或调用 `useI18n()`，将 `MCP_APPS` 改为只保存 `id` 和翻译键，避免目录中的英文 label 绕过 i18n。

- [ ] **Step 4: 运行 MCP 页面测试确认通过**

运行：`pnpm test:run -- tests/McpScreen.test.tsx tests/McpAppSelector.test.tsx`

预期：PASS；英文和简体中文下控件、客户端名称、加载态、空态和结构化错误均有本地化文本。

### Task 3: 本地化 Skills 页面和编辑器

**Files:**
- Modify: `src/screens/SkillsScreen.tsx`
- Modify: `src/components/skills/SkillsToolbar.tsx`
- Modify: `src/components/skills/SkillsList.tsx`
- Modify: `src/lib/api/errorMessages.ts`
- Create: `tests/SkillsScreen.test.tsx`

**Interfaces:**
- `SkillsToolbar` 的 props 结构保持不变；组件内部使用 `useI18n()`。
- `SkillsList` 的 `items`、`locations`、`selectedId` 和回调签名保持不变。
- `SkillEditor` 暂时保留在 `SkillsScreen.tsx`，本任务不做无关拆分。

- [ ] **Step 1: 为 Skills 双语渲染写失败测试**

```tsx
it("renders the global Skills screen in Simplified Chinese", async () => {
  vi.mocked(skillsListAgents).mockResolvedValue(agentFixture);
  vi.mocked(skillsList).mockResolvedValue({ ...skillsFixture, skills: [] });
  render(
    <I18nProvider initialLanguage="zh-CN">
      <SkillsScreen />
    </I18nProvider>,
  );
  expect(await screen.findByText("技能")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "新建技能" })).toBeInTheDocument();
});
```

- [ ] **Step 2: 运行测试确认失败**

运行：`pnpm test:run -- tests/SkillsScreen.test.tsx`

预期：FAIL，因为工具栏、列表、编辑器和错误区域仍包含硬编码英文。

- [ ] **Step 3: 替换 Skills 工具栏和列表文案**

在 `SkillsToolbar` 中本地化：

```text
Instructions
Skills
Browse, edit and share...
Refresh
Agent
Global
Project
Project directory
Choose project directory
Filter Skills
New Skill
```

在 `SkillsList` 中本地化可用数量、项目目录缺失、扫描中、内置只读和位置状态。`locations.path` 继续原样显示。

- [ ] **Step 4: 替换编辑器和错误区域文案**

在 `SkillsScreen.tsx` 中本地化：

```text
New skill
Not saved
Read only
Edit Skill
Delete Skill
Skill id
Layout
Skill directory
Markdown file
Cancel
Saving...
Save
Loading Skill content...
Select a Skill to preview it, or create a new one.
Delete {id}?
The Skill operation failed.
```

错误区域使用 `ApiClientError.code` 映射，不再直接把后端英文 `message` 当作用户提示。

- [ ] **Step 5: 运行 Skills 页面测试确认通过**

运行：`pnpm test:run -- tests/SkillsScreen.test.tsx`

预期：PASS，覆盖英文/中文、项目目录空状态、只读状态、加载态和 API 错误。

### Task 4: 修复 Skills 双栏网格和滚动边界

**Files:**
- Modify: `src/screens/SkillsScreen.tsx`
- Modify: `src/components/skills/SkillsList.tsx`
- Modify: `src/components/skills/SkillsToolbar.tsx`
- Modify: `tests/SkillsScreen.test.tsx`

**Interfaces:**
- 保持现有 Skills API、React Query key 和编辑回调不变。
- 布局修复只改变容器 class 和必要的 aria/结构，不改变列表数据。

- [ ] **Step 1: 写布局结构测试**

```tsx
it("keeps the list and editor inside shrinkable scroll containers", async () => {
  render(<SkillsScreen />);
  const list = await screen.findByRole("complementary");
  const editor = screen.getByRole("main");
  expect(list.className).toContain("min-h-0");
  expect(list.className).toContain("overflow-hidden");
  expect(editor.className).toContain("min-w-0");
  expect(editor.className).toContain("overflow-hidden");
});
```

如果测试环境只暴露语义节点而无法可靠验证 Tailwind class，则把断言改为稳定的 `data-testid="skills-list"`、`data-testid="skills-editor"`，并只断言对应元素拥有设计要求的 class。

- [ ] **Step 2: 运行测试确认失败**

运行：`pnpm test:run -- tests/SkillsScreen.test.tsx`

预期：FAIL，当前 `<div>` 网格和 `<aside>` 没有完整的收缩与滚动边界。

- [ ] **Step 3: 修复主网格和左栏**

将 `SkillsScreen` 主网格调整为：

```tsx
<div className="grid min-h-0 min-w-0 gap-3 lg:grid-cols-[minmax(240px,280px)_minmax(0,1fr)]">
```

将 `SkillsList` 外层调整为：

```text
min-h-0 min-w-0 flex flex-col overflow-hidden
```

将条目容器调整为：

```text
min-h-0 flex-1 overflow-y-auto
```

位置列表固定在左栏底部并允许自身截断，不能把主列表高度继续撑大。

- [ ] **Step 4: 修复右侧编辑器和长文本**

为 `<main>`、`SkillEditor` 根节点和预览 `<pre>` 增加 `min-w-0`/`min-h-0`；正文区域保持 `overflow-auto`。长路径使用 `break-all`，长 JSON/Markdown 不允许撑开 grid track。编辑模式的 textarea 使用 `min-h-0 flex-1`，按钮区保持固定高度。

- [ ] **Step 5: 修复工具栏隐式列**

将 `SkillsToolbar` 当前四列 grid 改为与五个实际子项对应的可收缩布局。推荐使用：

```text
grid gap-2 lg:grid-cols-[minmax(180px,240px)_auto_minmax(180px,240px)_minmax(220px,1fr)_auto]
```

Scope 两个按钮仍作为同一控件组，Project 模式填充第三列，Global 模式使用空白列；搜索框和新建按钮分别占据第四、第五列。窄屏自动换行，不产生隐式列。

- [ ] **Step 6: 运行测试确认通过**

运行：`pnpm test:run -- tests/SkillsScreen.test.tsx`

预期：PASS；列表、编辑器和 toolbar 的语义行为不变，布局容器具有明确的收缩和滚动边界。

### Task 5: 补齐 MCP/Skills 回归测试并执行检查

**Files:**
- Modify: `tests/McpScreen.test.tsx`
- Modify: `tests/McpAppSelector.test.tsx`
- Modify: `tests/SkillsScreen.test.tsx`
- Create: `tests/api/errorMessages.test.ts`

- [ ] **Step 1: 运行定向测试**

运行：

```text
pnpm test:run -- tests/McpScreen.test.tsx tests/McpAppSelector.test.tsx tests/SkillsScreen.test.tsx tests/api/errorMessages.test.ts
```

预期：PASS，覆盖双语文案、错误码映射、列表空态、项目作用域、只读状态和布局 class。

- [ ] **Step 2: 运行类型检查**

运行：`pnpm typecheck`

预期：PASS，不出现翻译键类型、错误映射或组件 props 回归。

- [ ] **Step 3: 运行全量前端测试**

运行：`pnpm test:run`

预期：PASS；既有 MCP、设置、更新检查和 transport 测试不回归。

- [ ] **Step 4: 完成人工响应式验收**

在桌面窗口检查 375px、768px、1024px 和 1440px：

1. 左侧列表独立纵向滚动。
2. 长 Skill ID、路径和正文不会产生页面级横向滚动。
3. 窄屏下双栏变单栏，工具栏正常换行。
4. 切换 `en`/`zh-CN` 后没有 MCP/Skills 用户可见硬编码英文。
