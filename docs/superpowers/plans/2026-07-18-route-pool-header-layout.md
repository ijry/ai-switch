# Route Pool Header Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reflow the route pool header so the icon is leftmost, the middle content has two lines, and action buttons remain on the right.

**Architecture:** Keep the existing `AccountsScreen` route-pool card and all actions. Change only the JSX/classes for the header content and add a test assertion that verifies the second-line status is plain text rather than pill-styled status badges.

**Tech Stack:** React 18, TypeScript, Tailwind/Uno utility classes, Testing Library, Vitest.

## Global Constraints

- Work directly on `main` by default. Do not create or switch to feature branches/worktrees unless the user explicitly asks for a separate branch, worktree, or isolation.
- No behavior changes.
- No new data fields.
- No redesign of the statistics panel.
- The route-pool icon is the leftmost item.
- The middle area has two lines: pool/member info first, proxy/recent-route status second.
- Proxy and recent-route status are plain text, not background pills.
- Existing route pool controls continue to work.

---

## File Structure

- `src/screens/AccountsScreen.tsx`: update only the route pool header JSX and utility classes.
- `tests/AccountsScreen.test.tsx`: add a focused assertion around the route pool header status text.

---

### Task 1: Reflow Route Pool Header

**Files:**
- Modify: `src/screens/AccountsScreen.tsx`
- Modify: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes: existing `routeProxyQuery.data?.running`, `routeProxyQuery.data?.base_url`, `draftPoolIds.size`, and `lastRouteAccount` render state.
- Produces: route pool header with left icon, center two-line content, right action buttons.

- [ ] **Step 1: Add the failing layout assertion**

In `tests/AccountsScreen.test.tsx`, update the `starts proxy, writes configs, and tests the credential pool route` test after `expect(screen.getByText("最近路由到：Team Account")).toBeInTheDocument();`:

```ts
    const proxyStatus = screen.getByText("本地代理：http://127.0.0.1:43111");
    const recentRouteStatus = screen.getByText("最近路由到：Team Account");
    expect(proxyStatus.className).not.toContain("bg-white");
    expect(recentRouteStatus.className).not.toContain("bg-white");
```

- [ ] **Step 2: Run the focused failing test**

Run:

```powershell
pnpm test:run -- AccountsScreen.test.tsx -t "starts proxy, writes configs, and tests the credential pool route"
```

Expected: FAIL because the proxy and recent-route status are currently rendered as pill spans with `bg-white/90`.

- [ ] **Step 3: Update the header markup**

In `src/screens/AccountsScreen.tsx`, replace the route pool card header block from the opening:

```tsx
        <div className="mx-4 mt-3 rounded-2xl border border-emerald-200 bg-gradient-to-r from-emerald-50 to-white px-3 py-2.5">
```

through the closing `</div>` just before `{configWriteOutcomes.length > 0 && (` with this JSX:

```tsx
        <div className="mx-4 mt-3 rounded-2xl border border-emerald-200 bg-gradient-to-r from-emerald-50 to-white px-3 py-2.5">
          <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
            <div className="flex min-w-0 flex-1 items-start gap-2">
              <span className="grid h-9 w-9 shrink-0 place-items-center rounded-xl bg-emerald-600 text-white shadow-sm">
                <KeyRound className="h-4 w-4" />
              </span>
              <div className="min-w-0 flex-1 space-y-1">
                <div className="flex min-w-0 flex-wrap items-center gap-2">
                  <span className="inline-flex shrink-0 items-center gap-1.5 rounded-xl border border-emerald-200 bg-white/90 px-2.5 py-1.5 text-[12px] font-semibold text-emerald-900">
                    算力池
                  </span>
                  <span className="text-[12px] font-medium text-stone-600">
                    已加入 {draftPoolIds.size} 个账号
                  </span>
                </div>
                <div className="flex min-w-0 flex-wrap gap-x-4 gap-y-1 text-[12px] font-medium text-stone-500">
                  <span className="min-w-0 break-all">
                    本地代理：{routeProxyQuery.data?.running ? routeProxyQuery.data.base_url ?? "运行中" : "未启动"}
                  </span>
                  {lastRouteAccount && (
                    <span className="min-w-0 break-all">
                      最近路由到：{lastRouteAccount}
                    </span>
                  )}
                </div>
              </div>
            </div>

            <div className="flex shrink-0 flex-wrap items-center gap-2 lg:flex-nowrap">
              <button
                aria-label={routeProxyQuery.data?.running ? "停止本地路由代理" : "启动本地路由代理"}
                className="inline-flex items-center justify-center gap-1.5 rounded-xl border border-emerald-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-800 transition-colors hover:bg-emerald-50 disabled:opacity-50"
                disabled={startProxyMutation.isPending || stopProxyMutation.isPending}
                onClick={() => {
                  if (routeProxyQuery.data?.running) {
                    stopProxyMutation.mutate();
                  } else {
                    startProxyMutation.mutate();
                  }
                }}
                type="button"
              >
                {routeProxyQuery.data?.running ? <PowerOff className="h-3.5 w-3.5" /> : <Power className="h-3.5 w-3.5" />}
                {routeProxyQuery.data?.running ? "停止代理" : "启动代理"}
              </button>
              <button
                aria-label="写入路由配置文件"
                className="inline-flex items-center justify-center gap-1.5 rounded-xl border border-emerald-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-800 transition-colors hover:bg-emerald-50 disabled:opacity-50"
                disabled={!routeProxyQuery.data?.running || writeConfigsMutation.isPending}
                onClick={() => writeConfigsMutation.mutate()}
                type="button"
              >
                <FileCode2 className="h-3.5 w-3.5" />
                写入配置
              </button>
              <button
                aria-label="测试算力池路由"
                className="inline-flex items-center justify-center gap-1.5 rounded-xl bg-emerald-700 px-3 py-2 text-[13px] font-semibold text-white transition-colors hover:bg-emerald-800 disabled:opacity-50"
                disabled={draftPoolIds.size === 0 || routeOnceMutation.isPending}
                onClick={testRoute}
                type="button"
              >
                <Play className="h-3.5 w-3.5" />
                测试路由
              </button>
              <button
                aria-label="查看算力池统计"
                className="inline-flex items-center justify-center gap-1.5 rounded-xl border border-emerald-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-800 transition-colors hover:bg-emerald-50"
                onClick={toggleStatsPanel}
                type="button"
              >
                <BarChart3 className="h-3.5 w-3.5" />
                统计
              </button>
            </div>
          </div>
        </div>
```

- [ ] **Step 4: Run focused test**

Run:

```powershell
pnpm test:run -- AccountsScreen.test.tsx -t "starts proxy, writes configs, and tests the credential pool route"
```

Expected: PASS.

- [ ] **Step 5: Run final checks**

Run:

```powershell
pnpm test:run -- AccountsScreen.test.tsx
pnpm typecheck
```

Expected: both commands PASS.

- [ ] **Step 6: Commit**

Run:

```powershell
git add src/screens/AccountsScreen.tsx tests/AccountsScreen.test.tsx
git commit -m "fix: reflow route pool header status"
```
