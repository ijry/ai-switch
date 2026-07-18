# Vibe Starship Cockpit Skin Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a built-in, standard-editable Vibe skin package that renders a cinematic starship cockpit UI with radar scan, rotating spacecraft, typewriter telemetry, deep-space travel backdrop, and cockpit HUD panels.

**Architecture:** Reuse the existing Vibe skin package system: manifests stay JSON-only, region styling remains CSS-variable driven, and decorative visuals are selected by safe whitelisted React templates. The starship skin adds one built-in package plus reusable template components/classes so imported user skins can select the same safe cockpit blocks without executing arbitrary code.

**Tech Stack:** React 18, TypeScript, Vite, Vitest, Testing Library, CSS animations, existing JSON skin manifests.

## Global Constraints

- Work directly on `main`; do not create or switch branches/worktrees.
- Add `src/skins/starship-cockpit/skin.json` as a standard skin package manifest.
- Keep uploaded skin packages data-driven; no custom HTML, JavaScript, CSS files, native commands, external images, external fonts, Canvas, WebGL, or Three.js.
- Extend only the safe decoration whitelist with `starship-cockpit`, `space-ai-core`, `space-ship`, `space-radar`, `space-telemetry`, and `space-starmap`.
- Terminal theme background must be `"transparent"` and skin terminal panes must remain visually transparent.
- Visible skin copy should be Chinese.
- CSS animation must be decorative, layout-stable, and respect `prefers-reduced-motion: reduce`.
- Tabs close control remains an icon-style affordance with hover/focus feedback only, no separate button-like background.

---

## File Structure

- Create `src/skins/starship-cockpit/skin.json`: built-in standard package containing the cockpit color system, region styles, Chinese title/profile/showcase/status/taskbar text, and right-card template selections.
- Modify `src/lib/vibeSkin.ts`: import/register the built-in manifest and extend variant/template literal unions plus safe normalization sets.
- Modify `src/screens/VibeScreen.tsx`: render the new safe cockpit templates and specialized right-card layouts for radar, spacecraft, telemetry, and route map.
- Modify `src/styles.css`: add cockpit HUD, starfield, radar sweep, spacecraft rotation, telemetry cursor, responsive layout refinements, and reduced-motion handling.
- Modify `tests/lib/vibeSkin.test.ts`: assert built-in standard-package importability and whitelist normalization for the new cockpit templates.
- Modify `tests/VibeScreen.test.tsx`: assert the skin renders Chinese cockpit UI and all key template test IDs.
- Modify `docs/vibe-skins.md`: document the new built-in skin and supported cockpit template names.

---

### Task 1: Register Starship Skin Schema Entries

**Files:**
- Modify: `src/lib/vibeSkin.ts`
- Test: `tests/lib/vibeSkin.test.ts`

**Interfaces:**
- Consumes: existing `VibeSkinDefinition`, `VIBE_SKIN_DECORATION_VARIANTS`, `VIBE_SKIN_DECORATION_TEMPLATES`, `BUILT_IN_VIBE_SKINS`, `importVibeSkinPackage`.
- Produces: `starship-cockpit` variant and `space-*` templates accepted by built-in and imported JSON manifests.

- [ ] **Step 1: Add failing whitelist/import tests**

Add this test near the existing built-in skin tests in `tests/lib/vibeSkin.test.ts`:

```ts
  it("includes the starship cockpit skin as a standard importable skin package", async () => {
    const builtInSkin = BUILT_IN_VIBE_SKINS.find((skin) => skin.id === "starship-cockpit");

    expect(builtInSkin).toBeTruthy();
    expect(builtInSkin?.name).toBe("星舰驾驶舱");
    expect(builtInSkin?.terminal?.background).toBe("transparent");
    expect(builtInSkin?.decorations?.variant).toBe("starship-cockpit");
    expect(builtInSkin?.decorations?.avatarTemplate).toBe("space-ai-core");
    expect(builtInSkin?.decorations?.showcaseTemplate).toBe("space-ship");
    expect(builtInSkin?.decorations?.rightCards?.map((card) => card.template)).toEqual([
      "space-radar",
      "space-ship",
      "space-telemetry",
      "space-starmap",
    ]);

    const imported = await importVibeSkinPackage(
      new File([JSON.stringify(builtInSkin)], "starship-cockpit.aiskin", {
        type: "application/json",
      }),
    );

    expect(imported.id).toBe("starship-cockpit");
    expect(imported.decorations).toEqual(builtInSkin?.decorations);
  });
```

Add this test near normalization/sanitization tests:

```ts
  it("keeps whitelisted cockpit decoration templates from uploaded skins", async () => {
    const skin = await importVibeSkinPackage(
      new File(
        [
          JSON.stringify({
            id: "uploaded-starship",
            name: "上传星舰",
            ui: {
              accent: "#2ee8ff",
              accentText: "#001018",
              background: "#020617",
              backgroundOverlay: "transparent",
              panel: "rgba(2, 16, 32, 0.7)",
              panelStrong: "rgba(5, 24, 45, 0.9)",
              panelSubtle: "rgba(17, 44, 70, 0.72)",
              border: "rgba(46, 232, 255, 0.36)",
              text: "#e6fbff",
              mutedText: "#8ac9d8",
              button: "#2ee8ff",
              buttonText: "#001018",
              buttonHover: "#7cf6ff",
              dangerBackground: "#f97373",
              dangerText: "#1b0303",
              tabBar: "rgba(2, 16, 32, 0.72)",
              tabActive: "rgba(46, 232, 255, 0.18)",
              tabInactive: "rgba(7, 24, 46, 0.72)",
              tabHover: "rgba(46, 232, 255, 0.12)",
              focus: "#f8c76a",
            },
            terminal: {
              background: "transparent",
              foreground: "#d8fbff",
            },
            decorations: {
              variant: "starship-cockpit",
              avatarTemplate: "space-ai-core",
              showcaseTemplate: "space-ship",
              rightCards: [
                { title: "雷达阵列", template: "space-radar" },
                { title: "舰体模拟", template: "space-ship" },
                { title: "遥测输出", template: "space-telemetry", items: [{ label: "跃迁核心", badge: "稳定" }] },
                { title: "航线星图", template: "space-starmap" },
              ],
            },
          }),
        ],
        "uploaded-starship.aiskin",
        { type: "application/json" },
      ),
    );

    expect(skin.decorations?.variant).toBe("starship-cockpit");
    expect(skin.decorations?.avatarTemplate).toBe("space-ai-core");
    expect(skin.decorations?.showcaseTemplate).toBe("space-ship");
    expect(skin.decorations?.rightCards?.map((card) => card.template)).toEqual([
      "space-radar",
      "space-ship",
      "space-telemetry",
      "space-starmap",
    ]);
  });
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```powershell
pnpm test:run tests/lib/vibeSkin.test.ts
```

Expected: fail because `starship-cockpit` is not imported/registered and `space-*` templates are stripped by whitelist normalization.

- [ ] **Step 3: Register the manifest and safe string unions**

In `src/lib/vibeSkin.ts`, add:

```ts
import starshipCockpitSkinManifest from "../skins/starship-cockpit/skin.json";
```

Update the arrays:

```ts
export const VIBE_SKIN_DECORATION_VARIANTS = [
  "codex-2007",
  "rescue-pups",
  "starship-cockpit",
] as const;

export const VIBE_SKIN_DECORATION_TEMPLATES = [
  "qq-mascot",
  "qq-person",
  "rescue-rider",
  "rescue-hq",
  "rescue-dog-team",
  "rescue-civic",
  "rescue-mayor",
  "rescue-chicken",
  "space-ai-core",
  "space-ship",
  "space-radar",
  "space-telemetry",
  "space-starmap",
] as const;
```

Update built-ins:

```ts
export const BUILT_IN_VIBE_SKINS: VibeSkinDefinition[] = [
  asBuiltInVibeSkin(codex2007BlueSkinManifest),
  asBuiltInVibeSkin(rescuePupsAdventureBaySkinManifest),
  asBuiltInVibeSkin(starshipCockpitSkinManifest),
];
```

- [ ] **Step 4: Run tests**

Run:

```powershell
pnpm test:run tests/lib/vibeSkin.test.ts
```

Expected: the new whitelist test passes after the manifest exists in Task 2; if run before Task 2, the only remaining failure is missing `src/skins/starship-cockpit/skin.json`.

---

### Task 2: Add Standard Starship Cockpit Manifest

**Files:**
- Create: `src/skins/starship-cockpit/skin.json`
- Test: `tests/lib/vibeSkin.test.ts`

**Interfaces:**
- Consumes: `VibeSkinDefinition` JSON shape, existing region keys, and new template names from Task 1.
- Produces: a copyable JSON skin package with Chinese UI copy, transparent terminal, cockpit panel regions, right-side decorative cards, and taskbar/status blocks.

- [ ] **Step 1: Create the standard skin package**

Create `src/skins/starship-cockpit/skin.json` with this manifest:

```json
{
  "id": "starship-cockpit",
  "name": "星舰驾驶舱",
  "author": "AI Switch",
  "version": "1.0.0",
  "ui": {
    "accent": "#2ee8ff",
    "accentText": "#021019",
    "background": "radial-gradient(circle at 18% 12%, rgba(46,232,255,0.2), transparent 18rem), radial-gradient(circle at 78% 20%, rgba(248,199,106,0.13), transparent 17rem), linear-gradient(135deg, #020617 0%, #061425 48%, #01030a 100%)",
    "backgroundOverlay": "linear-gradient(105deg, rgba(46,232,255,0.08), transparent 28%, rgba(248,199,106,0.06) 62%, transparent), repeating-linear-gradient(90deg, rgba(255,255,255,0.035) 0 1px, transparent 1px 96px)",
    "panel": "rgba(2, 16, 32, 0.62)",
    "panelStrong": "rgba(4, 23, 43, 0.86)",
    "panelSubtle": "rgba(8, 35, 61, 0.64)",
    "border": "rgba(46, 232, 255, 0.32)",
    "text": "#e6fbff",
    "mutedText": "#8bc9d8",
    "button": "linear-gradient(180deg, #7cf6ff 0%, #2ee8ff 45%, #087d9a 100%)",
    "buttonText": "#001018",
    "buttonHover": "linear-gradient(180deg, #b7fbff 0%, #52f1ff 45%, #0aa8c9 100%)",
    "dangerBackground": "linear-gradient(180deg, #ff9b7a, #f45454)",
    "dangerText": "#190607",
    "tabBar": "linear-gradient(180deg, rgba(1,12,24,0.82), rgba(4,25,45,0.7))",
    "tabActive": "linear-gradient(180deg, rgba(46,232,255,0.24), rgba(9,63,92,0.72))",
    "tabInactive": "linear-gradient(180deg, rgba(6,28,52,0.62), rgba(2,14,28,0.68))",
    "tabHover": "rgba(46, 232, 255, 0.12)",
    "focus": "#f8c76a"
  },
  "terminal": {
    "background": "transparent",
    "foreground": "#d8fbff",
    "black": "#06111d",
    "red": "#ff6f7d",
    "green": "#70ffb5",
    "yellow": "#f8c76a",
    "blue": "#4fb4ff",
    "magenta": "#b98cff",
    "cyan": "#2ee8ff",
    "white": "#d8fbff",
    "brightBlack": "#557083",
    "brightRed": "#ff95a0",
    "brightGreen": "#a7ffd3",
    "brightYellow": "#ffe4a6",
    "brightBlue": "#87ceff",
    "brightMagenta": "#d4b8ff",
    "brightCyan": "#a3fbff",
    "brightWhite": "#ffffff"
  },
  "regions": {
    "app": {
      "background": "radial-gradient(circle at 18% 10%, rgba(46,232,255,0.17), transparent 18rem), radial-gradient(circle at 82% 18%, rgba(248,199,106,0.12), transparent 16rem), linear-gradient(135deg, #020617 0%, #061425 44%, #01030a 100%)",
      "backgroundOverlay": "linear-gradient(105deg, rgba(46,232,255,0.08), transparent 26%, rgba(248,199,106,0.06) 64%, transparent), repeating-linear-gradient(90deg, rgba(255,255,255,0.035) 0 1px, transparent 1px 96px)",
      "border": "rgba(46,232,255,0.38)",
      "color": "#e6fbff"
    },
    "titlebar": {
      "background": "linear-gradient(180deg, rgba(12,45,75,0.88), rgba(2,15,29,0.82))",
      "backgroundOverlay": "linear-gradient(90deg, rgba(46,232,255,0.25), transparent 30%, rgba(248,199,106,0.14) 70%, transparent)",
      "border": "rgba(46,232,255,0.34)",
      "color": "#e6fbff",
      "shadow": "inset 0 1px 0 rgba(163,251,255,0.26), 0 12px 28px rgba(0,0,0,0.28)"
    },
    "sidebar": {
      "background": "linear-gradient(180deg, rgba(2,16,32,0.72), rgba(3,28,51,0.58))",
      "backgroundOverlay": "radial-gradient(circle at 20% 10%, rgba(46,232,255,0.12), transparent 9rem), linear-gradient(90deg, rgba(46,232,255,0.08), transparent)",
      "border": "rgba(46,232,255,0.28)",
      "shadow": "inset -1px 0 0 rgba(163,251,255,0.12)"
    },
    "sidebarHeader": {
      "background": "linear-gradient(180deg, rgba(7,38,67,0.78), rgba(2,16,32,0.66))",
      "border": "rgba(46,232,255,0.32)",
      "shadow": "0 14px 28px rgba(0,0,0,0.2), inset 0 1px 0 rgba(163,251,255,0.2)"
    },
    "controlPanel": {
      "background": "linear-gradient(180deg, rgba(5,29,52,0.7), rgba(2,16,32,0.58))",
      "border": "rgba(46,232,255,0.24)",
      "shadow": "0 12px 24px rgba(0,0,0,0.18)"
    },
    "groupPanel": {
      "background": "linear-gradient(180deg, rgba(5,27,50,0.62), rgba(1,11,22,0.48))",
      "border": "rgba(46,232,255,0.22)",
      "shadow": "inset 0 1px 0 rgba(163,251,255,0.12)"
    },
    "workspace": {
      "background": "linear-gradient(180deg, rgba(2,14,28,0.4), rgba(2,10,20,0.22))",
      "backgroundOverlay": "radial-gradient(circle at 60% 8%, rgba(46,232,255,0.1), transparent 16rem), linear-gradient(90deg, rgba(46,232,255,0.05), transparent 34%)",
      "border": "rgba(46,232,255,0.22)",
      "shadow": "inset 1px 0 0 rgba(163,251,255,0.1)"
    },
    "tabBar": {
      "background": "linear-gradient(180deg, rgba(1,12,24,0.82), rgba(4,25,45,0.7))",
      "border": "rgba(46,232,255,0.24)"
    },
    "tab": {
      "background": "linear-gradient(180deg, rgba(6,28,52,0.62), rgba(2,14,28,0.68))",
      "border": "rgba(46,232,255,0.18)",
      "color": "#8bc9d8"
    },
    "tabActive": {
      "background": "linear-gradient(180deg, rgba(46,232,255,0.24), rgba(9,63,92,0.72))",
      "border": "rgba(46,232,255,0.38)",
      "color": "#e6fbff",
      "shadow": "0 0 24px rgba(46,232,255,0.14), inset 0 1px 0 rgba(163,251,255,0.25)"
    },
    "tabClose": {
      "color": "#8bc9d8"
    },
    "terminalShell": {
      "background": "linear-gradient(180deg, rgba(1,9,18,0.28), rgba(1,6,12,0.14))",
      "border": "rgba(46,232,255,0.22)",
      "shadow": "inset 0 1px 0 rgba(163,251,255,0.12), 0 20px 40px rgba(0,0,0,0.2)",
      "borderRadius": "20px"
    },
    "rightRail": {
      "background": "linear-gradient(180deg, rgba(2,16,32,0.68), rgba(1,9,18,0.58))",
      "backgroundOverlay": "radial-gradient(circle at 62% 10%, rgba(46,232,255,0.13), transparent 9rem)",
      "border": "rgba(46,232,255,0.28)",
      "shadow": "inset 1px 0 0 rgba(163,251,255,0.12)"
    },
    "rightCard": {
      "background": "linear-gradient(180deg, rgba(5,28,51,0.72), rgba(1,11,22,0.58))",
      "border": "rgba(46,232,255,0.26)",
      "shadow": "0 18px 30px rgba(0,0,0,0.2), inset 0 1px 0 rgba(163,251,255,0.14)"
    },
    "showcaseStage": {
      "background": "radial-gradient(circle at 50% 22%, rgba(46,232,255,0.18), transparent 8rem), linear-gradient(180deg, rgba(5,29,52,0.5), rgba(1,8,17,0.4))",
      "border": "rgba(46,232,255,0.25)"
    },
    "showcaseFooter": {
      "background": "rgba(2,16,32,0.48)",
      "border": "rgba(46,232,255,0.2)",
      "color": "#8bc9d8"
    },
    "statusBar": {
      "background": "linear-gradient(180deg, rgba(7,38,67,0.72), rgba(2,14,28,0.7))",
      "border": "rgba(46,232,255,0.24)",
      "color": "#a3fbff"
    },
    "taskbar": {
      "background": "linear-gradient(180deg, rgba(7,38,67,0.86), rgba(2,14,28,0.86))",
      "border": "rgba(46,232,255,0.28)",
      "color": "#e6fbff",
      "shadow": "inset 0 1px 0 rgba(163,251,255,0.18), 0 -12px 28px rgba(0,0,0,0.24)"
    },
    "taskbarStartButton": {
      "background": "linear-gradient(180deg, #7cf6ff 0%, #2ee8ff 45%, #087d9a 100%)",
      "border": "rgba(163,251,255,0.62)",
      "color": "#001018"
    },
    "taskbarItem": {
      "background": "rgba(46,232,255,0.08)",
      "border": "rgba(46,232,255,0.22)",
      "color": "#c6f7ff"
    },
    "taskbarItemActive": {
      "background": "rgba(46,232,255,0.18)",
      "border": "rgba(46,232,255,0.38)",
      "color": "#ffffff"
    },
    "taskbarTray": {
      "background": "rgba(1,10,19,0.42)",
      "border": "rgba(46,232,255,0.22)"
    },
    "taskbarClock": {
      "background": "rgba(46,232,255,0.08)",
      "color": "#a3fbff"
    }
  },
  "blocks": {
    "titlebar": {
      "title": "星舰驾驶舱 - Vibe 终端",
      "subtitle": "深空跃迁 / 指令甲板",
      "badge": "航行中"
    },
    "profile": {
      "name": "舰桥 AI 核心",
      "status": "量子链路在线",
      "signature": "航线稳定，终端指令等待执行。",
      "badge": "CORE"
    },
    "showcase": {
      "enabled": true,
      "title": "星舰主视窗",
      "subtitle": "FTL 航行模拟",
      "body": "舰体姿态、雷达扫描与遥测输出已同步到 Vibe 工作区。",
      "badge": "驾驶舱",
      "footer": "主推进器低噪运行，外层星流已锁定"
    },
    "statusbar": {
      "left": "舰桥链路已建立",
      "right": "深空航行模式"
    },
    "taskbar": {
      "enabled": true,
      "startButton": {
        "label": "舰桥"
      },
      "startMenu": {
        "items": [
          { "label": "外观设置", "action": "openAppearance" },
          { "label": "切换到皮肤模式", "action": "setTheme", "theme": "skin" },
          { "label": "切换亮色主题", "action": "setTheme", "theme": "light" },
          { "label": "切换暗色主题", "action": "setTheme", "theme": "dark" },
          { "label": "导入皮肤...", "action": "importSkin" },
          { "type": "separator" },
          { "label": "星舰驾驶舱", "disabled": true }
        ]
      },
      "items": [
        { "label": "主终端", "active": true },
        { "label": "雷达阵列" },
        { "label": "跃迁航线" }
      ],
      "tray": ["雷达", "护盾", "在线"],
      "clockFormat": "HH:mm"
    }
  },
  "decorations": {
    "variant": "starship-cockpit",
    "titlebarMark": "AI",
    "avatarTemplate": "space-ai-core",
    "showcaseTemplate": "space-ship",
    "rightCards": [
      {
        "template": "space-radar",
        "title": "雷达扫描",
        "subtitle": "近轨目标追踪",
        "badge": "SCAN",
        "footer": "三处目标已标记"
      },
      {
        "template": "space-ship",
        "title": "舰体模拟",
        "subtitle": "姿态慢速旋转",
        "badge": "MODEL",
        "footer": "推进器矢量锁定"
      },
      {
        "template": "space-telemetry",
        "title": "遥测输出",
        "subtitle": "数据检测",
        "badge": "LIVE",
        "items": [
          { "label": "跃迁核心", "badge": "稳定" },
          { "label": "护盾矩阵", "badge": "97%" },
          { "label": "导航星图", "badge": "同步" },
          { "label": "生命维持", "badge": "正常" }
        ],
        "footer": "持续监测中"
      },
      {
        "template": "space-starmap",
        "title": "航线星图",
        "subtitle": "下一跃迁点",
        "badge": "ROUTE",
        "items": [
          { "label": "始发", "badge": "SOL" },
          { "label": "中继", "badge": "NOVA-7" },
          { "label": "目标", "badge": "ORION" }
        ],
        "footer": "预计抵达 T+04:18"
      }
    ]
  },
  "showcase": {
    "enabled": true,
    "title": "星舰驾驶舱",
    "subtitle": "Cinematic cockpit skin",
    "body": "Safe template blocks render radar, spacecraft, and telemetry without user-provided code.",
    "badge": "Vibe Skin",
    "footer": "Built-in standard skin package"
  }
}
```

- [ ] **Step 2: Run model tests**

Run:

```powershell
pnpm test:run tests/lib/vibeSkin.test.ts
```

Expected: model tests pass once Task 1 is complete.

- [ ] **Step 3: Commit model/package changes**

Run:

```powershell
git add src/lib/vibeSkin.ts src/skins/starship-cockpit/skin.json tests/lib/vibeSkin.test.ts
git commit -m "feat: register starship cockpit vibe skin"
```

Expected: commit succeeds on `main`.

---

### Task 3: Render Cockpit Templates In React

**Files:**
- Modify: `src/screens/VibeScreen.tsx`
- Test: `tests/VibeScreen.test.tsx`

**Interfaces:**
- Consumes: `VibeSkinDecorationTemplate`, `VibeSkinDecorationCard`, `VibeSkinDecorationItem`.
- Produces: testable React markup with `data-testid` values `vibe-skin-space-ai-core`, `vibe-skin-space-ship`, `vibe-skin-space-radar`, `vibe-skin-space-telemetry`, and `vibe-skin-space-starmap`.

- [ ] **Step 1: Add failing screen test**

Add this test near existing uploaded/built-in decoration tests in `tests/VibeScreen.test.tsx`:

```tsx
  it("renders the starship cockpit skin with Chinese HUD blocks", async () => {
    renderScreen();

    await switchToSkinTheme();
    await openAppearanceDialog();
    await userEvent.selectOptions(screen.getByLabelText("Vibe skin"), "starship-cockpit");
    await userEvent.click(screen.getByRole("button", { name: "Save appearance" }));

    expect(screen.getByText("星舰驾驶舱 - Vibe 终端")).toBeInTheDocument();
    expect(screen.getByText("深空跃迁 / 指令甲板")).toBeInTheDocument();
    expect(screen.getByText("舰桥 AI 核心")).toBeInTheDocument();
    expect(screen.getByText("星舰主视窗")).toBeInTheDocument();
    expect(screen.getByText("雷达扫描")).toBeInTheDocument();
    expect(screen.getByText("舰体模拟")).toBeInTheDocument();
    expect(screen.getByText("遥测输出")).toBeInTheDocument();
    expect(screen.getByText("航线星图")).toBeInTheDocument();
    expect(screen.getByTestId("vibe-skin-space-ai-core")).toBeInTheDocument();
    expect(screen.getAllByTestId("vibe-skin-space-ship").length).toBeGreaterThanOrEqual(1);
    expect(screen.getByTestId("vibe-skin-space-radar")).toBeInTheDocument();
    expect(screen.getByTestId("vibe-skin-space-telemetry")).toBeInTheDocument();
    expect(screen.getByTestId("vibe-skin-space-starmap")).toBeInTheDocument();
    expect(document.querySelector(".vibe-skin--starship-cockpit")).toBeTruthy();
  });
```

- [ ] **Step 2: Run test to verify failure**

Run:

```powershell
pnpm test:run tests/VibeScreen.test.tsx
```

Expected: fail because cockpit templates do not render.

- [ ] **Step 3: Add base template render branches**

In `renderSkinTemplateFigure` in `src/screens/VibeScreen.tsx`, add branches before the final `return null`:

```tsx
  if (template === "space-ai-core") {
    return (
      <div
        aria-label={label}
        className={`vibe-skin-space-ai-core ${className}`}
        data-testid="vibe-skin-space-ai-core"
        role="img"
      >
        <span className="vibe-skin-space-ai-core-ring" />
        <span className="vibe-skin-space-ai-core-eye" />
        <span className="vibe-skin-space-ai-core-pulse" />
      </div>
    );
  }

  if (template === "space-ship") {
    return (
      <div
        aria-label={label}
        className={`vibe-skin-space-ship ${className}`}
        data-testid="vibe-skin-space-ship"
        role="img"
      >
        <span className="vibe-skin-space-ship-halo" />
        <span className="vibe-skin-space-ship-body" />
        <span className="vibe-skin-space-ship-wing vibe-skin-space-ship-wing-left" />
        <span className="vibe-skin-space-ship-wing vibe-skin-space-ship-wing-right" />
        <span className="vibe-skin-space-ship-core" />
        <span className="vibe-skin-space-ship-thruster" />
      </div>
    );
  }

  if (template === "space-radar") {
    return (
      <div
        aria-label={label}
        className={`vibe-skin-space-radar ${className}`}
        data-testid="vibe-skin-space-radar"
        role="img"
      >
        <span className="vibe-skin-space-radar-grid" />
        <span className="vibe-skin-space-radar-sweep" />
        <span className="vibe-skin-space-radar-blip vibe-skin-space-radar-blip-a" />
        <span className="vibe-skin-space-radar-blip vibe-skin-space-radar-blip-b" />
        <span className="vibe-skin-space-radar-blip vibe-skin-space-radar-blip-c" />
      </div>
    );
  }

  if (template === "space-starmap") {
    return (
      <div
        aria-label={label}
        className={`vibe-skin-space-starmap ${className}`}
        data-testid="vibe-skin-space-starmap"
        role="img"
      >
        <span className="vibe-skin-space-starmap-orbit vibe-skin-space-starmap-orbit-a" />
        <span className="vibe-skin-space-starmap-orbit vibe-skin-space-starmap-orbit-b" />
        <span className="vibe-skin-space-starmap-node vibe-skin-space-starmap-node-a" />
        <span className="vibe-skin-space-starmap-node vibe-skin-space-starmap-node-b" />
        <span className="vibe-skin-space-starmap-node vibe-skin-space-starmap-node-c" />
        <span className="vibe-skin-space-starmap-route" />
      </div>
    );
  }
```

- [ ] **Step 4: Add specialized right-card layouts**

In `SkinDecorationCard` in `src/screens/VibeScreen.tsx`, add `space-radar`, `space-ship`, `space-telemetry`, and `space-starmap` branches before the generic card renderer. Use this structure for telemetry:

```tsx
  if (card.template === "space-telemetry") {
    const telemetryItems = card.items?.length
      ? card.items
      : [
          { label: "跃迁核心", badge: "稳定" },
          { label: "护盾矩阵", badge: "97%" },
          { label: "导航星图", badge: "同步" },
        ];

    return (
      <div
        className="vibe-skin-right-card vibe-skin-space-card vibe-skin-space-telemetry-card mt-3 rounded-2xl border p-3"
        data-testid="vibe-skin-space-telemetry"
      >
        <div className="flex items-center justify-between gap-2">
          <p className="text-[10px] font-semibold tracking-[0.18em] text-[var(--vibe-muted-text)]">
            {card.badge ?? "LIVE"}
          </p>
          <span className="vibe-skin-space-led rounded-full border px-2 py-0.5 text-[10px]">
            {card.title ?? "遥测输出"}
          </span>
        </div>
        <div className="vibe-skin-space-telemetry-lines mt-3 rounded-xl border p-2">
          {telemetryItems.slice(0, 6).map((item, index) => (
            <p className="vibe-skin-space-telemetry-line" key={`${item.label}-${index}`}>
              <span className="text-[var(--vibe-muted-text)]">&gt;</span>
              <span>{item.label}</span>
              <span className="ml-auto text-[var(--vibe-accent)]">{item.badge ?? "OK"}</span>
            </p>
          ))}
        </div>
        {card.footer && (
          <p className="mt-2 text-[11px] text-[var(--vibe-muted-text)]">{card.footer}</p>
        )}
      </div>
    );
  }
```

Use the shared `renderSkinTemplateFigure()` for `space-radar`, `space-ship`, and `space-starmap` cards so imported manifests can provide title/subtitle/footer copy and still get safe visuals.

- [ ] **Step 5: Run screen tests**

Run:

```powershell
pnpm test:run tests/VibeScreen.test.tsx
```

Expected: pass after CSS-independent markup exists.

- [ ] **Step 6: Commit rendering changes**

Run:

```powershell
git add src/screens/VibeScreen.tsx tests/VibeScreen.test.tsx
git commit -m "feat: render starship cockpit skin templates"
```

Expected: commit succeeds on `main`.

---

### Task 4: Add Cinematic Cockpit CSS

**Files:**
- Modify: `src/styles.css`
- Test: `tests/VibeScreen.test.tsx`

**Interfaces:**
- Consumes: class names from Task 3 and region CSS variables from existing skin architecture.
- Produces: CSS-only starfield, cockpit canopy/HUD styling, radar sweep, slow spacecraft rotation, telemetry typing/cursor effect, and reduced-motion overrides.

- [ ] **Step 1: Add CSS for starship variant and templates**

Append a dedicated section to `src/styles.css` after the existing skin template styles:

```css
.vibe-skin--starship-cockpit {
  font-family: "Share Tech Mono", "Fira Code", "Cascadia Mono", monospace;
  text-shadow: 0 0 10px rgba(46, 232, 255, 0.18);
}

.vibe-skin--starship-cockpit .vibe-skin-app::before {
  background:
    radial-gradient(circle at 22% 24%, rgba(255, 255, 255, 0.95) 0 1px, transparent 2px),
    radial-gradient(circle at 72% 18%, rgba(163, 251, 255, 0.9) 0 1px, transparent 2px),
    repeating-linear-gradient(112deg, transparent 0 18px, rgba(163, 251, 255, 0.14) 19px, transparent 21px);
  opacity: 0.45;
  animation: vibe-space-starflow 9s linear infinite;
}

.vibe-skin--starship-cockpit .vibe-skin-app::after {
  background:
    radial-gradient(ellipse at 50% -6%, rgba(46, 232, 255, 0.2), transparent 34%),
    radial-gradient(ellipse at 50% 112%, rgba(248, 199, 106, 0.12), transparent 36%),
    linear-gradient(115deg, transparent 0 20%, rgba(46, 232, 255, 0.08) 21% 22%, transparent 23% 78%, rgba(46, 232, 255, 0.08) 79% 80%, transparent 81%);
  opacity: 0.9;
}

.vibe-skin--starship-cockpit .vibe-skin-titlebar {
  clip-path: polygon(0 0, 100% 0, 98.5% 100%, 1.5% 100%);
}

.vibe-skin--starship-cockpit .vibe-skin-titlebar::before,
.vibe-skin--starship-cockpit .vibe-skin-right-card::before,
.vibe-skin--starship-cockpit .vibe-skin-terminal-shell::before {
  background: linear-gradient(90deg, transparent, rgba(163, 251, 255, 0.35), transparent);
}

.vibe-skin-space-card {
  position: relative;
  overflow: hidden;
}

.vibe-skin-space-ai-core,
.vibe-skin-space-ship,
.vibe-skin-space-radar,
.vibe-skin-space-starmap {
  position: relative;
  display: block;
  width: 8.75rem;
  height: 8.75rem;
  overflow: hidden;
  border: 1px solid rgba(46, 232, 255, 0.34);
  border-radius: 2rem;
  background:
    radial-gradient(circle at 50% 42%, rgba(46, 232, 255, 0.24), transparent 3.2rem),
    linear-gradient(180deg, rgba(3, 25, 46, 0.86), rgba(1, 7, 14, 0.72));
  box-shadow: inset 0 0 24px rgba(46, 232, 255, 0.1), 0 16px 30px rgba(0, 0, 0, 0.25);
}

.vibe-skin-space-ai-core {
  width: 100%;
  height: 100%;
  border-radius: 1rem;
}

.vibe-skin-space-ai-core-ring,
.vibe-skin-space-ai-core-eye,
.vibe-skin-space-ai-core-pulse {
  position: absolute;
  inset: 18%;
  border-radius: 999px;
}

.vibe-skin-space-ai-core-ring {
  border: 2px solid rgba(46, 232, 255, 0.72);
  box-shadow: 0 0 18px rgba(46, 232, 255, 0.45), inset 0 0 18px rgba(46, 232, 255, 0.25);
}

.vibe-skin-space-ai-core-eye {
  inset: 34%;
  background: radial-gradient(circle, #ffffff 0 16%, #a3fbff 17% 40%, rgba(46, 232, 255, 0.16) 41% 100%);
}

.vibe-skin-space-ai-core-pulse {
  inset: 8%;
  border: 1px dashed rgba(248, 199, 106, 0.48);
  animation: vibe-space-radar-spin 8s linear infinite;
}

.vibe-skin-space-ship {
  perspective: 640px;
}

.vibe-skin-space-ship-body,
.vibe-skin-space-ship-wing,
.vibe-skin-space-ship-core,
.vibe-skin-space-ship-thruster,
.vibe-skin-space-ship-halo {
  position: absolute;
  left: 50%;
  transform-style: preserve-3d;
}

.vibe-skin-space-ship-halo {
  top: 20%;
  width: 5.8rem;
  height: 5.8rem;
  border: 1px solid rgba(46, 232, 255, 0.26);
  border-radius: 999px;
  transform: translateX(-50%) rotateX(68deg);
}

.vibe-skin-space-ship-body {
  top: 30%;
  width: 1.45rem;
  height: 4.5rem;
  border-radius: 1rem 1rem 0.45rem 0.45rem;
  background: linear-gradient(90deg, #123653, #d9fbff 48%, #2ee8ff 52%, #0a5b75);
  box-shadow: 0 0 22px rgba(46, 232, 255, 0.32);
  transform: translateX(-50%) rotateX(62deg) rotateZ(45deg);
  animation: vibe-space-ship-roll 12s ease-in-out infinite;
}

.vibe-skin-space-ship-wing {
  top: 52%;
  width: 3.1rem;
  height: 1.1rem;
  border-radius: 0.25rem 1rem 1rem 0.25rem;
  background: linear-gradient(90deg, rgba(46, 232, 255, 0.8), rgba(216, 251, 255, 0.95));
  transform-origin: center left;
  animation: vibe-space-ship-roll 12s ease-in-out infinite;
}

.vibe-skin-space-ship-wing-left {
  margin-left: -3.35rem;
  transform: rotateZ(-18deg) skewX(-18deg);
}

.vibe-skin-space-ship-wing-right {
  margin-left: 0.25rem;
  transform: rotateZ(198deg) skewX(-18deg);
}

.vibe-skin-space-ship-core {
  top: 48%;
  width: 0.82rem;
  height: 0.82rem;
  border-radius: 999px;
  background: #f8c76a;
  box-shadow: 0 0 18px rgba(248, 199, 106, 0.8);
  transform: translateX(-50%);
}

.vibe-skin-space-ship-thruster {
  top: 67%;
  width: 0.55rem;
  height: 2.2rem;
  border-radius: 999px;
  background: linear-gradient(180deg, rgba(46, 232, 255, 0.95), transparent);
  filter: blur(0.5px);
  transform: translateX(-50%);
}

.vibe-skin-space-radar-grid,
.vibe-skin-space-radar-sweep,
.vibe-skin-space-radar-blip,
.vibe-skin-space-starmap-orbit,
.vibe-skin-space-starmap-node,
.vibe-skin-space-starmap-route {
  position: absolute;
}

.vibe-skin-space-radar-grid {
  inset: 12%;
  border: 1px solid rgba(46, 232, 255, 0.45);
  border-radius: 999px;
  background:
    linear-gradient(rgba(46, 232, 255, 0.18), rgba(46, 232, 255, 0.18)) 50% 0 / 1px 100% no-repeat,
    linear-gradient(90deg, rgba(46, 232, 255, 0.18), rgba(46, 232, 255, 0.18)) 0 50% / 100% 1px no-repeat,
    radial-gradient(circle, transparent 0 34%, rgba(46, 232, 255, 0.2) 35% 36%, transparent 37% 67%, rgba(46, 232, 255, 0.2) 68% 69%, transparent 70%);
}

.vibe-skin-space-radar-sweep {
  inset: 12%;
  border-radius: 999px;
  background: conic-gradient(from 0deg, rgba(46, 232, 255, 0.56), transparent 78deg);
  animation: vibe-space-radar-spin 4s linear infinite;
}

.vibe-skin-space-radar-blip {
  width: 0.42rem;
  height: 0.42rem;
  border-radius: 999px;
  background: #f8c76a;
  box-shadow: 0 0 12px rgba(248, 199, 106, 0.82);
}

.vibe-skin-space-radar-blip-a {
  top: 32%;
  left: 58%;
}

.vibe-skin-space-radar-blip-b {
  top: 62%;
  left: 36%;
}

.vibe-skin-space-radar-blip-c {
  top: 48%;
  left: 72%;
}

.vibe-skin-space-telemetry-lines {
  background: rgba(1, 9, 18, 0.48);
  border-color: rgba(46, 232, 255, 0.2);
}

.vibe-skin-space-telemetry-line {
  display: flex;
  gap: 0.45rem;
  overflow: hidden;
  white-space: nowrap;
  color: var(--vibe-text);
  font-size: 0.72rem;
  line-height: 1.55;
}

.vibe-skin-space-telemetry-line:last-child::after {
  width: 0.45rem;
  margin-left: 0.15rem;
  color: var(--vibe-accent);
  content: "_";
  animation: vibe-space-cursor 1s steps(2, end) infinite;
}

.vibe-skin-space-starmap-orbit {
  border: 1px solid rgba(46, 232, 255, 0.28);
  border-radius: 999px;
}

.vibe-skin-space-starmap-orbit-a {
  inset: 20% 10%;
  transform: rotate(-18deg);
}

.vibe-skin-space-starmap-orbit-b {
  inset: 28% 18%;
  transform: rotate(24deg);
}

.vibe-skin-space-starmap-node {
  width: 0.58rem;
  height: 0.58rem;
  border-radius: 999px;
  background: #a3fbff;
  box-shadow: 0 0 12px rgba(46, 232, 255, 0.75);
}

.vibe-skin-space-starmap-node-a {
  top: 34%;
  left: 22%;
}

.vibe-skin-space-starmap-node-b {
  top: 52%;
  left: 48%;
  background: #f8c76a;
}

.vibe-skin-space-starmap-node-c {
  top: 28%;
  left: 74%;
}

.vibe-skin-space-starmap-route {
  top: 40%;
  left: 24%;
  width: 4.9rem;
  height: 2.7rem;
  border-top: 1px dashed rgba(248, 199, 106, 0.72);
  border-right: 1px dashed rgba(248, 199, 106, 0.72);
  border-radius: 0 2rem 0 0;
  transform: rotate(-8deg);
}

@keyframes vibe-space-starflow {
  from {
    background-position: 0 0, 0 0, 0 0;
  }
  to {
    background-position: 180px 360px, -220px 260px, 0 320px;
  }
}

@keyframes vibe-space-radar-spin {
  to {
    transform: rotate(360deg);
  }
}

@keyframes vibe-space-ship-roll {
  0%,
  100% {
    filter: drop-shadow(0 0 12px rgba(46, 232, 255, 0.3));
  }
  50% {
    filter: drop-shadow(0 0 24px rgba(248, 199, 106, 0.28));
  }
}

@keyframes vibe-space-cursor {
  50% {
    opacity: 0;
  }
}

@media (prefers-reduced-motion: reduce) {
  .vibe-skin--starship-cockpit .vibe-skin-app::before,
  .vibe-skin-space-ai-core-pulse,
  .vibe-skin-space-radar-sweep,
  .vibe-skin-space-ship-body,
  .vibe-skin-space-ship-wing,
  .vibe-skin-space-telemetry-line:last-child::after {
    animation-duration: 60s;
    animation-iteration-count: 1;
  }
}
```

- [ ] **Step 2: Add CSS behavior assertion**

In `tests/VibeScreen.test.tsx`, extend the starship test from Task 3 with:

```tsx
    expect(document.querySelector(".vibe-skin-space-telemetry-card")).toBeTruthy();
    expect(document.querySelector(".vibe-skin-space-card")).toBeTruthy();
```

- [ ] **Step 3: Run screen tests**

Run:

```powershell
pnpm test:run tests/VibeScreen.test.tsx
```

Expected: pass.

- [ ] **Step 4: Commit CSS changes**

Run:

```powershell
git add src/styles.css tests/VibeScreen.test.tsx
git commit -m "feat: style starship cockpit vibe skin"
```

Expected: commit succeeds on `main`.

---

### Task 5: Document Built-In Skin Package And Verify

**Files:**
- Modify: `docs/vibe-skins.md`
- Test: `pnpm test:run tests/lib/vibeSkin.test.ts tests/VibeScreen.test.tsx`
- Test: `pnpm typecheck`

**Interfaces:**
- Consumes: final implementation from Tasks 1-4.
- Produces: user-facing documentation for copying/modifying the standard starship skin package and selecting safe cockpit templates.

- [ ] **Step 1: Update documentation**

In `docs/vibe-skins.md`, update the built-in package list:

```md
- `src/skins/codex-2007-blue/skin.json`
- `src/skins/rescue-pups-adventure-bay/skin.json`
- `src/skins/starship-cockpit/skin.json`
```

Update the supported decorations sentence:

```md
- `decorations.variant`: optional visual variant class. Supported values are `codex-2007`, `rescue-pups`, and `starship-cockpit`.
- `decorations.avatarTemplate`, `decorations.showcaseTemplate`, and `decorations.rightCards[].template`: safe built-in template names. The cockpit templates are `space-ai-core`, `space-ship`, `space-radar`, `space-telemetry`, and `space-starmap`.
```

- [ ] **Step 2: Run focused tests**

Run:

```powershell
pnpm test:run tests/lib/vibeSkin.test.ts tests/VibeScreen.test.tsx
```

Expected: pass.

- [ ] **Step 3: Run typecheck**

Run:

```powershell
pnpm typecheck
```

Expected: pass.

- [ ] **Step 4: Check worktree**

Run:

```powershell
git status --short
```

Expected: only planned files changed if commits were not made per task, or a clean worktree if each task commit was made.

- [ ] **Step 5: Commit documentation/verification changes**

Run:

```powershell
git add docs/vibe-skins.md
git commit -m "docs: document starship cockpit skin package"
```

Expected: commit succeeds if documentation changed.

---

## Self-Review

- Spec coverage: the plan covers the standard built-in package, safe template whitelist, Chinese visible copy, title/sidebar/workspace/right-rail/taskbar styling, radar, rotating spacecraft, telemetry, fast-space background, terminal transparency, and reduced-motion handling.
- Placeholder scan: the plan contains no `TBD`, `TODO`, or unspecified implementation steps.
- Type consistency: `starship-cockpit`, `space-ai-core`, `space-ship`, `space-radar`, `space-telemetry`, and `space-starmap` are consistently used across manifest, TypeScript unions, tests, render branches, and CSS classes.
