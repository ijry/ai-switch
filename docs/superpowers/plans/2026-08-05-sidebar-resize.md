# Sidebar Resize Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add pointer-based desktop sidebar resizing with a 216px default, 180–320px bounds, and persisted user width.

**Architecture:** Adapt the shared `otools-git` pointer lifecycle into a small React `useDragResize` hook. Keep width state in `AppLayout`, render the dynamic grid column through an inline style, and expose a narrow resize handle only for the expanded desktop sidebar. Persist only the clamped width value in local storage.

**Tech Stack:** React 18, TypeScript, Tailwind utility classes, Vitest + Testing Library, browser Pointer Events and `localStorage`.

## Global Constraints

- Default expanded width is exactly `216px` (current `236px` minus 20px).
- Expanded width is clamped to `180px` minimum and `320px` maximum.
- The existing `600px` responsive breakpoint and 56px icon-only collapsed state remain unchanged.
- Do not add a third-party split-pane dependency.
- Do not commit changes unless the user explicitly requests a commit.

---

### Task 1: Add the reusable drag-resize hook

**Files:**
- Create: `src/lib/useDragResize.ts`
- Test: `tests/useDragResize.test.tsx`

**Interfaces:**
- Produces `useDragResize(options)` returning `{ dragging: boolean, startDragging(event: PointerEvent): void }`.
- `options.axis` is `"x" | "y"`; `min` and `max` are numbers or zero-argument functions; `getInitialValue()` returns the starting number; `onChange(value)` receives a clamped value; optional `getValueFromPointer(event, state)` supports absolute positioning; optional `onStart`, `onEnd`, and `cursor` customize lifecycle behavior.

- [x] **Step 1: Write the failing hook tests**

Test `tests/useDragResize.test.tsx` with a small probe component that starts a horizontal drag from a button and exposes the current value. Cover:

```ts
fireEvent.pointerDown(handle, { button: 0, clientX: 100, pointerId: 7 });
fireEvent.pointerMove(document, { clientX: 160, pointerId: 7 });
expect(value).toHaveTextContent("160");
fireEvent.pointerUp(document, { pointerId: 7 });
```

Also assert that values below `min` and above `max` clamp, non-primary buttons do nothing, and unmounting after `pointerdown` removes the active drag state.

- [x] **Step 2: Run the focused test and verify it fails**

Run `pnpm vitest run tests/useDragResize.test.tsx`. It should fail because the hook does not exist yet.

- [x] **Step 3: Implement the pointer lifecycle**

In `src/lib/useDragResize.ts`, mirror the external utility's behavior:

- Keep a mutable drag state with `startX`, `startY`, and `startValue`.
- Ignore non-left-button starts.
- Set `dragging` true, set `document.body.style.cursor`, register `pointermove`, `pointerup`, `pointercancel`, and capture-loss listeners, and call `setPointerCapture` when available.
- Compute delta values (or use `getValueFromPointer`), clamp to resolved limits, and call `onChange`.
- On stop or unmount, restore the previous cursor, remove every listener, release pointer capture defensively, clear active target/id, set `dragging` false, and call `onEnd`.

- [x] **Step 4: Run the focused hook tests**

Run `pnpm vitest run tests/useDragResize.test.tsx`; all hook lifecycle and clamp assertions must pass.

### Task 2: Wire resizing into `AppLayout`

**Files:**
- Modify: `src/components/layout/AppLayout.tsx`
- Modify: `tests/AppLayout.test.tsx`

**Interfaces:**
- Consumes `useDragResize` from `src/lib/useDragResize.ts`.
- Produces a `data-testid="sidebar-resize-handle"` element with `aria-label="调整侧栏宽度"` in the expanded desktop layout.

- [x] **Step 1: Extend `AppLayout` state and storage helpers**

Add constants:

```ts
const SIDEBAR_DEFAULT_WIDTH = 216;
const SIDEBAR_MIN_WIDTH = 180;
const SIDEBAR_MAX_WIDTH = 320;
const SIDEBAR_WIDTH_STORAGE_KEY = "ai-switch.sidebar-width";
```

Initialize width from `localStorage`, accepting only finite numeric values and clamping them. Guard browser storage access with `typeof window !== "undefined"` and catch storage errors. Save the clamped width in an effect whenever it changes.

- [x] **Step 2: Use an inline dynamic grid column**

Replace the expanded `min-[600px]:grid-cols-[236px_minmax(0,1fr)]` class with an inline `gridTemplateColumns` value:

```ts
style={{
  gridTemplateColumns: `${sidebarCollapsed ? 56 : sidebarWidth}px minmax(0, 1fr)`,
}}
```

Keep the base/mobile grid class and existing collapsed class assertions compatible by retaining the 56px fallback class.

- [x] **Step 3: Add the resize handle and hook wiring**

Call `useDragResize` with `axis: "x"`, `min: 180`, `max: 320`, `getInitialValue: () => sidebarWidth`, and a `getValueFromPointer` implementation based on `app-shell.getBoundingClientRect().left`. Render a `button` or `div` on the sidebar's right edge with `data-testid`, `aria-label`, `role="separator"`, `aria-orientation="vertical"`, `aria-valuemin`, `aria-valuemax`, and `aria-valuenow`. Hide it for `sidebarCollapsed` and below-600px CSS. Give it `touch-action-none`, `cursor-col-resize`, and visible hover/drag feedback without changing layout width.

- [x] **Step 4: Add layout tests before running the full suite**

Extend `tests/AppLayout.test.tsx` to assert:

- no storage value renders the `216px` grid template;
- dragging the handle updates `gridTemplateColumns` and clamps at `180px` and `320px`;
- `localStorage` value `245` restores as `245px` on a fresh render;
- collapsed rendering still uses `56px` and hides the resize handle.

- [x] **Step 5: Run the focused layout tests**

Run `pnpm vitest run tests/AppLayout.test.tsx`; all existing navigation/collapse assertions and new resize assertions must pass.

### Task 3: Validate integration and polish

**Files:**
- Modify: `src/components/layout/AppLayout.tsx` only if focused tests expose responsive or accessibility regressions.

- [x] **Step 1: Run typecheck and the complete frontend test suite**

Run `pnpm typecheck` and `pnpm vitest run`. Confirm no TypeScript errors and no regressions outside the layout tests.

- [x] **Step 2: Run the production build**

Run `pnpm build` and confirm Vite emits a successful production bundle.

- [x] **Step 3: Inspect the final diff**

Run `git diff --check` and `git status --short`. Ensure only the hook, layout/tests, and the approved design/plan documents are changed; do not commit.
