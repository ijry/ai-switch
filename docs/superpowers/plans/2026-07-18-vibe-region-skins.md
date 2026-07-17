# Vibe Region Skins Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build region-level Vibe skin customization so skins can style specific UI areas and optional showcase content without obscuring terminal output.

**Architecture:** Keep skin parsing and CSS variable generation in `src/lib/vibeSkin.ts`. Keep Vibe skin layout wiring in `src/screens/VibeScreen.tsx`. Keep CSS variable consumers in `src/styles.css`, with dark/light theme paths unchanged.

**Tech Stack:** React 18, TypeScript, CSS custom properties, JSZip, Vitest, Testing Library.

## Global Constraints

- Work directly on `main`; do not create a branch or worktree.
- Skin mode only changes Vibe's `skin` theme path.
- Dark and light Vibe themes must keep their current layout and class behavior.
- Terminal text must remain readable; decorative region backgrounds apply to the terminal shell, not over `XtermPane` content.
- Imported zip assets may be used by `ui.backgroundImage`, `regions.*.backgroundImage`, and `showcase.image`.

---

## File Structure

- Modify `src/lib/vibeSkin.ts`: manifest types, region/showcase normalization, asset resolution, built-in skin, CSS variables.
- Modify `src/screens/VibeScreen.tsx`: skin-only layout regions, right showcase rail, terminal shell wrapper.
- Modify `src/styles.css`: CSS custom property consumers for each skin region.
- Modify `tests/lib/vibeSkin.test.ts`: region import, storage normalization, CSS variables.
- Modify `tests/VibeScreen.test.tsx`: skin-mode region/showcase rendering.
- Modify `docs/vibe-skins.md`: public skin manifest schema and zip examples.

### Task 1: Finish Region Skin Model And CSS Mapping

**Files:**
- Modify: `src/lib/vibeSkin.ts`
- Modify: `src/screens/VibeScreen.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- Consumes: `VibeSkinDefinition`, `importVibeSkinPackage(file: File)`, `skinToCssVariables(skin: VibeSkinDefinition): CSSProperties`
- Produces: `regions?: Partial<Record<VibeSkinRegionKey, VibeSkinRegionStyle>>`, `showcase?: VibeSkinShowcase`, CSS variables named `--vibe-<region>-<property>`

- [x] **Step 1: Check existing region key usage**

Run: `rg "vibe-skin-(control-panel|toolbar|terminal-shell|right-rail|status-bar)|VIBE_SKIN_REGION_KEYS" src tests docs`

Expected: Find all current region names that need matching between TypeScript keys and CSS variable names.

- [x] **Step 2: Align `controlPanel` CSS mapping**

Change `.vibe-skin-control-panel` in `src/styles.css` to consume `--vibe-control-panel-*` variables, while falling back to `--vibe-toolbar-*` for compatibility:

```css
.vibe-skin-control-panel {
  background: var(--vibe-control-panel-background-layer, var(--vibe-toolbar-background-layer, var(--vibe-panel-subtle)));
  background-position: var(--vibe-control-panel-background-position, var(--vibe-toolbar-background-position, center));
  background-repeat: var(--vibe-control-panel-background-repeat, var(--vibe-toolbar-background-repeat, no-repeat));
  background-size: var(--vibe-control-panel-background-size, var(--vibe-toolbar-background-size, cover));
  border-color: var(--vibe-control-panel-border, var(--vibe-toolbar-border, var(--vibe-border)));
  color: var(--vibe-control-panel-color, var(--vibe-toolbar-color, var(--vibe-text)));
  box-shadow: var(--vibe-control-panel-shadow, var(--vibe-toolbar-shadow));
  backdrop-filter: var(--vibe-control-panel-backdrop-filter, var(--vibe-toolbar-backdrop-filter));
  padding: var(--vibe-control-panel-padding, var(--vibe-toolbar-padding));
}
```

- [x] **Step 3: Keep terminal pane transparent only inside the skin shell**

Verify `src/styles.css` contains:

```css
.vibe-skin-terminal-shell .xterm,
.vibe-skin-terminal-shell .xterm-screen,
.vibe-skin-terminal-shell .xterm-viewport {
  background: transparent;
}
```

Expected: Skin backgrounds apply to `.vibe-skin-terminal-shell`; xterm content uses transparent canvas/viewport so the configured terminal theme and shell stay visible.

- [x] **Step 4: Run typecheck**

Run: `pnpm typecheck`

Expected: PASS.

### Task 2: Add Tests For Region Skins

**Files:**
- Modify: `tests/lib/vibeSkin.test.ts`
- Modify: `tests/VibeScreen.test.tsx`

**Interfaces:**
- Consumes: `importVibeSkinPackage`, `writeStoredVibeSkin`, `readStoredVibeSkin`, `skinToCssVariables`, `VIBE_SKIN_STORAGE_KEY`
- Produces: regression coverage for region asset imports, CSS variables, showcase rendering, and terminal theme override.

- [x] **Step 1: Add region asset import assertions**

Extend the existing zip import test with:

```ts
regions: {
  terminalShell: {
    backgroundImage: "assets/shell.png",
    backgroundPosition: "center top",
    borderRadius: "18px",
  },
},
showcase: {
  enabled: true,
  image: "assets/showcase.png",
},
```

Add zip entries:

```ts
zip.file("assets/shell.png", new Uint8Array([137, 80, 78, 71]));
zip.file("assets/showcase.png", new Uint8Array([137, 80, 78, 71]));
```

Assert:

```ts
expect(skin.regions?.terminalShell?.backgroundImage).toMatch(/^data:image\/png;base64,/);
expect(skin.regions?.terminalShell?.backgroundPosition).toBe("center top");
expect(skin.regions?.terminalShell?.borderRadius).toBe("18px");
expect(skin.showcase?.image).toMatch(/^data:image\/png;base64,/);
```

- [x] **Step 2: Add CSS variable assertions for stored region skins**

Extend the stored skin object with:

```ts
regions: {
  controlPanel: {
    background: "linear-gradient(#102030, #405060)",
    shadow: "0 0 20px rgba(0,255,238,0.4)",
  },
  terminalShell: {
    background: "#010203",
    borderRadius: "20px",
  },
},
showcase: {
  enabled: true,
  title: "Stored Showcase",
},
```

Assert:

```ts
const variables = skin ? (skinToCssVariables(skin) as Record<string, unknown>) : {};
expect(variables["--vibe-control-panel-background-layer"]).toBe("linear-gradient(#102030, #405060)");
expect(variables["--vibe-control-panel-shadow"]).toBe("0 0 20px rgba(0,255,238,0.4)");
expect(variables["--vibe-terminal-shell-border-radius"]).toBe("20px");
expect(skin?.showcase?.title).toBe("Stored Showcase");
```

- [x] **Step 3: Add Vibe showcase rendering test**

In `tests/VibeScreen.test.tsx`, store a custom skin before rendering:

```ts
window.localStorage.setItem(
  VIBE_SKIN_STORAGE_KEY,
  JSON.stringify({
    id: "showcase-skin",
    name: "Showcase Skin",
    ui: {
      accent: "#00ffee",
      background: "#001018",
      backgroundOverlay: "transparent",
      panel: "rgba(2,28,40,0.78)",
      panelStrong: "rgba(4,42,58,0.92)",
      panelSubtle: "rgba(0,255,238,0.12)",
      border: "rgba(0,255,238,0.35)",
      text: "#f4fbff",
      mutedText: "#9be7ff",
      button: "#00ffee",
      buttonText: "#001018",
      buttonHover: "#54fff5",
      dangerBackground: "#b91c1c",
      dangerText: "#ffffff",
      tabBar: "rgba(2,28,40,0.72)",
      tabActive: "rgba(0,255,238,0.22)",
      tabInactive: "rgba(2,28,40,0.42)",
      tabHover: "rgba(0,255,238,0.18)",
      focus: "#00ffee",
    },
    regions: {
      rightRail: { background: "#123456" },
      terminalShell: { background: "#010203" },
    },
    showcase: {
      enabled: true,
      title: "Right Rail Demo",
      badge: "Custom Rail",
      footer: "region keys",
    },
  }),
);
```

Click the theme button twice to enter skin mode and assert:

```ts
expect(screen.getByText("Right Rail Demo")).toBeInTheDocument();
expect(screen.getByText("Custom Rail")).toBeInTheDocument();
expect(screen.getByText("rightRail")).toBeInTheDocument();
expect(screen.getByText("terminalShell")).toBeInTheDocument();
expect(screen.getByText("region keys")).toBeInTheDocument();
```

- [x] **Step 4: Run targeted tests**

Run: `pnpm test:run tests/lib/vibeSkin.test.ts tests/VibeScreen.test.tsx`

Expected: PASS.

### Task 3: Update Skin Documentation And Final Verification

**Files:**
- Modify: `docs/vibe-skins.md`

**Interfaces:**
- Consumes: public schema from `src/lib/vibeSkin.ts`
- Produces: docs that tell users how to create `.json`, `.aiskin`, and `.zip` skins with region backgrounds and showcase metadata.

- [x] **Step 1: Replace background-only description**

Update `docs/vibe-skins.md` to explain that the built-in `Codex 2007 Blue` skin is QQ2007-inspired chrome, not a direct background image.

- [x] **Step 2: Add region manifest example**

Add this manifest shape to the docs:

```json
{
  "regions": {
    "titlebar": {
      "background": "linear-gradient(180deg, #e7fbff, #0f6bc4)",
      "border": "rgba(5, 82, 150, 0.65)",
      "color": "#ffffff"
    },
    "terminalShell": {
      "backgroundImage": "assets/terminal-shell.png",
      "backgroundSize": "cover",
      "borderRadius": "16px"
    },
    "rightRail": {
      "background": "linear-gradient(180deg, #d8f3ff, #4a9dde)"
    }
  },
  "showcase": {
    "enabled": true,
    "title": "My Display Area",
    "badge": "Custom Skin",
    "image": "assets/avatar.png",
    "footer": "made for Vibe"
  }
}
```

- [x] **Step 3: Run full frontend verification**

Run: `pnpm typecheck`

Expected: PASS.

Run: `pnpm test:run`

Expected: PASS.

- [x] **Step 4: Commit verified implementation**

Run:

```powershell
git status --short
git add src/lib/vibeSkin.ts src/screens/VibeScreen.tsx src/styles.css tests/lib/vibeSkin.test.ts tests/VibeScreen.test.tsx docs/vibe-skins.md docs/superpowers/plans/2026-07-18-vibe-region-skins.md
git commit -m "feat: support region-based vibe skins"
```

Expected: commit succeeds with only implementation, tests, docs, and this plan.

## Self Review

- Spec coverage: Task 1 covers skin schema, layout regions, CSS variables, right rail, and terminal isolation. Task 2 covers import/storage/rendering tests. Task 3 covers user documentation and final verification.
- Placeholder scan: No incomplete markers or ambiguous steps remain.
- Type consistency: Region names match `VibeSkinRegionKey`, and generated CSS variables use the `--vibe-<region>-<property>` convention.
