# Route Request Detail View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an expandable, metadata-only detail view for route pool request records and make manual test-route records easier to inspect.

**Architecture:** Keep the existing `getRoutePool` data flow and `usage_events.metadata_json` storage model. The UI parses metadata locally for compact row fields and expanded JSON detail content; test-route writes richer safe metadata through the existing `routePoolRouteOnce` call.

**Tech Stack:** React 18, TypeScript, TanStack Query, Vitest, Testing Library, Tauri IPC, Rust backend with existing SQLite `usage_events` table.

## Global Constraints

- Work directly on `main` by default. Do not create or switch to feature branches/worktrees unless the user explicitly asks for a separate branch, worktree, or isolation.
- Do not store request bodies, response bodies, prompts, completion text, API keys, or authorization headers.
- Do not change the `usage_events` schema.
- Do not add a separate detail API while the existing route pool response already carries the required row data.
- Do not attempt full HAR-style proxy logging in this change.
- Test-route metadata must include `source`, `path`, `status`, and `request_kind`.
- UI copy stays Chinese where the surrounding route pool UI is Chinese.

---

## File Structure

- Modify `src/screens/AccountsScreen.tsx`: extend metadata parsing, track the expanded request row, render the detail panel, and enrich test-route metadata.
- Modify `tests/AccountsScreen.test.tsx`: assert the expanded detail behavior and the richer test-route metadata payload.
- No backend file changes are required because the existing route-pool API already returns request rows with `metadata_json`.
- No type changes are required because `RoutePoolUsageLog.metadata_json` already exists in `src/lib/api/types.ts`.

### Task 1: Add Failing Frontend Coverage

**Files:**
- Modify: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes: `statsFixture(overrides: Partial<RoutePoolStats>): RoutePoolStats`, mocked `getRoutePool`, mocked `routePoolRouteOnce`, and the existing `AccountsScreen`.
- Produces: Failing expectations for `查看请求 <id> 详情`, expanded metadata rendering, invalid metadata fallback, and richer test-route metadata.

- [ ] **Step 1: Update the request statistics test fixture and assertions**

Replace the body of the existing `it("renders filtered route request statistics and paginates request rows", async () => { ... })` test with this version:

```tsx
it("renders filtered route request statistics, expands request details, and paginates request rows", async () => {
  const expectedMonthStart = new Date();
  expectedMonthStart.setHours(0, 0, 0, 0);
  expectedMonthStart.setDate(1);

  vi.mocked(getRoutePool).mockImplementation(
    async (platform, since, requestPage = 1, requestPageSize = 20) => ({
      platform,
      account_ids: ["cred-official-1"],
      stats: statsFixture({
        member_count: 1,
        request_count: 99,
        token_count: 2048,
        cost_micros: 1500,
        request_row_count: 42,
        request_page: requestPage ?? 1,
        request_page_size: requestPageSize ?? 20,
        requests: [
          {
            id: "request-success",
            account_id: "cred-official-1",
            account_name: "Team Account",
            source_label: "route_proxy",
            metric_type: "request",
            amount: 1,
            unit: "count",
            metadata_json: JSON.stringify({
              platform: "codex",
              route_credential_id: "cred-official-1",
              route_credential_name: "Team Account",
              path: "/v1/responses",
              status: 201,
            }),
            created_at: "2026-07-17T08:00:00Z",
          },
          {
            id: "request-invalid-metadata",
            account_id: "cred-api-1",
            account_name: "Broken Metadata Account",
            source_label: "route_proxy",
            metric_type: "request",
            amount: 1,
            unit: "count",
            metadata_json: "{bad json",
            created_at: "2026-07-17T08:01:00Z",
          },
        ],
      }),
    }),
  );

  renderScreen();

  await userEvent.click(await screen.findByLabelText("查看算力池统计"));

  expect(await screen.findByText("请求统计")).toBeInTheDocument();
  expect(screen.getByText("统计当前 Codex 的历史路由请求")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "当日" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "本周" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "本月" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "累计" })).toBeInTheDocument();
  expect(screen.getByText("共 42 条 · 每页 20 条")).toBeInTheDocument();
  expect(screen.getByText("第 1 / 3 页")).toBeInTheDocument();
  expect(screen.getByText("/v1/responses")).toBeInTheDocument();
  expect(screen.getByText("201")).toBeInTheDocument();
  expect(screen.getAllByText("route_proxy")).toHaveLength(2);
  expect(screen.getByLabelText("查看请求 request-success 详情")).toBeInTheDocument();

  const invalidMetadataRow = screen.getByText("Broken Metadata Account").closest("[data-route-request-row]");
  expect(invalidMetadataRow).not.toBeNull();
  expect(within(invalidMetadataRow as HTMLElement).getAllByText("-")).toHaveLength(2);

  await userEvent.click(screen.getByLabelText("查看请求 request-success 详情"));

  const successDetail = await screen.findByLabelText("请求 request-success 详情");
  expect(within(successDetail).getByText("请求详情")).toBeInTheDocument();
  expect(within(successDetail).getByText("request-success")).toBeInTheDocument();
  expect(within(successDetail).getByText("cred-official-1")).toBeInTheDocument();
  expect(within(successDetail).getByText("Team Account")).toBeInTheDocument();
  expect(within(successDetail).getByText("1 count")).toBeInTheDocument();
  expect(within(successDetail).getByText(/"path": "\/v1\/responses"/)).toBeInTheDocument();
  expect(within(successDetail).getByText(/"status": 201/)).toBeInTheDocument();

  await userEvent.click(screen.getByLabelText("查看请求 request-invalid-metadata 详情"));

  const invalidDetail = await screen.findByLabelText("请求 request-invalid-metadata 详情");
  expect(within(invalidDetail).getByText("metadata_json 无法解析，显示原始内容。")).toBeInTheDocument();
  expect(within(invalidDetail).getByText("{bad json")).toBeInTheDocument();

  await userEvent.click(screen.getByLabelText("下一页请求"));

  await waitFor(() =>
    expect(getRoutePool).toHaveBeenLastCalledWith(
      "codex",
      expect.any(String),
      2,
      20,
    ),
  );

  await userEvent.click(screen.getByRole("button", { name: "本月" }));

  await waitFor(() =>
    expect(getRoutePool).toHaveBeenLastCalledWith(
      "codex",
      expectedMonthStart.toISOString(),
      1,
      20,
    ),
  );

  await userEvent.click(screen.getByRole("button", { name: "累计" }));

  await waitFor(() => expect(getRoutePool).toHaveBeenLastCalledWith("codex", null, 1, 20));
});
```

- [ ] **Step 2: Update the test-route payload expectation**

In `it("starts proxy, writes configs, and tests the credential pool route", async () => { ... })`, replace the `routePoolRouteOnce` expectation with:

```tsx
await waitFor(() =>
  expect(routePoolRouteOnce).toHaveBeenCalledWith({
    platform: "codex",
    token_count: 1024,
    cost_micros: 1200,
    metadata_json: JSON.stringify({
      source: "ui_test_route",
      path: "/__ai-switch/test-route",
      status: "selected",
      request_kind: "manual_pool_selection",
    }),
  }),
);
```

- [ ] **Step 3: Run the focused frontend test and verify it fails for the expected reasons**

Run:

```powershell
pnpm test:run -- tests/AccountsScreen.test.tsx
```

Expected: FAIL. The expected failures mention missing detail controls such as `查看请求 request-success 详情`, missing `[data-route-request-row]`, and the old `metadata_json` payload containing only `{"source":"ui_test_route"}`.

### Task 2: Implement Metadata-Only Detail UI

**Files:**
- Modify: `src/screens/AccountsScreen.tsx`
- Test: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes: `RoutePoolUsageLog` from `src/lib/api/types.ts`, existing `routeStats?.requests`, and `routePoolRouteOnce`.
- Produces: `parseUsageMetadata(metadataJson: string): ParsedUsageMetadata`, expanded request state, detail panel markup with `aria-label="请求 <id> 详情"`, and richer safe test-route metadata.

- [ ] **Step 1: Import the request row type**

In `src/screens/AccountsScreen.tsx`, update the type import block to include `RoutePoolUsageLog`:

```tsx
import type {
  AccountStatus,
  InterfaceFormat,
  ModelMapping,
  RouteConfigWriteOutcome,
  RouteCredential,
  RoutePoolUsageLog,
} from "../lib/api/types";
```

- [ ] **Step 2: Replace the metadata parser with a detail-aware parser**

Replace the existing `parseUsageMetadata` function with:

```tsx
type ParsedUsageMetadata = {
  path: string;
  status: string;
  formattedJson: string;
  raw: string;
  valid: boolean;
};

function metadataField(record: Record<string, unknown>, key: string) {
  const value = record[key];
  if (typeof value === "string" && value.trim()) {
    return value;
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  return "-";
}

function parseUsageMetadata(metadataJson: string): ParsedUsageMetadata {
  try {
    const value = JSON.parse(metadataJson) as unknown;
    const formattedJson = JSON.stringify(value, null, 2) ?? metadataJson;
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      return {
        path: "-",
        status: "-",
        formattedJson,
        raw: metadataJson,
        valid: true,
      };
    }

    const record = value as Record<string, unknown>;
    return {
      path: metadataField(record, "path"),
      status: metadataField(record, "status"),
      formattedJson,
      raw: metadataJson,
      valid: true,
    };
  } catch {
    return {
      path: "-",
      status: "-",
      formattedJson: metadataJson,
      raw: metadataJson,
      valid: false,
    };
  }
}
```

- [ ] **Step 3: Add request expansion state**

Near the existing route stats state in `AccountsScreen`, add:

```tsx
const [expandedRequestId, setExpandedRequestId] = useState<string | null>(null);
```

After the existing `useEffect(() => { setRequestPage(1); }, [activePlatform]);`, add:

```tsx
useEffect(() => {
  setExpandedRequestId(null);
}, [activePlatform, statsPeriod, requestPage]);
```

- [ ] **Step 4: Add the detail panel helper inside the module**

Add this helper before `export function AccountsScreen(...)`:

```tsx
function RouteRequestDetail({
  metadata,
  request,
}: {
  metadata: ParsedUsageMetadata;
  request: RoutePoolUsageLog;
}) {
  return (
    <div
      aria-label={`请求 ${request.id} 详情`}
      className="border-t border-stone-100 bg-stone-50 px-3 py-3"
      id={`route-request-detail-${request.id}`}
    >
      <div className="flex items-center justify-between gap-2">
        <p className="text-[12px] font-semibold text-stone-800">请求详情</p>
        <p className="font-mono text-[11px] text-stone-500">{request.id}</p>
      </div>
      <div className="mt-3 grid gap-2 text-[12px] sm:grid-cols-2 lg:grid-cols-3">
        <div>
          <p className="text-[11px] font-medium text-stone-500">账号</p>
          <p className="mt-0.5 text-stone-800">{request.account_name ?? "-"}</p>
        </div>
        <div>
          <p className="text-[11px] font-medium text-stone-500">账号 ID</p>
          <p className="mt-0.5 break-all font-mono text-[11px] text-stone-700">{request.account_id ?? "-"}</p>
        </div>
        <div>
          <p className="text-[11px] font-medium text-stone-500">来源</p>
          <p className="mt-0.5 text-stone-800">{request.source_label}</p>
        </div>
        <div>
          <p className="text-[11px] font-medium text-stone-500">指标</p>
          <p className="mt-0.5 text-stone-800">
            {request.amount} {request.unit}
          </p>
        </div>
        <div>
          <p className="text-[11px] font-medium text-stone-500">时间</p>
          <p className="mt-0.5 text-stone-800">{formatUsageTime(request.created_at)}</p>
        </div>
      </div>
      <div className="mt-3">
        <p className="text-[11px] font-medium text-stone-500">
          {metadata.valid ? "metadata_json" : "metadata_json 无法解析，显示原始内容。"}
        </p>
        <pre className="mt-1 max-h-56 overflow-auto rounded-lg border border-stone-200 bg-white p-2 font-mono text-[11px] leading-relaxed text-stone-700">
          {metadata.valid ? metadata.formattedJson : metadata.raw}
        </pre>
      </div>
    </div>
  );
}
```

- [ ] **Step 5: Enrich the test-route metadata payload**

Replace the `metadata_json` value inside `testRoute` with:

```tsx
metadata_json: JSON.stringify({
  source: "ui_test_route",
  path: "/__ai-switch/test-route",
  status: "selected",
  request_kind: "manual_pool_selection",
}),
```

- [ ] **Step 6: Render expandable request rows**

In the request list mapping under `请求列表`, replace the current request row `<div key={request.id}>...</div>` return block with:

```tsx
const expanded = expandedRequestId === request.id;
return (
  <div className="bg-white" data-route-request-row key={request.id}>
    <div className="grid gap-2 px-3 py-2.5 text-[12px] text-stone-600 lg:grid-cols-[1.2fr_1fr_0.5fr_1.4fr_0.8fr_auto] lg:items-center">
      <span className="font-medium text-stone-800">{formatUsageTime(request.created_at)}</span>
      <span className="truncate">{request.account_name ?? request.account_id ?? "-"}</span>
      <span className="rounded-lg bg-stone-100 px-2 py-1 text-center font-semibold text-stone-700">
        {metadata.status}
      </span>
      <span className="truncate font-mono text-[11px]">{metadata.path}</span>
      <span className="truncate">{request.source_label}</span>
      <button
        aria-controls={`route-request-detail-${request.id}`}
        aria-expanded={expanded}
        aria-label={`${expanded ? "隐藏" : "查看"}请求 ${request.id} 详情`}
        className="inline-flex items-center justify-center rounded-lg border border-stone-200 bg-white px-2.5 py-1.5 text-[12px] font-semibold text-stone-700 transition-colors hover:bg-stone-50"
        onClick={() => setExpandedRequestId(expanded ? null : request.id)}
        type="button"
      >
        详情
      </button>
    </div>
    {expanded ? <RouteRequestDetail metadata={metadata} request={request} /> : null}
  </div>
);
```

- [ ] **Step 7: Run the focused frontend test and verify it passes**

Run:

```powershell
pnpm test:run -- tests/AccountsScreen.test.tsx
```

Expected: PASS for `tests/AccountsScreen.test.tsx`.

- [ ] **Step 8: Run the broader frontend verification**

Run:

```powershell
pnpm typecheck
pnpm test:run
```

Expected: both commands PASS.

- [ ] **Step 9: Commit implementation**

Run:

```powershell
git status --short
git add -- src/screens/AccountsScreen.tsx tests/AccountsScreen.test.tsx docs/superpowers/plans/2026-07-19-route-request-details.md
git commit -m "feat: add route request details"
```

Expected: a commit on `main` containing the route request detail implementation, tests, and plan document.
