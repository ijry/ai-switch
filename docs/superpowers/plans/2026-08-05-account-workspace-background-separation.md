# Account Workspace Background Separation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the large white account workspace surface and visually separate the filter toolbar from the account/request lists while preserving the fixed desktop layout.

**Architecture:** Keep the existing three-row workspace grid and scroll ownership. Make the workspace and scroll region transparent, retain compact toolbar/status surfaces, and give the filter area and each content section their own local background/border treatment.

**Tech Stack:** React, TypeScript, Tailwind utility classes, Vitest Testing Library.

## Global Constraints

- Keep the workspace structure as toolbar, scroll region, and status bar.
- Do not move pagination or account actions to another region.
- Preserve individual row/card readability on the existing page background.
- Avoid changing route credential data flow or API contracts.

---

### Task 1: Split Workspace Surfaces

**Files:**
- Modify: `src/screens/AccountsScreen.tsx:2460-3375`

**Interfaces:**
- Consumes: existing `account-workspace`, filter section, statistics section, and account list markup.
- Produces: transparent outer workspace/scroll surface with independent filter and list sections.

- [ ] **Step 1: Remove the large outer white surface**

  Change the workspace and scroll-region classes so the page base color shows through; keep the toolbar and status bar as compact fixed surfaces.

- [ ] **Step 2: Separate the filter toolbar**

  Remove the list-sharing white section background and give the sticky filter row its own subtle surface, padding, border, and spacing.

- [ ] **Step 3: Preserve local content surfaces**

  Keep request tables, statistic cards, empty/loading states, and account rows readable with their own local backgrounds; do not reintroduce a parent white fill.

### Task 2: Add Layout Regression Coverage

**Files:**
- Modify: `tests/AccountsScreen.test.tsx:400-430`

**Interfaces:**
- Consumes: `data-testid="account-workspace"`, `data-testid="account-workspace-toolbar"`, and `data-testid="account-workspace-status-bar"`.
- Produces: assertions that the workspace remains a three-row desktop shell while the scroll region does not become a single white panel.

- [ ] **Step 1: Assert the fixed workspace regions**

  Verify the toolbar, scroll region, and status bar remain direct children of the workspace.

- [ ] **Step 2: Run the focused screen tests**

  Run `pnpm test:run -- tests/AccountsScreen.test.tsx` and confirm all AccountsScreen tests pass.

### Task 3: Verify Build and Diff

**Files:**
- Modify: none

- [ ] **Step 1: Run typecheck and build**

  Run `pnpm typecheck` and `pnpm build`.

- [ ] **Step 2: Run the full test suite**

  Run `pnpm test:run` and confirm all test files pass.

- [ ] **Step 3: Check patch whitespace**

  Run `git diff --check` and inspect the final diff for unrelated changes.
