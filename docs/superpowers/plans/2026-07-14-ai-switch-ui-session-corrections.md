# AI Switch UI Session Corrections Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Correct the desktop shell, Settings hub, and session-management entry points so the installed app matches the requested product shape.

**Architecture:** Keep all changes on `main`. Reuse the existing Tauri updater plugin and local session scanner; improve the React screens and layout without merging the old `provider-switching-b1` branch.

**Tech Stack:** Tauri 2, React 18, TypeScript, React Query, UnoCSS/Tailwind utility classes, lucide-react.

## Global Constraints

- Work directly on `main` unless the user explicitly asks for a branch or worktree.
- Do not merge `provider-switching-b1`; use it only as reference because it contains unwanted bulk/instance/wakeup/sync scope.
- Keep Settings entries limited to Sessions, Updates, and Log.
- Use Tauri official updater plugin for update checking and installation.
- Do not add fake macOS window chrome inside the app.

---

### Task 1: Shell And Settings Cleanup

**Files:**
- Modify: `src/components/layout/AppLayout.tsx`
- Modify: `src/screens/SettingsScreen.tsx`
- Modify: `src/lib/i18n.tsx`
- Test: `pnpm typecheck`

**Interfaces:**
- Consumes: existing `activeScreen`, `onNavigate`, and i18n keys.
- Produces: a normal app shell without fake macOS controls and a Settings hub with only Sessions, Updates, and Log.

- [ ] Remove the fake red/yellow/green titlebar from `AppLayout`.
- [ ] Change the sidebar to a light translucent gradient with `backdrop-blur`.
- [ ] Remove unused Settings feature entries and type keys for Dashboard/Library/Routing/MCP/Instances/Wakeups/Bulk/Sync.
- [ ] Run `pnpm typecheck` and expect success.

### Task 2: Agent Tab Session Entry

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/screens/AccountsScreen.tsx`
- Test: `pnpm typecheck`

**Interfaces:**
- Consumes: `AccountsScreen` `platform` prop and `setScreen` navigation from `App`.
- Produces: each agent tab can open the Sessions screen pre-filtered to that agent.

- [ ] Add an optional `onOpenSessions(platform)` prop to `AccountsScreen`.
- [ ] Add a visible "Session management" entry near the current agent heading/action area.
- [ ] Store a pending session platform in `App` and pass it to `SessionsScreen`.
- [ ] Run `pnpm typecheck` and expect success.

### Task 3: Session Manager Upgrade

**Files:**
- Modify: `src/screens/SessionsScreen.tsx`
- Test: `pnpm typecheck`

**Interfaces:**
- Consumes: `listSessions(platform)` and `getSessionMessages({ providerId, sourcePath })`.
- Produces: a cc-switch-inspired local session manager with provider filtering, directory grouping, content search, copy resume, and copy directory.

- [ ] Add an `initialPlatform` prop and sync it into the provider filter.
- [ ] Add flat/grouped list mode grouped by provider and project directory.
- [ ] Add copy project directory and clearer resume command actions.
- [ ] Add in-session message search and quick message navigation.
- [ ] Run `pnpm typecheck` and expect success.

### Task 4: Verification And Packaging

**Files:**
- Build outputs only.

**Interfaces:**
- Produces: a verified local build and install package.

- [ ] Run `pnpm typecheck`.
- [ ] Run `pnpm build`.
- [ ] Run `pnpm rust:check`.
- [ ] Run `pnpm tauri:build`.
- [ ] Commit code changes with a focused message.
