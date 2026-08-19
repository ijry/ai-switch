# 测试模型名按账号缓存 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把真实生成测试弹窗的「测试模型（可选）」从按平台共用改为按账号各自缓存，并持久化到 localStorage。

**Architecture:** 新建纯函数模块 `src/lib/modelTestModels.ts` 负责 localStorage 的读写、规范化与孤儿键清理（不依赖 React，可独立单测）；`AccountsScreen.tsx` 删掉 `routeTestModelsByPlatform`，换成一个由 localStorage 初始化的内存 map，`routeTestModel` 由「当前弹窗对象」派生。后端零改动。

**Tech Stack:** React 18、TypeScript、Vitest、Testing Library、localStorage。

## Global Constraints

- 后端零改动：不修改 `src-tauri/` 下任何文件，不改任何命令输入结构，无数据库迁移。
- localStorage 键恰为 `ai-switch.model-test-models`，全部缓存存在这一个键下的 JSON 对象里。
- 账号用**裸 id** 作键（`route_credentials.id` 是 `TEXT PRIMARY KEY`，全局唯一），不加平台前缀。
- 算力池键恰为 `pool:<platform>`，且只能由 `poolModelTestKey()` 构造，不得在别处拼字符串。
- 每个条目的形状恰为 `{ model: string; platform: PlatformId }`。`platform` 字段不可省略——清理逻辑依赖它。
- 落盘只发生在点「开始测试」时；输入框 `onChange` 只改内存，不落盘。
- 模型名 trim 后为空时**删除该键**，不得存空字符串。
- 回填时**原样显示，不做任何校验**，即使该模型已不在当前 datalist 选项里。
- 清理时必须跳过全部 `pool:` 前缀键，以及 `entry.platform !== platform` 的键。
- **内存态永不被整体重载覆盖。** 账号列表查询开着 `refetchOnWindowFocus: true`（`AccountsScreen.tsx:2083`），若在其回调里 `setModelTestModels(loadModelTestModels())`，用户切出再切回窗口时正在输入的内容会被落盘快照打回去。所有对内存态的更新都必须是**函数式增量更新**。
- localStorage 不可用或内容畸形时一律降级为「当作没缓存」，绝不抛异常给渲染。
- 不迁移旧的按平台值（原状态从未落盘，没有历史数据）。
- 不引入 i18n：`AccountsScreen.tsx` 全文硬编码中文，新增字符串同样硬编码中文。
- 文档使用中文（`docs/superpowers/plans` 与 `docs/superpowers/specs`）。

---

## 文件结构

- `src/lib/modelTestModels.ts`（新建）：localStorage 读写、规范化、清理。五个纯函数（比 spec 多一个 `pruneModelTestModelMap`，理由见 Task 1）加一个键常量与两个类型，storage 可注入。
- `tests/lib/modelTestModels.test.ts`（新建）：覆盖键格式、畸形数据降级、写入语义、清理规则、引用稳定性、storage 异常。
- `src/screens/AccountsScreen.tsx`（修改）：删除 `routeTestModelsByPlatform`，接入新模块。
- `tests/AccountsScreen.test.tsx`（修改）：追加 6 个界面测试，**删除** 1 个被取代的既有测试。

---

### Task 1: localStorage 纯函数模块

**Files:**
- Create: `src/lib/modelTestModels.ts`
- Test: `tests/lib/modelTestModels.test.ts`

**Interfaces:**
- Consumes: `PlatformId` from `src/lib/api/types.ts`。
- Produces: `MODEL_TEST_MODELS_STORAGE_KEY: string`（值为 `"ai-switch.model-test-models"`）。
- Produces: `type ModelTestModelEntry = { model: string; platform: PlatformId }`。
- Produces: `type ModelTestModelMap = Record<string, ModelTestModelEntry>`。
- Produces: `poolModelTestKey(platform: PlatformId): string`。
- Produces: `loadModelTestModels(storage?: Pick<Storage, "getItem">): ModelTestModelMap`。
- Produces: `pruneModelTestModelMap(map: ModelTestModelMap, liveAccountIds: Iterable<string>, platform: PlatformId): ModelTestModelMap`。
- Produces: `saveModelTestModel(key: string, model: string, platform: PlatformId, storage?: Pick<Storage, "getItem" | "setItem">): void`。
- Produces: `pruneModelTestModels(liveAccountIds: Iterable<string>, platform: PlatformId, storage?: Pick<Storage, "getItem" | "setItem">): void`。

`pruneModelTestModelMap` 是纯 map→map 变换，**无孤儿键时必须返回传入的同一个对象引用**。Task 2 靠这个性质让清理 effect 在绝大多数触发中不产生 state 变更、不触发重渲染。`saveModelTestModel` 与 `pruneModelTestModels` 的 storage 类型是 `Pick<Storage, "getItem" | "setItem">` 而非只有 `setItem`——两者都要先读出整张表再改写。

- [ ] **Step 1: 写入失败测试**

创建 `tests/lib/modelTestModels.test.ts`：

```ts
import { beforeEach, describe, expect, it } from "vitest";
import {
  loadModelTestModels,
  MODEL_TEST_MODELS_STORAGE_KEY,
  poolModelTestKey,
  pruneModelTestModelMap,
  pruneModelTestModels,
  saveModelTestModel,
} from "../../src/lib/modelTestModels";

function seed(value: unknown) {
  window.localStorage.setItem(
    MODEL_TEST_MODELS_STORAGE_KEY,
    typeof value === "string" ? value : JSON.stringify(value),
  );
}

function stored() {
  return JSON.parse(
    window.localStorage.getItem(MODEL_TEST_MODELS_STORAGE_KEY) ?? "null",
  );
}

describe("modelTestModels", () => {
  beforeEach(() => window.localStorage.clear());

  it("builds the pool key and defaults to an empty map", () => {
    expect(poolModelTestKey("codex")).toBe("pool:codex");
    expect(poolModelTestKey("claude")).toBe("pool:claude");
    expect(loadModelTestModels()).toEqual({});
  });

  it("reads account and pool entries verbatim", () => {
    seed({
      "cred-api-1": { model: "gpt-5.6-sol", platform: "codex" },
      "pool:codex": { model: "gpt-5", platform: "codex" },
    });

    expect(loadModelTestModels()).toEqual({
      "cred-api-1": { model: "gpt-5.6-sol", platform: "codex" },
      "pool:codex": { model: "gpt-5", platform: "codex" },
    });
  });

  it("falls back to an empty map for malformed top-level data", () => {
    seed("not-json");
    expect(loadModelTestModels()).toEqual({});

    seed(42);
    expect(loadModelTestModels()).toEqual({});

    seed(null);
    expect(loadModelTestModels()).toEqual({});

    seed([{ model: "gpt-5", platform: "codex" }]);
    expect(loadModelTestModels()).toEqual({});
  });

  it("skips malformed entries but keeps the valid ones", () => {
    seed({
      "bad-not-object": "gpt-5",
      "bad-no-model": { platform: "codex" },
      "bad-model-type": { model: 7, platform: "codex" },
      "bad-no-platform": { model: "gpt-5" },
      "bad-null": null,
      "good-1": { model: "gpt-5.6-sol", platform: "codex" },
    });

    expect(loadModelTestModels()).toEqual({
      "good-1": { model: "gpt-5.6-sol", platform: "codex" },
    });
  });

  it("writes one entry without disturbing the others", () => {
    seed({
      "cred-b": { model: "claude-opus-4-8", platform: "claude" },
      "pool:codex": { model: "gpt-5", platform: "codex" },
    });

    saveModelTestModel("cred-a", "gpt-5.6-sol", "codex");

    expect(stored()).toEqual({
      "cred-a": { model: "gpt-5.6-sol", platform: "codex" },
      "cred-b": { model: "claude-opus-4-8", platform: "claude" },
      "pool:codex": { model: "gpt-5", platform: "codex" },
    });
  });

  it("deletes the key when the model name is blank", () => {
    seed({
      "cred-a": { model: "gpt-5.6-sol", platform: "codex" },
      "cred-b": { model: "claude-opus-4-8", platform: "claude" },
    });

    saveModelTestModel("cred-a", "   ", "codex");

    expect(stored()).toEqual({
      "cred-b": { model: "claude-opus-4-8", platform: "claude" },
    });
  });

  it("trims the model name before storing it", () => {
    saveModelTestModel("cred-a", "  gpt-5.6-sol  ", "codex");

    expect(stored()).toEqual({
      "cred-a": { model: "gpt-5.6-sol", platform: "codex" },
    });
  });

  it("prunes orphaned account keys of the given platform only", () => {
    const map = {
      "cred-live": { model: "gpt-5", platform: "codex" as const },
      "cred-gone": { model: "gpt-4o", platform: "codex" as const },
      "cred-other-platform": { model: "claude-opus-4-8", platform: "claude" as const },
      "pool:codex": { model: "gpt-5", platform: "codex" as const },
      "pool:claude": { model: "claude-sonnet-4-5", platform: "claude" as const },
    };

    // cred-gone dropped; other-platform key, both pool keys and the live key stay.
    expect(pruneModelTestModelMap(map, ["cred-live"], "codex")).toEqual({
      "cred-live": { model: "gpt-5", platform: "codex" },
      "cred-other-platform": { model: "claude-opus-4-8", platform: "claude" },
      "pool:codex": { model: "gpt-5", platform: "codex" },
      "pool:claude": { model: "claude-sonnet-4-5", platform: "claude" },
    });
  });

  it("returns the very same map object when nothing is orphaned", () => {
    const map = {
      "cred-live": { model: "gpt-5", platform: "codex" as const },
      "pool:codex": { model: "gpt-5", platform: "codex" as const },
    };

    // Reference equality matters: the screen feeds this straight into setState,
    // and an unchanged reference is what stops a needless re-render.
    expect(pruneModelTestModelMap(map, new Set(["cred-live", "cred-extra"]), "codex")).toBe(map);
  });

  it("writes the pruned map back to storage", () => {
    seed({
      "cred-live": { model: "gpt-5", platform: "codex" },
      "cred-gone": { model: "gpt-4o", platform: "codex" },
      "pool:codex": { model: "gpt-5", platform: "codex" },
    });

    pruneModelTestModels(["cred-live"], "codex");

    expect(stored()).toEqual({
      "cred-live": { model: "gpt-5", platform: "codex" },
      "pool:codex": { model: "gpt-5", platform: "codex" },
    });
  });

  it("never writes anything when there is nothing to prune", () => {
    // No stored data at all: the key must not be created.
    pruneModelTestModels(["cred-live"], "codex");
    expect(window.localStorage.getItem(MODEL_TEST_MODELS_STORAGE_KEY)).toBeNull();

    seed({ "cred-live": { model: "gpt-5", platform: "codex" } });
    pruneModelTestModels(["cred-live"], "codex");
    expect(stored()).toEqual({ "cred-live": { model: "gpt-5", platform: "codex" } });
  });

  it("never throws when storage is unavailable", () => {
    const blocked = {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
    };

    expect(loadModelTestModels(blocked)).toEqual({});
    expect(() => saveModelTestModel("cred-a", "gpt-5", "codex", blocked)).not.toThrow();
    expect(() => pruneModelTestModels(["cred-a"], "codex", blocked)).not.toThrow();
  });
});
```

- [ ] **Step 2: 运行测试确认失败**

Run:

```powershell
pnpm test:run -- tests/lib/modelTestModels.test.ts
```

Expected: FAIL，模块 `src/lib/modelTestModels.ts` 不存在。

- [ ] **Step 3: 实现纯函数模块**

创建 `src/lib/modelTestModels.ts`：

```ts
import type { PlatformId } from "./api/types";

export const MODEL_TEST_MODELS_STORAGE_KEY = "ai-switch.model-test-models";

const POOL_KEY_PREFIX = "pool:";

export type ModelTestModelEntry = {
  model: string;
  platform: PlatformId;
};

export type ModelTestModelMap = Record<string, ModelTestModelEntry>;

export function poolModelTestKey(platform: PlatformId): string {
  return `${POOL_KEY_PREFIX}${platform}`;
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function normalizeEntry(value: unknown): ModelTestModelEntry | null {
  if (!isPlainObject(value)) {
    return null;
  }
  const { model, platform } = value;
  if (typeof model !== "string" || typeof platform !== "string") {
    return null;
  }
  return { model, platform: platform as PlatformId };
}

export function loadModelTestModels(
  storage: Pick<Storage, "getItem"> = window.localStorage,
): ModelTestModelMap {
  try {
    const raw = storage.getItem(MODEL_TEST_MODELS_STORAGE_KEY);
    if (!raw) {
      return {};
    }
    const parsed: unknown = JSON.parse(raw);
    if (!isPlainObject(parsed)) {
      return {};
    }
    const map: ModelTestModelMap = {};
    for (const [key, value] of Object.entries(parsed)) {
      const entry = normalizeEntry(value);
      if (entry) {
        map[key] = entry;
      }
    }
    return map;
  } catch {
    // Storage can be unavailable in restricted browser contexts.
    return {};
  }
}

export function pruneModelTestModelMap(
  map: ModelTestModelMap,
  liveAccountIds: Iterable<string>,
  platform: PlatformId,
): ModelTestModelMap {
  const live = new Set(liveAccountIds);
  const orphans = Object.keys(map).filter((key) => {
    // Pool keys belong to no account, and other platforms' accounts are absent
    // from this platform's account list, so neither can be judged here.
    if (key.startsWith(POOL_KEY_PREFIX) || map[key].platform !== platform) {
      return false;
    }
    return !live.has(key);
  });
  if (orphans.length === 0) {
    // Same reference means "nothing changed" to both setState and the writer.
    return map;
  }
  const next = { ...map };
  for (const key of orphans) {
    delete next[key];
  }
  return next;
}

function writeMap(
  map: ModelTestModelMap,
  storage: Pick<Storage, "setItem">,
): void {
  try {
    storage.setItem(MODEL_TEST_MODELS_STORAGE_KEY, JSON.stringify(map));
  } catch {
    // Storage can be unavailable in restricted browser contexts.
  }
}

export function saveModelTestModel(
  key: string,
  model: string,
  platform: PlatformId,
  storage: Pick<Storage, "getItem" | "setItem"> = window.localStorage,
): void {
  const map = loadModelTestModels(storage);
  const trimmed = model.trim();
  if (trimmed) {
    map[key] = { model: trimmed, platform };
  } else {
    // An empty name means "no cache", so drop the key instead of storing "".
    delete map[key];
  }
  writeMap(map, storage);
}

export function pruneModelTestModels(
  liveAccountIds: Iterable<string>,
  platform: PlatformId,
  storage: Pick<Storage, "getItem" | "setItem"> = window.localStorage,
): void {
  const map = loadModelTestModels(storage);
  const pruned = pruneModelTestModelMap(map, liveAccountIds, platform);
  if (pruned !== map) {
    writeMap(pruned, storage);
  }
}
```

- [ ] **Step 4: 运行测试确认通过**

Run:

```powershell
pnpm test:run -- tests/lib/modelTestModels.test.ts
pnpm typecheck
```

Expected: PASS，12 个测试全绿。

- [ ] **Step 5: 提交纯函数模块**

```powershell
git add src/lib/modelTestModels.ts tests/lib/modelTestModels.test.ts
git commit -m "feat: add per-account model test cache helpers"
```

---

### Task 2: 弹窗接入按账号缓存

**Files:**
- Modify: `src/screens/AccountsScreen.tsx:110`（导入区，在 `accountPresets` 导入块之前插入）
- Modify: `src/screens/AccountsScreen.tsx:1794`（替换 `routeTestModelsByPlatform` state）
- Modify: `src/screens/AccountsScreen.tsx:1815`（替换 `routeTestModel` 派生）
- Modify: `src/screens/AccountsScreen.tsx:2078-2084`（在 `allCredentialsQuery` 之后加清理 effect）
- Modify: `src/screens/AccountsScreen.tsx:3394-3401`（`submitModelTest` 落盘）
- Modify: `src/screens/AccountsScreen.tsx:5026-5031`（输入框 `onChange`）
- Test: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes: `loadModelTestModels()`、`pruneModelTestModelMap(map, ids, platform)`、`pruneModelTestModels(ids, platform)`、`saveModelTestModel(key, model, platform)`、`poolModelTestKey(platform)`、`type ModelTestModelMap` from Task 1。
- Consumes 现有 state：`modelTestAccount`（`:1802`）、`activePlatform`、`allCredentialsQuery`（`:2078`）。
- Produces: `modelTestStorageKey` 与 `routeTestModel` 两个派生值。

`routeTestModel` 的**变量名与类型（`string`）保持不变**，所以这三个读取点无需任何改动：`submitModelTest` 的 `model` 字段（`:3401`）、复制 curl 的 `requestedModel`（`:3309`）、进行中提示条（`:3914`）。

- [ ] **Step 1: 写入界面失败测试**

先在 `tests/AccountsScreen.test.tsx` 的 import 区加入：

```ts
import { MODEL_TEST_MODELS_STORAGE_KEY } from "../src/lib/modelTestModels";
```

然后在既有的 `it("keeps the optional test model separately for each agent tab", ...)`（`:2658-2701`）**之后**插入下面六个测试。

三处必须照抄、不能想当然的细节：

1. 单账号测试按钮的 aria-label 是 `测试 ${credential.display_name}`（`AccountsScreen.tsx:4540`），即 `测试 API Account` / `测试 Team Account`。
2. 算力池测试按钮的 aria-label 是 **`真实生成测试算力池路由`**（`AccountsScreen.tsx:3742`），不是 `真实生成测试`；它还带 `disabled={!modelTestEnabled || !hasEligiblePoolModelTestCredential || ...}`（`:3744`）。默认 fixture 的算力池是空的（`tests/AccountsScreen.test.tsx:382`），所以**必须先 `poolStateByPlatform.set("codex", [...])` 再 render，并 `waitFor` 到按钮 enabled**，照抄既有用例 `:2456-2461` 的写法。
3. `beforeEach` 已有 `window.localStorage.clear()`（`:252`），预置数据要写在测试体内部、`renderScreen` 之前。

```ts
  it("hydrates the test model from localStorage for that account", async () => {
    window.localStorage.setItem(
      MODEL_TEST_MODELS_STORAGE_KEY,
      JSON.stringify({
        "cred-api-1": { model: "gpt-5.6-sol", platform: "codex" },
      }),
    );

    renderScreen("codex", "out_of_pool");

    await userEvent.click(await screen.findByLabelText("测试 API Account"));
    expect(await screen.findByLabelText("弹窗测试模型")).toHaveValue("gpt-5.6-sol");
  });

  it("persists the test model only after the test starts", async () => {
    renderScreen("codex", "out_of_pool");

    await userEvent.click(await screen.findByLabelText("测试 API Account"));
    await userEvent.type(await screen.findByLabelText("弹窗测试模型"), "  gpt-4o  ");
    // Typing must not reach storage; only submitting does.
    expect(window.localStorage.getItem(MODEL_TEST_MODELS_STORAGE_KEY)).toBeNull();

    await userEvent.click(screen.getByLabelText("开始真实生成测试"));

    await waitFor(() =>
      expect(
        JSON.parse(window.localStorage.getItem(MODEL_TEST_MODELS_STORAGE_KEY) ?? "null"),
      ).toEqual({
        "cred-api-1": { model: "gpt-4o", platform: "codex" },
      }),
    );
    // What got stored must equal what was actually sent upstream.
    expect(routePoolTestModel).toHaveBeenCalledWith(
      expect.objectContaining({ account_id: "cred-api-1", model: "gpt-4o" }),
    );
  });

  it("keeps the optional test model separately for each account", async () => {
    renderScreen("codex", "out_of_pool");

    await userEvent.click(await screen.findByLabelText("测试 API Account"));
    await userEvent.type(await screen.findByLabelText("弹窗测试模型"), "gpt-4o");
    await userEvent.click(screen.getByLabelText("关闭真实生成测试弹窗"));

    // A different account on the same platform starts empty.
    const officialTest = screen.getByLabelText("测试 Team Account");
    await waitFor(() => expect(officialTest).toBeEnabled());
    await userEvent.click(officialTest);
    const officialInput = await screen.findByLabelText("弹窗测试模型");
    expect(officialInput).toHaveValue("");
    await userEvent.type(officialInput, "gpt-5.5");
    await userEvent.click(screen.getByLabelText("关闭真实生成测试弹窗"));

    // Each account still remembers its own value.
    await userEvent.click(screen.getByLabelText("测试 API Account"));
    expect(await screen.findByLabelText("弹窗测试模型")).toHaveValue("gpt-4o");
    await userEvent.click(screen.getByLabelText("关闭真实生成测试弹窗"));

    await userEvent.click(screen.getByLabelText("测试 Team Account"));
    expect(await screen.findByLabelText("弹窗测试模型")).toHaveValue("gpt-5.5");
  });

  it("keeps the pool test model separate from any account cache", async () => {
    window.localStorage.setItem(
      MODEL_TEST_MODELS_STORAGE_KEY,
      JSON.stringify({
        "cred-api-1": { model: "gpt-4o", platform: "codex" },
        "pool:codex": { model: "gpt-5", platform: "codex" },
      }),
    );
    // The pool test button stays disabled while the pool has no eligible member.
    poolStateByPlatform.set("codex", ["cred-official-1"]);
    renderScreen("codex", "out_of_pool");

    await userEvent.click(await screen.findByLabelText("测试 API Account"));
    expect(await screen.findByLabelText("弹窗测试模型")).toHaveValue("gpt-4o");
    await userEvent.click(screen.getByLabelText("关闭真实生成测试弹窗"));

    await waitFor(() =>
      expect(screen.getByLabelText("真实生成测试算力池路由")).toBeEnabled(),
    );
    await userEvent.click(screen.getByLabelText("真实生成测试算力池路由"));
    expect(await screen.findByLabelText("弹窗测试模型")).toHaveValue("gpt-5");
  });

  it("drops the cached model when the field is cleared and the test starts", async () => {
    window.localStorage.setItem(
      MODEL_TEST_MODELS_STORAGE_KEY,
      JSON.stringify({
        "cred-api-1": { model: "gpt-4o", platform: "codex" },
        "pool:codex": { model: "gpt-5", platform: "codex" },
      }),
    );

    renderScreen("codex", "out_of_pool");

    await userEvent.click(await screen.findByLabelText("测试 API Account"));
    await userEvent.clear(await screen.findByLabelText("弹窗测试模型"));
    await userEvent.click(screen.getByLabelText("开始真实生成测试"));

    await waitFor(() =>
      expect(
        JSON.parse(window.localStorage.getItem(MODEL_TEST_MODELS_STORAGE_KEY) ?? "null"),
      ).toEqual({
        "pool:codex": { model: "gpt-5", platform: "codex" },
      }),
    );
  });

  it("prunes cached models for accounts that no longer exist", async () => {
    window.localStorage.setItem(
      MODEL_TEST_MODELS_STORAGE_KEY,
      JSON.stringify({
        "cred-api-1": { model: "gpt-4o", platform: "codex" },
        "cred-deleted": { model: "gpt-4.1", platform: "codex" },
        "cred-claude": { model: "claude-opus-4-8", platform: "claude" },
        "pool:codex": { model: "gpt-5", platform: "codex" },
      }),
    );

    renderScreen("codex", "out_of_pool");
    await screen.findByLabelText("测试 API Account");

    // Only this platform's orphan is dropped; other platforms and pool keys stay.
    await waitFor(() =>
      expect(
        JSON.parse(window.localStorage.getItem(MODEL_TEST_MODELS_STORAGE_KEY) ?? "null"),
      ).toEqual({
        "cred-api-1": { model: "gpt-4o", platform: "codex" },
        "cred-claude": { model: "claude-opus-4-8", platform: "claude" },
        "pool:codex": { model: "gpt-5", platform: "codex" },
      }),
    );
  });
```

- [ ] **Step 2: 删除被取代的既有测试**

删除 `tests/AccountsScreen.test.tsx:2658-2701` 整个 `it("keeps the optional test model separately for each agent tab", ...)` 块。

它 `view.rerender` 切到 claude 后断言输入框为空，但 `listRouteCredentials` 被 mock 成无论传什么平台都返回同一份 `credentialsFixture`（`:378`），所以 claude 标签页里出现的仍是 `id: "cred-api-1"` 那个账号。新设计按账号 id 取值，键没变、值就还在，`toHaveValue("")` 必然失败——**而这个失败是正确的**：同一个账号 id 就该共享同一份缓存。它的替代者是 Step 1 里的 `keeps the optional test model separately for each account`（同平台两个不同账号各自记住各自的值）。跨平台隔离改由 `keeps the pool test model separate from any account cache` 覆盖，`pool:codex` 与 `pool:claude` 是天然不同的键。

- [ ] **Step 3: 运行测试确认失败**

Run:

```powershell
pnpm test:run -- tests/AccountsScreen.test.tsx
```

Expected: FAIL，六个新测试都报错——`MODEL_TEST_MODELS_STORAGE_KEY` 尚未被 `AccountsScreen` 读写，回填为空、localStorage 无写入、孤儿键未清理。

- [ ] **Step 4: 导入 Task 1 的纯函数**

在 `AccountsScreen.tsx` 现有的 `accountPresets` 导入块（`:110`）**之前**插入：

```ts
import {
  loadModelTestModels,
  type ModelTestModelMap,
  poolModelTestKey,
  pruneModelTestModelMap,
  pruneModelTestModels,
  saveModelTestModel,
} from "../lib/modelTestModels";
```

- [ ] **Step 5: 替换 state 与派生值**

把 `:1794` 这一行：

```ts
  const [routeTestModelsByPlatform, setRouteTestModelsByPlatform] = useState<Partial<Record<PlatformKey, string>>>({});
```

替换为：

```ts
  const [modelTestModels, setModelTestModels] = useState<ModelTestModelMap>(
    () => loadModelTestModels(),
  );
```

把 `:1815` 这一行：

```ts
  const routeTestModel = routeTestModelsByPlatform[activePlatform] ?? "";
```

替换为：

```ts
  const modelTestStorageKey = modelTestAccount?.id ?? poolModelTestKey(activePlatform);
  const routeTestModel = modelTestModels[modelTestStorageKey]?.model ?? "";
```

`modelTestAccount` 声明在 `:1802`，早于此处，可直接引用。`useState` 必须用惰性初始化（传函数而非直接调用 `loadModelTestModels()`），否则每次渲染都读一遍 localStorage。

- [ ] **Step 6: 加入孤儿键清理 effect**

在 `allCredentialsQuery` 声明块（`:2078-2084`）**之后**插入：

```ts
  const allCredentials = allCredentialsQuery.data;
  useEffect(() => {
    if (!allCredentials) {
      return;
    }
    const liveIds = allCredentials.map((credential) => credential.id);
    // The query returns only this platform's non-archived accounts, so pruning
    // is scoped to that platform and skips pool keys.
    pruneModelTestModels(liveIds, activePlatform);
    // Incremental, never a wholesale reload: this effect re-runs on every
    // window-focus refetch, and reloading from storage would wipe whatever the
    // user is typing right now. pruneModelTestModelMap returns the same object
    // when nothing is orphaned, so setState then bails out without a re-render.
    setModelTestModels((current) => pruneModelTestModelMap(current, liveIds, activePlatform));
  }, [activePlatform, allCredentials]);
```

`if (!allCredentials) return;` 这一行是必须的：查询未完成时 `data` 是 `undefined`，此时把空列表当作「现存账号全集」会把该平台所有缓存误删。

- [ ] **Step 7: 输入框只改内存**

把输入框（`:5022-5034`）的 `onChange`：

```tsx
                onChange={(event) =>
                  setRouteTestModelsByPlatform((current) => ({
                    ...current,
                    [activePlatform]: event.target.value,
                  }))
                }
```

替换为：

```tsx
                onChange={(event) =>
                  setModelTestModels((current) => ({
                    ...current,
                    [modelTestStorageKey]: {
                      model: event.target.value,
                      platform: activePlatform,
                    },
                  }))
                }
```

存**未 trim 的原值**，让用户能正常输入空格；trim 只在提交时做。

- [ ] **Step 8: 提交时落盘**

在 `submitModelTest`（`:3387`）里，把 `:3394-3395` 这两行：

```ts
    const accountId = modelTestAccount?.id ?? null;
    setTestingAccountId(accountId);
```

替换为：

```ts
    const accountId = modelTestAccount?.id ?? null;
    // Persist exactly what gets sent, so the cache means "what was last tested".
    const trimmedModel = routeTestModel.trim();
    saveModelTestModel(modelTestStorageKey, trimmedModel, activePlatform);
    setModelTestModels((current) => {
      const next = { ...current };
      if (trimmedModel) {
        next[modelTestStorageKey] = { model: trimmedModel, platform: activePlatform };
      } else {
        delete next[modelTestStorageKey];
      }
      return next;
    });
    setTestingAccountId(accountId);
```

并把下方 `:3401` 的 `model: routeTestModel.trim() || null` 改为复用同一个变量：

```ts
      model: trimmedModel || null,
```

内存态跟着做同样的增量更新（而不是重新 `loadModelTestModels()`），是为了让「trim 后为空 → 删键」「有值 → 存 trim 后的值」同步进内存，否则输入框里会残留未 trim 的空白串。

- [ ] **Step 9: 运行测试确认通过**

Run:

```powershell
pnpm test:run -- tests/AccountsScreen.test.tsx
pnpm typecheck
```

Expected: PASS，六个新测试全绿，既有账号测试无回归。若 `PlatformKey` 因不再被 `:1794` 使用而触发未使用报错，检查它在别处仍有使用（`:136` 声明为 `type PlatformKey = PlatformId`，全文多处引用），无需删除。

- [ ] **Step 10: 运行完整回归检查**

Run:

```powershell
pnpm test:run
pnpm rust:check
```

Expected: 全部成功。`pnpm rust:check` 用于确认本次改动确实没牵连后端（本计划不应修改 `src-tauri/` 下任何文件）。

- [ ] **Step 11: 检查差异并提交**

Run:

```powershell
git diff --check
git status --short
```

确认只改动了 `src/screens/AccountsScreen.tsx` 和 `tests/AccountsScreen.test.tsx` 后提交：

```powershell
git add src/screens/AccountsScreen.tsx tests/AccountsScreen.test.tsx
git commit -m "feat: cache the model test name per account"
```

---

## 完成判定

- 同平台的两个账号各自记住各自的测试模型名，互不覆盖。
- 关闭并重开应用后，账号的测试模型名仍在（值落在 localStorage 键 `ai-switch.model-test-models` 下）。
- 算力池测试的模型名存在 `pool:<platform>` 键下，不与任何单账号缓存串味。
- 打字过程不落盘，点「开始测试」后才落盘，且落盘值与实际发送给后端的 `model` 一致（同一个 `trimmedModel`）。
- 输入过程中切出再切回窗口（触发账号列表 refetch）不会清空正在输入的内容。
- 清空输入框并提交后该键被删除，下次打开显示占位符。
- 删除或归档账号后，切回该平台标签页时其孤儿缓存被清理；其它平台的账号键与全部 `pool:` 键不受影响。
- localStorage 不可用或内容畸形时，弹窗照常工作，输入框视作无缓存。
- 既有测试 `keeps the optional test model separately for each agent tab` 已被 `keeps the optional test model separately for each account` 取代。
- `src-tauri/` 无改动；`pnpm test:run`、`pnpm typecheck`、`pnpm rust:check` 全部通过。
