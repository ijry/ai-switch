# AI Switch 应用自启动实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 Windows、macOS 和 Linux 桌面端增加随系统登录启动的开关；自启动触发时隐藏主窗口并保留托盘和后台服务。

**Architecture:** 使用官方 `tauri-plugin-autostart` 负责各平台登录项注册，系统注册项是唯一状态来源。Rust 外壳为注册项附加 `--autostart` 参数并在 `setup` 中隐藏窗口；前端通过独立适配模块读取和修改状态，设置页只在桌面运行时显示控件。

**Tech Stack:** Tauri 2、Rust、React 18、TypeScript、TanStack Query、Vitest、Testing Library、`tauri-plugin-autostart`。

**Spec:** `docs/superpowers/specs/2026-08-28-ai-switch-app-autostart-design.md`

## Global Constraints

- 桌面目标仅为 Windows、macOS、Linux；不为 Android 或 iOS 增加入口。
- 自启动注册项必须使用精确参数 `--autostart`，普通启动不得隐藏主窗口。
- 不向 `AppSettings`、`settings.json` 或 SQLite 写入自启动字段，也不新增 Web API/Tauri 自定义命令。
- Web 浏览器运行时不得渲染自启动控件或调用 `plugin:autostart|...`。
- `enable`、`disable` 或 `isEnabled` 失败时不得伪造成功状态；错误必须在界面可见。
- `hide` 失败只记录错误并继续初始化托盘、Web 服务、路由代理和恢复任务。
- `docs/superpowers/specs` 与 `docs/superpowers/plans` 中的文档使用中文。

---

### Task 1: 添加自启动依赖与能力权限

**Files:**
- Modify: `tests/TauriConfig.test.ts`
- Modify: `package.json`
- Modify: `pnpm-lock.yaml`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/capabilities/default.json`
- Generated after Tauri configuration: `src-tauri/gen/schemas/acl-manifests.json`, `src-tauri/gen/schemas/capabilities.json`, `src-tauri/gen/schemas/desktop-schema.json`, `src-tauri/gen/schemas/windows-schema.json`

**Interfaces:**
- Produces the npm package `@tauri-apps/plugin-autostart`, Rust crate `tauri-plugin-autostart`, and the `autostart:default` capability consumed by Task 2 and Task 3.

- [ ] **Step 1: Write the failing configuration test**

在 `tests/TauriConfig.test.ts` 中加入以下测试。它读取真实配置文件，要求依赖和能力声明同时存在：

```
function readSource(path: string) {
  return readFileSync(resolve(process.cwd(), path), "utf8");
}

it("declares the autostart packages and capability", () => {
  const packageJson = JSON.parse(
    readFileSync(resolve(process.cwd(), "package.json"), "utf8"),
  ) as { dependencies?: Record<string, string> };
  const cargo = readSource("src-tauri/Cargo.toml");
  const capability = JSON.parse(
    readFileSync(resolve(process.cwd(), "src-tauri/capabilities/default.json"), "utf8"),
  ) as { permissions?: string[] };

  expect(packageJson.dependencies?.["@tauri-apps/plugin-autostart"]).toBeDefined();
  expect(cargo).toMatch(/^tauri-plugin-autostart\s*=\s*"2/m);
  expect(capability.permissions).toContain("autostart:default");
});
```

- [ ] **Step 2: Run the test and verify it fails for the missing declarations**

Run: `pnpm vitest run tests/TauriConfig.test.ts -t "autostart"`

Expected: FAIL because the package dependency, Rust dependency, and capability are not yet declared.

- [ ] **Step 3: Add the dependency and capability declarations**

在 `package.json` 的 `dependencies` 中加入：

```
"@tauri-apps/plugin-autostart": "^2.5.1"
```

在 `src-tauri/Cargo.toml` 的 `[dependencies]` 中加入：

```
tauri-plugin-autostart = "2"
```

在 `src-tauri/capabilities/default.json` 的 `permissions` 数组中加入字符串 `autostart:default`，保留现有权限不变。

- [ ] **Step 4: Resolve the lockfile and rerun the configuration test**

Run: `pnpm install --lockfile-only`

Run: `pnpm vitest run tests/TauriConfig.test.ts -t "autostart"`

Expected: PASS；`pnpm-lock.yaml` 包含新的 npm 包和解析版本。

- [ ] **Step 5: Commit the dependency change**

```
git add tests/TauriConfig.test.ts package.json pnpm-lock.yaml src-tauri/Cargo.toml src-tauri/capabilities/default.json
git commit -m "build: add desktop autostart dependencies"
```

### Task 2: 注册 Rust 插件并隐藏自启动窗口

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `tests/TauriConfig.test.ts`

**Interfaces:**
- Produces `const AUTOSTART_ARG: &str = "--autostart"` and `fn is_autostart_launch<I, S>(args: I) -> bool` for the startup path.
- Consumes the `tauri-plugin-autostart` dependency and capability from Task 1.

- [ ] **Step 1: Write the failing Rust behavior tests**

在 `src-tauri/src/lib.rs` 文件末尾加入测试模块，先引用尚不存在的纯函数：

```
#[cfg(test)]
mod tests {
    use super::is_autostart_launch;

    #[test]
    fn recognizes_the_exact_autostart_argument() {
        assert!(is_autostart_launch([
            "ai-switch".to_string(),
            "--autostart".to_string(),
        ]));
    }

    #[test]
    fn ignores_normal_and_similar_arguments() {
        assert!(!is_autostart_launch([
            "ai-switch".to_string(),
            "--autostart=true".to_string(),
            "--auto-start".to_string(),
        ]));
    }
}
```

- [ ] **Step 2: Run the Rust tests and verify the expected missing-function failure**

Run: `pnpm rust:test --lib`

Expected: FAIL with an unresolved `is_autostart_launch` symbol, not with a test harness or syntax error.

- [ ] **Step 3: Add the constant and pure argument matcher**

在 `src-tauri/src/lib.rs` 的辅助函数附近加入：

```
const AUTOSTART_ARG: &str = "--autostart";

fn is_autostart_launch<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .any(|arg| arg.as_ref() == AUTOSTART_ARG)
}
```

在 `run()` 开头计算一次启动来源：

```
let launched_from_autostart = is_autostart_launch(std::env::args().skip(1));
```

- [ ] **Step 4: Register the plugin with the startup argument**

在现有 Tauri builder 的插件链中加入：

```
.plugin(
    tauri_plugin_autostart::Builder::new()
        .args([AUTOSTART_ARG])
        .app_name("AI Switch")
        .build(),
)
```

保留已有插件顺序和 `generate_handler!` 命令列表，不把插件命令复制到应用命令列表。

- [ ] **Step 5: Hide only the autostart window at setup time**

把下面的代码放在 `setup` 闭包的第一段、托盘菜单创建之前：

```
if launched_from_autostart {
    if let Some(window) = app.get_webview_window("main") {
        if let Err(error) = window.hide() {
            eprintln!("failed to hide window for autostart launch: {error}");
        }
    }
}
```

这段代码不得调用 `focus_main_window`，也不得跳过现有后台恢复任务。

- [ ] **Step 6: Add the static registration contract and run all Rust checks**

在 `tests/TauriConfig.test.ts` 加入：

```
it("registers the autostart plugin and hidden-launch argument", () => {
  const source = readSource("src-tauri/src/lib.rs");

  expect(source).toContain("tauri_plugin_autostart::Builder::new()");
  expect(source).toContain(".args([AUTOSTART_ARG])");
  expect(source).toContain("is_autostart_launch(std::env::args().skip(1))");
  expect(source).toContain("window.hide()");
});
```

Run: `cargo fmt --all -- --check`（工作目录 `src-tauri`）

Run: `pnpm rust:test --lib`

Run: `pnpm vitest run tests/TauriConfig.test.ts -t "autostart"`

Expected: 三个命令均 PASS；普通启动和相似参数测试均通过。

- [ ] **Step 7: Commit the Rust startup behavior**

```
git add src-tauri/src/lib.rs tests/TauriConfig.test.ts
git commit -m "feat: hide the window for autostart launches"
```

### Task 3: 封装前端插件 API

**Files:**
- Create: `src/lib/autostart.ts`
- Create: `tests/lib/autostart.test.ts`

**Interfaces:**
- Produces `isAutostartEnabled(): Promise<boolean>`, `enableAutostart(): Promise<void>`, and `disableAutostart(): Promise<void>`.
- Consumes `@tauri-apps/plugin-autostart` only; does not inspect runtime or persist local state.

- [ ] **Step 1: Write the failing adapter tests**

创建 `tests/lib/autostart.test.ts`：

```
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  disable,
  enable,
  isEnabled,
} from "@tauri-apps/plugin-autostart";
import {
  disableAutostart,
  enableAutostart,
  isAutostartEnabled,
} from "../../src/lib/autostart";

vi.mock("@tauri-apps/plugin-autostart", () => ({
  disable: vi.fn(),
  enable: vi.fn(),
  isEnabled: vi.fn(),
}));

describe("autostart adapter", () => {
  beforeEach(() => {
    vi.mocked(disable).mockReset();
    vi.mocked(enable).mockReset();
    vi.mocked(isEnabled).mockReset();
  });

  it("returns the system registration state", async () => {
    vi.mocked(isEnabled).mockResolvedValue(true);

    await expect(isAutostartEnabled()).resolves.toBe(true);
    expect(isEnabled).toHaveBeenCalledTimes(1);
  });

  it("delegates enable and disable operations", async () => {
    await enableAutostart();
    await disableAutostart();

    expect(enable).toHaveBeenCalledTimes(1);
    expect(disable).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 2: Run the adapter tests and verify the missing-module failure**

Run: `pnpm vitest run tests/lib/autostart.test.ts`

Expected: FAIL because `src/lib/autostart.ts` does not exist yet.

- [ ] **Step 3: Implement the minimal adapter**

创建 `src/lib/autostart.ts`：

```
import {
  disable,
  enable,
  isEnabled,
} from "@tauri-apps/plugin-autostart";

export function isAutostartEnabled(): Promise<boolean> {
  return isEnabled();
}

export function enableAutostart(): Promise<void> {
  return enable();
}

export function disableAutostart(): Promise<void> {
  return disable();
}
```

- [ ] **Step 4: Run the adapter tests and typecheck**

Run: `pnpm vitest run tests/lib/autostart.test.ts`

Run: `pnpm typecheck`

Expected: PASS，且适配模块不在导入阶段调用插件。

- [ ] **Step 5: Commit the adapter**

```
git add src/lib/autostart.ts tests/lib/autostart.test.ts
git commit -m "feat: add frontend autostart adapter"
```

### Task 4: 增加桌面设置控件与双语文案

**Files:**
- Create: `src/components/settings/autostart-settings.tsx`
- Create: `tests/AutostartSettings.test.tsx`
- Modify: `src/screens/SettingsScreen.tsx`
- Modify: `src/lib/i18n.tsx`

**Interfaces:**
- `AutostartSettings` renders nothing when `isDesktop()` is false.
- On desktop it reads query key `['autostart']`, calls the Task 3 adapter, and updates that cache only after the plugin operation succeeds.

- [ ] **Step 1: Write the failing component tests**

创建 `tests/AutostartSettings.test.tsx`，使用独立 QueryClient，模拟桌面检测和 Task 3 适配器：

```
import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  disableAutostart,
  enableAutostart,
  isAutostartEnabled,
} from "../src/lib/autostart";
import { isDesktop } from "../src/lib/transport";
import { I18nProvider } from "../src/lib/i18n";
import { createQueryClient } from "../src/lib/query/queryClient";
import { AutostartSettings } from "../src/components/settings/autostart-settings";

vi.mock("../src/lib/autostart", () => ({
  disableAutostart: vi.fn(),
  enableAutostart: vi.fn(),
  isAutostartEnabled: vi.fn(),
}));
vi.mock("../src/lib/transport", () => ({ isDesktop: vi.fn() }));

function renderControl() {
  return render(
    <QueryClientProvider client={createQueryClient()}>
      <I18nProvider initialLanguage="zh-CN">
        <AutostartSettings />
      </I18nProvider>
    </QueryClientProvider>,
  );
}

describe("AutostartSettings", () => {
  beforeEach(() => {
    vi.mocked(isDesktop).mockReset();
    vi.mocked(isAutostartEnabled).mockReset();
    vi.mocked(enableAutostart).mockReset();
    vi.mocked(disableAutostart).mockReset();
    vi.mocked(isDesktop).mockReturnValue(true);
    vi.mocked(enableAutostart).mockResolvedValue(undefined);
    vi.mocked(disableAutostart).mockResolvedValue(undefined);
  });

  it("loads the system state and enables autostart", async () => {
    vi.mocked(isAutostartEnabled).mockResolvedValue(false);

    renderControl();
    const checkbox = await screen.findByRole("checkbox", { name: "随系统启动 AI Switch" });
    expect(checkbox).not.toBeChecked();

    await userEvent.click(checkbox);

    await waitFor(() => expect(enableAutostart).toHaveBeenCalledTimes(1));
    expect(checkbox).toBeChecked();
  });

  it("disables autostart and keeps the unchecked state", async () => {
    vi.mocked(isAutostartEnabled).mockResolvedValue(true);

    renderControl();
    const checkbox = await screen.findByRole("checkbox", { name: "随系统启动 AI Switch" });
    await userEvent.click(checkbox);

    await waitFor(() => expect(disableAutostart).toHaveBeenCalledTimes(1));
    expect(checkbox).not.toBeChecked();
  });

  it("keeps the old state and reports a failed update", async () => {
    vi.mocked(isAutostartEnabled).mockResolvedValue(false);
    vi.mocked(enableAutostart).mockRejectedValue(new Error("permission denied"));

    renderControl();
    const checkbox = await screen.findByRole("checkbox", { name: "随系统启动 AI Switch" });
    await userEvent.click(checkbox);

    expect(await screen.findByText("无法更新自启动设置。"))
      .toBeInTheDocument();
    expect(checkbox).not.toBeChecked();
  });

  it("disables the control and reports a failed state read", async () => {
    vi.mocked(isAutostartEnabled).mockRejectedValue(new Error("unavailable"));

    renderControl();

    expect(await screen.findByText("无法读取自启动状态。"))
      .toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "随系统启动 AI Switch" }))
      .toBeDisabled();
  });

  it("does not render or query the plugin in Web runtime", async () => {
    vi.mocked(isDesktop).mockReturnValue(false);

    renderControl();

    expect(screen.queryByRole("checkbox", { name: "随系统启动 AI Switch" }))
      .not.toBeInTheDocument();
    expect(isAutostartEnabled).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run the component tests and verify they fail before implementation**

Run: `pnpm vitest run tests/AutostartSettings.test.tsx`

Expected: FAIL because `AutostartSettings` has not been created.

- [ ] **Step 3: Add matching English and Chinese translation keys**

在 `src/lib/i18n.tsx` 的 `en` 和 `zh` 对象中分别加入同名键：

```
// en
"settings.autostart.label": "Start AI Switch with the system",
"settings.autostart.description": "Launch in the tray after you sign in.",
"settings.autostart.readError": "Could not read the startup setting.",
"settings.autostart.updateError": "Could not update the startup setting.",

// zh
"settings.autostart.label": "随系统启动 AI Switch",
"settings.autostart.description": "登录系统后启动，并隐藏到托盘。",
"settings.autostart.readError": "无法读取自启动状态。",
"settings.autostart.updateError": "无法更新自启动设置。",
```

- [ ] **Step 4: Implement the desktop-only component**

创建 `src/components/settings/autostart-settings.tsx`，使用以下行为和接口：

```
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useI18n } from "../../lib/i18n";
import {
  disableAutostart,
  enableAutostart,
  isAutostartEnabled,
} from "../../lib/autostart";
import { isDesktop } from "../../lib/transport";

const AUTOSTART_QUERY_KEY = ["autostart"] as const;

export function AutostartSettings() {
  const desktop = isDesktop();
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const stateQuery = useQuery({
    queryKey: AUTOSTART_QUERY_KEY,
    queryFn: isAutostartEnabled,
    enabled: desktop,
    retry: false,
    refetchOnWindowFocus: false,
  });
  const updateMutation = useMutation({
    mutationFn: (enabled: boolean) =>
      enabled ? enableAutostart() : disableAutostart(),
    onSuccess: (_result, enabled) => {
      queryClient.setQueryData(AUTOSTART_QUERY_KEY, enabled);
    },
  });

  if (!desktop) {
    return null;
  }

  return (
    <div className="grid gap-1">
      <label className="flex max-w-xl items-start gap-2 rounded-xl border border-stone-200 bg-white px-3 py-2.5 text-[12px] font-semibold text-stone-700">
        <input
          aria-label={t("settings.autostart.label")}
          checked={stateQuery.data === true}
          className="mt-0.5"
          disabled={stateQuery.isPending || stateQuery.isError || updateMutation.isPending}
          onChange={(event) => updateMutation.mutate(event.target.checked)}
          type="checkbox"
        />
        <span className="grid gap-1">
          <span>{t("settings.autostart.label")}</span>
          <span className="text-[11px] font-medium text-stone-500">
            {t("settings.autostart.description")}
          </span>
        </span>
      </label>
      {stateQuery.isError ? (
        <p className="text-[11px] font-medium text-red-700">
          {t("settings.autostart.readError")}
        </p>
      ) : null}
      {updateMutation.isError ? (
        <p className="text-[11px] font-medium text-red-700">
          {t("settings.autostart.updateError")}
        </p>
      ) : null}
    </div>
  );
}
```

- [ ] **Step 5: Integrate the component into the existing app settings section**

在 `src/screens/SettingsScreen.tsx` 导入 `AutostartSettings`，并在数据目录提示之后、现有 cc-switch 复选框之前插入：

```
<AutostartSettings />
```

不得把自启动值并入传给 `saveSettings` 的 `AppSettings` 对象。

- [ ] **Step 6: Run focused UI tests and the existing settings suite**

Run: `pnpm vitest run tests/AutostartSettings.test.tsx tests/SettingsScreen.test.tsx`

Run: `pnpm typecheck`

Expected: 新测试和既有设置测试均 PASS；Web 运行时的既有设置界面不出现自启动控件。

- [ ] **Step 7: Commit the settings UI**

```
git add src/components/settings/autostart-settings.tsx tests/AutostartSettings.test.tsx src/screens/SettingsScreen.tsx src/lib/i18n.tsx
git commit -m "feat: add autostart setting"
```

### Task 5: 更新桌面文档、生成 Tauri schema 并完成回归验证

**Files:**
- Modify: `docs-site/docs/deploy/desktop.md`
- Modify: `docs-site/docs/en/deploy/desktop.md`
- Generated as needed: `src-tauri/gen/schemas/acl-manifests.json`, `src-tauri/gen/schemas/capabilities.json`, `src-tauri/gen/schemas/desktop-schema.json`, `src-tauri/gen/schemas/windows-schema.json`

**Interfaces:**
- Documents the user-visible behavior from the approved spec without adding a second configuration model.

- [ ] **Step 1: Extend the Chinese desktop deployment documentation**

在“自动启动”章节现有两个后台组件之后加入：

```
- **应用本身**：在“应用设置”中开启“随系统启动 AI Switch”后，用户登录系统时会启动桌面端。该启动会保留托盘和后台服务，但主窗口默认隐藏；点击托盘菜单即可显示。
```

- [ ] **Step 2: Add the equivalent English documentation**

在英文 `## Auto-start` 章节加入：

```
- **The desktop app**: Enable “Start AI Switch with the system” in App preferences to launch the desktop app when you sign in. The tray and background services remain available, while the main window starts hidden; use the tray menu to show it.
```

- [ ] **Step 3: Regenerate tracked Tauri schemas and inspect the diff**

Run: `pnpm tauri build --debug --no-bundle`

确认生成的 schema 中出现 `autostart:default`、`autostart:allow-enable`、`autostart:allow-disable` 和 `autostart:allow-is-enabled`，且没有无关能力或配置变化。若构建在外部 sidecar 打包阶段失败，保留前置阶段产生的 schema 变更并单独记录该失败原因，不手工删除插件权限条目。

- [ ] **Step 4: Run the complete verification suite**

Run: `pnpm typecheck`

Run: `pnpm test:run`

Run: `pnpm rust:check`

Run: `pnpm rust:test`

Expected: 所有命令退出码为 0，且没有 TypeScript、Rust 或 Vitest 警告导致的失败。

- [ ] **Step 5: Review the final diff and commit documentation/schema changes**

Run: `git diff --check`

Run: `git status --short`

确认工作区只包含本计划涉及的文档和生成 schema 后执行：

```
git add docs-site/docs/deploy/desktop.md docs-site/docs/en/deploy/desktop.md src-tauri/gen/schemas
git commit -m "docs: document desktop autostart"
```
