# Vibe Taskbar Appearance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a QQ2007/Windows-XP-inspired skin taskbar, move Vibe theme and skin controls into one appearance popup, and compact left-rail directory labels to the final two path segments.

**Architecture:** Extend the existing safe skin `blocks` data model with a normalized `taskbar` block and additional region keys. Keep all behavior in `VibeScreen` through a small hardcoded taskbar action dispatcher; skin files only provide strings, image references, and region style values. The taskbar replaces the skin status bar only when enabled, while dark and light layouts keep their current layout and use the new appearance popup entry point.

**Tech Stack:** React 19, TypeScript, Tailwind utility classes, Vitest, Testing Library, JSZip, existing Vibe skin import/storage helpers.

## Global Constraints

- Work directly on `main`; do not create or switch branches or worktrees.
- Skin packages must not execute JavaScript, HTML, arbitrary URLs, native window calls, or custom callbacks.
- Safe taskbar actions are only `openAppearance`, `setTheme`, `importSkin`, and `clearSkin`.
- `setTheme` accepts only `dark`, `light`, or `skin`.
- Missing taskbar values fall back to built-in QQ2007-style defaults.
- Relative `blocks.taskbar.startButton.icon` and `blocks.taskbar.items[].icon` paths resolve from zip skin packages.
- The built-in skin enables the taskbar by default; custom skins can disable it with `blocks.taskbar.enabled: false`.
- If `blocks.taskbar.enabled` is true in skin mode, the taskbar is the single bottom chrome row and replaces the existing skin status bar.
- If `blocks.taskbar.enabled` is false, the existing skin status bar renders unchanged.
- Directory display labels use only the final two path segments, joined with `/`; full paths remain in titles, aria labels, resume logic, group keys, and command inputs.

---

## File Structure

- Modify `src/lib/vibeSkin.ts`: taskbar type definitions, region keys, default taskbar data, taskbar normalization, taskbar icon asset resolution, `getVibeSkinBlocks()` output.
- Modify `tests/lib/vibeSkin.test.ts`: taskbar normalization, asset resolution, defaults, CSS region variable coverage.
- Modify `src/screens/VibeScreen.tsx`: appearance popup state/rendering, toolbar entry point, start menu state/rendering, safe action dispatch, statusbar/taskbar switch, compact directory helper.
- Modify `tests/VibeScreen.test.tsx`: update old theme-cycle tests, add appearance popup, start menu, safe action, taskbar, custom disabled taskbar, import placement, and compact directory assertions.
- Modify `src/styles.css`: taskbar/start menu/tray/clock region consumers and default glossy QQ2007/XP styling.
- Modify `docs/vibe-skins.md`: document taskbar manifest, actions, region keys, zip assets, and appearance workflow.

---

### Task 1: Skin Taskbar Model

**Files:**
- Modify: `src/lib/vibeSkin.ts`
- Test: `tests/lib/vibeSkin.test.ts`

**Interfaces:**
- Consumes: existing `VibeSkinDefinition`, `VibeSkinBlocks`, `ResolvedVibeSkinBlocks`, `importVibeSkinPackage(file: File): Promise<VibeSkinDefinition>`, `readStoredVibeSkin(): VibeSkinDefinition | null`, `getVibeSkinBlocks(skin: VibeSkinDefinition): ResolvedVibeSkinBlocks`, `skinToCssVariables(skin: VibeSkinDefinition): CSSProperties`.
- Produces: `VibeSkinTaskbarAction`, `VibeSkinTaskbarMenuItem`, `VibeSkinTaskbarButton`, `VibeSkinTaskbarItem`, `VibeSkinTaskbarBlock`, `ResolvedVibeSkinTaskbarBlock`, and `ResolvedVibeSkinBlocks.taskbar`.

- [ ] **Step 1: Write failing tests for imported taskbar blocks and new region variables**

Append this case to `tests/lib/vibeSkin.test.ts` inside `describe("vibeSkin", () => { ... })`:

```ts
  it("imports taskbar blocks, taskbar icon assets, and taskbar region variables", async () => {
    const zip = new JSZip();
    zip.file(
      "skin.json",
      JSON.stringify({
        id: "taskbar-skin",
        name: "Taskbar Skin",
        ui: {
          accent: "#1678d8",
          background: "#0f6bc4",
        },
        regions: {
          taskbar: {
            background: "linear-gradient(#4bb5ff, #0d65bd)",
            border: "rgba(5,82,150,0.65)",
          },
          taskbarStartButton: {
            backgroundImage: "assets/start-bg.png",
            borderRadius: "999px",
          },
          taskbarStartMenu: {
            background: "linear-gradient(#ffffff, #c7ecff)",
          },
          taskbarMenuItem: {
            color: "#12375f",
          },
          taskbarItemActive: {
            background: "linear-gradient(#ffffff, #80caff)",
          },
          taskbarClock: {
            color: "#ffffff",
          },
        },
        blocks: {
          taskbar: {
            enabled: true,
            startButton: {
              label: "开始",
              icon: "assets/start.png",
            },
            startMenu: {
              items: [
                { label: "外观设置", action: "openAppearance" },
                { label: "切换亮色主题", action: "setTheme", theme: "light" },
                { label: "导入皮肤...", action: "importSkin" },
                { type: "separator" },
                { label: "禁用项", disabled: true },
                { label: "非法项", action: "launchNativeWindow" },
              ],
            },
            items: [
              { label: "AI Switch 终端", icon: "assets/app.png", active: true },
              { label: "资料卡", active: false },
            ],
            tray: ["Vibe", "在线"],
            clockFormat: "HH:mm",
          },
        },
      }),
    );
    zip.file("assets/start.png", new Uint8Array([137, 80, 78, 71]));
    zip.file("assets/app.png", new Uint8Array([137, 80, 78, 71]));
    zip.file("assets/start-bg.png", new Uint8Array([137, 80, 78, 71]));

    const blob = await zip.generateAsync({ type: "blob" });
    const skin = await importVibeSkinPackage(
      new File([blob], "taskbar.zip", { type: "application/zip" }),
    );
    const blocks = getVibeSkinBlocks(skin);
    const variables = skinToCssVariables(skin) as Record<string, unknown>;

    expect(blocks.taskbar.enabled).toBe(true);
    expect(blocks.taskbar.startButton.label).toBe("开始");
    expect(blocks.taskbar.startButton.icon).toMatch(/^data:image\/png;base64,/);
    expect(blocks.taskbar.startMenu.items).toEqual([
      { label: "外观设置", action: "openAppearance" },
      { label: "切换亮色主题", action: "setTheme", theme: "light" },
      { label: "导入皮肤...", action: "importSkin" },
      { type: "separator" },
      { label: "禁用项", disabled: true },
    ]);
    expect(blocks.taskbar.items[0]).toMatchObject({
      label: "AI Switch 终端",
      active: true,
    });
    expect(blocks.taskbar.items[0]?.icon).toMatch(/^data:image\/png;base64,/);
    expect(blocks.taskbar.items[1]).toEqual({ label: "资料卡", active: false });
    expect(blocks.taskbar.tray).toEqual(["Vibe", "在线"]);
    expect(blocks.taskbar.clockFormat).toBe("HH:mm");
    expect(variables["--vibe-taskbar-background-layer"]).toBe(
      "linear-gradient(#4bb5ff, #0d65bd)",
    );
    expect(variables["--vibe-taskbar-start-button-background-image"]).toMatch(
      /^url\("data:image\/png;base64,/,
    );
    expect(variables["--vibe-taskbar-start-menu-background-layer"]).toBe(
      "linear-gradient(#ffffff, #c7ecff)",
    );
    expect(variables["--vibe-taskbar-clock-color"]).toBe("#ffffff");
  });
```

- [ ] **Step 2: Write failing tests for defaults and custom taskbar disabling**

Append this case to `tests/lib/vibeSkin.test.ts`:

```ts
  it("resolves taskbar defaults and preserves custom disabled taskbars", () => {
    const builtIn = getVibeSkinBlocks({
      id: "minimal",
      name: "Minimal",
      ui: {
        accent: "#1678d8",
        accentText: "#ffffff",
        background: "#0f6bc4",
        backgroundOverlay: "transparent",
        panel: "rgba(226,245,255,0.88)",
        panelStrong: "rgba(255,255,255,0.96)",
        panelSubtle: "rgba(188,226,250,0.8)",
        border: "rgba(14,99,181,0.42)",
        text: "#0d315d",
        mutedText: "#3d6d9f",
        button: "#1678d8",
        buttonText: "#ffffff",
        buttonHover: "#2088e5",
        dangerBackground: "#b72434",
        dangerText: "#ffffff",
        tabBar: "rgba(239,250,255,0.94)",
        tabActive: "#ffffff",
        tabInactive: "rgba(151,210,247,0.54)",
        tabHover: "rgba(255,255,255,0.72)",
        focus: "#44a7ff",
      },
    });
    const disabled = getVibeSkinBlocks({
      id: "disabled",
      name: "Disabled",
      ui: {
        accent: "#1678d8",
        accentText: "#ffffff",
        background: "#0f6bc4",
        backgroundOverlay: "transparent",
        panel: "rgba(226,245,255,0.88)",
        panelStrong: "rgba(255,255,255,0.96)",
        panelSubtle: "rgba(188,226,250,0.8)",
        border: "rgba(14,99,181,0.42)",
        text: "#0d315d",
        mutedText: "#3d6d9f",
        button: "#1678d8",
        buttonText: "#ffffff",
        buttonHover: "#2088e5",
        dangerBackground: "#b72434",
        dangerText: "#ffffff",
        tabBar: "rgba(239,250,255,0.94)",
        tabActive: "#ffffff",
        tabInactive: "rgba(151,210,247,0.54)",
        tabHover: "rgba(255,255,255,0.72)",
        focus: "#44a7ff",
      },
      blocks: {
        taskbar: {
          enabled: false,
        },
      },
    });

    expect(builtIn.taskbar.enabled).toBe(true);
    expect(builtIn.taskbar.startButton.label).toBe("开始");
    expect(builtIn.taskbar.startMenu.items).toContainEqual({
      label: "外观设置",
      action: "openAppearance",
    });
    expect(builtIn.taskbar.startMenu.items).toContainEqual({
      label: "切换暗色主题",
      action: "setTheme",
      theme: "dark",
    });
    expect(builtIn.taskbar.items).toContainEqual({
      label: "AI Switch 终端",
      active: true,
    });
    expect(builtIn.taskbar.tray).toEqual(["Vibe", "在线"]);
    expect(disabled.taskbar.enabled).toBe(false);
    expect(disabled.taskbar.startButton.label).toBe("开始");
  });
```

- [ ] **Step 3: Run tests to verify they fail**

Run:

```bash
pnpm vitest run tests/lib/vibeSkin.test.ts
```

Expected: fail with TypeScript or assertion errors because `blocks.taskbar`, taskbar region keys, and taskbar icon resolution are not implemented.

- [ ] **Step 4: Add taskbar types and region keys**

In `src/lib/vibeSkin.ts`, extend `VIBE_SKIN_REGION_KEYS` by appending these entries after `"showcaseOrb"`:

```ts
  "taskbar",
  "taskbarStartButton",
  "taskbarStartMenu",
  "taskbarMenuItem",
  "taskbarItem",
  "taskbarItemActive",
  "taskbarTray",
  "taskbarClock",
```

Add these type definitions after `VibeSkinStatusbarBlock`:

```ts
export type VibeSkinTaskbarAction = "openAppearance" | "setTheme" | "importSkin" | "clearSkin";

export type VibeSkinTaskbarMenuItem =
  | {
      type: "separator";
    }
  | {
      label: string;
      action?: VibeSkinTaskbarAction;
      theme?: "dark" | "light" | "skin";
      disabled?: boolean;
    };

export type VibeSkinTaskbarButton = {
  label?: string;
  icon?: string;
};

export type VibeSkinTaskbarItem = {
  label?: string;
  icon?: string;
  active?: boolean;
};

export type VibeSkinTaskbarBlock = {
  enabled?: boolean;
  startButton?: VibeSkinTaskbarButton;
  startMenu?: {
    items?: VibeSkinTaskbarMenuItem[];
  };
  items?: VibeSkinTaskbarItem[];
  tray?: string[];
  clockFormat?: "HH:mm";
};

export type ResolvedVibeSkinTaskbarBlock = {
  enabled: boolean;
  startButton: Required<Pick<VibeSkinTaskbarButton, "label">> &
    Pick<VibeSkinTaskbarButton, "icon">;
  startMenu: {
    items: VibeSkinTaskbarMenuItem[];
  };
  items: Array<Required<Pick<VibeSkinTaskbarItem, "label" | "active">> &
    Pick<VibeSkinTaskbarItem, "icon">>;
  tray: string[];
  clockFormat: "HH:mm";
};
```

Add `taskbar?: VibeSkinTaskbarBlock;` to `VibeSkinBlocks`, and add `taskbar: ResolvedVibeSkinTaskbarBlock;` to `ResolvedVibeSkinBlocks`.

- [ ] **Step 5: Add built-in taskbar defaults**

Append `taskbar` to `DEFAULT_VIBE_SKIN_BLOCKS`:

```ts
  taskbar: {
    enabled: true,
    startButton: {
      label: "开始",
    },
    startMenu: {
      items: [
        { label: "外观设置", action: "openAppearance" },
        { label: "切换到皮肤模式", action: "setTheme", theme: "skin" },
        { label: "切换亮色主题", action: "setTheme", theme: "light" },
        { label: "切换暗色主题", action: "setTheme", theme: "dark" },
        { label: "导入皮肤...", action: "importSkin" },
        { type: "separator" },
        { label: "AI Switch 终端", disabled: true },
      ],
    },
    items: [{ label: "AI Switch 终端", active: true }],
    tray: ["Vibe", "在线"],
    clockFormat: "HH:mm",
  },
```

Add the same taskbar object to `BUILT_IN_VIBE_SKINS[0].blocks`.

- [ ] **Step 6: Add taskbar normalizers**

Add these helpers after `normalizeStatusbarBlock`:

```ts
const SAFE_TASKBAR_ACTIONS = new Set<VibeSkinTaskbarAction>([
  "openAppearance",
  "setTheme",
  "importSkin",
  "clearSkin",
]);

function normalizeTaskbarMenuItem(value: unknown): VibeSkinTaskbarMenuItem | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }

  const source = value as Record<string, unknown>;
  if (source.type === "separator") {
    return { type: "separator" };
  }

  const label = optionalString(source.label);
  if (!label) {
    return undefined;
  }

  const disabled = source.disabled === true;
  if (disabled) {
    return { label, disabled: true };
  }

  const rawAction = optionalString(source.action);
  if (!rawAction || !SAFE_TASKBAR_ACTIONS.has(rawAction as VibeSkinTaskbarAction)) {
    return undefined;
  }

  const item: VibeSkinTaskbarMenuItem = {
    label,
    action: rawAction as VibeSkinTaskbarAction,
  };
  if (item.action === "setTheme") {
    const theme = optionalString(source.theme);
    if (theme !== "dark" && theme !== "light" && theme !== "skin") {
      return undefined;
    }
    item.theme = theme;
  }
  return item;
}

function normalizeTaskbarButton(value: unknown): VibeSkinTaskbarButton | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }

  const source = value as Record<string, unknown>;
  const button: VibeSkinTaskbarButton = {};
  const label = optionalString(source.label);
  const icon = optionalString(source.icon);
  if (label) {
    button.label = label;
  }
  if (icon) {
    button.icon = icon;
  }
  return Object.keys(button).length > 0 ? button : undefined;
}

function normalizeTaskbarItems(value: unknown): VibeSkinTaskbarItem[] | undefined {
  if (!Array.isArray(value)) {
    return undefined;
  }

  const items = value
    .map((item): VibeSkinTaskbarItem | undefined => {
      if (!item || typeof item !== "object" || Array.isArray(item)) {
        return undefined;
      }
      const source = item as Record<string, unknown>;
      const label = optionalString(source.label);
      if (!label) {
        return undefined;
      }
      const normalized: VibeSkinTaskbarItem = { label };
      const icon = optionalString(source.icon);
      if (icon) {
        normalized.icon = icon;
      }
      if (typeof source.active === "boolean") {
        normalized.active = source.active;
      }
      return normalized;
    })
    .filter((item): item is VibeSkinTaskbarItem => Boolean(item));
  return items.length > 0 ? items : undefined;
}

function normalizeTaskbarBlock(value: unknown): VibeSkinTaskbarBlock | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }

  const source = value as Record<string, unknown>;
  const block: VibeSkinTaskbarBlock = {};
  if (typeof source.enabled === "boolean") {
    block.enabled = source.enabled;
  }
  const startButton = normalizeTaskbarButton(source.startButton);
  if (startButton) {
    block.startButton = startButton;
  }
  const startMenuSource =
    source.startMenu && typeof source.startMenu === "object" && !Array.isArray(source.startMenu)
      ? (source.startMenu as Record<string, unknown>)
      : undefined;
  const menuItems = Array.isArray(startMenuSource?.items)
    ? startMenuSource.items
        .map(normalizeTaskbarMenuItem)
        .filter((item): item is VibeSkinTaskbarMenuItem => Boolean(item))
    : undefined;
  if (menuItems && menuItems.length > 0) {
    block.startMenu = { items: menuItems };
  }
  const items = normalizeTaskbarItems(source.items);
  if (items) {
    block.items = items;
  }
  if (Array.isArray(source.tray)) {
    const tray = source.tray
      .map((item) => optionalString(item))
      .filter((item): item is string => Boolean(item));
    if (tray.length > 0) {
      block.tray = tray.slice(0, 6);
    }
  }
  if (source.clockFormat === "HH:mm") {
    block.clockFormat = "HH:mm";
  }
  return Object.keys(block).length > 0 ? block : undefined;
}
```

In `normalizeBlocks`, add:

```ts
    taskbar: normalizeTaskbarBlock(source.taskbar),
```

- [ ] **Step 7: Resolve taskbar icon assets**

In `resolveBlockAssets`, after showcase figure resolution, add:

```ts
  if (blocks.taskbar?.startButton?.icon) {
    blocks.taskbar.startButton.icon = await resolveImageReference(
      blocks.taskbar.startButton.icon,
      "blocks.taskbar.startButton.icon",
      resolveAsset,
    );
  }

  for (const item of blocks.taskbar?.items ?? []) {
    if (item.icon) {
      item.icon = await resolveImageReference(
        item.icon,
        "blocks.taskbar.items.icon",
        resolveAsset,
      );
    }
  }
```

- [ ] **Step 8: Merge taskbar defaults in `getVibeSkinBlocks()`**

Inside `getVibeSkinBlocks`, define `const taskbarSource = skin.blocks?.taskbar;` and add this property to the returned object:

```ts
    taskbar: {
      ...DEFAULT_VIBE_SKIN_BLOCKS.taskbar,
      ...taskbarSource,
      enabled: taskbarSource?.enabled ?? DEFAULT_VIBE_SKIN_BLOCKS.taskbar.enabled,
      startButton: {
        ...DEFAULT_VIBE_SKIN_BLOCKS.taskbar.startButton,
        ...taskbarSource?.startButton,
      },
      startMenu: {
        items:
          taskbarSource?.startMenu?.items ?? DEFAULT_VIBE_SKIN_BLOCKS.taskbar.startMenu.items,
      },
      items:
        taskbarSource?.items?.map((item) => ({
          active: false,
          ...item,
          label: item.label ?? "AI Switch 终端",
        })) ?? DEFAULT_VIBE_SKIN_BLOCKS.taskbar.items,
      tray: taskbarSource?.tray ?? DEFAULT_VIBE_SKIN_BLOCKS.taskbar.tray,
      clockFormat: taskbarSource?.clockFormat ?? DEFAULT_VIBE_SKIN_BLOCKS.taskbar.clockFormat,
    },
```

- [ ] **Step 9: Run focused skin model tests**

Run:

```bash
pnpm vitest run tests/lib/vibeSkin.test.ts
```

Expected: pass.

- [ ] **Step 10: Commit skin model task**

Run:

```bash
git add src/lib/vibeSkin.ts tests/lib/vibeSkin.test.ts
git commit -m "feat: add vibe skin taskbar model"
```

Expected: commit succeeds.

---

### Task 2: Vibe Screen Taskbar And Directory Labels

**Files:**
- Modify: `src/screens/VibeScreen.tsx`
- Test: `tests/VibeScreen.test.tsx`

**Interfaces:**
- Consumes: `ResolvedVibeSkinBlocks.taskbar`, `VibeSkinTaskbarMenuItem`, `skinFileInputRef`, `importSkin(event)`, `clearCustomSkin()`, `setThemeMode(theme)`, `setActiveSkinId(id)`.
- Produces: `compactDirectoryLabel(directory: string): string`, skin-mode taskbar rendering, start menu rendering, safe taskbar action dispatcher.

- [ ] **Step 1: Update test helpers for appearance-based theme switching**

In `tests/VibeScreen.test.tsx`, replace `switchToSkinTheme` with:

```ts
async function openAppearanceDialog() {
  await userEvent.click(await screen.findByRole("button", { name: "Switch Vibe theme" }));
  return screen.findByRole("dialog", { name: "Appearance" });
}

async function switchThemeFromAppearance(theme: "Solarized Dark" | "Light" | "Skin") {
  await openAppearanceDialog();
  await userEvent.click(await screen.findByRole("button", { name: theme }));
}

async function switchToSkinTheme() {
  await switchThemeFromAppearance("Skin");
}
```

- [ ] **Step 2: Write failing tests for compact directory labels**

Replace the first expectation in `"groups local sessions by project directory"` with:

```ts
    expect(await screen.findByText("repo/app")).toBeInTheDocument();
    expect(screen.queryByText("D:/repo/app")).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Expand folder D:/repo/app" }),
    ).toHaveAttribute("title", "D:/repo/app");
```

Keep `expandProjectDirectory()` unchanged so it verifies the full aria label still works.

- [ ] **Step 3: Write failing tests for skin taskbar rendering and statusbar replacement**

Append this case to `tests/VibeScreen.test.tsx`:

```ts
  it("renders the skin taskbar and replaces the old skin status bar when enabled", async () => {
    renderScreen();

    await switchToSkinTheme();

    expect(screen.getByRole("button", { name: "开始" })).toBeInTheDocument();
    expect(screen.getByText("AI Switch 终端")).toBeInTheDocument();
    expect(screen.getByText("AI Switch 已连接")).toBeInTheDocument();
    expect(screen.getByText("皮肤区域已启用")).toBeInTheDocument();
    expect(screen.getByText("Vibe")).toBeInTheDocument();
    expect(screen.getByText("在线")).toBeInTheDocument();
    expect(document.querySelector(".vibe-skin-taskbar")).toBeTruthy();
    expect(document.querySelector(".vibe-skin-status-bar")).toBeFalsy();
  });
```

- [ ] **Step 4: Write failing tests for disabled custom taskbars**

Append this case to `tests/VibeScreen.test.tsx`:

```ts
  it("falls back to the skin status bar when a custom skin disables the taskbar", async () => {
    window.localStorage.setItem(
      VIBE_SKIN_STORAGE_KEY,
      JSON.stringify({
        id: "no-taskbar",
        name: "No Taskbar",
        ui: {
          accent: "#1678d8",
          background: "#dff5ff",
          panel: "rgba(232,247,255,0.78)",
          panelStrong: "rgba(255,255,255,0.92)",
          panelSubtle: "rgba(216,239,255,0.68)",
          border: "rgba(15,99,184,0.34)",
          text: "#0d315d",
          mutedText: "#386b9e",
          button: "#1678d8",
          buttonText: "#ffffff",
          buttonHover: "#0f61ae",
        },
        blocks: {
          taskbar: {
            enabled: false,
          },
          statusbar: {
            left: "自定义左侧状态",
            right: "自定义右侧状态",
          },
        },
      }),
    );
    renderScreen();

    await switchToSkinTheme();

    expect(screen.queryByRole("button", { name: "开始" })).not.toBeInTheDocument();
    expect(document.querySelector(".vibe-skin-taskbar")).toBeFalsy();
    expect(document.querySelector(".vibe-skin-status-bar")).toBeTruthy();
    expect(screen.getByText("自定义左侧状态")).toBeInTheDocument();
    expect(screen.getByText("自定义右侧状态")).toBeInTheDocument();
  });
```

- [ ] **Step 5: Write failing tests for start menu interactions and safe actions**

Append this case to `tests/VibeScreen.test.tsx`:

```ts
  it("opens the taskbar start menu and executes only safe actions", async () => {
    window.localStorage.setItem(
      VIBE_SKIN_STORAGE_KEY,
      JSON.stringify({
        id: "menu-skin",
        name: "Menu Skin",
        ui: {
          accent: "#1678d8",
          background: "#dff5ff",
          panel: "rgba(232,247,255,0.78)",
          panelStrong: "rgba(255,255,255,0.92)",
          panelSubtle: "rgba(216,239,255,0.68)",
          border: "rgba(15,99,184,0.34)",
          text: "#0d315d",
          mutedText: "#386b9e",
          button: "#1678d8",
          buttonText: "#ffffff",
          buttonHover: "#0f61ae",
        },
        blocks: {
          taskbar: {
            startMenu: {
              items: [
                { label: "外观设置", action: "openAppearance" },
                { label: "切换暗色主题", action: "setTheme", theme: "dark" },
                { label: "非法动作", action: "nativeCloseWindow" },
                { type: "separator" },
                { label: "不可点击", disabled: true },
              ],
            },
          },
        },
      }),
    );
    renderScreen();

    await switchToSkinTheme();
    await userEvent.click(screen.getByRole("button", { name: "开始" }));

    expect(screen.getByRole("menu", { name: "开始菜单" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "外观设置" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "切换暗色主题" })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "非法动作" })).not.toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "不可点击" })).toBeDisabled();

    await userEvent.click(screen.getByRole("menuitem", { name: "切换暗色主题" }));

    expect(screen.queryByRole("button", { name: "开始" })).not.toBeInTheDocument();
    expect(screen.getByText("Solarized Dark")).toBeInTheDocument();

    await switchToSkinTheme();
    await userEvent.click(screen.getByRole("button", { name: "开始" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "外观设置" }));

    expect(await screen.findByRole("dialog", { name: "Appearance" })).toBeInTheDocument();
    expect(screen.queryByRole("menu", { name: "开始菜单" })).not.toBeInTheDocument();
  });
```

- [ ] **Step 6: Run tests to verify they fail**

Run:

```bash
pnpm vitest run tests/VibeScreen.test.tsx
```

Expected: fail because the toolbar still cycles themes directly, directory labels are full paths, taskbar rendering does not exist, and the start menu does not exist.

- [ ] **Step 7: Add compact directory helper**

In `src/screens/VibeScreen.tsx`, add this helper after `directoryLabel`:

```ts
function compactDirectoryLabel(directory: string) {
  const trimmed = directory.trim();
  const parts = trimmed.split(/[\\/]+/).filter(Boolean);
  if (parts.length < 2) {
    return directory;
  }
  return parts.slice(-2).join("/");
}
```

In the directory group trigger button, add `title={group.directory}` and replace the visible span content with:

```tsx
                      <span className="truncate">{compactDirectoryLabel(group.directory)}</span>
```

- [ ] **Step 8: Add state and helpers for appearance and start menu**

In `src/screens/VibeScreen.tsx`, add these imports:

```ts
import type {
  VibeSkinDefinition,
  VibeSkinTaskbarMenuItem,
} from "../lib/vibeSkin";
```

Replace the existing single type import if present.

Add these state hooks near the existing `themeMode` state:

```ts
  const [appearanceOpen, setAppearanceOpen] = useState(false);
  const [startMenuOpen, setStartMenuOpen] = useState(false);
```

Add this helper near `themeLabel`:

```ts
  const taskbarEnabled = Boolean(isSkin && skinBlocks.taskbar.enabled);
  const currentTime = new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date());
```

Add these handlers before `return (`:

```ts
  const openAppearance = () => {
    setStartMenuOpen(false);
    setAppearanceOpen(true);
  };

  const triggerSkinImport = () => {
    setStartMenuOpen(false);
    skinFileInputRef.current?.click();
  };

  const runTaskbarMenuItem = (item: VibeSkinTaskbarMenuItem) => {
    if ("type" in item || item.disabled || !item.action) {
      return;
    }

    setStartMenuOpen(false);
    if (item.action === "openAppearance") {
      setAppearanceOpen(true);
      return;
    }
    if (item.action === "setTheme") {
      if (item.theme === "dark" || item.theme === "light" || item.theme === "skin") {
        setThemeMode(item.theme);
      }
      return;
    }
    if (item.action === "importSkin") {
      skinFileInputRef.current?.click();
      return;
    }
    if (item.action === "clearSkin" && customSkin) {
      clearCustomSkin();
    }
  };
```

- [ ] **Step 9: Render taskbar and start menu**

Replace the current bottom statusbar block:

```tsx
        {isSkin && (
          <div className="vibe-skin-status-bar flex h-9 shrink-0 items-center justify-between gap-3 border-t px-4 text-[11px] font-medium">
            <span className="truncate">{skinBlocks.statusbar.left}</span>
            <span className="truncate">{skinBlocks.statusbar.right}</span>
          </div>
        )}
```

with:

```tsx
        {taskbarEnabled ? (
          <div className="vibe-skin-taskbar relative flex h-10 shrink-0 items-center gap-2 border-t px-2 text-[11px] font-medium">
            <button
              aria-expanded={startMenuOpen}
              className="vibe-skin-taskbar-start-button inline-flex h-8 shrink-0 items-center gap-2 rounded-full border px-3 font-semibold"
              onClick={() => setStartMenuOpen((current) => !current)}
              type="button"
            >
              {skinBlocks.taskbar.startButton.icon && (
                <img
                  alt=""
                  className="h-4 w-4 shrink-0 rounded"
                  src={skinBlocks.taskbar.startButton.icon}
                />
              )}
              <span>{skinBlocks.taskbar.startButton.label}</span>
            </button>
            {startMenuOpen && (
              <div
                aria-label="开始菜单"
                className="vibe-skin-taskbar-start-menu absolute bottom-full left-2 z-40 mb-2 w-64 overflow-hidden rounded-2xl border p-2 shadow-2xl"
                role="menu"
              >
                <div className="mb-2 rounded-xl px-3 py-2 text-[12px] font-semibold">
                  {skinBlocks.titlebar.title}
                </div>
                {skinBlocks.taskbar.startMenu.items.map((item, index) =>
                  "type" in item ? (
                    <div
                      className="my-1 h-px bg-[color-mix(in_srgb,var(--vibe-border)_72%,transparent)]"
                      key={`separator-${index}`}
                      role="separator"
                    />
                  ) : (
                    <button
                      className="vibe-skin-taskbar-menu-item flex w-full items-center rounded-xl px-3 py-2 text-left text-[12px] transition disabled:opacity-55"
                      disabled={item.disabled}
                      key={`${item.label}-${index}`}
                      onClick={() => runTaskbarMenuItem(item)}
                      role="menuitem"
                      type="button"
                    >
                      {item.label}
                    </button>
                  ),
                )}
              </div>
            )}
            <div className="flex min-w-0 flex-1 items-center gap-1">
              {skinBlocks.taskbar.items.map((item, index) => (
                <div
                  className={
                    item.active
                      ? "vibe-skin-taskbar-item-active inline-flex h-8 min-w-0 max-w-[180px] items-center gap-2 rounded-xl border px-3"
                      : "vibe-skin-taskbar-item inline-flex h-8 min-w-0 max-w-[180px] items-center gap-2 rounded-xl border px-3"
                  }
                  key={`${item.label}-${index}`}
                >
                  {item.icon && <img alt="" className="h-4 w-4 shrink-0 rounded" src={item.icon} />}
                  <span className="truncate">{item.label}</span>
                </div>
              ))}
            </div>
            <div className="vibe-skin-taskbar-tray flex h-8 shrink-0 items-center gap-2 rounded-xl border px-2">
              <span className="hidden max-w-[120px] truncate sm:inline">{skinBlocks.statusbar.left}</span>
              {skinBlocks.taskbar.tray.map((item) => (
                <span className="shrink-0" key={item}>
                  {item}
                </span>
              ))}
              <span className="hidden max-w-[120px] truncate sm:inline">{skinBlocks.statusbar.right}</span>
              <span className="vibe-skin-taskbar-clock rounded-lg px-2 py-1">{currentTime}</span>
            </div>
          </div>
        ) : (
          isSkin && (
            <div className="vibe-skin-status-bar flex h-9 shrink-0 items-center justify-between gap-3 border-t px-4 text-[11px] font-medium">
              <span className="truncate">{skinBlocks.statusbar.left}</span>
              <span className="truncate">{skinBlocks.statusbar.right}</span>
            </div>
          )
        )}
```

- [ ] **Step 10: Run focused screen tests**

Run:

```bash
pnpm vitest run tests/VibeScreen.test.tsx
```

Expected: appearance-related failures remain; taskbar and compact directory tests pass after the popup is implemented in Task 3.

- [ ] **Step 11: Commit screen taskbar task**

Run:

```bash
git add src/screens/VibeScreen.tsx tests/VibeScreen.test.tsx
git commit -m "feat: render vibe skin taskbar"
```

Expected: commit succeeds after Task 3 if the focused suite is kept green in a single working commit; if this task is committed before Task 3, commit only when existing tests are adjusted to pass.

---

### Task 3: Appearance Popup

**Files:**
- Modify: `src/screens/VibeScreen.tsx`
- Modify: `src/lib/i18n.tsx`
- Test: `tests/VibeScreen.test.tsx`

**Interfaces:**
- Consumes: `appearanceOpen`, `setAppearanceOpen`, `themeMode`, `setThemeMode`, `availableSkins`, `activeSkinId`, `setActiveSkinId`, `customSkin`, `clearCustomSkin()`, `skinFileInputRef`, `triggerSkinImport()`.
- Produces: one shared appearance dialog containing theme choices, skin select, import button, clear custom skin button, and help text.

- [ ] **Step 1: Replace old theme cycle test with appearance popup expectations**

In `tests/VibeScreen.test.tsx`, replace the full `"cycles Vibe through dark, light, and skin themes"` case with:

```ts
  it("opens appearance settings and switches Vibe themes from the dialog", async () => {
    renderScreen();

    expect(screen.getByText("Solarized Dark")).toBeInTheDocument();

    await openAppearanceDialog();
    expect(screen.getByRole("button", { name: "Solarized Dark" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: "Light" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );

    await userEvent.click(screen.getByRole("button", { name: "Light" }));

    expect(screen.getByText("Light")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Expand folder D:/repo/app" })).toHaveClass(
      "text-stone-800",
    );
    expect(screen.getByText("No terminal tabs yet.").parentElement).toHaveClass("bg-white/85");
    expect(screen.getByText("Start or resume a session")).toHaveClass("text-stone-900");

    await openAppearanceDialog();
    await userEvent.click(screen.getByRole("button", { name: "Skin" }));

    expect(screen.getByRole("button", { name: "Switch Vibe theme" })).toHaveTextContent("Skin");
    expect(screen.getByLabelText("Vibe skin")).toHaveValue("codex-2007-blue");
    expect(screen.getByText("No terminal tabs yet.").parentElement).toHaveClass("vibe-skin-tabbar");
  });
```

- [ ] **Step 2: Write failing tests for moving skin controls into the popup**

Append this case to `tests/VibeScreen.test.tsx`:

```ts
  it("keeps skin select, import, and clear controls inside the appearance dialog", async () => {
    window.localStorage.setItem(
      VIBE_SKIN_STORAGE_KEY,
      JSON.stringify({
        id: "stored-popup-skin",
        name: "Stored Popup Skin",
        ui: {
          accent: "#1678d8",
          background: "#dff5ff",
          panel: "rgba(232,247,255,0.78)",
          panelStrong: "rgba(255,255,255,0.92)",
          panelSubtle: "rgba(216,239,255,0.68)",
          border: "rgba(15,99,184,0.34)",
          text: "#0d315d",
          mutedText: "#386b9e",
          button: "#1678d8",
          buttonText: "#ffffff",
          buttonHover: "#0f61ae",
        },
      }),
    );
    renderScreen();

    expect(screen.queryByLabelText("Vibe skin")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Import skin" })).not.toBeInTheDocument();

    await openAppearanceDialog();

    const dialog = screen.getByRole("dialog", { name: "Appearance" });
    expect(dialog).toContainElement(screen.getByLabelText("Vibe skin"));
    expect(dialog).toContainElement(screen.getByRole("button", { name: "Import skin" }));
    expect(dialog).toContainElement(screen.getByRole("button", { name: "Clear custom skin" }));

    await userEvent.selectOptions(screen.getByLabelText("Vibe skin"), "codex-2007-blue");
    expect(screen.getByLabelText("Vibe skin")).toHaveValue("codex-2007-blue");

    await userEvent.click(screen.getByRole("button", { name: "Clear custom skin" }));
    expect(window.localStorage.getItem(VIBE_SKIN_STORAGE_KEY)).toBeNull();
  });
```

- [ ] **Step 3: Write failing test for importing from the appearance dialog**

Update `"imports a custom Vibe skin package and applies its terminal theme"` so the upload is performed after opening the appearance dialog:

```ts
    await openAppearanceDialog();
    await userEvent.upload(screen.getByLabelText("Choose Vibe skin package"), skinFile);
```

Keep the existing `waitFor`, local storage, and terminal assertions.

- [ ] **Step 4: Run tests to verify they fail**

Run:

```bash
pnpm vitest run tests/VibeScreen.test.tsx
```

Expected: fail because the appearance popup and moved controls are not rendered.

- [ ] **Step 5: Add i18n labels**

In `src/lib/i18n.tsx`, add these English keys near the existing `vibe.switchTheme` entries:

```ts
  "vibe.appearanceTitle": "Appearance",
  "vibe.appearanceSubtitle": "Theme and Vibe skin settings",
  "vibe.themeChoices": "Theme",
  "vibe.appearanceHelp": "Skin files can customize safe blocks and region styles only.",
  "vibe.clearSkin": "Clear custom skin",
```

Add these Chinese keys near the existing Chinese `vibe.switchTheme` entries:

```ts
  "vibe.appearanceTitle": "外观",
  "vibe.appearanceSubtitle": "主题与 Vibe 皮肤设置",
  "vibe.themeChoices": "主题",
  "vibe.appearanceHelp": "皮肤文件仅支持安全区块和区域样式。",
  "vibe.clearSkin": "清除自定义皮肤",
```

- [ ] **Step 6: Replace toolbar theme cycling and remove inline skin controls**

In `src/screens/VibeScreen.tsx`, keep the hidden file input but change the visible toolbar button:

```tsx
              <button
                aria-label={t("vibe.switchTheme")}
                className={
                  isSkin
                    ? "vibe-skin-ghost inline-flex items-center justify-center gap-2 rounded-xl border px-3 py-2 text-[12px] font-semibold transition"
                    : isDark
                    ? "inline-flex items-center justify-center gap-2 rounded-xl border border-[#586e75] bg-[#073642] px-3 py-2 text-[12px] font-semibold text-[#93a1a1] transition hover:border-[#839496] hover:text-[#fdf6e3]"
                    : "inline-flex items-center justify-center gap-2 rounded-xl border border-stone-200 bg-white/75 px-3 py-2 text-[12px] font-semibold text-stone-600 transition hover:border-stone-300 hover:bg-white hover:text-stone-950"
                }
                onClick={openAppearance}
                type="button"
              >
                {themeMode === "dark" ? (
                  <MoonStar className="h-4 w-4" />
                ) : themeMode === "light" ? (
                  <SunMedium className="h-4 w-4" />
                ) : (
                  <Palette className="h-4 w-4" />
                )}
                <span>{themeLabel}</span>
              </button>
```

Remove the always-visible skin select and import/clear buttons from this toolbar. Keep:

```tsx
              <input
                accept=".aiskin,.json,.zip,application/json,application/zip"
                aria-label="Choose Vibe skin package"
                className="hidden"
                onChange={(event) => void importSkin(event)}
                ref={skinFileInputRef}
                type="file"
              />
```

- [ ] **Step 7: Render the appearance dialog**

Before the create-session dialog block, add:

```tsx
      {appearanceOpen && (
        <div
          className="fixed inset-0 z-50 grid place-items-center bg-black/45 p-4"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) {
              setAppearanceOpen(false);
            }
          }}
        >
          <div
            aria-labelledby="vibe-appearance-title"
            aria-modal="true"
            className={
              isSkin
                ? "vibe-skin-modal vibe-skin-panel-strong w-full max-w-md rounded-3xl border p-4 shadow-2xl"
                : isDark
                ? "w-full max-w-md rounded-3xl border border-[#073642] bg-[#002b36] p-4 text-[#fdf6e3] shadow-2xl shadow-black/40"
                : "w-full max-w-md rounded-3xl border border-stone-200 bg-white p-4 text-stone-950 shadow-2xl shadow-stone-950/15"
            }
            role="dialog"
          >
            <div className="mb-4 flex items-start justify-between gap-3">
              <div>
                <h2 id="vibe-appearance-title" className="text-base font-semibold">
                  {t("vibe.appearanceTitle")}
                </h2>
                <p className={isSkin ? "mt-1 text-[12px] text-[var(--vibe-muted-text)]" : isDark ? "mt-1 text-[12px] text-[#93a1a1]" : "mt-1 text-[12px] text-stone-500"}>
                  {t("vibe.appearanceSubtitle")}
                </p>
              </div>
              <button
                aria-label={t("vibe.cancel")}
                className={
                  isSkin
                    ? "vibe-skin-ghost grid h-8 w-8 place-items-center rounded-xl border transition"
                    : isDark
                    ? "grid h-8 w-8 place-items-center rounded-xl border border-[#586e75] text-[#93a1a1] transition hover:text-[#fdf6e3]"
                    : "grid h-8 w-8 place-items-center rounded-xl border border-stone-200 text-stone-500 transition hover:text-stone-950"
                }
                onClick={() => setAppearanceOpen(false)}
                type="button"
              >
                <X className="h-4 w-4" />
              </button>
            </div>

            <div className="space-y-4">
              <div>
                <p className={isSkin ? "mb-2 text-[12px] font-semibold text-[var(--vibe-muted-text)]" : isDark ? "mb-2 text-[12px] font-semibold text-[#93a1a1]" : "mb-2 text-[12px] font-semibold text-stone-600"}>
                  {t("vibe.themeChoices")}
                </p>
                <div className="grid grid-cols-3 gap-2">
                  {[
                    { mode: "dark" as const, label: t("vibe.themeDark") },
                    { mode: "light" as const, label: t("vibe.themeLight") },
                    { mode: "skin" as const, label: t("vibe.themeSkin") },
                  ].map((choice) => (
                    <button
                      aria-pressed={themeMode === choice.mode}
                      className={
                        themeMode === choice.mode
                          ? isSkin
                            ? "vibe-skin-primary rounded-xl border px-3 py-2 text-[12px] font-semibold"
                            : "rounded-xl bg-stone-950 px-3 py-2 text-[12px] font-semibold text-white"
                          : isSkin
                            ? "vibe-skin-ghost rounded-xl border px-3 py-2 text-[12px] font-semibold"
                            : isDark
                              ? "rounded-xl border border-[#586e75] px-3 py-2 text-[12px] font-semibold text-[#93a1a1]"
                              : "rounded-xl border border-stone-200 px-3 py-2 text-[12px] font-semibold text-stone-600"
                      }
                      key={choice.mode}
                      onClick={() => setThemeMode(choice.mode)}
                      type="button"
                    >
                      {choice.label}
                    </button>
                  ))}
                </div>
              </div>

              {(isSkin || themeMode === "skin") && (
                <label className={isSkin ? "block text-[12px] font-semibold text-[var(--vibe-muted-text)]" : isDark ? "block text-[12px] font-semibold text-[#93a1a1]" : "block text-[12px] font-semibold text-stone-600"}>
                  {t("vibe.skinSelect")}
                  <select
                    aria-label={t("vibe.skinSelect")}
                    className={
                      isSkin
                        ? "vibe-skin-select mt-1 w-full rounded-xl border px-3 py-2 text-[13px] outline-none transition"
                        : isDark
                          ? "mt-1 w-full rounded-xl border border-[#586e75] bg-[#073642] px-3 py-2 text-[13px] text-[#fdf6e3] outline-none focus:border-[#268bd2]"
                          : "mt-1 w-full rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[13px] text-stone-950 outline-none focus:border-blue-400"
                    }
                    onChange={(event) => {
                      setActiveSkinId(event.target.value);
                      setThemeMode("skin");
                    }}
                    value={activeSkinId}
                  >
                    {availableSkins.map((skin) => (
                      <option key={skin.id} value={skin.id}>
                        {skin.name}
                      </option>
                    ))}
                  </select>
                </label>
              )}

              <div className="flex flex-wrap gap-2">
                <button
                  className={
                    isSkin
                      ? "vibe-skin-ghost inline-flex items-center gap-2 rounded-xl border px-3 py-2 text-[12px] font-semibold transition"
                      : isDark
                        ? "inline-flex items-center gap-2 rounded-xl border border-[#586e75] px-3 py-2 text-[12px] font-semibold text-[#93a1a1] transition hover:text-[#fdf6e3]"
                        : "inline-flex items-center gap-2 rounded-xl border border-stone-200 px-3 py-2 text-[12px] font-semibold text-stone-600 transition hover:text-stone-950"
                  }
                  onClick={triggerSkinImport}
                  type="button"
                >
                  <Upload className="h-4 w-4" />
                  <span>{t("vibe.importSkinShort")}</span>
                </button>
                {customSkin && (
                  <button
                    className={
                      isSkin
                        ? "vibe-skin-danger rounded-xl border px-3 py-2 text-[12px] font-semibold transition"
                        : isDark
                          ? "rounded-xl border border-red-400/60 px-3 py-2 text-[12px] font-semibold text-red-200 transition hover:bg-red-500/20"
                          : "rounded-xl border border-red-200 px-3 py-2 text-[12px] font-semibold text-red-700 transition hover:bg-red-50"
                    }
                    onClick={clearCustomSkin}
                    type="button"
                  >
                    {t("vibe.clearSkin")}
                  </button>
                )}
              </div>
              <p className={isSkin ? "text-[12px] leading-5 text-[var(--vibe-muted-text)]" : isDark ? "text-[12px] leading-5 text-[#93a1a1]" : "text-[12px] leading-5 text-stone-500"}>
                {t("vibe.appearanceHelp")}
              </p>
            </div>
          </div>
        </div>
      )}
```

- [ ] **Step 8: Run focused appearance tests**

Run:

```bash
pnpm vitest run tests/VibeScreen.test.tsx
```

Expected: pass.

- [ ] **Step 9: Commit appearance popup task**

Run:

```bash
git add src/screens/VibeScreen.tsx src/lib/i18n.tsx tests/VibeScreen.test.tsx
git commit -m "feat: add vibe appearance popup"
```

Expected: commit succeeds.

---

### Task 4: Taskbar Styling And Skin Documentation

**Files:**
- Modify: `src/styles.css`
- Modify: `docs/vibe-skins.md`
- Test: `tests/lib/vibeSkin.test.ts`, `tests/VibeScreen.test.tsx`

**Interfaces:**
- Consumes: new CSS variables emitted by `skinToCssVariables()`, taskbar DOM classes from `VibeScreen`.
- Produces: QQ2007/XP-style taskbar visual defaults and skin author documentation for the new taskbar fields.

- [ ] **Step 1: Add CSS for taskbar regions**

In `src/styles.css`, add this block after `.vibe-skin-status-bar`:

```css
.vibe-skin-taskbar {
  background: var(--vibe-taskbar-background-layer, linear-gradient(180deg, #48b9ff 0%, #1683da 48%, #0759a8 100%));
  background-position: var(--vibe-taskbar-background-position, center);
  background-repeat: var(--vibe-taskbar-background-repeat, no-repeat);
  background-size: var(--vibe-taskbar-background-size, cover);
  border-color: var(--vibe-taskbar-border, rgba(4, 72, 139, 0.72));
  box-shadow: var(--vibe-taskbar-shadow, inset 0 1px 0 rgba(255,255,255,0.72), 0 -10px 20px rgba(5,62,118,0.2));
  color: var(--vibe-taskbar-color, #ffffff);
}

.vibe-skin-taskbar-start-button {
  background: var(--vibe-taskbar-start-button-background-layer, linear-gradient(180deg, #9ef7a2 0%, #35b95d 45%, #08762f 100%));
  background-position: var(--vibe-taskbar-start-button-background-position, center);
  background-repeat: var(--vibe-taskbar-start-button-background-repeat, no-repeat);
  background-size: var(--vibe-taskbar-start-button-background-size, cover);
  border-color: var(--vibe-taskbar-start-button-border, rgba(255,255,255,0.72));
  border-radius: var(--vibe-taskbar-start-button-border-radius, 999px);
  box-shadow: var(--vibe-taskbar-start-button-shadow, inset 0 1px 0 rgba(255,255,255,0.82), 0 4px 10px rgba(0,68,37,0.35));
  color: var(--vibe-taskbar-start-button-color, #ffffff);
  padding: var(--vibe-taskbar-start-button-padding);
}

.vibe-skin-taskbar-start-menu {
  background: var(--vibe-taskbar-start-menu-background-layer, linear-gradient(180deg, rgba(255,255,255,0.98), rgba(207,238,255,0.96) 52%, rgba(127,199,244,0.94)));
  background-position: var(--vibe-taskbar-start-menu-background-position, center);
  background-repeat: var(--vibe-taskbar-start-menu-background-repeat, no-repeat);
  background-size: var(--vibe-taskbar-start-menu-background-size, cover);
  border-color: var(--vibe-taskbar-start-menu-border, rgba(17,100,181,0.45));
  box-shadow: var(--vibe-taskbar-start-menu-shadow, 0 18px 40px rgba(0,50,105,0.28), inset 0 1px 0 rgba(255,255,255,0.9));
  color: var(--vibe-taskbar-start-menu-color, var(--vibe-text));
}

.vibe-skin-taskbar-menu-item {
  background: var(--vibe-taskbar-menu-item-background-layer, transparent);
  color: var(--vibe-taskbar-menu-item-color, var(--vibe-text));
  font-size: var(--vibe-taskbar-menu-item-font-size);
  letter-spacing: var(--vibe-taskbar-menu-item-letter-spacing);
  padding: var(--vibe-taskbar-menu-item-padding);
}

.vibe-skin-taskbar-menu-item:hover:not(:disabled) {
  background: var(--vibe-button-hover-background-layer, rgba(37,135,216,0.18));
}

.vibe-skin-taskbar-item,
.vibe-skin-taskbar-item-active {
  background: var(--vibe-taskbar-item-background-layer, rgba(255,255,255,0.14));
  border-color: var(--vibe-taskbar-item-border, rgba(255,255,255,0.34));
  color: var(--vibe-taskbar-item-color, #ffffff);
  box-shadow: var(--vibe-taskbar-item-shadow, inset 0 1px 0 rgba(255,255,255,0.24));
  padding: var(--vibe-taskbar-item-padding);
}

.vibe-skin-taskbar-item-active {
  background: var(--vibe-taskbar-item-active-background-layer, linear-gradient(180deg, rgba(255,255,255,0.42), rgba(41,139,222,0.42)));
  border-color: var(--vibe-taskbar-item-active-border, rgba(255,255,255,0.56));
  color: var(--vibe-taskbar-item-active-color, #ffffff);
  box-shadow: var(--vibe-taskbar-item-active-shadow, inset 0 1px 0 rgba(255,255,255,0.5), 0 1px 4px rgba(0,45,96,0.22));
}

.vibe-skin-taskbar-tray {
  background: var(--vibe-taskbar-tray-background-layer, rgba(4,80,151,0.36));
  border-color: var(--vibe-taskbar-tray-border, rgba(255,255,255,0.28));
  color: var(--vibe-taskbar-tray-color, #eef9ff);
  box-shadow: var(--vibe-taskbar-tray-shadow, inset 0 1px 0 rgba(255,255,255,0.22));
  padding: var(--vibe-taskbar-tray-padding);
}

.vibe-skin-taskbar-clock {
  background: var(--vibe-taskbar-clock-background-layer, rgba(255,255,255,0.16));
  border-color: var(--vibe-taskbar-clock-border, transparent);
  color: var(--vibe-taskbar-clock-color, #ffffff);
  font-size: var(--vibe-taskbar-clock-font-size);
  letter-spacing: var(--vibe-taskbar-clock-letter-spacing);
  padding: var(--vibe-taskbar-clock-padding);
}
```

- [ ] **Step 2: Document taskbar manifest fields**

In `docs/vibe-skins.md`, update the zip asset sentence to include:

```markdown
`blocks.taskbar.startButton.icon`, or `blocks.taskbar.items[].icon`
```

In the minimal manifest, add this `taskbar` block after `statusbar`:

```json
    "taskbar": {
      "enabled": true,
      "startButton": {
        "label": "开始",
        "icon": "assets/start.png"
      },
      "startMenu": {
        "items": [
          { "label": "外观设置", "action": "openAppearance" },
          { "label": "切换到皮肤模式", "action": "setTheme", "theme": "skin" },
          { "label": "切换暗色主题", "action": "setTheme", "theme": "dark" },
          { "label": "导入皮肤...", "action": "importSkin" },
          { "type": "separator" },
          { "label": "AI Switch 终端", "disabled": true }
        ]
      },
      "items": [
        { "label": "AI Switch 终端", "icon": "assets/app.png", "active": true }
      ],
      "tray": ["Vibe", "在线"],
      "clockFormat": "HH:mm"
    }
```

Add these bullets under Supported blocks:

```markdown
- `blocks.taskbar.enabled`: enables the taskbar. Set to `false` to keep the older status bar.
- `blocks.taskbar.startButton.label`: start button text, for example `开始`.
- `blocks.taskbar.startButton.icon`: start button image path or data URL.
- `blocks.taskbar.startMenu.items`: safe start-menu items. Supported actions are `openAppearance`, `setTheme`, `importSkin`, and `clearSkin`.
- `blocks.taskbar.items`: decorative taskbar application items with `label`, optional `icon`, and optional `active`.
- `blocks.taskbar.tray`: short tray labels.
- `blocks.taskbar.clockFormat`: currently supports `HH:mm`.
```

Add these region keys to the Region Keys list:

```markdown
- `taskbar`
- `taskbarStartButton`
- `taskbarStartMenu`
- `taskbarMenuItem`
- `taskbarItem`
- `taskbarItemActive`
- `taskbarTray`
- `taskbarClock`
```

Add this paragraph after the decorative titlebar note:

```markdown
The taskbar start menu supports only a fixed allowlist of app actions. Unknown actions, malformed `setTheme` values, disabled items, and separators do nothing. Skins cannot provide callbacks or native window commands.
```

- [ ] **Step 3: Run focused and full validation**

Run:

```bash
pnpm vitest run tests/lib/vibeSkin.test.ts tests/VibeScreen.test.tsx tests/terminal/XtermPane.test.tsx
pnpm typecheck
```

Expected: both commands pass.

- [ ] **Step 4: Commit styling and docs**

Run:

```bash
git add src/styles.css docs/vibe-skins.md
git commit -m "docs: document vibe taskbar skins"
```

Expected: commit succeeds after validation.

---

## Self-Review

- Spec coverage: Task 1 covers the manifest model, defaults, icon asset resolution, safe action normalization, tray values, clock format, and region CSS variables. Task 2 covers skin-mode taskbar rendering, statusbar replacement/fallback, start menu rendering, safe action dispatch, and compact directory labels. Task 3 covers replacing direct theme cycling with a unified appearance dialog and moving select/import/clear controls into that dialog. Task 4 covers visual QQ2007/XP-style taskbar styling and documentation for skin authors.
- Placeholder scan: The plan contains concrete file paths, code snippets, commands, expected test results, and commit commands. It does not rely on undefined future behavior.
- Type consistency: `VibeSkinTaskbarMenuItem`, `VibeSkinTaskbarBlock`, `ResolvedVibeSkinTaskbarBlock`, `ResolvedVibeSkinBlocks.taskbar`, `compactDirectoryLabel`, `taskbarEnabled`, `appearanceOpen`, and `startMenuOpen` are introduced before later tasks consume them.
