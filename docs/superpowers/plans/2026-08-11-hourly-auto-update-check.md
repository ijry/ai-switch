# 每小时自动检查更新实现计划

> **致代理开发者：** 使用 `superpowers:executing-plans` 按任务逐项执行本计划。每个步骤使用复选框跟踪；除非用户明确要求，不执行 Git 提交或推送。

**目标：** 将桌面端自动更新检查改为启动时立即检查，并在应用运行期间每隔一小时检查一次。

**架构：** 复用全局挂载的 `AutoUpdatePrompt`，在组件生命周期内管理立即检查和小时定时器。移除现有按自然日限流的 `localStorage` 逻辑，使用请求进行中标记避免检查请求重叠；更新页面的手动检查保持独立。

**技术栈：** React 18、TypeScript、Tauri updater plugin、Vitest、Testing Library。

## 全局约束

- 仅修改 `src/components/updates/AutoUpdatePrompt.tsx` 和 `tests/AutoUpdatePrompt.test.tsx`。
- 自动检查仅在桌面环境执行；非桌面环境不调用 `check()`，也不创建定时器。
- 保留检查失败静默、下载安装、重启应用和现有更新弹窗文案。
- 用户关闭提示后，下一小时检测到同一版本时再次显示。
- 不修改 Tauri updater 配置、更新清单格式、后端代码或手动更新页面。
- 不执行 Git 提交、创建分支或推送，除非用户另行明确要求。

---

### 任务 1：扩展自动更新定时行为测试

**文件：**
- 修改：`tests/AutoUpdatePrompt.test.tsx`

**接口：**
- 消费：`AutoUpdatePrompt` 的现有自动检查行为和 `@tauri-apps/plugin-updater` 的 `check()` mock。
- 产出：覆盖立即检查、每小时检查、重复提示、并发保护和卸载清理的回归测试。

- [ ] **步骤 1：替换每日限流测试**

将原有“每天一次”的测试改为使用 Vitest 假定时器，验证启动立即检查、未满一小时不检查、满一小时再次检查：

```tsx
const updateIntervalMs = 60 * 60 * 1000;

it("checks immediately and every hour while the app is running", async () => {
  vi.useFakeTimers();
  mocks.check.mockResolvedValue({ version: "0.3.0", body: "修复问题" });

  renderPrompt();
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });

  expect(mocks.check).toHaveBeenCalledTimes(1);

  await act(async () => {
    await vi.advanceTimersByTimeAsync(updateIntervalMs - 1);
  });
  expect(mocks.check).toHaveBeenCalledTimes(1);

  await act(async () => {
    await vi.advanceTimersByTimeAsync(1);
  });
  expect(mocks.check).toHaveBeenCalledTimes(2);
});
```

同时移除对 `ai-switch.last-update-check` 的断言，并从导入中加入 `act`。

- [ ] **步骤 2：运行测试确认当前实现失败**

运行：

```powershell
pnpm test:run -- tests/AutoUpdatePrompt.test.tsx
```

预期：新增的每小时检查断言失败，因为当前实现仍使用本地日期键阻止定时检查，且尚未创建定时器。

- [ ] **步骤 3：增加关闭后重复提示测试**

增加测试，使用同一个 `check()` 返回值，关闭首次弹窗后推进一小时，并断言同一版本的弹窗再次出现：

```tsx
it("shows the same release again after dismissal on the next hourly check", async () => {
  vi.useFakeTimers();
  mocks.check.mockResolvedValue({ version: "0.3.0" });

  renderPrompt();
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });

  const dialog = screen.getByRole("dialog", { name: "发现新版本" });
  fireEvent.click(within(dialog).getAllByRole("button", { name: "稍后" }).at(-1)!);
  expect(screen.queryByRole("dialog", { name: "发现新版本" })).not.toBeInTheDocument();

  await act(async () => {
    await vi.advanceTimersByTimeAsync(60 * 60 * 1000);
  });
  expect(screen.getByRole("dialog", { name: "发现新版本" })).toBeInTheDocument();
});
```

- [ ] **步骤 4：增加并发和卸载清理测试**

覆盖两个生命周期约束：

```tsx
it("does not overlap checks while a request is pending", async () => {
  vi.useFakeTimers();
  let resolveCheck: (value: null) => void = () => undefined;
  mocks.check.mockImplementation(
    () =>
      new Promise((resolve) => {
        resolveCheck = resolve;
      }),
  );

  const view = renderPrompt();
  await act(async () => {
    await vi.advanceTimersByTimeAsync(60 * 60 * 1000);
  });
  expect(mocks.check).toHaveBeenCalledTimes(1);

  resolveCheck(null);
  await act(async () => {
    await Promise.resolve();
  });
  view.unmount();
});

it("cleans up the hourly timer when unmounted", async () => {
  vi.useFakeTimers();
  mocks.check.mockResolvedValue(null);

  const view = renderPrompt();
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
  view.unmount();

  await act(async () => {
    await vi.advanceTimersByTimeAsync(60 * 60 * 1000);
  });
  expect(mocks.check).toHaveBeenCalledTimes(1);
});
```

在测试文件中通过 `afterEach(() => vi.useRealTimers())` 确保每个假定时器测试结束后恢复真实时钟。

- [ ] **步骤 5：运行测试确认新增断言仍失败**

运行：

```powershell
pnpm test:run -- tests/AutoUpdatePrompt.test.tsx
```

预期：测试仍因生产代码缺少小时定时器、并发标记和卸载清理而失败；失败范围限定在本次新增行为。

### 任务 2：实现每小时自动检查

**文件：**
- 修改：`src/components/updates/AutoUpdatePrompt.tsx`

**接口：**
- 消费：Tauri updater 的 `check()`、现有桌面环境判断和弹窗状态。
- 产出：组件挂载时立即检查、每小时检查、并发保护和定时器清理。

- [ ] **步骤 1：移除自然日限流状态**

删除 `LAST_UPDATE_CHECK_STORAGE_KEY` 和 `localDateKey()`，将 React 导入扩展为：

```tsx
import { useEffect, useRef, useState } from "react";
```

在组件内增加：

```tsx
const checkInProgressRef = useRef(false);
```

- [ ] **步骤 2：实现可复用的自动检查函数**

将现有 `useEffect` 替换为下面的生命周期逻辑：

```tsx
useEffect(() => {
  if (!isDesktop() || typeof window === "undefined") {
    return;
  }

  let cancelled = false;
  const checkForUpdate = async () => {
    if (checkInProgressRef.current) {
      return;
    }

    checkInProgressRef.current = true;
    try {
      const nextUpdate = await check();
      if (!cancelled && nextUpdate) {
        setUpdate(nextUpdate);
      }
    } catch {
    } finally {
      checkInProgressRef.current = false;
    }
  };

  void checkForUpdate();
  const intervalId = window.setInterval(() => {
    void checkForUpdate();
  }, 60 * 60 * 1000);

  return () => {
    cancelled = true;
    window.clearInterval(intervalId);
  };
}, []);
```

保持 `nextUpdate` 为空时不改变当前弹窗状态，保持自动检查错误静默；这样不会因为一次空响应或网络错误强制关闭用户正在查看的更新提示。

- [ ] **步骤 3：运行聚焦测试确认实现通过**

运行：

```powershell
pnpm test:run -- tests/AutoUpdatePrompt.test.tsx
```

预期：`AutoUpdatePrompt` 的所有测试通过，包含立即检查、小时检查、重复提示、并发保护和卸载清理。

### 任务 3：执行类型和全量回归验证

**文件：**
- 无新增文件。
- 复核：`src/components/updates/AutoUpdatePrompt.tsx`
- 复核：`tests/AutoUpdatePrompt.test.tsx`

**接口：**
- 消费：任务 1 和任务 2 的实现与测试。
- 产出：通过类型检查和完整前端测试的工作区改动。

- [ ] **步骤 1：运行更新组件测试**

运行：

```powershell
pnpm test:run -- tests/AutoUpdatePrompt.test.tsx tests/UpdatesScreen.test.tsx
```

预期：两个更新相关测试文件全部通过。

- [ ] **步骤 2：运行 TypeScript 类型检查**

运行：

```powershell
pnpm typecheck
```

预期：命令以退出码 `0` 完成，没有新增类型错误。

- [ ] **步骤 3：运行前端全量测试**

运行：

```powershell
pnpm test:run
```

预期：所有现有 Vitest 测试通过。

- [ ] **步骤 4：复核最终变更范围**

运行：

```powershell
git diff --check
git status --short
```

预期：没有空白错误；实现阶段仅涉及自动更新组件和测试，工作区同时保留本计划与规格文档这两份设计产物，不创建提交或 tag。
