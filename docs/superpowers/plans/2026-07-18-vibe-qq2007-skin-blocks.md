# Vibe QQ2007 Skin Blocks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a more faithful QQ2007-inspired Vibe skin system with Chinese default skin copy, visual-only titlebar controls, profile/avatar blocks, a QQ秀-style right showcase, and transparent terminal surfaces in skin mode.

**Architecture:** Keep the current safe app-controlled layout and extend the skin manifest with a typed `blocks` object for content plus new `regions` keys for styling. `src/lib/vibeSkin.ts` owns normalization, zip asset resolution, storage compatibility, default block fallbacks, and CSS variables; `src/screens/VibeScreen.tsx` consumes resolved blocks to render skin-only decorative areas; `src/components/terminal/XtermPane.tsx` exposes a transparent-surface mode used only by skin mode.

**Tech Stack:** React 18, TypeScript, Vite, Vitest, Testing Library, JSZip, xterm.js, UnoCSS/Tailwind-style utility classes, plain CSS variables.

## Global Constraints

- Work directly on `main`; do not create or switch to feature branches/worktrees.
- Only change `themeMode === "skin"` behavior; dark and light Vibe modes keep their existing layout and behavior.
- Window minimize, maximize, and close buttons are visual-only and must not call Tauri window APIs.
- User-created skins can provide strings and image references only; arbitrary HTML, CSS files, and scripts remain unsupported.
- Existing manifests using `showcase` remain valid; `showcase` maps to the right rail only when `blocks.showcase` is absent.
- Relative `blocks.profile.avatar` and `blocks.showcase.figure` paths resolve from zip/aiskin packages using the existing asset resolver.
- Imported package size remains 8 MB before extraction; stored skin size remains 4,500,000 serialized characters.
- New terminal sessions in skin mode must not paint an opaque xterm or wrapper background over `.vibe-skin-terminal-shell`.

---

## File Structure

- Modify `src/lib/vibeSkin.ts`: add new region keys, typed block schema, default QQ2007 block copy, manifest/storage normalization, zip asset resolution, `getVibeSkinBlocks()`, and CSS variable generation for the added region keys.
- Modify `tests/lib/vibeSkin.test.ts`: cover block normalization, stored block fallback, zip asset resolution for avatar/figure, and CSS variables for the new region keys.
- Modify `src/screens/VibeScreen.tsx`: render Chinese built-in titlebar/profile/showcase/status blocks in skin mode, add decorative window controls, pass `transparentSurface` to `XtermPane`, and keep legacy showcase fallback.
- Modify `tests/VibeScreen.test.tsx`: cover built-in QQ2007 skin visuals, custom blocks, legacy showcase compatibility, transparent terminal prop, and dark/light absence of QQ2007-only blocks.
- Modify `src/components/terminal/XtermPane.tsx`: add `transparentSurface?: boolean`, mark the host wrapper, and force xterm theme background to `transparent` only when that prop is true.
- Modify `tests/terminal/XtermPane.test.tsx`: cover transparent host class and transparent constructor theme while preserving event subscription behavior.
- Modify `src/styles.css`: add CSS consumers for the new regions and stronger transparent xterm layer rules.
- Modify `docs/vibe-skins.md`: document `blocks`, new region keys, visual-only window controls, legacy `showcase`, and skin-mode terminal transparency.

---

### Task 1: Skin Manifest Blocks And Region Model

**Files:**
- Modify: `src/lib/vibeSkin.ts`
- Test: `tests/lib/vibeSkin.test.ts`

**Interfaces:**
- Consumes: existing `VibeSkinDefinition`, `VIBE_SKIN_REGION_KEYS`, `importVibeSkinPackage()`, `readStoredVibeSkin()`, `writeStoredVibeSkin()`, `skinToCssVariables()`.
- Produces: `VibeSkinBlocks`, `ResolvedVibeSkinBlocks`, `DEFAULT_VIBE_SKIN_BLOCKS`, `getVibeSkinBlocks(skin: VibeSkinDefinition): ResolvedVibeSkinBlocks`.

- [ ] **Step 1: Write failing tests for blocks, asset resolution, and new region variables**

Replace the first test manifest in `tests/lib/vibeSkin.test.ts` with this manifest content and add the matching assertions:

```ts
zip.file(
  "skin.json",
  JSON.stringify({
    id: "retro-blue",
    name: "Retro Blue",
    ui: {
      accent: "#1278d8",
      backgroundImage: "assets/background.png",
    },
    terminal: {
      background: "#f4fbff",
      foreground: "#12375f",
    },
    regions: {
      terminalShell: {
        backgroundImage: "assets/shell.png",
        backgroundPosition: "center top",
        borderRadius: "18px",
      },
      sidebarProfile: {
        background: "linear-gradient(#ffffff, #bce7ff)",
        border: "rgba(21, 104, 184, 0.42)",
      },
      showcaseFigure: {
        shadow: "0 18px 30px rgba(10,82,154,0.24)",
      },
      windowButtonClose: {
        background: "linear-gradient(#ff9aa2, #b51f2e)",
      },
    },
    showcase: {
      enabled: true,
      image: "assets/showcase.png",
    },
    blocks: {
      titlebar: {
        title: "自定义终端",
        subtitle: "复古蓝色皮肤",
        badge: "正在运行",
      },
      profile: {
        name: "测试用户",
        status: "在线",
        signature: "正在测试皮肤包",
        badge: "蓝钻",
        avatar: "assets/avatar.png",
      },
      showcase: {
        title: "QQ秀展示",
        subtitle: "Retro Blue",
        body: "右侧展示区来自 blocks.showcase。",
        figure: "assets/figure.png",
        footer: "自定义展示区",
      },
      statusbar: {
        left: "已连接",
        right: "皮肤区域已启用",
      },
    },
  }),
);
zip.file("assets/background.png", new Uint8Array([137, 80, 78, 71]));
zip.file("assets/shell.png", new Uint8Array([137, 80, 78, 71]));
zip.file("assets/showcase.png", new Uint8Array([137, 80, 78, 71]));
zip.file("assets/avatar.png", new Uint8Array([137, 80, 78, 71]));
zip.file("assets/figure.png", new Uint8Array([137, 80, 78, 71]));
```

Add these assertions after the existing `showcase.image` assertion:

```ts
expect(skin.blocks?.titlebar?.title).toBe("自定义终端");
expect(skin.blocks?.profile?.name).toBe("测试用户");
expect(skin.blocks?.profile?.avatar).toMatch(/^data:image\/png;base64,/);
expect(skin.blocks?.showcase?.figure).toMatch(/^data:image\/png;base64,/);
expect(skin.blocks?.statusbar?.left).toBe("已连接");

const variables = skinToCssVariables(skin) as Record<string, unknown>;
expect(variables["--vibe-sidebar-profile-background-layer"]).toBe(
  "linear-gradient(#ffffff, #bce7ff)",
);
expect(variables["--vibe-showcase-figure-shadow"]).toBe(
  "0 18px 30px rgba(10,82,154,0.24)",
);
expect(variables["--vibe-window-button-close-background-layer"]).toBe(
  "linear-gradient(#ff9aa2, #b51f2e)",
);
```

Add `getVibeSkinBlocks` to the import list and append this test:

```ts
it("normalizes stored blocks and maps legacy showcase when blocks showcase is absent", () => {
  writeStoredVibeSkin({
    id: "stored-block-skin",
    name: "Stored Block Skin",
    ui: {
      accent: "#1678d8",
      accentText: "#ffffff",
      background: "#0f6bc4",
      backgroundOverlay: "linear-gradient(rgba(255,255,255,0.3), transparent)",
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
      titlebar: {
        title: "存储终端",
      },
      profile: {
        name: "存储用户",
        status: "在线",
      },
      statusbar: {
        right: "右侧状态",
      },
    },
    showcase: {
      enabled: true,
      title: "旧展示",
      image: "data:image/png;base64,AAAA",
      footer: "旧页脚",
    },
  });

  const skin = readStoredVibeSkin();
  const blocks = skin ? getVibeSkinBlocks(skin) : null;

  expect(skin?.blocks?.titlebar?.title).toBe("存储终端");
  expect(blocks?.titlebar.title).toBe("存储终端");
  expect(blocks?.titlebar.subtitle).toBe("QQ2007 蓝色经典");
  expect(blocks?.profile.name).toBe("存储用户");
  expect(blocks?.showcase.title).toBe("旧展示");
  expect(blocks?.showcase.figure).toBe("data:image/png;base64,AAAA");
  expect(blocks?.showcase.footer).toBe("旧页脚");
  expect(blocks?.statusbar.right).toBe("右侧状态");
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```powershell
pnpm vitest run tests/lib/vibeSkin.test.ts
```

Expected: FAIL with TypeScript or assertion errors because `blocks`, new region keys, and `getVibeSkinBlocks` do not exist yet.

- [ ] **Step 3: Extend region keys, block types, and defaults**

In `src/lib/vibeSkin.ts`, replace `VIBE_SKIN_REGION_KEYS` with:

```ts
export const VIBE_SKIN_REGION_KEYS = [
  "app",
  "body",
  "titlebar",
  "titlebarControls",
  "windowButton",
  "windowButtonMinimize",
  "windowButtonMaximize",
  "windowButtonClose",
  "toolbar",
  "sidebar",
  "sidebarHeader",
  "sidebarProfile",
  "avatar",
  "onlineBadge",
  "profileBadge",
  "controlPanel",
  "sessionList",
  "listTrigger",
  "sessionRow",
  "groupPanel",
  "workspace",
  "tabBar",
  "tab",
  "tabActive",
  "tabClose",
  "terminalShell",
  "emptyState",
  "modal",
  "rightRail",
  "rightCard",
  "showcaseStage",
  "showcaseFigure",
  "showcaseFooter",
  "statusBar",
  "button",
  "buttonHover",
  "ghostButton",
  "field",
  "select",
  "danger",
  "showcaseOrb",
] as const;
```

Add these types after `VibeSkinShowcase`:

```ts
export type VibeSkinTitlebarBlock = {
  title?: string;
  subtitle?: string;
  badge?: string;
};

export type VibeSkinProfileBlock = {
  name?: string;
  status?: string;
  signature?: string;
  badge?: string;
  avatar?: string;
};

export type VibeSkinShowcaseBlock = {
  enabled?: boolean;
  title?: string;
  subtitle?: string;
  body?: string;
  badge?: string;
  figure?: string;
  footer?: string;
};

export type VibeSkinStatusbarBlock = {
  left?: string;
  right?: string;
};

export type VibeSkinBlocks = {
  titlebar?: VibeSkinTitlebarBlock;
  profile?: VibeSkinProfileBlock;
  showcase?: VibeSkinShowcaseBlock;
  statusbar?: VibeSkinStatusbarBlock;
};

export type ResolvedVibeSkinBlocks = {
  titlebar: Required<VibeSkinTitlebarBlock>;
  profile: Omit<Required<VibeSkinProfileBlock>, "avatar"> & Pick<VibeSkinProfileBlock, "avatar">;
  showcase: Omit<Required<VibeSkinShowcaseBlock>, "figure"> &
    Pick<VibeSkinShowcaseBlock, "figure">;
  statusbar: Required<VibeSkinStatusbarBlock>;
};

export const DEFAULT_VIBE_SKIN_BLOCKS: ResolvedVibeSkinBlocks = {
  titlebar: {
    title: "AI Switch 终端",
    subtitle: "QQ2007 蓝色经典",
    badge: "皮肤模式",
  },
  profile: {
    name: "AI Switch",
    status: "在线",
    signature: "正在使用 Vibe 终端",
    badge: "经典蓝钻",
  },
  showcase: {
    enabled: true,
    title: "QQ秀展示",
    subtitle: "Codex 2007 Blue",
    body: "右侧展示区可由皮肤定义图片、舞台和说明。",
    badge: "我的QQ秀",
    footer: "自定义展示区",
  },
  statusbar: {
    left: "AI Switch 已连接",
    right: "皮肤区域已启用",
  },
};
```

Add `blocks?: VibeSkinBlocks;` to `VibeSkinDefinition`.

- [ ] **Step 4: Add block normalization, legacy mapping, and asset resolution**

Add these helpers after `normalizeShowcase()`:

```ts
function normalizeTitlebarBlock(value: unknown): VibeSkinTitlebarBlock | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const source = value as Record<string, unknown>;
  const block: VibeSkinTitlebarBlock = {};
  for (const key of ["title", "subtitle", "badge"] as const) {
    const item = optionalString(source[key]);
    if (item) {
      block[key] = item;
    }
  }
  return Object.keys(block).length > 0 ? block : undefined;
}

function normalizeProfileBlock(value: unknown): VibeSkinProfileBlock | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const source = value as Record<string, unknown>;
  const block: VibeSkinProfileBlock = {};
  for (const key of ["name", "status", "signature", "badge", "avatar"] as const) {
    const item = optionalString(source[key]);
    if (item) {
      block[key] = item;
    }
  }
  return Object.keys(block).length > 0 ? block : undefined;
}

function normalizeShowcaseBlock(value: unknown): VibeSkinShowcaseBlock | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const source = value as Record<string, unknown>;
  const block: VibeSkinShowcaseBlock = {};
  if (typeof source.enabled === "boolean") {
    block.enabled = source.enabled;
  }
  for (const key of ["title", "subtitle", "body", "badge", "figure", "footer"] as const) {
    const item = optionalString(source[key]);
    if (item) {
      block[key] = item;
    }
  }
  return Object.keys(block).length > 0 ? block : undefined;
}

function normalizeStatusbarBlock(value: unknown): VibeSkinStatusbarBlock | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const source = value as Record<string, unknown>;
  const block: VibeSkinStatusbarBlock = {};
  for (const key of ["left", "right"] as const) {
    const item = optionalString(source[key]);
    if (item) {
      block[key] = item;
    }
  }
  return Object.keys(block).length > 0 ? block : undefined;
}

function normalizeBlocks(value: unknown): VibeSkinBlocks | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }

  const source = value as Record<string, unknown>;
  const blocks: VibeSkinBlocks = {
    titlebar: normalizeTitlebarBlock(source.titlebar),
    profile: normalizeProfileBlock(source.profile),
    showcase: normalizeShowcaseBlock(source.showcase),
    statusbar: normalizeStatusbarBlock(source.statusbar),
  };
  return Object.values(blocks).some(Boolean) ? blocks : undefined;
}
```

Add this helper after `resolveRegionAssets()`:

```ts
async function resolveBlockAssets(blocks: VibeSkinBlocks | undefined, resolveAsset?: AssetResolver) {
  if (!blocks) {
    return;
  }

  if (blocks.profile?.avatar) {
    blocks.profile.avatar = await resolveImageReference(
      blocks.profile.avatar,
      "blocks.profile.avatar",
      resolveAsset,
    );
  }

  if (blocks.showcase?.figure) {
    blocks.showcase.figure = await resolveImageReference(
      blocks.showcase.figure,
      "blocks.showcase.figure",
      resolveAsset,
    );
  }
}
```

In `normalizeSkinManifest()`, add block normalization and asset resolution:

```ts
const regions = normalizeRegions(raw.regions);
const showcase = normalizeShowcase(raw.showcase);
const blocks = normalizeBlocks(raw.blocks);

ui.backgroundImage = await resolveImageReference(
  ui.backgroundImage,
  "ui.backgroundImage",
  resolveAsset,
);
await resolveRegionAssets(regions, resolveAsset);
await resolveBlockAssets(blocks, resolveAsset);
if (showcase?.image) {
  showcase.image = await resolveImageReference(showcase.image, "showcase.image", resolveAsset);
}
```

Return `blocks` from `normalizeSkinManifest()` and `normalizeStoredSkin()`:

```ts
blocks,
```

Use this exact field in `normalizeStoredSkin()`:

```ts
blocks: normalizeBlocks(raw.blocks),
```

Add this exported resolver before `readStoredVibeSkin()`:

```ts
function legacyShowcaseToBlock(showcase: VibeSkinShowcase | undefined): VibeSkinShowcaseBlock | undefined {
  if (!showcase) {
    return undefined;
  }
  return {
    enabled: showcase.enabled,
    title: showcase.title,
    subtitle: showcase.subtitle,
    body: showcase.body,
    badge: showcase.badge,
    figure: showcase.image,
    footer: showcase.footer,
  };
}

export function getVibeSkinBlocks(skin: VibeSkinDefinition): ResolvedVibeSkinBlocks {
  const showcaseSource = skin.blocks?.showcase ?? legacyShowcaseToBlock(skin.showcase);
  return {
    titlebar: {
      ...DEFAULT_VIBE_SKIN_BLOCKS.titlebar,
      ...skin.blocks?.titlebar,
    },
    profile: {
      ...DEFAULT_VIBE_SKIN_BLOCKS.profile,
      ...skin.blocks?.profile,
    },
    showcase: {
      ...DEFAULT_VIBE_SKIN_BLOCKS.showcase,
      ...showcaseSource,
      enabled: showcaseSource?.enabled ?? DEFAULT_VIBE_SKIN_BLOCKS.showcase.enabled,
    },
    statusbar: {
      ...DEFAULT_VIBE_SKIN_BLOCKS.statusbar,
      ...skin.blocks?.statusbar,
    },
  };
}
```

- [ ] **Step 5: Update the built-in QQ2007 skin definition**

In `BUILT_IN_VIBE_SKINS[0]`, set `terminal.background` to `"transparent"` and add these built-in block values after `regions`:

```ts
blocks: {
  titlebar: {
    title: "AI Switch 终端",
    subtitle: "QQ2007 蓝色经典",
    badge: "皮肤模式",
  },
  profile: {
    name: "AI Switch",
    status: "在线",
    signature: "正在使用 Vibe 终端",
    badge: "经典蓝钻",
  },
  showcase: {
    enabled: true,
    title: "QQ秀展示",
    subtitle: "Codex 2007 Blue",
    body: "右侧展示区可由皮肤定义图片、舞台和说明。",
    badge: "我的QQ秀",
    footer: "自定义展示区",
  },
  statusbar: {
    left: "AI Switch 已连接",
    right: "皮肤区域已启用",
  },
},
```

Keep the existing `showcase` object for compatibility during this task; `getVibeSkinBlocks()` ignores it when `blocks.showcase` exists.

- [ ] **Step 6: Run focused tests**

Run:

```powershell
pnpm vitest run tests/lib/vibeSkin.test.ts
```

Expected: PASS.

- [ ] **Step 7: Commit skin model changes**

Run:

```powershell
git add src/lib/vibeSkin.ts tests/lib/vibeSkin.test.ts
git commit -m "feat: add vibe skin content blocks"
```

Expected: commit succeeds with only the schema and model tests staged.

---

### Task 2: QQ2007 Skin Layout Blocks In Vibe Screen

**Files:**
- Modify: `src/screens/VibeScreen.tsx`
- Test: `tests/VibeScreen.test.tsx`

**Interfaces:**
- Consumes: `getVibeSkinBlocks(skin: VibeSkinDefinition): ResolvedVibeSkinBlocks` from Task 1.
- Produces: skin-only titlebar controls, profile card, showcase stage, Chinese statusbar copy, and `transparentSurface={isSkin}` passed to `XtermPane`.

- [ ] **Step 1: Update the VibeScreen xterm mock and add skin-switch helper**

In `tests/VibeScreen.test.tsx`, replace the `XtermPane` mock with:

```tsx
vi.mock("../src/components/terminal/XtermPane", () => ({
  XtermPane: ({
    session,
    themeOverride,
    transparentSurface,
  }: {
    session: TerminalSession;
    themeOverride?: unknown;
    transparentSurface?: boolean;
  }) => (
    <div
      data-testid={`terminal-pane-${session.id}`}
      data-theme-override={themeOverride ? "yes" : "no"}
      data-transparent-surface={transparentSurface ? "yes" : "no"}
    >
      {session.title}
    </div>
  ),
}));
```

Add this helper after `expandProjectDirectory()`:

```ts
async function switchToSkinTheme() {
  const themeButton = await screen.findByRole("button", { name: "Switch Vibe theme" });
  await userEvent.click(themeButton);
  await userEvent.click(themeButton);
  return themeButton;
}
```

- [ ] **Step 2: Write failing tests for built-in Chinese QQ2007 skin visuals**

Append this test after `"cycles Vibe through dark, light, and skin themes"`:

```tsx
it("renders built-in QQ2007 skin blocks with Chinese decorative UI", async () => {
  renderScreen();

  await switchToSkinTheme();

  expect(screen.getByText("AI Switch 终端")).toBeInTheDocument();
  expect(screen.getByText("QQ2007 蓝色经典")).toBeInTheDocument();
  expect(screen.getAllByText("皮肤模式").length).toBeGreaterThan(0);
  expect(screen.getByText("在线")).toBeInTheDocument();
  expect(screen.getByText("正在使用 Vibe 终端")).toBeInTheDocument();
  expect(screen.getByText("经典蓝钻")).toBeInTheDocument();
  expect(screen.getByText("QQ秀展示")).toBeInTheDocument();
  expect(screen.getByText("我的QQ秀")).toBeInTheDocument();
  expect(screen.getByText("自定义展示区")).toBeInTheDocument();
  expect(screen.getByText("AI Switch 已连接")).toBeInTheDocument();
  expect(screen.getByText("皮肤区域已启用")).toBeInTheDocument();

  const controls = screen.getByTestId("vibe-window-controls");
  expect(controls).toHaveAttribute("aria-hidden", "true");
  expect(controls).toHaveTextContent("—");
  expect(controls).toHaveTextContent("□");
  expect(controls).toHaveTextContent("×");
  expect(screen.queryByRole("button", { name: /minimize|maximize|close window/i })).not.toBeInTheDocument();
});
```

Add this test after it:

```tsx
it("does not render QQ2007 decorative skin blocks in dark or light themes", async () => {
  renderScreen();

  expect(await screen.findByText("D:/repo/app")).toBeInTheDocument();
  expect(screen.queryByText("QQ秀展示")).not.toBeInTheDocument();
  expect(screen.queryByTestId("vibe-window-controls")).not.toBeInTheDocument();

  await userEvent.click(screen.getByRole("button", { name: "Switch Vibe theme" }));

  expect(screen.queryByText("QQ秀展示")).not.toBeInTheDocument();
  expect(screen.queryByTestId("vibe-window-controls")).not.toBeInTheDocument();
});
```

- [ ] **Step 3: Write failing tests for custom blocks, legacy showcase, and transparent prop**

Replace the `"renders custom skin showcase regions"` stored manifest with:

```ts
{
  id: "showcase-skin",
  name: "Showcase Skin",
  ui: {
    accent: "#00ffee",
    background: "#001018",
    panel: "rgba(2,28,40,0.78)",
    panelStrong: "rgba(4,42,58,0.92)",
    panelSubtle: "rgba(0,255,238,0.12)",
    border: "rgba(0,255,238,0.35)",
    text: "#f4fbff",
    mutedText: "#9be7ff",
    button: "#00ffee",
    buttonText: "#001018",
    buttonHover: "#54fff5",
  },
  regions: {
    rightRail: { background: "#123456" },
    terminalShell: { background: "#010203" },
    showcaseStage: { background: "#102030" },
  },
  blocks: {
    titlebar: {
      title: "霓虹终端",
      subtitle: "自定义标题栏",
      badge: "自定义皮肤",
    },
    profile: {
      name: "霓虹用户",
      status: "忙碌",
      signature: "正在调试右侧展示区",
      badge: "VIP",
      avatar: "data:image/png;base64,AAAA",
    },
    showcase: {
      title: "右侧QQ秀",
      subtitle: "Neon Figure",
      body: "blocks.showcase 控制展示内容。",
      badge: "Custom Rail",
      figure: "data:image/png;base64,BBBB",
      footer: "region keys",
    },
    statusbar: {
      left: "霓虹已连接",
      right: "状态栏右侧",
    },
  },
}
```

Update the assertions in that test to:

```tsx
expect(screen.getByLabelText("Vibe skin")).toHaveValue("showcase-skin");
expect(screen.getByText("霓虹终端")).toBeInTheDocument();
expect(screen.getByText("霓虹用户")).toBeInTheDocument();
expect(screen.getByText("忙碌")).toBeInTheDocument();
expect(screen.getByText("右侧QQ秀")).toBeInTheDocument();
expect(screen.getByText("Custom Rail")).toBeInTheDocument();
expect(screen.getByText("terminalShell")).toBeInTheDocument();
expect(screen.getByText("rightRail")).toBeInTheDocument();
expect(screen.getByText("showcaseStage")).toBeInTheDocument();
expect(screen.getByText("region keys")).toBeInTheDocument();
expect(screen.getByRole("img", { name: "霓虹用户 avatar" })).toHaveAttribute(
  "src",
  "data:image/png;base64,AAAA",
);
expect(screen.getByRole("img", { name: "右侧QQ秀 figure" })).toHaveAttribute(
  "src",
  "data:image/png;base64,BBBB",
);
```

Append this test after the custom block test:

```tsx
it("renders legacy showcase content when blocks.showcase is absent", async () => {
  window.localStorage.setItem(
    VIBE_SKIN_STORAGE_KEY,
    JSON.stringify({
      id: "legacy-showcase-skin",
      name: "Legacy Showcase Skin",
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
      showcase: {
        enabled: true,
        title: "旧版展示标题",
        badge: "旧版徽标",
        image: "data:image/png;base64,CCCC",
        footer: "旧版页脚",
      },
    }),
  );
  renderScreen();

  await switchToSkinTheme();

  expect(screen.getByText("旧版展示标题")).toBeInTheDocument();
  expect(screen.getByText("旧版徽标")).toBeInTheDocument();
  expect(screen.getByRole("img", { name: "旧版展示标题 figure" })).toHaveAttribute(
    "src",
    "data:image/png;base64,CCCC",
  );
  expect(screen.getByText("旧版页脚")).toBeInTheDocument();
});
```

In `"imports a custom Vibe skin package and applies its terminal theme"`, append:

```tsx
expect(await screen.findByTestId("terminal-pane-term-1")).toHaveAttribute(
  "data-transparent-surface",
  "yes",
);
```

- [ ] **Step 4: Run VibeScreen tests to verify they fail**

Run:

```powershell
pnpm vitest run tests/VibeScreen.test.tsx
```

Expected: FAIL because `getVibeSkinBlocks()` is not consumed by the screen, QQ2007 blocks are not rendered, and `transparentSurface` is not passed.

- [ ] **Step 5: Render resolved skin blocks in `VibeScreen.tsx`**

Add `getVibeSkinBlocks` to the `../lib/vibeSkin` import list:

```ts
  getVibeSkinBlocks,
```

Replace the current skin showcase derivation at lines 343-347 with:

```ts
const skinBlocks = useMemo(() => getVibeSkinBlocks(activeSkin), [activeSkin]);
const showSkinShowcase = Boolean(isSkin && skinBlocks.showcase.enabled);
const skinBodyGridClass = showSkinShowcase
  ? "vibe-skin-body grid min-h-0 flex-1 grid-cols-1 lg:grid-cols-[300px_minmax(0,1fr)_260px]"
  : "vibe-skin-body grid min-h-0 flex-1 grid-cols-1 lg:grid-cols-[300px_minmax(0,1fr)]";
```

Replace the skin titlebar block at lines 364-378 with:

```tsx
{isSkin && (
  <div className="vibe-skin-titlebar flex h-11 shrink-0 items-center justify-between gap-3 border-b px-3 text-[11px] font-semibold">
    <div className="flex min-w-0 items-center gap-3">
      <span className="grid h-5 w-5 shrink-0 place-items-center rounded-full border border-[rgba(255,255,255,0.65)] bg-[var(--vibe-accent)] text-[10px] text-white shadow-[inset_0_1px_0_rgba(255,255,255,0.72)]">
        Q
      </span>
      <div className="min-w-0">
        <p className="truncate text-[13px] tracking-normal">{skinBlocks.titlebar.title}</p>
        <p className="truncate text-[10px] tracking-[0.12em] opacity-85">
          {skinBlocks.titlebar.subtitle}
        </p>
      </div>
    </div>
    <div className="flex items-center gap-2">
      <span className="rounded-full border border-[rgba(255,255,255,0.48)] px-2 py-1 text-[10px] tracking-[0.12em]">
        {skinBlocks.titlebar.badge}
      </span>
      <div
        aria-hidden="true"
        className="vibe-skin-titlebar-controls flex items-center gap-1"
        data-testid="vibe-window-controls"
      >
        <span className="vibe-skin-window-button vibe-skin-window-button-minimize">—</span>
        <span className="vibe-skin-window-button vibe-skin-window-button-maximize">□</span>
        <span className="vibe-skin-window-button vibe-skin-window-button-close">×</span>
      </div>
    </div>
  </div>
)}
```

Replace the skin branch inside the sidebar header at lines 407-432 with this skin-only content while leaving the existing dark/light content intact:

```tsx
{isSkin ? (
  <>
    <div className="vibe-skin-profile flex min-w-0 flex-1 items-center gap-3">
      <div className="vibe-skin-avatar relative grid h-14 w-14 shrink-0 place-items-center overflow-hidden rounded-2xl border">
        {skinBlocks.profile.avatar ? (
          <img
            alt={`${skinBlocks.profile.name} avatar`}
            className="h-full w-full object-cover"
            src={skinBlocks.profile.avatar}
          />
        ) : (
          <AiSwitchLogo className="h-9 w-9 rounded-xl" />
        )}
        <span className="vibe-skin-online-badge absolute bottom-1 right-1 h-3.5 w-3.5 rounded-full border-2" />
      </div>
      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-2">
          <h1 className="truncate text-[14px] font-semibold text-[var(--vibe-text)]">
            {skinBlocks.profile.name}
          </h1>
          <span className="vibe-skin-profile-badge rounded-full border px-2 py-0.5 text-[10px]">
            {skinBlocks.profile.badge}
          </span>
        </div>
        <p className="mt-0.5 truncate text-[11px] text-[var(--vibe-muted-text)]">
          {skinBlocks.profile.status}
        </p>
        <p className="mt-1 truncate text-[11px] text-[var(--vibe-text)] opacity-80">
          {skinBlocks.profile.signature}
        </p>
      </div>
    </div>
    <button
      aria-label={t("layout.switchToAgent")}
      className="vibe-skin-ghost grid h-8 w-8 shrink-0 place-items-center rounded-xl border shadow-sm transition-colors focus:outline-none focus-visible:ring-2"
      onClick={onExitVibe}
      type="button"
    >
      <PanelLeftClose className="h-4 w-4" />
    </button>
  </>
) : (
  <>
    <div className="flex min-w-0 items-center gap-2">
      <AiSwitchLogo className="h-9 w-9 shrink-0 rounded-2xl shadow-sm" />
      <div className="min-w-0">
        <h1 className={isDark ? "truncate text-[13px] font-semibold text-[#fdf6e3]" : "truncate text-[13px] font-semibold text-stone-950"}>
          {t("vibe.title")} · {t("vibe.kicker")}
        </h1>
        <p className={isDark ? "truncate text-[11px] text-[#93a1a1]" : "truncate text-[11px] text-stone-500"}>
          {t("vibe.subtitle")}
        </p>
      </div>
    </div>
    <button
      aria-label={t("layout.switchToAgent")}
      className={
        isDark
          ? "grid h-8 w-8 shrink-0 place-items-center rounded-xl border border-[#586e75] bg-[#073642] text-[#93a1a1] shadow-sm transition-colors hover:border-[#839496] hover:text-[#fdf6e3] focus:outline-none focus-visible:ring-2 focus-visible:ring-[#268bd2]"
          : "grid h-8 w-8 shrink-0 place-items-center rounded-xl border border-stone-200 bg-white/70 text-stone-600 shadow-sm transition-colors hover:border-stone-300 hover:bg-white hover:text-stone-950 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-400"
      }
      onClick={onExitVibe}
      type="button"
    >
      <PanelLeftClose className="h-4 w-4" />
    </button>
  </>
)}
```

Replace the right rail block at lines 765-817 with:

```tsx
{showSkinShowcase && (
  <aside className="vibe-skin-right-rail hidden min-h-0 flex-col overflow-hidden border-l p-3 lg:flex">
    <div className="vibe-skin-right-card flex min-h-0 flex-1 flex-col rounded-3xl border p-3">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="text-[10px] font-semibold tracking-[0.18em] text-[var(--vibe-muted-text)]">
            {skinBlocks.showcase.badge}
          </p>
          <h2 className="mt-1 truncate text-lg font-semibold text-[var(--vibe-text)]">
            {skinBlocks.showcase.title}
          </h2>
          <p className="mt-1 text-[12px] text-[var(--vibe-muted-text)]">
            {skinBlocks.showcase.subtitle}
          </p>
        </div>
      </div>
      <div className="vibe-skin-showcase-stage mt-3 flex min-h-[220px] flex-1 flex-col items-center justify-center rounded-3xl border p-3 text-center">
        {skinBlocks.showcase.figure ? (
          <img
            alt={`${skinBlocks.showcase.title} figure`}
            className="vibe-skin-showcase-figure max-h-52 w-full max-w-[168px] object-contain"
            src={skinBlocks.showcase.figure}
          />
        ) : (
          <div className="vibe-skin-showcase-figure vibe-skin-showcase-orb grid h-32 w-28 place-items-center rounded-[2rem] border">
            <AiSwitchLogo className="h-14 w-14 rounded-2xl" />
          </div>
        )}
        <p className="mt-3 text-[13px] leading-6 text-[var(--vibe-text)] opacity-90">
          {skinBlocks.showcase.body}
        </p>
      </div>
      <div className="vibe-skin-showcase-footer mt-3 rounded-2xl border px-3 py-2 text-[11px] text-[var(--vibe-muted-text)]">
        {skinBlocks.showcase.footer}
      </div>
    </div>
    <div className="vibe-skin-right-card mt-3 rounded-2xl border p-3">
      <p className="text-[10px] font-semibold tracking-[0.18em] text-[var(--vibe-muted-text)]">
        皮肤区域
      </p>
      <div className="mt-2 flex flex-wrap gap-1.5">
        {activeSkinRegionKeys.length > 0 ? (
          activeSkinRegionKeys.slice(0, 8).map((region) => (
            <span key={region} className="rounded-full border px-2 py-1 text-[11px]">
              {region}
            </span>
          ))
        ) : (
          <span className="rounded-full border px-2 py-1 text-[11px]">ui</span>
        )}
      </div>
    </div>
  </aside>
)}
```

Update the `XtermPane` call:

```tsx
<XtermPane
  active={tab.id === activeId}
  key={tab.id}
  onStatusChange={updateStatus}
  session={tab}
  themeMode={terminalThemeMode}
  themeOverride={isSkin ? activeSkin.terminal : undefined}
  transparentSurface={isSkin}
/>
```

Replace the skin status bar content at lines 821-830 with:

```tsx
{isSkin && (
  <div className="vibe-skin-status-bar flex h-9 shrink-0 items-center justify-between gap-3 border-t px-4 text-[11px] font-medium">
    <span className="truncate">{skinBlocks.statusbar.left}</span>
    <span className="truncate">{skinBlocks.statusbar.right}</span>
  </div>
)}
```

- [ ] **Step 6: Run focused screen tests**

Run:

```powershell
pnpm vitest run tests/VibeScreen.test.tsx
```

Expected: PASS.

- [ ] **Step 7: Commit screen layout changes**

Run:

```powershell
git add src/screens/VibeScreen.tsx tests/VibeScreen.test.tsx
git commit -m "feat: render qq2007 vibe skin blocks"
```

Expected: commit succeeds with only Vibe screen and Vibe screen tests staged.

---

### Task 3: Skin Terminal Transparency And Region CSS

**Files:**
- Modify: `src/components/terminal/XtermPane.tsx`
- Modify: `src/styles.css`
- Test: `tests/terminal/XtermPane.test.tsx`

**Interfaces:**
- Consumes: `transparentSurface={isSkin}` from Task 2.
- Produces: `XtermPaneProps.transparentSurface?: boolean`, host class `xterm-pane-skin-transparent`, and xterm theme background `"transparent"` when `transparentSurface` is true.

- [ ] **Step 1: Write failing XtermPane transparency test**

In `tests/terminal/XtermPane.test.tsx`, replace the xterm mock with a hoisted constructor capture:

```ts
const terminalConstructorOptions = vi.hoisted(() => [] as Array<Record<string, unknown>>);

vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    cols = 80;
    rows = 24;
    options: Record<string, unknown>;
    dispose = vi.fn();
    focus = vi.fn();
    loadAddon = vi.fn();
    open = vi.fn();
    refresh = vi.fn();
    write = vi.fn();
    writeln = vi.fn();
    onData = vi.fn(() => ({ dispose: vi.fn() }));

    constructor(options: Record<string, unknown>) {
      this.options = options;
      terminalConstructorOptions.push(options);
    }
  },
}));
```

Update `afterEach()`:

```ts
afterEach(() => {
  subscribe.mockClear();
  terminalConstructorOptions.length = 0;
});
```

Append this test:

```tsx
it("marks skin panes transparent and uses a transparent xterm background", async () => {
  const { container } = render(
    <XtermPane
      session={session}
      themeMode="light"
      themeOverride={{
        background: "#010203",
        foreground: "#eafcff",
      }}
      transparentSurface
    />,
  );

  expect(container.querySelector(".xterm-pane-skin-transparent")).not.toBeNull();
  await waitFor(() => expect(terminalConstructorOptions).toHaveLength(1));

  expect(terminalConstructorOptions[0]?.theme).toMatchObject({
    background: "transparent",
    foreground: "#eafcff",
  });
});
```

- [ ] **Step 2: Run XtermPane tests to verify they fail**

Run:

```powershell
pnpm vitest run tests/terminal/XtermPane.test.tsx
```

Expected: FAIL because `transparentSurface` does not exist and the host class/theme override are missing.

- [ ] **Step 3: Add transparent surface support to `XtermPane.tsx`**

Update `XtermPaneProps`:

```ts
type XtermPaneProps = {
  session: TerminalSession;
  active?: boolean;
  themeMode?: "dark" | "light";
  themeOverride?: VibeTerminalTheme;
  transparentSurface?: boolean;
  onStatusChange?: (sessionId: string, status: TerminalStatus) => void;
};
```

Replace `createTheme()` with:

```ts
function createTheme(
  themeMode: "dark" | "light",
  themeOverride?: VibeTerminalTheme,
  transparentSurface = false,
) {
  const baseTheme =
    themeMode === "light"
      ? {
          background: "#f8fafc",
          black: "#334155",
          blue: "#2563eb",
          brightBlack: "#64748b",
          brightBlue: "#3b82f6",
          brightCyan: "#06b6d4",
          brightGreen: "#16a34a",
          brightMagenta: "#c026d3",
          brightRed: "#dc2626",
          brightWhite: "#0f172a",
          brightYellow: "#ca8a04",
          cyan: "#0891b2",
          foreground: "#0f172a",
          green: "#15803d",
          magenta: "#a21caf",
          red: "#b91c1c",
          white: "#475569",
          yellow: "#a16207",
        }
      : {
          background: "#002b36",
          black: "#073642",
          blue: "#268bd2",
          brightBlack: "#586e75",
          brightBlue: "#839496",
          brightCyan: "#2aa198",
          brightGreen: "#859900",
          brightMagenta: "#d33682",
          brightRed: "#dc322f",
          brightWhite: "#fdf6e3",
          brightYellow: "#b58900",
          cyan: "#2aa198",
          foreground: "#d8e2dc",
          green: "#859900",
          magenta: "#6c71c4",
          red: "#dc322f",
          white: "#93a1a1",
          yellow: "#b58900",
        };

  return {
    ...baseTheme,
    ...themeOverride,
    ...(transparentSurface ? { background: "transparent" } : {}),
  };
}
```

Destructure `transparentSurface = false` in `XtermPane()` and update the memo:

```ts
const theme = useMemo(
  () => createTheme(themeMode, themeOverride, transparentSurface),
  [themeMode, themeOverride, transparentSurface],
);
```

Replace the returned host class:

```tsx
className={`xterm-pane h-full min-h-0 ${transparentSurface ? "xterm-pane-skin-transparent" : ""} ${
  active ? "block" : "hidden"
}`}
```

- [ ] **Step 4: Add CSS for new regions and transparent xterm layers**

In `src/styles.css`, add these rules after `.vibe-skin-titlebar`:

```css
.vibe-skin-titlebar-controls {
  background: var(--vibe-titlebar-controls-background-layer, transparent);
  background-position: var(--vibe-titlebar-controls-background-position, center);
  background-repeat: var(--vibe-titlebar-controls-background-repeat, no-repeat);
  background-size: var(--vibe-titlebar-controls-background-size, cover);
  border-color: var(--vibe-titlebar-controls-border, transparent);
  color: var(--vibe-titlebar-controls-color, var(--vibe-titlebar-color, var(--vibe-text)));
  box-shadow: var(--vibe-titlebar-controls-shadow);
}

.vibe-skin-window-button {
  display: inline-grid;
  height: 1.35rem;
  width: 1.55rem;
  place-items: center;
  border: 1px solid var(--vibe-window-button-border, rgba(255, 255, 255, 0.58));
  border-radius: 0.45rem;
  background: var(--vibe-window-button-background-layer, linear-gradient(180deg, rgba(255,255,255,0.92), rgba(71,159,230,0.88) 48%, rgba(16,100,188,0.9)));
  color: var(--vibe-window-button-color, #ffffff);
  box-shadow: var(--vibe-window-button-shadow, inset 0 1px 0 rgba(255,255,255,0.8), 0 1px 2px rgba(0,45,96,0.3));
  font-size: 0.8rem;
  line-height: 1;
}

.vibe-skin-window-button-minimize {
  background: var(--vibe-window-button-minimize-background-layer, var(--vibe-window-button-background-layer, linear-gradient(180deg, #effbff, #3294e5 52%, #0d62b5)));
}

.vibe-skin-window-button-maximize {
  background: var(--vibe-window-button-maximize-background-layer, var(--vibe-window-button-background-layer, linear-gradient(180deg, #effbff, #3294e5 52%, #0d62b5)));
}

.vibe-skin-window-button-close {
  background: var(--vibe-window-button-close-background-layer, linear-gradient(180deg, #ffb0b7, #f05b68 46%, #b8202f));
}
```

Add these rules after `.vibe-skin-sidebar-header`:

```css
.vibe-skin-profile {
  background: var(--vibe-sidebar-profile-background-layer, transparent);
  background-position: var(--vibe-sidebar-profile-background-position, center);
  background-repeat: var(--vibe-sidebar-profile-background-repeat, no-repeat);
  background-size: var(--vibe-sidebar-profile-background-size, cover);
  border-color: var(--vibe-sidebar-profile-border, transparent);
  color: var(--vibe-sidebar-profile-color, var(--vibe-text));
  box-shadow: var(--vibe-sidebar-profile-shadow);
  padding: var(--vibe-sidebar-profile-padding);
}

.vibe-skin-avatar {
  background: var(--vibe-avatar-background-layer, radial-gradient(circle at 35% 20%, #ffffff, #9edbff 46%, #2684d5));
  background-position: var(--vibe-avatar-background-position, center);
  background-repeat: var(--vibe-avatar-background-repeat, no-repeat);
  background-size: var(--vibe-avatar-background-size, cover);
  border-color: var(--vibe-avatar-border, rgba(255, 255, 255, 0.78));
  box-shadow: var(--vibe-avatar-shadow, inset 0 1px 0 rgba(255,255,255,0.82), 0 8px 18px rgba(13,104,190,0.22));
}

.vibe-skin-online-badge {
  background: var(--vibe-online-badge-background-layer, linear-gradient(180deg, #8cff9f, #16a34a));
  border-color: var(--vibe-online-badge-border, #ffffff);
  box-shadow: var(--vibe-online-badge-shadow, 0 0 0 1px rgba(16,129,62,0.22));
}

.vibe-skin-profile-badge {
  background: var(--vibe-profile-badge-background-layer, linear-gradient(180deg, #fff8c9, #f3b61f));
  border-color: var(--vibe-profile-badge-border, rgba(157, 101, 0, 0.26));
  color: var(--vibe-profile-badge-color, #7a4a00);
  box-shadow: var(--vibe-profile-badge-shadow);
}
```

Replace the existing `.vibe-skin-terminal-shell .xterm` transparent block with:

```css
.vibe-skin-terminal-shell .xterm-pane,
.vibe-skin-terminal-shell .xterm-pane-skin-transparent,
.vibe-skin-terminal-shell .xterm,
.vibe-skin-terminal-shell .xterm-screen,
.vibe-skin-terminal-shell .xterm-viewport,
.vibe-skin-terminal-shell .xterm-helpers,
.vibe-skin-terminal-shell .xterm-rows,
.vibe-skin-terminal-shell .xterm-screen canvas,
.vibe-skin-terminal-shell canvas {
  background: transparent !important;
  background-color: transparent !important;
}
```

Add these rules before `.vibe-skin-status-bar`:

```css
.vibe-skin-showcase-stage {
  background: var(--vibe-showcase-stage-background-layer, radial-gradient(circle at 50% 12%, rgba(255,255,255,0.95), rgba(195,232,255,0.72) 42%, rgba(73,155,221,0.42)));
  background-position: var(--vibe-showcase-stage-background-position, center);
  background-repeat: var(--vibe-showcase-stage-background-repeat, no-repeat);
  background-size: var(--vibe-showcase-stage-background-size, cover);
  border-color: var(--vibe-showcase-stage-border, var(--vibe-border));
  color: var(--vibe-showcase-stage-color, var(--vibe-text));
  box-shadow: var(--vibe-showcase-stage-shadow, inset 0 1px 0 rgba(255,255,255,0.82));
}

.vibe-skin-showcase-figure {
  background: var(--vibe-showcase-figure-background-layer, transparent);
  background-position: var(--vibe-showcase-figure-background-position, center);
  background-repeat: var(--vibe-showcase-figure-background-repeat, no-repeat);
  background-size: var(--vibe-showcase-figure-background-size, cover);
  border-color: var(--vibe-showcase-figure-border, rgba(255,255,255,0.58));
  color: var(--vibe-showcase-figure-color, var(--vibe-text));
  box-shadow: var(--vibe-showcase-figure-shadow, 0 18px 30px rgba(10,82,154,0.2));
}

.vibe-skin-showcase-footer {
  background: var(--vibe-showcase-footer-background-layer, rgba(255,255,255,0.5));
  background-position: var(--vibe-showcase-footer-background-position, center);
  background-repeat: var(--vibe-showcase-footer-background-repeat, no-repeat);
  background-size: var(--vibe-showcase-footer-background-size, cover);
  border-color: var(--vibe-showcase-footer-border, var(--vibe-border));
  color: var(--vibe-showcase-footer-color, var(--vibe-muted-text));
  box-shadow: var(--vibe-showcase-footer-shadow);
}
```

- [ ] **Step 5: Run focused terminal tests**

Run:

```powershell
pnpm vitest run tests/terminal/XtermPane.test.tsx
```

Expected: PASS.

- [ ] **Step 6: Run screen tests after CSS and terminal changes**

Run:

```powershell
pnpm vitest run tests/VibeScreen.test.tsx
```

Expected: PASS.

- [ ] **Step 7: Commit terminal transparency and CSS changes**

Run:

```powershell
git add src/components/terminal/XtermPane.tsx src/styles.css tests/terminal/XtermPane.test.tsx tests/VibeScreen.test.tsx
git commit -m "fix: keep vibe skin terminals transparent"
```

Expected: commit succeeds with terminal, CSS, and related test changes staged.

---

### Task 4: Skin Author Documentation And Full Verification

**Files:**
- Modify: `docs/vibe-skins.md`

**Interfaces:**
- Consumes: `blocks` schema, region keys, and terminal transparency behavior from Tasks 1-3.
- Produces: public documentation for making QQ2007-style skins without arbitrary HTML or scripts.

- [ ] **Step 1: Update asset resolution overview**

In `docs/vibe-skins.md`, replace the asset paragraph under the file type list with:

```md
Zip packages can include image assets. When `ui.backgroundImage`, `regions.*.backgroundImage`, `showcase.image`, `blocks.profile.avatar`, or `blocks.showcase.figure` is a relative path, Vibe resolves it from inside the zip package and stores it as a data URL in local storage.
```

- [ ] **Step 2: Replace the minimal manifest example with blocks**

Replace the JSON example in `docs/vibe-skins.md` with:

```json
{
  "id": "my-blue-skin",
  "name": "My Blue Skin",
  "author": "Your Name",
  "version": "1.0.0",
  "ui": {
    "accent": "#1678d8",
    "background": "linear-gradient(135deg, #dff5ff, #1157a4)",
    "backgroundImage": "assets/background.png",
    "panel": "rgba(232, 247, 255, 0.78)",
    "panelStrong": "rgba(255, 255, 255, 0.92)",
    "panelSubtle": "rgba(216, 239, 255, 0.68)",
    "border": "rgba(15, 99, 184, 0.34)",
    "text": "#0d315d",
    "mutedText": "#386b9e",
    "button": "linear-gradient(180deg, #49a7ff, #126fc5)",
    "buttonHover": "linear-gradient(180deg, #5eb4ff, #0f61ae)",
    "focus": "#44a7ff"
  },
  "terminal": {
    "background": "transparent",
    "foreground": "#12375f",
    "blue": "#0d6ec9",
    "green": "#2f854d",
    "red": "#b73546",
    "yellow": "#b37613"
  },
  "regions": {
    "titlebar": {
      "background": "linear-gradient(180deg, #e7fbff, #0f6bc4)",
      "border": "rgba(5, 82, 150, 0.65)",
      "color": "#ffffff"
    },
    "windowButtonClose": {
      "background": "linear-gradient(180deg, #ffb0b7, #b8202f)"
    },
    "sidebarProfile": {
      "background": "linear-gradient(180deg, rgba(255,255,255,0.75), rgba(186,231,255,0.5))"
    },
    "avatar": {
      "backgroundImage": "assets/avatar-frame.png",
      "backgroundSize": "cover"
    },
    "terminalShell": {
      "background": "rgba(247,251,255,0.42)",
      "backgroundImage": "assets/terminal-shell.png",
      "borderRadius": "16px",
      "shadow": "0 18px 34px rgba(18,91,166,0.14)"
    },
    "showcaseStage": {
      "backgroundImage": "assets/showcase-stage.png",
      "backgroundSize": "cover"
    },
    "showcaseFigure": {
      "shadow": "0 18px 30px rgba(10,82,154,0.2)"
    }
  },
  "blocks": {
    "titlebar": {
      "title": "AI Switch 终端",
      "subtitle": "QQ2007 蓝色经典",
      "badge": "皮肤模式"
    },
    "profile": {
      "name": "AI Switch",
      "status": "在线",
      "signature": "正在使用 Vibe 终端",
      "badge": "经典蓝钻",
      "avatar": "assets/avatar.png"
    },
    "showcase": {
      "title": "QQ秀展示",
      "subtitle": "Codex 2007 Blue",
      "body": "右侧展示区可由皮肤定义图片、舞台和说明。",
      "badge": "我的QQ秀",
      "figure": "assets/qqshow.png",
      "footer": "自定义展示区"
    },
    "statusbar": {
      "left": "AI Switch 已连接",
      "right": "皮肤区域已启用"
    }
  }
}
```

- [ ] **Step 3: Add blocks documentation**

Add this section after the paragraph that explains optional `ui` and `terminal`:

```md
## Content Blocks

`blocks` customizes safe app-rendered skin content. Values are strings and image references only; the app never executes user HTML, CSS files, or scripts from a skin package.

Supported blocks:

- `blocks.titlebar.title`: main Chinese-style title text in the skin titlebar.
- `blocks.titlebar.subtitle`: smaller titlebar subtitle.
- `blocks.titlebar.badge`: small badge before the visual window controls.
- `blocks.profile.name`: nickname in the left profile card.
- `blocks.profile.status`: presence text, for example `在线`.
- `blocks.profile.signature`: short status signature under the nickname.
- `blocks.profile.badge`: small profile badge.
- `blocks.profile.avatar`: avatar image path or data URL.
- `blocks.showcase.title`: right QQ秀-style display title.
- `blocks.showcase.subtitle`: right display subtitle.
- `blocks.showcase.body`: descriptive text inside the showcase stage.
- `blocks.showcase.badge`: small right rail label.
- `blocks.showcase.figure`: figure image path or data URL for the QQ秀-style stage.
- `blocks.showcase.footer`: footer tag under the showcase stage.
- `blocks.statusbar.left`: left status bar text.
- `blocks.statusbar.right`: right status bar text.

If `blocks.showcase` is omitted, the older `showcase` object still renders in the right rail. If `blocks.showcase` exists, it takes precedence over `showcase`.

The minimize, maximize, and close controls in the skin titlebar are decorative. They are intentionally inert and do not call native window APIs.
```

- [ ] **Step 4: Extend the region key list**

Add these keys to the Region Keys list in their visual order:

```md
- `titlebarControls`
- `windowButton`
- `windowButtonMinimize`
- `windowButtonMaximize`
- `windowButtonClose`
- `sidebarProfile`
- `avatar`
- `onlineBadge`
- `profileBadge`
- `showcaseStage`
- `showcaseFigure`
- `showcaseFooter`
```

Add this paragraph after the `terminalShell` explanation:

```md
In skin mode, Vibe keeps the xterm wrapper, viewport, screen, rows, and canvas layers transparent so the `terminalShell` region can remain visible after creating new terminals. Use `terminal.foreground` and the other terminal color keys for text readability, and keep decorative terminal backgrounds on `regions.terminalShell`.
```

- [ ] **Step 5: Update zip layout**

Replace the zip layout block with:

```text
my-skin.zip
  skin.json
  assets/
    background.png
    sidebar.png
    terminal-shell.png
    avatar-frame.png
    avatar.png
    showcase-stage.png
    qqshow.png
```

- [ ] **Step 6: Run full verification**

Run:

```powershell
pnpm vitest run tests/lib/vibeSkin.test.ts tests/VibeScreen.test.tsx tests/terminal/XtermPane.test.tsx
pnpm typecheck
```

Expected: both commands PASS.

- [ ] **Step 7: Commit docs and verified implementation**

Run:

```powershell
git add docs/vibe-skins.md
git commit -m "docs: document vibe skin blocks"
```

Expected: commit succeeds with only documentation staged.

---

## Self-Review Checklist

- Spec coverage: Task 1 covers block schema, new region keys, zip asset resolution, storage normalization, default Chinese QQ2007 copy, and legacy showcase mapping.
- Spec coverage: Task 2 covers Chinese skin UI, visual-only window controls, profile/avatar/online/badge elements, QQ秀-style right display, statusbar copy, custom blocks, dark/light absence, and passing skin-mode terminal transparency.
- Spec coverage: Task 3 covers new CSS region consumers, stronger terminal transparency selectors, xterm host class, and transparent terminal theme background in skin mode.
- Spec coverage: Task 4 covers public manifest docs, block fields, region keys, visual-only controls, terminal transparency guidance, zip assets, and verification.
- Placeholder scan: The plan contains no placeholder markers, deferred work, arbitrary error-handling instructions, or unspecified tests.
- Type consistency: `VibeSkinBlocks`, `ResolvedVibeSkinBlocks`, `DEFAULT_VIBE_SKIN_BLOCKS`, `getVibeSkinBlocks()`, and `transparentSurface` are defined before later tasks consume them.
