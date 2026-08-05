# Collapsible Sidebar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task with review checkpoints.

**Goal:** Add a desktop-only collapsible left navigation rail and synchronize the account workspace title with its expanded or collapsed state.

**Architecture:** `App` owns a transient `sidebarCollapsed` boolean and passes it to both `AppLayout` and `AccountsScreen`. `AppLayout` changes the desktop grid column between `236px` and `56px`, while `NavButton` renders icon-only controls in the compact state. `AccountsScreen` uses the same prop to switch its top-left title without changing account behavior.

**Tech Stack:** React, TypeScript, Tailwind utility classes, `lucide-react`, Vitest, and Testing Library.

## Global Constraints

- Keep the existing expanded sidebar width at `236px` and the collapsed desktop rail at approximately `56px`.
- Keep a `56px` icon rail below `600px`; allow the full sidebar to remain available on wider compact windows.
- Preserve all existing navigation labels as accessible names and add `title` text for icon-only controls.
- Do not persist the collapse state or add a settings surface for it in this change.
- Leave account workflows, route actions, and non-agent screens behaviorally unchanged.
- Do not commit changes unless the user explicitly requests a commit.

---

### Task 1: Thread Sidebar State Through The Shell

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/components/layout/AppLayout.tsx`
- Create: `src/components/brand/AgentIcon.tsx`
- Test: `tests/AppLayout.test.tsx`

**Interfaces:**
- `AppLayout` consumes `sidebarCollapsed: boolean` and `onToggleSidebar: () => void`.
- `App` produces the shared `sidebarCollapsed` value for `AppLayout` and the agent workspace child.

- [ ] **Step 1: Add failing shell-state tests**

Extend `tests/AppLayout.test.tsx` to render `AppLayout` with `sidebarCollapsed={false}` and an `onToggleSidebar` spy. Assert the toggle has `aria-expanded="true"`, then click it and assert the callback was called. Add a collapsed render asserting `aria-expanded="false"` and that the shell exposes a compact grid class containing `min-[600px]:grid-cols-[56px_minmax(0,1fr)]`.

- [ ] **Step 2: Run the focused test and verify it fails**

Run: `pnpm test:run -- tests/AppLayout.test.tsx`

Expected: FAIL because `AppLayout` does not yet accept the new state props or render the toggle state.

- [ ] **Step 3: Add the shared state in `App`**

In `src/App.tsx`, add `const [sidebarCollapsed, setSidebarCollapsed] = useState(false)`, pass `sidebarCollapsed` and `onToggleSidebar={() => setSidebarCollapsed((value) => !value)}` to `AppLayout`, and pass `sidebarCollapsed` to the `AccountsScreen` render.

- [ ] **Step 4: Update the `AppLayout` contract and grid columns**

Extend `AppLayoutProps` with the two required props. Use the state to choose the desktop grid class while retaining `grid-cols-1` on smaller screens:

```tsx
const desktopGrid = sidebarCollapsed
  ? "min-[600px]:grid-cols-[56px_minmax(0,1fr)]"
  : "min-[600px]:grid-cols-[236px_minmax(0,1fr)]";
```

Keep the existing right-pane padding and account-workspace overflow behavior intact.

- [ ] **Step 5: Run the shell tests and typecheck**

Run: `pnpm test:run -- tests/AppLayout.test.tsx`

Expected: 2 existing AppLayout tests plus the new state tests pass.

Run: `pnpm typecheck`

Expected: TypeScript completes with no errors.

### Task 2: Render The Compact Icon Rail

**Files:**
- Modify: `src/components/layout/AppLayout.tsx`
- Test: `tests/AppLayout.test.tsx`

**Interfaces:**
- `NavButton` consumes `{ label, icon, active, collapsed, onClick, variant }`.
- `NavButton` produces a button with the same accessible label in expanded and collapsed states.

- [ ] **Step 1: Add failing icon-rail assertions**

In `tests/AppLayout.test.tsx`, render a collapsed layout and assert the agent button named `Codex` remains present, has a `title` of `Codex`, and hides its visible text with a `sr-only` class. Assert the section labels and language selector are not visible in the collapsed desktop variant.

- [ ] **Step 2: Run the focused test and verify it fails**

Run: `pnpm test:run -- tests/AppLayout.test.tsx`

Expected: FAIL because navigation buttons currently have no icon prop or collapsed rendering.

- [ ] **Step 3: Add shared platform icon metadata and compact rendering**

Extract the existing platform-specific SVG marks into `src/components/brand/AgentIcon.tsx`, use those marks for each agent `NavButton`, and keep `KeyRound`, `ScanText`, and `Settings2` for system entries. Change `NavButton` so expanded buttons keep the current label layout, while collapsed buttons center the icon, hide the text visually, set `title={label}`, and retain `aria-label={label}` and focus styles.

- [ ] **Step 4: Add the three-line toggle and compact header rules**

Add a `Menu` button to the sidebar header with `aria-label="展开侧栏"` or `aria-label="收起侧栏"`, `aria-expanded={collapsed ? "false" : "true"}`, `title` matching the action, and `onClick={onToggleSidebar}`. Hide nonessential brand text, language controls, and section headings in the compact state; keep the existing Vibe action available as an icon button with its accessible label. Use a base `56px` grid, `min-[600px]:` desktop rules, and `max-[599px]:` compact rules so the rail auto-collapses below `600px`.

- [ ] **Step 5: Run the focused test and typecheck**

Run: `pnpm test:run -- tests/AppLayout.test.tsx`

Expected: all AppLayout tests pass, including accessible icon navigation and toggle behavior.

Run: `pnpm typecheck`

Expected: TypeScript completes with no errors.

### Task 3: Synchronize The Account Workspace Title

**Files:**
- Modify: `src/screens/AccountsScreen.tsx`
- Modify: `src/App.tsx`
- Test: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- `AccountsScreenProps` gains optional `sidebarCollapsed?: boolean` with a default of `false` for existing direct renders.
- `AccountsScreen` consumes the prop and renders one title: `AI Switch` when `true`, otherwise `算力中心`.

- [ ] **Step 1: Add the title-switching test**

Update the test helper to accept an optional `sidebarCollapsed` argument and add a test that renders `AccountsScreen` with `sidebarCollapsed: true`, waits for the toolbar, and asserts `AI Switch` is present while `算力中心` is absent. Keep an expanded assertion in the existing render test.

- [ ] **Step 2: Run the focused test and verify it fails**

Run: `pnpm test:run -- tests/AccountsScreen.test.tsx`

Expected: FAIL because `AccountsScreen` currently always renders `算力中心` and does not accept the prop.

- [ ] **Step 3: Implement the title prop**

Extend `AccountsScreenProps`, destructure `sidebarCollapsed = false`, and replace the toolbar heading with:

```tsx
<h1 className="mt-0.5 text-lg font-semibold leading-tight tracking-tight text-stone-950">
  {sidebarCollapsed ? "AI Switch" : "算力中心"}
</h1>
```

Pass the shared state from `App` when rendering `AccountsScreen`.

- [ ] **Step 4: Run account tests and typecheck**

Run: `pnpm test:run -- tests/AccountsScreen.test.tsx`

Expected: all account tests pass, including the title toggle test.

Run: `pnpm typecheck`

Expected: TypeScript completes with no errors.

### Task 4: Full Verification

**Files:**
- Test: `tests/AppLayout.test.tsx`
- Test: `tests/AccountsScreen.test.tsx`

- [ ] **Step 1: Run the complete test suite**

Run: `pnpm test:run`

Expected: all test files pass with no regressions in non-agent screens.

- [ ] **Step 2: Run the production build**

Run: `pnpm build`

Expected: Vite completes successfully; existing OCRAD bundling warnings may remain non-fatal.

- [ ] **Step 3: Check the final diff**

Run: `git diff --check`

Expected: no whitespace errors.
