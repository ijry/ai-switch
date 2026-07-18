# Route Pool Stats Refresh And Pagination Design

## Summary

Extend the existing per-platform route pool statistics panel with automatic refresh, backend-backed request pagination, and a corrected statistics scope. The panel remains inside `AccountsScreen`; this is not a new global statistics page.

The statistics scope changes from "currently enabled route pool members" to "all existing route credentials for the current platform". This means an account that was used through the route pool or proxy still contributes historical usage after the user removes it from the active pool. The current active pool membership count stays available as a separate operational value.

## Goals

- Auto-refresh the statistics panel while it is open.
- Paginate the request list with backend limits and totals.
- Include historical usage for accounts that still exist but are no longer active route pool members.
- Preserve period filters: `today`, `week`, `month`, and `all`.
- Keep existing pool membership, route testing, route proxy, and config-writing behavior working.

## Non-Goals

- No new global statistics route or dashboard.
- No export, deletion, or custom date-range workflow.
- No usage archive for deleted credentials. Deleted credentials do not reliably preserve platform and display-name metadata in the current schema, so this design only guarantees history for credentials that still exist after being removed from the pool.
- No schema migration unless implementation discovers an indexing need.

## User Experience

When the user opens the statistics panel, it refreshes immediately and then refreshes every 5 seconds. Closing the panel stops the interval. Changing platform, period, or page fetches the relevant data immediately.

The request list defaults to page 1 with 20 rows per page. Controls show the current page, total pages, and total request rows for the selected period. The user can move to the previous or next page. Switching platform or period resets the page to 1. Auto-refresh keeps the current page selected.

The panel copy should no longer say the totals only include current pool accounts. It should say the totals cover the current platform's historical route usage. The active pool count can remain in the pool header as "已加入 N 个账号".

## API Design

Extend `get_route_pool` rather than adding a separate request-list command:

- Input:
  - `platform: string`
  - `since?: string | null`
  - `request_page?: number | null`
  - `request_page_size?: number | null`
- Output remains `RoutePoolState`, with `RoutePoolStats` extended by pagination metadata:
  - `request_row_count: number`
  - `request_page: number`
  - `request_page_size: number`

The backend normalizes invalid or missing pagination values:

- Page defaults to `1`.
- Page size defaults to `20`.
- Page size is clamped to a small fixed range, such as `1..100`.

`request_count` remains the aggregate request metric total for the selected period. `request_row_count` is the number of request log rows available for pagination.

## Backend Design

`RoutePoolService::get` accepts normalized pagination values and passes them to the repository. `set_members` can keep returning state with default pagination because that path is about pool editing, not user-driven request-list navigation. `route_once` can also keep returning default stats because the frontend invalidates the main route-pool query after route tests.

`RoutePoolRepository::stats` changes its usage joins:

- Active member count still comes from `route_pool_members` for the current platform.
- Summary totals query `usage_events` joined to `route_credentials` by `usage_events.route_credential_id = route_credentials.id`.
- The platform filter uses `route_credentials.platform = ?`, not `route_pool_members.platform = ?`.
- The optional `since` lower bound applies to all totals and request list queries.
- Request list rows use the same route credential join and `metric_type = 'request'`.
- Request rows are ordered by `created_at DESC`, with a stable secondary sort by `id DESC`.
- `LIMIT` and `OFFSET` implement pagination.
- A separate count query computes `request_row_count` for the same filter.

Display names are still best-effort through the route credential join. Existing credentials that were removed from the pool will still show their names. Deleted credentials are outside this design's guarantee.

## Frontend Design

`getRoutePool` accepts pagination arguments and includes them in the invoke payload. `AccountsScreen` adds `requestPage` state and uses a page size constant of `20`.

The route pool query key includes:

- active platform
- selected `since`
- current request page
- page size

React Query uses `refetchInterval: statsOpen ? 5000 : false` so polling happens only when the statistics panel is open. The query remains enabled for the screen's existing pool state needs, but polling is scoped to the open panel.

Page reset rules:

- When `activePlatform` changes, set `requestPage` to `1`.
- When `statsPeriod` changes, set `requestPage` to `1`.
- When auto-refresh runs, do not reset the page.

If a refresh returns fewer pages than the current page, the UI should move back to the last available page or page 1. This handles data disappearing after period changes or cleanup.

## Error Handling

- Invalid platform returns the existing route pool validation error.
- Invalid `since` keeps using the existing validation error.
- Invalid page values are normalized rather than treated as fatal user errors.
- Invalid `metadata_json` keeps rendering fallback path/status values.
- Database errors use the existing `AppError::Database` pattern.

## Testing

Backend tests:

- Stats include usage for a credential after it is removed from `route_pool_members`.
- Stats do not include usage from another platform.
- Request list pagination returns the requested page and total row count.
- Existing `since` filtering still applies to totals and request rows.
- Existing current pool member count still reflects active pool members only.

Frontend tests:

- Opening the statistics panel enables 5-second auto-refresh; closing it stops polling.
- Pagination controls call `getRoutePool` with the expected page and page size.
- Changing period resets the request page to 1.
- Historical usage copy no longer says stats only include current pool accounts.
- Request rows still render metadata fallbacks for invalid metadata.

## Acceptance Criteria

- The statistics panel refreshes automatically every 5 seconds while open.
- The request list supports backend-backed pagination.
- Removing an account from the active route pool does not remove its existing usage from current-platform statistics.
- Summary totals are period-filtered and not affected by request-list pagination.
- Current pool editing, route testing, proxy controls, and existing period filters keep working.
