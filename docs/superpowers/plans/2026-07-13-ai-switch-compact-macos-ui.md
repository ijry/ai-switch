# AI Switch Compact macOS UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convert the current large-card UI into a compact macOS-style desktop interface backed by UnoCSS.

**Architecture:** Keep the existing React component boundaries and API contracts. Replace Tailwind integration with UnoCSS at the Vite/style layer, then restyle the app shell and key screens without changing business logic.

**Tech Stack:** React 18, Vite, UnoCSS, Vitest, Tauri/Rust.

## Global Constraints

- Preserve account management, pool, proxy, config write, import, edit, and settings behavior.
- Remove development-facing UI copy from visible screens.
- Keep form labels accessible and visible.
- Use stable hover/focus states without layout-shifting transforms.
- Verify with `pnpm typecheck`, `pnpm test:run`, and Rust formatting.

---

### Task 1: UnoCSS Integration

**Files:**
- Modify: `package.json`
- Modify: `vite.config.ts`
- Modify: `src/styles.css`

**Interfaces:**
- Consumes: Existing Vite React app entry.
- Produces: UnoCSS utility generation for existing class names.

- [ ] Add `unocss` to dev dependencies and install lockfile changes with `pnpm install`.
- [ ] Import `UnoCSS` in `vite.config.ts` and add it before `react()` in the plugin list.
- [ ] Replace Tailwind directives in `src/styles.css` with `@unocss preflight;`, `@unocss default;`, and compact desktop base styles.
- [ ] Run `pnpm typecheck`.

### Task 2: Compact App Shell

**Files:**
- Modify: `src/components/layout/AppLayout.tsx`
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: Existing `activeScreen`, `onNavigate`, and i18n state.
- Produces: A compact shell with top traffic-light area, left navigation, and dense content pane.

- [ ] Replace large header cards with a macOS-style window frame.
- [ ] Remove settings hint copy from the visible sidebar.
- [ ] Keep navigation button text and active behavior unchanged.
- [ ] Replace unimplemented screen copy with a neutral empty state.

### Task 3: Account Screen Density

**Files:**
- Modify: `src/screens/AccountsScreen.tsx`
- Test: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes: Existing route credential API functions and visible labels used by tests.
- Produces: Dense account toolbar, pool controls, compact credential rows, modal, and drawer.

- [ ] Replace the hero panel with a compact toolbar and summary chips.
- [ ] Remove explanatory development copy and keep concise user labels.
- [ ] Tighten account rows to list style with smaller controls.
- [ ] Keep import/edit form labels stable unless tests are updated explicitly.
- [ ] Update tests for changed user-facing status text.

### Task 4: Settings Hub Density

**Files:**
- Modify: `src/screens/SettingsScreen.tsx`
- Test: `tests/SettingsScreen.test.tsx`

**Interfaces:**
- Consumes: Existing settings API and `onOpenFeature`.
- Produces: Compact settings list and app preferences panel.

- [ ] Replace large feature cards with compact rows.
- [ ] Remove long subtitle copy from the visible UI.
- [ ] Keep language and theme controls behavior unchanged.
- [ ] Run settings tests.

### Task 5: Verification and Commit

**Files:**
- Verify all modified UI/config/test files.

**Interfaces:**
- Consumes: Completed Tasks 1-4.
- Produces: Passing checks and a standalone UI commit.

- [ ] Run `pnpm typecheck`.
- [ ] Run `pnpm test:run`.
- [ ] Run `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`.
- [ ] Commit with message `Redesign UI with compact UnoCSS shell`.
